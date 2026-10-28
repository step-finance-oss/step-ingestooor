#![allow(clippy::unwrap_used)]
use std::{
    collections::HashMap,
    env,
    fs::File,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

use anyhow::Result;
use clap::{Parser, ValueEnum};
use heck::{ToShoutySnekCase, ToSnakeCase};
use proc_macro2::Span;
use quote::quote;
use serde_json::Value;

pub mod field_type;
use field_type::*;

#[derive(Debug, Clone, ValueEnum, PartialEq, Eq)]
pub enum RunMode {
    /// Output generated code to stdout only
    Dry,
    /// Write files to a temporary local directory in parser-gen/temp
    /// SQL will always be in this directory on Temp or Live
    Temp,
    /// Write files to actual parser directories and update mod files
    Live,
}

#[derive(Debug, Clone, ValueEnum, PartialEq, Eq)]
pub enum SqlTarget {
    Local,
    Live,
}

#[derive(Parser, PartialEq, Eq, Debug)]
pub struct ParserGenArgs {
    #[clap(short)]
    pub idl: String,
    // Name of the program. Be sure to name this correctly.
    // This will be used to name the parser and the dooots.
    #[clap(short)]
    pub program_name: String,
    // Type of parser this is generating
    // (lending, staking, etc)
    #[clap(long)]
    pub parser_type: String,
    /// Run mode: dry (stdout), temp (to parser-gen/ temp dir), or live (mutate actual directories)
    #[clap(short, value_enum, default_value = "dry")]
    pub run_mode: RunMode,
    /// Path to dooot directory (relative to workspace root)
    #[clap(
        long,
        default_value = "libs/step-common-rust/crates/ingestooor-sdk/src/dooot"
    )]
    pub dooot_dir: String,

    /// Path to parsers directory (relative to workspace root)
    #[clap(long, default_value = "crates/parsers/src/")]
    pub parsers_dir: String,
    // Target for sql files. Local will not add 'on cluster' to the sql files etc
    #[clap(short, value_enum, default_value = "local")]
    pub sql_target: SqlTarget,
}

impl ParserGenArgs {
    pub fn get_dooot_path(&self) -> Result<PathBuf> {
        Ok(find_workspace_root()?.join(&self.dooot_dir))
    }

    pub fn get_parsers_path(&self) -> Result<PathBuf> {
        Ok(find_workspace_root()?.join(&self.parsers_dir))
    }
}

fn find_workspace_root() -> Result<PathBuf> {
    let mut current_dir = env::current_dir()?;

    loop {
        let cargo_toml = current_dir.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                if content.contains("[workspace]") {
                    return Ok(current_dir);
                }
            }
        }

        if let Some(parent) = current_dir.parent() {
            current_dir = parent.to_path_buf();
        } else {
            return Err(anyhow::anyhow!("Could not find workspace root"));
        }
    }
}

fn main() -> Result<()> {
    let args = ParserGenArgs::parse();
    let mut file_path = env::current_dir()?;
    file_path.push(args.idl.clone());
    read_idl(file_path, args)?;

    Ok(())
}

fn read_idl(path: PathBuf, args: ParserGenArgs) -> Result<()> {
    // read file, see if it has anchor spec, else we need to convert it.

    let file = File::open(path.clone()).expect("to open idl");
    let idl: Value = serde_json::from_reader(file).expect("idl to be json");

    match idl.get("metadata") {
        Some(metadata) => match metadata.get("spec") {
            Some(_) => {
                println!("idl already converted");
                parse_idl(idl, args)?;
            }
            None => {
                // run commnad to anchor convert
                let mut anchor_convert = Command::new("anchor")
                    .args(["idl", "convert", path.to_str().unwrap()])
                    .stdout(Stdio::piped())
                    .spawn()?;
                let convert_data = anchor_convert
                    .stdout
                    .take()
                    .expect("anchor convert to have stdout");
                println!("reading stdout");

                let idl: Value = serde_json::from_reader(convert_data)?;
                parse_idl(idl, args)?;
            }
        },
        None => panic!("Add metadata.address to the idl"),
    }

    Ok(())
}

pub struct AccountData {
    discrim: Vec<String>,
}

