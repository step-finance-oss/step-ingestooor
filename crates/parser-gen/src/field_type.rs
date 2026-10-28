#![allow(clippy::unwrap_used)]
use std::collections::HashMap;

use anyhow::{bail, Result};
use quote::quote;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdlKind {
    Struct,
    Enum,
}

#[derive(Debug, Clone)]
pub enum FieldType {
    U128,
    U64,
    U32,
    U16,
    U8,
    I128,
    I64,
    I32,
    I16,
    I8,
    F64,
    F32,
    String,
    Pubkey,
    Bool,

    // Strings will be names of the other types
    Struct(String),
    Enum(String),
    Array(Box<FieldType>, usize),
    Vec(Box<FieldType>),
    Option(Box<FieldType>),
}

#[derive(Debug)]
pub struct TypeDef {
    pub name: String,
    pub field_type: FieldType,
}

impl TypeDef {
    pub fn create_type_def(name: String, field_type: &Value) -> Result<Self> {
        let parsed_type = parse_field_type_recursive(field_type, None)?;
        Ok(Self {
            name,
            field_type: parsed_type,
        })
    }
}

#[derive(Debug)]
pub enum TypeKind {
    Struct(Vec<TypeDef>),
    Enum(EnumDef),
    Alias(FieldType),
}

#[derive(Debug)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<String>,
}

impl FieldType {
    pub fn from_primitive_str(type_str: &str) -> Option<Self> {
        match type_str {
            "u128" => Some(FieldType::U128),
            "u64" => Some(FieldType::U64),
            "u32" => Some(FieldType::U32),
            "u16" => Some(FieldType::U16),
            "u8" => Some(FieldType::U8),
            "i128" => Some(FieldType::I128),
            "i64" => Some(FieldType::I64),
            "i32" => Some(FieldType::I32),
            "i16" => Some(FieldType::I16),
            "i8" => Some(FieldType::I8),
            "f64" => Some(FieldType::F64),
            "f32" => Some(FieldType::F32),
            "string" => Some(FieldType::String),
            "bool" => Some(FieldType::Bool),
            "pubkey" => Some(FieldType::Pubkey),
            _ => None,
        }
    }

