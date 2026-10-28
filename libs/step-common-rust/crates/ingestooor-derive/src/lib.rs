#![allow(clippy::unwrap_used)]

use proc_macro::TokenStream;
use quote::{quote, quote_spanned, ToTokens};
use syn::{parse_macro_input, spanned::Spanned, DeriveInput, LitStr};

/// This macro is used to derive the Ingestooor DoootTrait trait for a struct
/// The attribute `#[dooot(name = "name", proc = "proc_name", topic)]` on the struct is used to specify the dooot name and proc name.
/// ommision of the proc name will result in no database writing. The topic attribute will include the dooot name as a topic
/// The attribute `#[topic]` on a field is used to specify that a field should be used as a topic.
/// ex:
/// ```rust,ignore
/// #[derive(Dooot)]
/// #[dooot(name = "my_dooot", proc = "my_proc")]
/// struct MyStruct {
///   #[topic]
///   field1: String,
///   #[topic]
///   field2: i64,
///   field3: f64,
/// }
/// ```
/// This will cause the struct to write to the database through the proc `my_proc`.
/// The dooot name will not be written to the database, nor will it be included as a topic value.
/// The topics will be `my_dooot.field1.<value>` and `my_dooot.field2.<value>`.
///
/// The supported types for topics are `String`, `i64`, `u64`, `f64`, and `bool`.
/// Optional fields are supported, and will be written as `null` if they are not present.
///
/// Topics are used by broadcastooor. You can subscribe to a topic and receive messages that
/// match the topic. Thus, the topic is used to filter messages.
#[proc_macro_derive(Dooot, attributes(dooot, topic))]
pub fn derive_dooot(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = &input.ident;

    //get the dooot and proc name
    let mut dooot_name: Option<LitStr> = None;
    let mut include_dooot_as_topic = false;
    for attr in input.attrs {
        if attr.path().is_ident("dooot") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("name") {
                    let value = meta.value()?;
                    let s: LitStr = value.parse().expect("cannot parse name");
                    dooot_name = Some(s);
                    Ok(())
                } else if meta.path.is_ident("topic") {
                    include_dooot_as_topic = true;
                    Ok(())
                } else {
                    Err(meta.error("unrecognized dooot"))
                }
            })
            .expect("cannot parse dooot");
        }
    }
    let dooot_name = dooot_name.expect("dooot name not found");

    //get all the fields of the struct
    let fields = match input.data {
        syn::Data::Struct(ref data) => match data.fields {
            syn::Fields::Named(ref fields) => &fields.named,
            _ => unimplemented!(),
        },
        _ => unimplemented!(),
    };

    //get all the fields with the topic attribute and create a topic string
    let topics = fields.iter().filter_map(|field| {
        if field.attrs.iter().any(|attr| attr.path().is_ident("topic")) {
            let field_name = field.ident.as_ref().unwrap();
            let topic_prefix = format!("{}.{}.", dooot_name.value(), field_name);
            //if its an option, unwrap or use null
            if field.ty.to_token_stream().into_iter().next().unwrap().to_string() == "Option" {
                let ret =
                    quote_spanned!(field.span()=>
                        #topic_prefix.to_string() + &self.#field_name.as_ref().unwrap_or(&"null".to_string())
                    );
                Some(
                    ret
                )
            } else {
                let ret =
                    quote_spanned!(field.span()=>
                        #topic_prefix.to_string() + &self.#field_name.to_string()
                    );
                Some(
                    ret
                )
            }
        } else {
            None
        }
    //chain the dooot name as a topic if the attribute is present
    }).chain(if include_dooot_as_topic {
        Some(quote_spanned! {dooot_name.span()=>
            #dooot_name.to_string()
        })
    } else {
        None
    });

    //get all the fields that are supported types and add them to the context
    let get_expr_context_innards = fields.iter().filter_map(|field|{
        if let syn::Type::Path(_tp) = &field.ty {
            // let segment = &tp.path.segments.first().unwrap();
            let field_name = field.ident.as_ref().unwrap();
            let val_assignment = quote_spanned! { field.span() =>
                let val: evalexpr::Value = crate::step_common_rust::evalexpr::ValueWrapper::from(&self.#field_name).unwrap();
                evalexpr::ContextWithMutableVariables::set_value(
                    &mut context,
                    stringify!(#field_name).to_string(),
                    val
                ).map_err(|e| {
                    log::error!("Failed to set value for {} cause: {}", stringify!(#field_name), e);
                    e
                }).ok();
            };

            Some(val_assignment)
        } else {
            None
        }
    });

    let expanded = quote! {
        impl super::DoootTrait for #name {
            fn get_dooot_name(&self) -> &'static str {
                #dooot_name
            }
            fn get_topics(&self) -> Vec<String> {
                vec![
                    #(#topics),*
                ]
            }
            fn get_expr_context(&self) -> evalexpr::HashMapContext {
                let mut context = evalexpr::HashMapContext::new();

                crate::step_common_rust::evalexpr::add_common_functions(&mut context).unwrap();

                #(#get_expr_context_innards)
                *
                context
            }
        }
    };

    //eprintln!("expanded: {}", expanded);

    // Hand the output tokens back to the compiler
    TokenStream::from(expanded)
}
