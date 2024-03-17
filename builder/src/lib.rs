use proc_macro::{Span, TokenStream};
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Ident};

#[proc_macro_derive(Builder)]
pub fn derive(input: TokenStream) -> TokenStream {
    let mut res = TokenStream::new();

    //eprintln!("Derive input is: {input:#?}");

    let derive_input = parse_macro_input!(input as DeriveInput);

    let struct_name = derive_input.ident;
    let builder = Ident::new(&format!("{struct_name}Builder"), Span::call_site().into());

    // builder struct and struct.builder fn
    let quoted_builder = quote!(
        pub struct #builder {
            executable: String,
            args: Vec<String>,
            env: Vec<String>,
            current_dir: String,
        }

        impl #struct_name {
            pub fn builder() -> #builder {
                #builder {
                    executable: String::new(),
                    args: vec!(),
                    env: vec!(),
                    current_dir: String::new(),
                }
            }
        }
    );
    res.extend(TokenStream::from(quoted_builder));

    match derive_input.data {
        syn::Data::Struct(data) => match data.fields {
            syn::Fields::Named(ref fields) => {
                for f in &fields.named {
                    let name = &f.ident;
                    let typ = &f.ty;
                    // setter for each field
                    let q = quote!(
                        impl #builder {
                            pub fn #name (&mut self, #name : #typ) -> &mut Self {
                                self.#name = #name;
                                self
                            }
                        }
                    );
                    res.extend(TokenStream::from(q));
                }
            }
            syn::Fields::Unnamed(_) | syn::Fields::Unit => unimplemented!(),
        },
        syn::Data::Enum(_) | syn::Data::Union(_) => unimplemented!(),
    }

    // builder.build()
    let q = quote!(
        impl #builder {
            pub fn build(self) -> Result<#struct_name, Box<dyn std::error::Error>> {
                let s = #struct_name {
                    executable: self.executable,
                    args: self.args,
                    env: self.env,
                    current_dir: self.current_dir,
                };
                Ok(s)
            }
        }
    );

    res.extend(TokenStream::from(q));

    res
}