fn parse_idl(idl: Value, args: ParserGenArgs) -> Result<()> {
    let program_name = args.program_name.clone();
    let accounts_arr = idl.get("accounts").expect("to find accounts in idl");
    let types_arr = idl.get("types").expect("to find types in idl");
    let program_id = idl
        .get("address")
        .expect("to find program id in idl")
        .as_str()
        .expect("to convert to string")
        .to_string();
    let mut account_data: HashMap<String, AccountData> = HashMap::new();
    let mut type_defs: HashMap<String, TypeKind> = HashMap::new();

    for acc in accounts_arr.as_array().expect("accounts to be array") {
        let name = acc
            .get("name")
            .expect("account to have name")
            .as_str()
            .expect("to convert to string");
        let discrim: Vec<String> = acc
            .get("discriminator")
            .expect("account to have discrim")
            .as_array()
            .expect("discrim to be an array")
            .clone()
            .iter_mut()
            .map(|x| x.as_u64().expect("discrims to be a number").to_string())
            .collect();

        account_data.insert(name.to_string(), AccountData { discrim });
    }

    // need to parse types first to build up the type registry
    // We need to do this to handle enums and structs that are used in other types
    // Two-pass parsing approach:
    // Pass 1: Build type registry with type names and kinds
    // Pass 2: Parse field types with access to complete type registry

    // Pass 1: Build type registry
    for t in types_arr.as_array().expect("types to be array") {
        let ty_type = t.get("type").expect("type to have a type");
        let parent_name = t
            .get("name")
            .expect("type to have a name")
            .as_str()
            .unwrap()
            .to_string();
        let kind = ty_type
            .get("kind")
            .expect("idl types type to have a kind")
            .as_str()
            .expect("kind to be a string");

        match kind {
            "enum" => {
                let variants = ty_type
                    .get("variants")
                    .expect("enum type to have variants")
                    .as_array()
                    .expect("variants to be an array");
                let mut ty_def = EnumDef {
                    name: parent_name.clone(),
                    variants: vec![],
                };
                for variant in variants {
                    let name = variant
                        .get("name")
                        .expect("enum variant to have name")
                        .as_str()
                        .unwrap()
                        .to_string();
                    ty_def.variants.push(name);
                }
                type_defs.insert(parent_name.to_string(), TypeKind::Enum(ty_def));
            }
            "struct" => {
                // Initialize empty struct - fields will be populated in pass 2
                type_defs.insert(parent_name.clone(), TypeKind::Struct(vec![]));
            }
            "type" => {
                let alias = ty_type.get("alias").unwrap();
                let alias_type = parse_field_type_recursive(alias, None)?;
                type_defs.insert(parent_name.to_string(), TypeKind::Alias(alias_type));
            }
            _ => panic!("unknown type kind {}", kind),
        }
    }

    // Pass 2: Parse struct fields with complete type registry
    for t in types_arr.as_array().expect("types to be array") {
        let ty_type = t.get("type").expect("type to have a type");
        let parent_name = t
            .get("name")
            .expect("type to have a name")
            .as_str()
            .unwrap()
            .to_string();
        let kind = ty_type
            .get("kind")
            .expect("idl types type to have a kind")
            .as_str()
            .expect("kind to be a string");

        if kind == "struct" {
            let fields = ty_type.get("fields");
            let Some(fields) = fields else {
                // this struct is empty entirely. We already initialized it above.
                // I think this case happens for pdas that hold no data
                continue;
            };

            let fields = fields.as_array().expect("fields struct to be array");

            for field in fields {
                let child_name = field
                    .get("name")
                    .expect("field to have a name")
                    .as_str()
                    .unwrap()
                    .to_string();
                let field_type = field.get("type").expect("field to have type");

                // Now we can pass the complete type_defs to correctly identify enums vs structs
                let parsed_type = parse_field_type_recursive(field_type, Some(&type_defs))?;
                let ty_def = TypeDef {
                    name: child_name,
                    field_type: parsed_type,
                };

                type_defs.entry(parent_name.to_string()).and_modify(|x| {
                    if let TypeKind::Struct(x) = x {
                        x.push(ty_def)
                    }
                });
            }
        }
    }

    // println!("type defs: {:#?}", type_defs);

    let mut dooot_names = vec![];
    let mut dooot_formatted_code = Vec::new();
    let imports = quote! {
        use chrono::NaiveDateTime;
        use clickhouse::Row;
        use evalexpr::Value;
        use serde::Deserialize;
        use serde::Serialize;
        use std::fmt::Display;
        use step_common_rust::evalexpr::ValueWrapper;
        use step_ingestooor_clickhouse_derive::ClickhouseDooot;
        use step_ingestooor_derive::Dooot;
        use ts_rs::TS;
    };
    let syntax_tree = syn::parse2(imports).unwrap();
    let formatted_code = prettyplease::unparse(&syntax_tree);
    dooot_formatted_code.push(formatted_code);
    // Generate code for accounts dooots
    for account in account_data.iter() {
        let account_name = account.0;
        let account_type = type_defs.get(account_name).unwrap();
        let dooot_name = format!("Ps{}{}", program_name, account_name);
        let doot_struct_name = format!("{}Dooot", dooot_name);
        dooot_names.push(dooot_name.clone());
        let incoming_table =
            format!("incoming_ps_{}_{}_dooot", program_name, account_name).to_snake_case();
        let queue_name = format!("ps_{}_{}_dooot", program_name, account_name).to_snake_case();
        let dooot_name_ident = syn::parse_str::<syn::Ident>(&doot_struct_name).unwrap();
        let fields = match account_type {
            TypeKind::Struct(fields) => fields,
            _ => panic!("Account type is not a struct"),
        };
        let field_definitions = fields
            .iter()
            .map(|field| {
                let field_name = syn::parse_str::<syn::Ident>(&field.name).unwrap();
                let rust_type_str = field.field_type.to_dooot_representation();
                let rust_type = syn::parse_str::<syn::Type>(&rust_type_str).unwrap();
                quote! {
                    pub #field_name: #rust_type,
                }
            })
            .collect::<Vec<_>>();

        let dooot = quote! {
            #[derive(
                Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, Row,
            )]
            #[dooot(name = #dooot_name)]
            #[clickhouse(
                table = #incoming_table,
                queue = #queue_name,
                flush_ms = 5000
            )]
            pub struct #dooot_name_ident {
                 #[serde(
                    serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
                    deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
                )]
                pub time: NaiveDateTime,
                pub account_pubkey: String,
                #(#field_definitions)*
                pub closed: bool,
            }
        };

        let syntax_tree = syn::parse2(dooot).unwrap();
        let formatted_code = prettyplease::unparse(&syntax_tree);
        dooot_formatted_code.push(formatted_code);
    }

    // Generate code for all non-account types (enums, structs, type aliases)
    for (type_name, type_def) in type_defs.iter() {
        // Skip account types - they're handled separately above
        if account_data.contains_key(type_name) {
            continue;
        }

        let type_name_ident = syn::parse_str::<syn::Ident>(type_name).unwrap();
        let type_name_str = syn::LitStr::new(type_name, Span::call_site());

        match type_def {
            TypeKind::Enum(enum_def) => {
                let variant_definitions = enum_def
                    .variants
                    .iter()
                    .map(|variant_name| {
                        let variant_name_ident =
                            syn::parse_str::<syn::Ident>(variant_name).unwrap();
                        quote! { #variant_name_ident }
                    })
                    .collect::<Vec<_>>();
                // for ex. TunaPositionState::Normal => write!(f, "Normal"),
                let variant_definitions_to_string = enum_def
                    .variants
                    .iter()
                    .map(|variant_name| {
                        let variant_name_ident =
                            syn::parse_str::<syn::Ident>(variant_name).unwrap();
                        let variant_str = syn::LitStr::new(variant_name, Span::call_site());
                        quote! { #type_name_ident::#variant_name_ident => write!(f, #variant_str), }
                    })
                    .collect::<Vec<_>>();

                let variant_from_u8_arms = enum_def
                    .variants
                    .iter()
                    .enumerate()
                    .map(|(index, variant_name)| {
                        let variant_name_ident =
                            syn::parse_str::<syn::Ident>(variant_name).unwrap();
                        let index = index as u8;
                        quote! { #index => #type_name_ident::#variant_name_ident, }
                    })
                    .collect::<Vec<_>>();

                let enum_def = quote! {
                    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS, Default)]
                    pub enum #type_name_ident {
                        #[default]
                        #(#variant_definitions),*
                    }

                    impl Display for #type_name_ident {
                        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                            match self {
                                #(#variant_definitions_to_string)*
                            }
                        }
                    }

                    impl From<u8> for #type_name_ident {
                        fn from(value: u8) -> Self {
                            match value {
                                #(#variant_from_u8_arms)*
                                _ => Self::default(), // fallback to default variant for unknown values
                            }
                        }
                    }

                    impl From<#type_name_ident> for ValueWrapper {
                        fn from(_: #type_name_ident) -> Self {
                            panic!("ValueWrapper conversion not implemented for {}", #type_name_str)
                        }
                    }
                };

                let syntax_tree = syn::parse2(enum_def).unwrap();
                let formatted_code = prettyplease::unparse(&syntax_tree);
                dooot_formatted_code.push(formatted_code);
            }
            TypeKind::Struct(fields) => {
                let field_definitions = fields
                    .iter()
                    .map(|field| {
                        let field_name = syn::parse_str::<syn::Ident>(&field.name).unwrap();
                        let rust_type_str = field.field_type.to_dooot_representation();
                        let rust_type = syn::parse_str::<syn::Type>(&rust_type_str).unwrap();
                        quote! {
                            pub #field_name: #rust_type,
                        }
                    })
                    .collect::<Vec<_>>();

                let struct_def = quote! {
                    #[derive(Serialize, Deserialize, Debug, Clone, TS, Default)]
                    pub struct #type_name_ident {
                        #(#field_definitions)*
                    }
                };

                let syntax_tree = syn::parse2(struct_def).unwrap();
                let formatted_code = prettyplease::unparse(&syntax_tree);
                dooot_formatted_code.push(formatted_code);

                // Generate ValueWrapper implementation for the struct
                let value_wrapper_impl = quote! {
                    impl From<#type_name_ident> for ValueWrapper {
                        fn from(_: #type_name_ident) -> Self {
                            panic!("ValueWrapper conversion not implemented for {}", #type_name_str)
                        }
                    }
                };

                let syntax_tree = syn::parse2(value_wrapper_impl).unwrap();
                let formatted_code = prettyplease::unparse(&syntax_tree);
                dooot_formatted_code.push(formatted_code);
            }
            TypeKind::Alias(alias_type) => {
                let rust_type_str = alias_type.to_dooot_representation();
                let rust_type = syn::parse_str::<syn::Type>(&rust_type_str).unwrap();

                let alias_def = quote! {
                    pub type #type_name_ident = #rust_type;
                };

                let syntax_tree = syn::parse2(alias_def).unwrap();
                let formatted_code = prettyplease::unparse(&syntax_tree);
                dooot_formatted_code.push(formatted_code);
            }
        }
    }

    let sql_incoming_tables =
        generate_sql_incoming_tables(&account_data, &type_defs, &program_name);

    let sql_time_tables =
        generate_sql_time_tables(&account_data, &type_defs, &program_name, &args.sql_target);

    match args.run_mode {
        RunMode::Temp => {
            write_generated_files(
                &program_name,
                &dooot_formatted_code.join("\n"),
                &sql_incoming_tables,
                &sql_time_tables,
                true,
            )?;
        }
        RunMode::Live => {
            write_generated_files(
                &program_name,
                &dooot_formatted_code.join("\n"),
                &sql_incoming_tables,
                &sql_time_tables,
                false,
            )?;
        }
        RunMode::Dry => {
            println!(
                "Would have written dooot: {}",
                dooot_formatted_code.join("\n")
            );
            println!("\nWould have written SQL tables:\n{}", sql_incoming_tables);
            println!("\nWould have written SQL time tables:\n{}", sql_time_tables);
            return Ok(());
        }
    }

    generate_parser_files(
        &args,
        &account_data,
        &type_defs,
        &program_name,
        &program_id,
        &dooot_names,
    )?;

    // Update Dooot enum in Live mode
    if args.run_mode == RunMode::Live {
        update_dooot_enum(&dooot_names, &program_name, &args)?;
    }

    Ok(())
}