    // to rust dooot representation
    pub fn to_dooot_representation(&self) -> String {
        match self {
            FieldType::U128 => "String".to_string(),
            FieldType::U64 => "u64".to_string(),
            FieldType::U32 => "u32".to_string(),
            FieldType::U16 => "u16".to_string(),
            FieldType::U8 => "u8".to_string(),
            FieldType::I128 => "String".to_string(),
            FieldType::I64 => "i64".to_string(),
            FieldType::I32 => "i32".to_string(),
            FieldType::I16 => "i16".to_string(),
            FieldType::I8 => "i8".to_string(),
            FieldType::F64 => "f64".to_string(),
            FieldType::F32 => "f32".to_string(),
            FieldType::String => "String".to_string(),
            FieldType::Bool => "bool".to_string(),
            FieldType::Pubkey => "String".to_string(),
            FieldType::Struct(name) => name.clone(),
            // enums to string for clickhouse
            FieldType::Enum(_) => "String".to_string(),
            FieldType::Array(inner, _size) => {
                if let FieldType::U8 = **inner {
                    return "String".to_string();
                }
                // ClickHouse Array(...) types expect dynamic arrays with length prefixes in binary format:
                // [length][element1][element2]...[elementN]
                //
                // But Rust's fixed arrays [T; N] serialize as raw consecutive elements without length prefixes:
                // [element1][element2]...[elementN]
                //
                // This causes ClickHouse to misinterpret the first few bytes of element1 as the array length,
                // leading to massive "bytes expected" errors (e.g., 268MB when it reads random data as length).
                //
                // Solution: Use Vec<T> which serializes with proper length prefixes that ClickHouse expects.
                format!("Vec<{}>", inner.to_dooot_representation())
            }
            //TODO we need to handle vec type sizes. The size of the vec is unknown when we find it here.
            // Its known in the definition of the type, but not here.
            FieldType::Vec(inner) => format!("Vec<{}>", inner.to_dooot_representation()),
            FieldType::Option(inner) => format!("Option<{}>", inner.to_dooot_representation()),
        }
    }
    // Incoming tables for clickhouse
    pub fn to_incoming_table_representation(
        &self,
        type_defs: Option<&HashMap<String, TypeKind>>,
    ) -> String {
        match self {
            // we store u128 as string for clickhouse incoming
            FieldType::U128 => "String".to_string(),
            FieldType::U64 => "UInt64".to_string(),
            FieldType::U32 => "UInt32".to_string(),
            FieldType::U16 => "UInt16".to_string(),
            FieldType::U8 => "UInt8".to_string(),
            FieldType::I128 => "String".to_string(),
            FieldType::I64 => "Int64".to_string(),
            FieldType::I32 => "Int32".to_string(),
            FieldType::I16 => "Int16".to_string(),
            FieldType::I8 => "Int8".to_string(),
            FieldType::F64 => "Float64".to_string(),
            FieldType::F32 => "Float32".to_string(),
            FieldType::String => "String".to_string(),
            FieldType::Bool => "Bool".to_string(),
            FieldType::Pubkey => "String".to_string(),
            FieldType::Struct(name) => {
                if let Some(type_defs) = type_defs {
                    if let Some(TypeKind::Struct(fields)) = type_defs.get(name) {
                        let tuple_fields: Vec<String> = fields
                            .iter()
                            .map(|field| {
                                format!(
                                    "{} {}",
                                    field.name,
                                    field
                                        .field_type
                                        .to_incoming_table_representation(Some(type_defs))
                                )
                            })
                            .collect();

                        return format!("Tuple({})", tuple_fields.join(", "));
                    }
                }
                panic!("Type definition not found for {}", name);
            }
            // enums to string for clickhouse
            FieldType::Enum(_) => "String".to_string(),
            FieldType::Array(inner, _size) => {
                if let FieldType::U8 = **inner {
                    return "String".to_string();
                }
                format!(
                    "Array({})",
                    inner.to_incoming_table_representation(type_defs)
                )
            }
            FieldType::Vec(inner) => {
                format!(
                    "Array({})",
                    inner.to_incoming_table_representation(type_defs)
                )
            }
            FieldType::Option(inner) => {
                format!(
                    "Nullable({})",
                    inner.to_incoming_table_representation(type_defs)
                )
            }
        }
    }

    // Time tables for clickhouse (similar to incoming but with FixedString for pubkeys)
    pub fn to_time_table_representation(
        &self,
        type_defs: Option<&HashMap<String, TypeKind>>,
    ) -> String {
        match self {
            FieldType::U128 => "UInt128".to_string(),
            FieldType::U64 => "UInt64".to_string(),
            FieldType::U32 => "UInt32".to_string(),
            FieldType::U16 => "UInt16".to_string(),
            FieldType::U8 => "UInt8".to_string(),
            FieldType::I128 => "Int128".to_string(),
            FieldType::I64 => "Int64".to_string(),
            FieldType::I32 => "Int32".to_string(),
            FieldType::I16 => "Int16".to_string(),
            FieldType::I8 => "Int8".to_string(),
            FieldType::F64 => "Float64".to_string(),
            FieldType::F32 => "Float32".to_string(),
            FieldType::String => "String".to_string(),
            FieldType::Bool => "Bool".to_string(),
            FieldType::Pubkey => "FixedString(32)".to_string(),
            FieldType::Struct(name) => {
                if let Some(type_defs) = type_defs {
                    if let Some(TypeKind::Struct(fields)) = type_defs.get(name) {
                        let tuple_fields: Vec<String> = fields
                            .iter()
                            .map(|field| {
                                format!(
                                    "{} {}",
                                    field.name,
                                    field
                                        .field_type
                                        .to_time_table_representation(Some(type_defs))
                                )
                            })
                            .collect();

                        return format!("Tuple({})", tuple_fields.join(", "));
                    }
                }
                // Fallback to String if type definition not found
                "String".to_string()
            }
            // enums to string for clickhouse
            FieldType::Enum(_) => "LowCardinality(String)".to_string(),
            FieldType::Array(inner, size) => {
                if let FieldType::U8 = **inner {
                    return format!("FixedString({})", size);
                }
                format!("Array({})", inner.to_time_table_representation(type_defs))
            }
            FieldType::Vec(inner) => {
                format!("Array({})", inner.to_time_table_representation(type_defs))
            }
            FieldType::Option(inner) => {
                format!(
                    "Nullable({})",
                    inner.to_time_table_representation(type_defs)
                )
            }
        }
    }

