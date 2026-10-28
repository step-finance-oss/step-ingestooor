use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, LitInt, LitStr};

#[proc_macro_derive(ClickhouseDooot, attributes(clickhouse))]
pub fn derive_ch_dooot(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let mut table_name: Option<LitStr> = None;
    let mut queue_name: Option<LitStr> = None;
    let mut flush_ms: Option<LitInt> = None;
    let mut inserter_max_rows: Option<LitInt> = None;

    for attr in &input.attrs {
        if attr.path().is_ident("clickhouse") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("table") {
                    let value = meta.value()?;
                    let s: LitStr = value.parse().expect("cannot parse table");
                    table_name = Some(s);
                    Ok(())
                } else if meta.path.is_ident("queue") {
                    let value = meta.value()?;
                    let s: LitStr = value.parse().expect("cannot parse queue");
                    queue_name = Some(s);
                    Ok(())
                } else if meta.path.is_ident("flush_ms") {
                    let value = meta.value()?;
                    let s: LitInt = value.parse().expect("cannot parse flush_ms");
                    flush_ms = Some(s);
                    Ok(())
                } else if meta.path.is_ident("inserter_max_rows") {
                    let value = meta.value()?;
                    let s: LitInt = value.parse().expect("cannot parse inserter_max_rows");
                    inserter_max_rows = Some(s);
                    Ok(())
                } else {
                    Err(meta.error(format!("unrecognized clickhouse, got: {:?}", meta.path)))
                }
            })
            .expect("cannot parse clickhouse");
        }
    }

    let expanded_table_name = match table_name {
        Some(s) => quote! {
            Some(#s)
        },
        None => quote! {
            None
        },
    };

    let expaneded_queue_name = match queue_name {
        Some(s) => quote! {
            Some(#s)
        },
        None => quote! {
            None
        },
    };

    let flush_ms = match flush_ms {
        Some(s) => quote! {
            #s
        },
        None => quote! {
            5000
        },
    };

    let inserter_max_rows = match inserter_max_rows {
        Some(s) => quote! {
            #s
        },
        None => quote! {
            5000
        },
    };

    let expanded = quote! {
        impl super::ClickhouseDoootTrait for #name {
            fn get_table_name(&self) -> Option<&'static str> {
                #expanded_table_name
            }
            fn get_queue_name(&self) -> Option<&'static str> {
                #expaneded_queue_name
            }

            fn get_flush_ms(&self) -> u64 {
                #flush_ms
            }

            fn get_inserter_max_rows(&self) -> u64 {
                #inserter_max_rows
            }
        }
    };

    TokenStream::from(expanded)
}