fn write_generated_files(
    program_name: &str,
    dooot_code: &str,
    sql_incoming_tables: &str,
    sql_time_tables: &str,
    include_dooot: bool,
) -> Result<()> {
    let base_path = env::current_dir()?.join("crates/parser-gen/gen-temp/");

    std::fs::create_dir_all(&base_path)?;

    if include_dooot {
        let dooot_path = base_path.join(format!("{}.rs", program_name.to_lowercase()));
        let mut file = File::create(&dooot_path)?;
        file.write_all(dooot_code.as_bytes())?;
    }

    let incoming_sql_path = base_path.join(format!("{}.sql", program_name.to_lowercase()));
    let mut file = File::create(&incoming_sql_path)?;
    file.write_all(sql_incoming_tables.as_bytes())?;

    let time_sql_path = base_path.join(format!("time_ps_{}.sql", program_name.to_lowercase()));
    let mut file = File::create(&time_sql_path)?;
    file.write_all(sql_time_tables.as_bytes())?;

    Ok(())
}

fn generate_sql_incoming_tables(
    account_data: &HashMap<String, AccountData>,
    type_defs: &HashMap<String, TypeKind>,
    program_name: &String,
) -> String {
    let mut sql_statements = Vec::new();

    for account_name in account_data.keys() {
        let table_name =
            format!("incoming_ps_{}_{}_dooot", program_name, account_name).to_snake_case();

        let mut columns = vec![
            "time DateTime64(6, 'UTC')".to_string(),
            "account_pubkey String".to_string(),
        ];

        // Get the account type definition
        if let Some(TypeKind::Struct(fields)) = type_defs.get(account_name) {
            for field in fields {
                let column_type = field
                    .field_type
                    .to_incoming_table_representation(Some(type_defs));
                columns.push(format!("{} {}", field.name, column_type));
            }
        }

        let sql = format!(
            "CREATE TABLE {} (\n    {},\n    closed Bool\n) ENGINE = Null;",
            table_name,
            columns.join(",\n    ")
        );

        sql_statements.push(sql);
    }

    sql_statements.join("\n\n")
}