    // Generate materialized view field conversion for time tables (top-level with AS aliases)
    pub fn to_materialized_view_conversion(
        &self,
        field_name: &str,
        type_defs: Option<&HashMap<String, TypeKind>>,
    ) -> String {
        self.to_materialized_view_conversion_depth(field_name, type_defs, 0)
    }

    fn to_materialized_view_conversion_depth(
        &self,
        field_name: &str,
        type_defs: Option<&HashMap<String, TypeKind>>,
        depth: usize,
    ) -> String {
        match self {
            // Numeric types - direct conversion
            FieldType::U128
            | FieldType::U64
            | FieldType::U32
            | FieldType::U16
            | FieldType::U8
            | FieldType::I128
            | FieldType::I64
            | FieldType::I32
            | FieldType::I16
            | FieldType::I8
            | FieldType::F64
            | FieldType::F32
            | FieldType::Bool
            | FieldType::String => field_name.to_string(),
            FieldType::Pubkey => {
                if depth == 0 {
                    format!("base58Decode({}) as {}", field_name, field_name)
                } else {
                    format!("base58Decode({})", field_name)
                }
            }
            FieldType::Enum(_) => field_name.to_string(),
            FieldType::Array(inner, _size) => {
                self.process_collection_conversion(inner, field_name, type_defs, depth)
            }
            FieldType::Vec(inner) => {
                self.process_collection_conversion(inner, field_name, type_defs, depth)
            }
            FieldType::Option(_) => field_name.to_string(),
            _ => field_name.to_string(),
        }
    }

    // Helper function to handle both Array and Vec conversion logic
    fn process_collection_conversion(
        &self,
        inner: &FieldType,
        field_name: &str,
        type_defs: Option<&HashMap<String, TypeKind>>,
        depth: usize,
    ) -> String {
        match inner {
            FieldType::U8 => {
                if depth == 0 {
                    format!("base64Decode({}) as {}", field_name, field_name)
                } else {
                    format!("base64Decode({})", field_name)
                }
            }
            FieldType::Struct(struct_name) => {
                if let Some(type_defs) = type_defs {
                    if let Some(TypeKind::Struct(fields)) = type_defs.get(struct_name) {
                        let conversions: Vec<String> = fields
                            .iter()
                            .map(|f| {
                                let inner_expr = format!("x.{}", f.name);
                                f.field_type.to_materialized_view_conversion_depth(
                                    &inner_expr,
                                    Some(type_defs),
                                    depth + 2,
                                )
                            })
                            .collect();

                        let map_lambda = format!("x -> tuple({})", conversions.join(", "));
                        let array_map = format!("arrayMap({}, {})", map_lambda, field_name);
                        if depth == 0 {
                            format!("{} as {}", array_map, field_name)
                        } else {
                            array_map
                        }
                    } else {
                        field_name.to_string()
                    }
                } else {
                    field_name.to_string()
                }
            }
            _ => field_name.to_string(),
        }
    }

