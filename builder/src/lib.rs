use proc_macro::{Span, TokenStream};
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Ident};

#[proc_macro_derive(Builder)]
pub fn derive(input: TokenStream) -> TokenStream {
    let _ = input;

    //eprintln!("Derive input is: {input:#?}");

    let derive_input = parse_macro_input!(input as DeriveInput);

    let struct_name = derive_input.ident;
    let builder = Ident::new(&format!("{struct_name}Builder"), Span::call_site().into());

    let quoted = quote!(
        pub struct #builder {
            executable: String,
            args: Vec<String>,
            env: Vec<String>,
            current_dir: String,
        }

        impl #struct_name {
            pub fn builder() -> #struct_name {
                #struct_name {
                    executable: String::new(),
                    args: vec!(),
                    env: vec!(),
                    current_dir: String::new(),
                }
            }
        }
    );

    //eprintln!("quoted is {quoted:#?}");

    TokenStream::from(quoted)
}