fn generate_sql_time_tables(
    account_data: &HashMap<String, AccountData>,
    type_defs: &HashMap<String, TypeKind>,
    program_name: &String,
    sql_target: &SqlTarget,
) -> String {
    let mut sql_statements = Vec::new();

    for account_name in account_data.keys() {
        let time_table_name = format!("time_ps_{}_{}", program_name, account_name).to_snake_case();
        let incoming_table_name =
            format!("incoming_ps_{}_{}_dooot", program_name, account_name).to_snake_case();

        let mut time_columns = vec![
            "time DateTime64(6, 'UTC')".to_string(),
            "account_pubkey FixedString(32)".to_string(),
        ];

        let mut mv_select_fields = vec![
            "time".to_string(),
            "base58Decode(account_pubkey) as account_pubkey".to_string(),
        ];

        if let Some(TypeKind::Struct(fields)) = type_defs.get(account_name) {
            for field in fields {
                let time_column_type = field
                    .field_type
                    .to_time_table_representation(Some(type_defs));
                time_columns.push(format!("{} {}", field.name, time_column_type));

                let mv_field = field
                    .field_type
                    .to_materialized_view_conversion(&field.name, Some(type_defs));
                mv_select_fields.push(mv_field);
            }
        }

        let time_table_sql = if *sql_target == SqlTarget::Live {
            format!(
                r#"CREATE TABLE {} ON CLUSTER "{{cluster}}" (
    {}
) ENGINE = ReplicatedMergeTree()
PARTITION BY toYYYYMM(time)
ORDER BY (account_pubkey, time)
TTL toDateTime(time) + INTERVAL 1 MONTH TO VOLUME 'volume_s3'
SETTINGS storage_policy = 's3_tiered_policy';"#,
                time_table_name,
                time_columns.join(",\n    "),
            )
        } else {
            format!(
                r#"CREATE TABLE {} (
    {},
    closed Bool
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(time)
ORDER BY (account_pubkey, time);"#,
                time_table_name,
                time_columns.join(",\n    "),
            )
        };

        let mv_name = format!("mv_{}", time_table_name);
        let mv_sql = format!(
            r#"CREATE MATERIALIZED VIEW {} TO {} 
AS SELECT
    {},
    closed
FROM {};"#,
            mv_name,
            time_table_name,
            mv_select_fields.join(",\n    "),
            incoming_table_name
        );

        sql_statements.push(time_table_sql);
        sql_statements.push(mv_sql);
    }

    sql_statements.join("\n\n")
}