    pub fn generate_type_token(&self) -> proc_macro2::TokenStream {
        match self {
            FieldType::U8 => quote! { u8 },
            FieldType::U16 => quote! { u16 },
            FieldType::U32 => quote! { u32 },
            FieldType::U64 => quote! { u64 },
            FieldType::U128 => quote! { u128 },
            FieldType::I8 => quote! { i8 },
            FieldType::I16 => quote! { i16 },
            FieldType::I32 => quote! { i32 },
            FieldType::I64 => quote! { i64 },
            FieldType::I128 => quote! { i128 },
            FieldType::F32 => quote! { f32 },
            FieldType::F64 => quote! { f64 },
            FieldType::Bool => quote! { bool },
            FieldType::String => quote! { String },
            FieldType::Pubkey => quote! { String }, // We convert pubkeys to strings
            FieldType::Struct(name) => {
                let struct_ident = syn::parse_str::<syn::Ident>(name).unwrap();
                quote! { #struct_ident }
            }
            FieldType::Enum(name) => {
                let enum_ident = syn::parse_str::<syn::Ident>(name).unwrap();
                quote! { #enum_ident }
            }
            FieldType::Array(inner_type, size) => {
                let inner_token = inner_type.generate_type_token();
                // Maybe? need to convert arrays to vec for large arrays with size > 32 cause
                // Serde doesn't support arrays > 32
                if *size > 32 {
                    quote! { Vec<#inner_token> }
                } else {
                    quote! { [#inner_token; #size] }
                }
            }
            FieldType::Vec(inner_type) => {
                let inner_token = inner_type.generate_type_token();
                quote! { Vec<#inner_token> }
            }
            FieldType::Option(inner_type) => {
                let inner_token = inner_type.generate_type_token();
                quote! { Option<#inner_token> }
            }
        }
    }

    // Helper function to generate field reading code based on field type
    pub fn generate_field_read_code(
        &self,
        type_defs: Option<&std::collections::HashMap<String, TypeKind>>,
    ) -> proc_macro2::TokenStream {
        match self {
            FieldType::Pubkey => quote! {
                reader.read_next_pubkey()?.to_string()
            },
            FieldType::String => quote! {
                reader.read_next_borsh_type::<String>()?.replace('\0', "")
            },
            FieldType::U8 => quote! {
                reader.read_next_single_byte()?
            },
            FieldType::U16 => quote! {
                reader.read_next_le_u16()?
            },
            FieldType::U32 => quote! {
                reader.read_next_le_u32()?
            },
            FieldType::U64 => quote! {
                reader.read_next_le_u64()?
            },
            FieldType::U128 => quote! {
                reader.read_next_le_u128()?.to_string()
            },
            FieldType::I8 => quote! {
                reader.read_next_le_i8()?
            },
            FieldType::I16 => quote! {
                reader.read_next_le_i16()?
            },
            FieldType::I32 => quote! {
                reader.read_next_le_i32()?
            },
            FieldType::I64 => quote! {
                reader.read_next_le_i64()?
            },
            FieldType::I128 => quote! {
                reader.read_next_le_i128()?.to_string()
            },
            FieldType::Bool => quote! {
                reader.read_next_bool()?
            },
            FieldType::F32 => quote! {
                reader.read_next_le_f32()?
            },
            FieldType::F64 => quote! {
                reader.read_next_le_f64()?
            },
            FieldType::Struct(struct_name) => {
                if let Some(type_defs) = type_defs {
                    // Check if this "struct" is actually an enum
                    if let Some(TypeKind::Enum(_)) = type_defs.get(struct_name) {
                        let enum_ident = syn::parse_str::<syn::Ident>(struct_name).unwrap();
                        quote! {
                            reader.read_next_borsh_type::<#enum_ident>()?.to_string()
                        }
                    } else if let Some(TypeKind::Struct(fields)) = type_defs.get(struct_name) {
                        let struct_ident = syn::parse_str::<syn::Ident>(struct_name).unwrap();
                        let field_assignments = fields
                            .iter()
                            .map(|field| {
                                let field_name = syn::parse_str::<syn::Ident>(&field.name).unwrap();
                                let field_read =
                                    field.field_type.generate_field_read_code(Some(type_defs));
                                quote! { #field_name: #field_read }
                            })
                            .collect::<Vec<_>>();

                        quote! {
                            #struct_ident {
                                #(#field_assignments,)*
                            }
                        }
                    } else {
                        panic!("Unknown struct: {}", struct_name);
                    }
                } else {
                    panic!("No type definitions available for struct: {}", struct_name);
                }
            }
            FieldType::Vec(inner_type) => {
                match inner_type.as_ref() {
                    FieldType::Struct(struct_name) => {
                        if let Some(type_defs) = type_defs {
                            if let Some(TypeKind::Struct(fields)) = type_defs.get(struct_name) {
                                let struct_ident =
                                    syn::parse_str::<syn::Ident>(struct_name).unwrap();
                                let field_assignments = fields
                                    .iter()
                                    .map(|field| {
                                        let field_name =
                                            syn::parse_str::<syn::Ident>(&field.name).unwrap();
                                        let field_read = field
                                            .field_type
                                            .generate_field_read_code(Some(type_defs));
                                        quote! { #field_name: #field_read }
                                    })
                                    .collect::<Vec<_>>();

                                quote! {
                                    {
                                        let vec_len = reader.read_next_le_u32()? as usize;
                                        let mut vec_result: Vec<#struct_ident> = Vec::with_capacity(vec_len);
                                        for _i in 0..vec_len {
                                            vec_result.push(#struct_ident {
                                                #(#field_assignments,)*
                                            });
                                        }
                                        vec_result
                                    }
                                }
                            } else {
                                quote! {
                                    {
                                        let vec_len = reader.read_next_le_u32()? as usize;
                                        reader.skip_bytes(vec_len * std::mem::size_of::<u64>())?;
                                        Vec::new()
                                    }
                                }
                            }
                        } else {
                            quote! {
                                {
                                    let vec_len = reader.read_next_le_u32()? as usize;
                                    reader.skip_bytes(vec_len * std::mem::size_of::<u64>())?;
                                    Vec::new()
                                }
                            }
                        }
                    }
                    _ => {
                        // For primitive or other types, use borsh
                        let inner_token = inner_type.generate_type_token();
                        quote! {
                            reader.read_next_borsh_type::<Vec<#inner_token>>()?
                        }
                    }
                }
            }
            FieldType::Array(inner_type, size) => {
                match inner_type.as_ref() {
                    FieldType::Struct(struct_name) => {
                        if let Some(type_defs) = type_defs {
                            if let Some(TypeKind::Struct(fields)) = type_defs.get(struct_name) {
                                let struct_ident =
                                    syn::parse_str::<syn::Ident>(struct_name).unwrap();
                                let field_assignments = fields
                                    .iter()
                                    .map(|field| {
                                        let field_name =
                                            syn::parse_str::<syn::Ident>(&field.name).unwrap();
                                        let field_read = field
                                            .field_type
                                            .generate_field_read_code(Some(type_defs));
                                        quote! { #field_name: #field_read }
                                    })
                                    .collect::<Vec<_>>();

                                quote! {
                                    {
                                        let mut array_result: Vec<#struct_ident> = Vec::with_capacity(#size as usize);
                                        for _i in 0..#size {
                                            array_result.push(#struct_ident {
                                                #(#field_assignments,)*
                                            });
                                        }
                                        array_result
                                    }
                                }
                            } else {
                                quote! {
                                    {
                                        reader.skip_bytes((#size as usize) * std::mem::size_of::<u64>())?;
                                        Default::default()
                                    }
                                }
                            }
                        } else {
                            quote! {
                                {
                                    reader.skip_bytes((#size as usize) * std::mem::size_of::<u64>())?;
                                    Default::default()
                                }
                            }
                        }
                    }
                    _ => {
                        let inner_token = inner_type.generate_type_token();
                        // if inner_token is u8 we can just use string
                        if inner_token.to_string() == "u8" {
                            quote! {
                                reader.read_next_bytes_to_base_64(#size)?
                            }
                        } else if inner_token.to_string() == "u128" {
                            let size_usize = { *size };
                            let error_message = format!("Expected exactly {} u128 entries", size);
                            quote! {
                                {
                                    let mut result = Vec::with_capacity(#size_usize);
                                    for _ in 0..#size_usize {
                                        let raw = reader.read_next_le_u128()?;
                                        result.push(raw.to_string());
                                    }
                                    result.try_into().map_err(|_| {
                                        anyhow::anyhow!(#error_message)
                                    })?
                                }
                            }
                        } else {
                            quote! {
                                reader.read_next_borsh_type::<Vec<#inner_token>>()?
                            }
                        }
                    }
                }
            }
            FieldType::Option(inner_type) => {
                match inner_type.as_ref() {
                    FieldType::Struct(struct_name) => {
                        if let Some(type_defs) = type_defs {
                            if let Some(TypeKind::Struct(fields)) = type_defs.get(struct_name) {
                                let struct_ident =
                                    syn::parse_str::<syn::Ident>(struct_name).unwrap();
                                let field_assignments = fields
                                    .iter()
                                    .map(|field| {
                                        let field_name =
                                            syn::parse_str::<syn::Ident>(&field.name).unwrap();
                                        let field_read = field
                                            .field_type
                                            .generate_field_read_code(Some(type_defs));
                                        quote! { #field_name: #field_read }
                                    })
                                    .collect::<Vec<_>>();

                                quote! {
                                    {
                                        Some(#struct_ident {
                                            #(#field_assignments,)*
                                        })
                                    }
                                }
                            } else {
                                quote! {
                                    {
                                        let is_some = reader.read_next_single_byte()? != 0;
                                        if is_some {
                                            reader.skip_bytes(std::mem::size_of::<u64>())?;
                                        }
                                        None
                                    }
                                }
                            }
                        } else {
                            quote! {
                                {
                                    let is_some = reader.read_next_single_byte()? != 0;
                                    if is_some {
                                        reader.skip_bytes(std::mem::size_of::<u64>())?;
                                    }
                                    None
                                }
                            }
                        }
                    }
                    _ => {
                        // For primitive options, use borsh
                        let inner_token = inner_type.generate_type_token();
                        quote! {
                            reader.read_next_borsh_type::<Option<#inner_token>>()?
                        }
                    }
                }
            }
            FieldType::Enum(enum_name) => {
                let enum_ident = syn::parse_str::<syn::Ident>(enum_name).unwrap();
                quote! {
                    #enum_ident::from(reader.read_next_single_byte()?).to_string()
                }
            }
        }
    }
}

pub fn parse_field_type_recursive(
    field_type: &Value,
    type_defs: Option<&HashMap<String, TypeKind>>,
) -> Result<FieldType> {
    // primitive types. ie "u64", "string", "bool", etc
    if field_type.is_string() {
        let type_str = field_type.as_str().unwrap();
        if let Some(primitive) = FieldType::from_primitive_str(type_str) {
            return Ok(primitive);
        } else {
            return Err(anyhow::anyhow!("Unknown primitive type: {}", type_str));
        }
    }

    if field_type.is_object() {
        // array types (including nested arrays)
        if let Some(array) = field_type.get("array") {
            let array = array.as_array().unwrap();
            let inner_type = parse_field_type_recursive(&array[0], type_defs)?;
            let size = array[1].as_u64().unwrap() as usize;
            return Ok(FieldType::Array(Box::new(inner_type), size));
        }

        if let Some(vec_type) = field_type.get("vec") {
            let inner_type = parse_field_type_recursive(vec_type, type_defs)?;
            return Ok(FieldType::Vec(Box::new(inner_type)));
        }

        if let Some(option_type) = field_type.get("option") {
            let inner_type = parse_field_type_recursive(option_type, type_defs)?;
            return Ok(FieldType::Option(Box::new(inner_type)));
        }

        if let Some(defined) = field_type.get("defined") {
            let defined_name = defined.get("name").unwrap().as_str().unwrap().to_string();

            // Check if we have type definitions to determine if this is a struct or enum
            if let Some(type_defs) = type_defs {
                if let Some(type_kind) = type_defs.get(&defined_name) {
                    match type_kind {
                        TypeKind::Struct(_) => return Ok(FieldType::Struct(defined_name)),
                        TypeKind::Enum(_) => return Ok(FieldType::Enum(defined_name)),
                        TypeKind::Alias(alias_type) => return Ok(alias_type.clone()),
                    }
                }
            }

            // Default to struct if we can't determine the type
            // return Ok(FieldType::Struct(defined_name));
            bail!("Unknown field type: {:#?}", field_type);
        }
    }

    Err(anyhow::anyhow!("Unknown field type: {:#?}", field_type))
}
