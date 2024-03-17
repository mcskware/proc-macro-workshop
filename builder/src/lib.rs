use proc_macro::{Span, TokenStream};
use quote::quote;

use syn::{parse_macro_input, DeriveInput, Ident};

#[allow(clippy::missing_panics_doc, clippy::too_many_lines)]
#[proc_macro_derive(Builder)]
pub fn derive(input: TokenStream) -> TokenStream {
    let mut res = TokenStream::new();

    //eprintln!("Derive input is: {input:#?}");

    let derive_input = parse_macro_input!(input as DeriveInput);

    let struct_name = derive_input.ident;
    let builder = Ident::new(&format!("{struct_name}Builder"), Span::call_site().into());

    // build a vec of quotes for each arg and its type
    let mut field_definitions = quote!();
    let mut field_initializations = quote!();
    let mut build_initializations = quote!();
    #[allow(clippy::single_match)]
    match &derive_input.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(fields) => {
                for f in &fields.named {
                    let ident = &f.ident;
                    let ty = &f.ty;
                    field_definitions.extend(quote!(
                        #ident : Option<#ty>,
                    ));
                    field_initializations.extend(quote!(
                        #ident : None,
                    ));
                }
            }
            syn::Fields::Unnamed(_) | syn::Fields::Unit => unimplemented!(),
        },
        _ => (),
    }

    res.extend(TokenStream::from(quote!(
        pub struct #builder {
            #field_definitions
        }
    )));

    res.extend(TokenStream::from(quote!(
        impl #struct_name {
            pub fn builder() -> #builder {
                #builder {
                    #field_initializations
                }
            }
        }
    )));

    match &derive_input.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(ref fields) => {
                for f in &fields.named {
                    let name = &f.ident;
                    let typ = &f.ty;
                    let mut was_option = false;
                    if let syn::Type::Path(path) = typ {
                        #[allow(clippy::collapsible_if)]
                        if path.qself.is_none() {
                            // only one thing inside the Option (Option takes a single generic argument)
                            if path.path.segments.len() == 1 {
                                let segment = path.path.segments.first().unwrap();
                                let ident = &segment.ident;
                                // are we an Option?
                                if ident == &Ident::new("Option", Span::call_site().into()) {
                                    if let syn::PathArguments::AngleBracketed(args) =
                                        &segment.arguments
                                    {
                                        was_option = true;
                                        let a = args.args.first().unwrap();
                                        match a {
                                            syn::GenericArgument::Type(t) => {
                                                // we have Option<Foo> where t = Foo
                                                let q = quote!(
                                                    impl #builder {
                                                        pub fn #name (&mut self, #name : #t) -> &mut Self {
                                                            self.#name = Some(Some(#name));
                                                            self
                                                        }
                                                    }
                                                );
                                                res.extend(TokenStream::from(q));
                                                build_initializations.extend(quote!(
                                                    #name: if self.#name.is_some() {
                                                        self.#name.take().unwrap()
                                                    } else {
                                                        None
                                                    },
                                                ));
                                            }
                                            _ => unimplemented!(),
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if !was_option {
                        // setter for each field
                        let q = quote!(
                            impl #builder {
                                pub fn #name (&mut self, #name : #typ) -> &mut Self {
                                    self.#name = Some(#name);
                                    self
                                }
                            }
                        );
                        res.extend(TokenStream::from(q));
                        build_initializations.extend(quote!(
                            #name: self.#name.take().unwrap(),
                        ));
                    }
                }
            }
            syn::Fields::Unnamed(_) | syn::Fields::Unit => unimplemented!(),
        },
        syn::Data::Enum(_) | syn::Data::Union(_) => unimplemented!(),
    }

    // builder.build()
    // builder.current_dir = Option<Option<Foo>>
    // so it could be:
    // - None
    // - Some(None)
    // - Some(Some(foo))
    let q = quote!(
        impl #builder {
            pub fn build(&mut self) -> Result<#struct_name, Box<dyn std::error::Error>> {
                let s = #struct_name {
                    #build_initializations
                };
                Ok(s)
            }
        }
    );

    res.extend(TokenStream::from(q));

    res
}