fn generate_parser_files(
    args: &ParserGenArgs,
    account_data: &HashMap<String, AccountData>,
    type_defs: &HashMap<String, TypeKind>,
    program_name: &String,
    program_id: &String,
    dooot_names: &[String],
) -> Result<()> {
    let discriminator_constants = account_data
        .iter()
        .map(|(account_name, account_data)| {
            let const_name = format!("{}_DISCRIMINATOR", account_name.TO_SHOUTY_SNEK_CASE());
            let const_name_ident = syn::parse_str::<syn::Ident>(&const_name).unwrap();
            let discriminator_bytes = account_data
                .discrim
                .iter()
                .map(|d| d.parse::<u8>().unwrap())
                .collect::<Vec<_>>();

            quote! {
                const #const_name_ident: &[u8] = &[#(#discriminator_bytes),*];
            }
        })
        .collect::<Vec<_>>();

    let dooot_imports = dooot_names
        .iter()
        .map(|name| {
            let dooot_name = format!("{}Dooot", name);
            syn::parse_str::<syn::Ident>(&dooot_name).unwrap()
        })
        .collect::<Vec<_>>();

    let type_imports = type_defs
        .keys()
        .filter(|name| !account_data.contains_key(*name)) // Exclude accounts, they're imported separately
        .map(|name| syn::parse_str::<syn::Ident>(name).unwrap())
        .collect::<Vec<_>>();

    let program_id_const = format!("{}_PROGRAM_ID", program_name.to_uppercase());
    let program_id_const_ident = syn::parse_str::<syn::Ident>(&program_id_const).unwrap();
    let program_id_literal = format!("\"{}\"", program_id);
    let program_id_indent = syn::parse_str::<syn::LitStr>(&program_id_literal).unwrap();

    let program_name_ident = syn::parse_str::<syn::Ident>(program_name).unwrap();
    let program_name_str = syn::LitStr::new(program_name, Span::call_site());

    let match_arms = account_data
        .iter()
        .map(|(account_name, _account_data)| {
            let const_name = format!("{}_DISCRIMINATOR", account_name.TO_SHOUTY_SNEK_CASE());
            let const_name_ident = syn::parse_str::<syn::Ident>(&const_name).unwrap();
            let dooot_variant = format!("Ps{}{}", program_name, account_name);
            let dooot_variant_ident = syn::parse_str::<syn::Ident>(&dooot_variant).unwrap();
            let dooot_struct = format!("Ps{}{}Dooot", program_name, account_name);
            let dooot_struct_ident = syn::parse_str::<syn::Ident>(&dooot_struct).unwrap();

            let account_type = type_defs.get(account_name).unwrap();
            let fields = match account_type {
                TypeKind::Struct(fields) => fields,
                _ => panic!("Account type is not a struct"),
            };

            let field_assignments = fields
                .iter()
                .map(|field| {
                    let field_name = syn::parse_str::<syn::Ident>(&field.name).unwrap();
                    let field_read = field.field_type.generate_field_read_code(Some(type_defs));
                    quote! { #field_name: #field_read }
                })
                .collect::<Vec<_>>();

            quote! {
                #const_name_ident => {
                    let mut reader = StepAccountReader::new(&data[8..]);

                    let dooot = Dooot::#dooot_variant_ident(#dooot_struct_ident {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        #(#field_assignments,)*
                        closed,
                    });
                    dooots.push(dooot);
                }
            }
        })
        .collect::<Vec<_>>();

    let state_match_arms = account_data
        .iter()
        .map(|(account_name, _account_data)| {
            let const_name = format!("{}_DISCRIMINATOR", account_name.TO_SHOUTY_SNEK_CASE());
            let const_name_ident = syn::parse_str::<syn::Ident>(&const_name).unwrap();
            let dooot_variant = format!("Ps{}{}", program_name, account_name);
            let dooot_variant_ident = syn::parse_str::<syn::Ident>(&dooot_variant).unwrap();
            let dooot_struct = format!("Ps{}{}Dooot", program_name, account_name);
            let dooot_struct_ident = syn::parse_str::<syn::Ident>(&dooot_struct).unwrap();

            let account_type = type_defs.get(account_name).unwrap();
            let fields = match account_type {
                TypeKind::Struct(fields) => fields,
                _ => panic!("Account type is not a struct"),
            };

            let field_assignments = fields
                .iter()
                .map(|field| {
                    let field_name = syn::parse_str::<syn::Ident>(&field.name).unwrap();
                    let field_read = field.field_type.generate_field_read_code(Some(type_defs));
                    quote! { #field_name: #field_read }
                })
                .collect::<Vec<_>>();

            quote! {
                #const_name_ident => {
                    let mut reader = StepAccountReader::new(&data);

                    let dooot = Dooot::#dooot_variant_ident(#dooot_struct_ident {
                        time,
                        account_pubkey: acc.pubkey.to_string(),
                        #(#field_assignments,)*
                        closed: false,
                    });
                    tx.send(dooot)?;
                }
            }
        })
        .collect::<Vec<_>>();

    let parser_def = quote! {
        use std::str::FromStr;
        use crate::{state_source::StateSource, TransactionParserFilter, TransactionParserTrait};
        use anyhow::Result;
        use async_trait::async_trait;
        use borsh::BorshDeserialize;
        use solana_sdk::{account::ReadableAccount, pubkey::Pubkey};
        use step_ingestooor_sdk::{
            dooot::{
                Dooot, #(#dooot_imports),*,
                #(#type_imports),*,
            },
            trait_defs::{cache::TimeSeriesCacheLike, transaction::TransactionLike},
            utils::{step_account_reader::StepAccountReader, step_utils::StepUtils, util_fns::convert_blocktime_to_micros},
        };


        pub const #program_id_const_ident: &str = #program_id_indent;

        #(#discriminator_constants)*

        pub struct #program_name_ident;

        #[async_trait]
        impl TransactionParserTrait for #program_name_ident {
            fn get_name(&self) -> &'static str {
                #program_name_str
            }

            fn get_filters(&self) -> Vec<TransactionParserFilter> {
                vec![TransactionParserFilter::ProgramId(
                    #program_id_const_ident.to_string()
                )]
            }

            async fn invoke(
                &self,
                txn: &impl TransactionLike,
                dooots: &mut Vec<Dooot>,
                _ts_cache: &impl TimeSeriesCacheLike,
                _step_utils: &StepUtils,
            ) -> Result<()> {
                let time = txn.get_block_time();
                let acct_iter = txn.iter_accounts().filter(|a| {
                    (a.pre_owner_program == Some(#program_id_const_ident)
                        || a.post_owner_program == Some(#program_id_const_ident))
                        && a.datum_changed
                });

                for acct in acct_iter {
                    let pre_data = acct.unpack_pre_datum()?.map(|d| (d.is_empty(), d));
                    let post_data = acct.unpack_post_datum()?.map(|d| (d.is_empty(), d));

                    let (closed, data) = match (pre_data, post_data) {
                        (_, Some((false, post))) => (false, post),
                        (Some((false, pre)), None | Some((true, _))) => (true, pre),
                        _ => continue,
                    };

                    let Some(disc) = data.get(..8) else {
                        continue;
                    };

                    match disc {
                        #(#match_arms)*
                        _ => continue,
                    }
                }

                Ok(())
            }
        }

        impl StateSource for #program_name_ident {
            fn get_name(&self) -> &'static str {
                #program_name_str
            }

            fn get_state(
                &self,
                _config: Option<String>,
                acct_provider: impl crate::state_source::AccountProvider,
                tx: std::sync::mpsc::SyncSender<Dooot>,
                _ts_cache: &impl TimeSeriesCacheLike,
            ) -> Result<()> {
                let block_time = acct_provider.get_block_time();
                let time = convert_blocktime_to_micros(block_time, Some(0), None)?;
                let p_acc_iter = acct_provider.program_accounts(Pubkey::from_str(#program_id_const_ident)?, None);

                for acc in p_acc_iter {
                    let data = acc.account.data();
                    let Some(disc) = data.get(..8) else {
                        continue;
                    };

                    match disc {
                        #(#state_match_arms)*
                        _ => continue,
                    }
                }

                Ok(())
            }
        }
    };

    let parser_syntax_tree = syn::parse2(parser_def).unwrap();
    let parser_formatted_code = prettyplease::unparse(&parser_syntax_tree);

    if args.run_mode == RunMode::Dry {
        println!("Would have written parser: {}", parser_formatted_code);
        return Ok(());
    }

    let parsers_path = match args.run_mode {
        RunMode::Temp => {
            let mut path = env::current_dir()?;
            path.push("crates/parser-gen/gen-temp/");
            path
        }
        RunMode::Live => args.get_parsers_path()?,
        _ => return Ok(()),
    };

    let parser_type_dir = parsers_path.join(&args.parser_type);
    if parser_type_dir.exists() {
        let parser_file_path = parser_type_dir.join(format!("{}.rs", program_name.to_lowercase()));
        let mut parser_file = File::create(&parser_file_path)?;
        parser_file.write_all(parser_formatted_code.as_bytes())?;

        let mod_rs_path = parser_type_dir.join("mod.rs");
        if mod_rs_path.exists() {
            let mut mod_content = std::fs::read_to_string(&mod_rs_path)?;
            let module_line = format!("pub mod {};\n", program_name.to_lowercase());

            if !mod_content.contains(&format!("pub mod {};", program_name.to_lowercase())) {
                mod_content.push_str(&module_line);
                std::fs::write(&mod_rs_path, mod_content)?;
            }
        } else {
            let module_line = format!("pub mod {};\n", program_name.to_lowercase());
            std::fs::write(&mod_rs_path, module_line)?;
        }

        println!(
            "Added parser to existing directory: {}",
            parser_type_dir.display()
        );
    } else {
        std::fs::create_dir_all(&parser_type_dir)?;

        let parser_file_path = parser_type_dir.join(format!("{}.rs", program_name.to_lowercase()));
        let mut parser_file = File::create(&parser_file_path)?;
        parser_file.write_all(parser_formatted_code.as_bytes())?;
        let mod_rs_path = parser_type_dir.join("mod.rs");
        let module_line = format!("pub mod {};\n", program_name.to_lowercase());
        std::fs::write(&mod_rs_path, module_line)?;

        let lib_rs_path = parsers_path.join("lib.rs");
        let lib_content = if lib_rs_path.exists() {
            std::fs::read_to_string(&lib_rs_path)?
        } else {
            String::new()
        };
        let new_module_line = format!("pub mod {};\n", args.parser_type);

        let lines: Vec<&str> = lib_content.lines().collect();
        let mut insert_index = 0;

        for (i, line) in lines.iter().enumerate() {
            if line.starts_with("pub mod ") {
                insert_index = i + 1;
            } else if !line.trim().is_empty() && !line.starts_with("pub mod ") {
                break;
            }
        }

        if !lib_content.contains(&format!("pub mod {};", args.parser_type)) {
            let mut new_lines = lines;
            new_lines.insert(insert_index, &new_module_line);
            let new_content = new_lines.join("\n");
            std::fs::write(&lib_rs_path, new_content)?;
        }

        println!(
            "Created new parser directory: {}",
            parser_type_dir.display()
        );
    }

    println!(
        "Parser generated successfully for {} in {}",
        program_name, args.parser_type
    );

    Ok(())
}

fn update_dooot_enum(
    dooot_names: &[String],
    program_name: &str,
    args: &ParserGenArgs,
) -> Result<()> {
    let dooot_mod_path = args.get_dooot_path()?.join("mod.rs");

    if !dooot_mod_path.exists() {
        return Err(anyhow::anyhow!(
            "Dooot mod.rs file not found at {:?}",
            dooot_mod_path
        ));
    }

    let content = std::fs::read_to_string(&dooot_mod_path)?;
    let lines: Vec<&str> = content.lines().collect();

    // Find the end of the Dooot enum (closing brace)
    let mut enum_end_line = None;
    let mut in_dooot_enum = false;

    for (i, line) in lines.iter().enumerate() {
        if line.contains("pub enum Dooot") {
            in_dooot_enum = true;
        } else if in_dooot_enum && line.trim() == "}" {
            enum_end_line = Some(i);
            break;
        }
    }

    let enum_end_line =
        enum_end_line.ok_or_else(|| anyhow::anyhow!("Could not find end of Dooot enum"))?;

    // Find the ToSql impl match block - look for the match statement inside ToSql impl
    let mut tosql_match_end = None;
    let mut in_tosql_impl = false;
    let mut in_match_block = false;
    let mut brace_depth = 0;

    for (i, line) in lines.iter().enumerate() {
        if line.contains("impl ToSql for Dooot") {
            in_tosql_impl = true;
        } else if in_tosql_impl && line.trim().starts_with("match self") {
            in_match_block = true;
            // Count the opening brace of the match
            if line.contains('{') {
                brace_depth = 1;
            }
        } else if in_match_block {
            // Count braces to find the end of the match block
            for ch in line.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => {
                        brace_depth -= 1;
                        if brace_depth == 0 {
                            tosql_match_end = Some(i);
                            in_match_block = false;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if !in_match_block {
                break;
            }
        }
    }

    let tosql_match_end = tosql_match_end
        .ok_or_else(|| anyhow::anyhow!("Could not find end of ToSql match block"))?;

    // Find the to_sql_checked impl match block end - look for the second match block
    let mut tosql_checked_match_end = None;
    let mut in_tosql_checked_impl = false;
    let mut in_match_block = false;
    let mut brace_depth = 0;

    for (i, line) in lines.iter().enumerate().skip(tosql_match_end + 1) {
        if line.contains("fn to_sql_checked") {
            in_tosql_checked_impl = true;
        } else if in_tosql_checked_impl && line.trim().starts_with("match self") {
            in_match_block = true;
            // Count the opening brace of the match
            if line.contains('{') {
                brace_depth = 1;
            }
        } else if in_match_block {
            // Count braces to find the end of the match block
            for ch in line.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => {
                        brace_depth -= 1;
                        if brace_depth == 0 {
                            tosql_checked_match_end = Some(i);
                            in_match_block = false;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if !in_match_block {
                break;
            }
        }
    }

    let tosql_checked_match_end = tosql_checked_match_end
        .ok_or_else(|| anyhow::anyhow!("Could not find end of to_sql_checked match block"))?;

    let mut new_variants = Vec::new();
    let mut new_match_arms = Vec::new();
    let mut new_checked_match_arms = Vec::new();

    for dooot_name in dooot_names {
        let account_name = dooot_name
            .strip_prefix(&format!("Ps{}", program_name))
            .and_then(|s| s.strip_suffix("Dooot"))
            .unwrap_or(dooot_name);

        let variant_name = format!("Ps{}{}", program_name, account_name);
        let variant_line = format!("    {}({}),", variant_name, dooot_name);

        let variant_exists = lines
            .iter()
            .any(|line| line.contains(&format!("{}(", variant_name)));

        if !variant_exists {
            new_variants.push(variant_line);

            // Create corresponding ToSql match arm following the existing pattern
            let match_arm = format!("            Dooot::{}(_s) => {{", variant_name);
            let error_msg = format!(
                "                Err(\"{}{} cannot write to postgres\".into())",
                program_name, account_name
            );
            let match_arm_close = "            }".to_string();

            new_match_arms.push(match_arm);
            new_match_arms.push(error_msg);
            new_match_arms.push(match_arm_close);

            // Create corresponding ToSqlChecked match arm following the existing pattern
            let checked_match_arm = format!("            Dooot::{}(_s) => {{", variant_name);
            let checked_error_msg = format!(
                "                Err(\"{}{} cannot write to postgres\".into())",
                program_name, account_name
            );
            let checked_match_arm_close = "            }".to_string();

            new_checked_match_arms.push(checked_match_arm);
            new_checked_match_arms.push(checked_error_msg);
            new_checked_match_arms.push(checked_match_arm_close);
        }
    }

    if new_variants.is_empty() {
        println!(
            "All Dooot variants already exist for {}, no updates needed",
            program_name
        );
        return Ok(());
    }

    // Build new content
    let mut new_lines = Vec::new();

    // Add lines up to enum end
    new_lines.extend_from_slice(&lines[..enum_end_line]);

    // Add new variants
    for variant in &new_variants {
        new_lines.push(variant.as_str());
    }

    // Add enum closing brace and lines up to ToSql match end
    new_lines.extend_from_slice(&lines[enum_end_line..tosql_match_end]);

    // Add new match arms
    for match_arm in &new_match_arms {
        new_lines.push(match_arm.as_str());
    }

    // Add remaining lines up to ToSqlChecked match end
    new_lines.extend_from_slice(&lines[tosql_match_end..tosql_checked_match_end]);

    // Add new checked match arms
    for match_arm in &new_checked_match_arms {
        new_lines.push(match_arm.as_str());
    }

    // Add remaining lines
    new_lines.extend_from_slice(&lines[tosql_checked_match_end..]);

    // Write back to file
    let new_content = new_lines.join("\n");
    std::fs::write(&dooot_mod_path, new_content)?;

    println!(
        "Updated Dooot enum with {} new variants for {}",
        new_variants.len(),
        program_name
    );

    Ok(())
}
