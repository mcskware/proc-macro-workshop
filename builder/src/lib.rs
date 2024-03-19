use indoc::indoc;
use proc_macro::{Span, TokenStream};
use quote::quote;

use syn::parse::Parse;
use syn::Expr;
use syn::Field;
use syn::Lit;
use syn::Meta;
use syn::MetaNameValue;
use syn::Type;
use syn::{parse_macro_input, DeriveInput, Ident};

struct AnnotatedField {
    /// The field name
    name: Ident,
    /// The field type, like `u8` or `Option<String>`
    ty: Type,
    /// Is this field an `Option` field?
    is_optional: bool,
    /// Optional name of a one-by-one setter function, declared via:
    /// ```rust
    /// # use derive_builder::Builder;
    /// # #[derive(Builder)]
    /// # struct Foo {
    ///     #[builder(each = "my_field_setter")]
    ///     my_field: Vec<String>,
    /// # }
    /// ```
    /// In this case, a setter function named `my_field_setter` will be created that adds to the
    /// growing `Vec<String>`, taking a `String`, and can be called repeatedly.
    one_by_one_setter: Option<Ident>,
    /// If the field is an `Option` field, this type will represent what `Type` is in
    /// the `Option`. If the field is a `Vec`, it will represent what is in the `Vec`.
    inner_type: Option<Type>,
    parsed: Option<TokenStream>,
}

impl From<&Field> for AnnotatedField {
    fn from(field: &Field) -> Self {
        let ident = &field.ident;
        let name = ident.clone().expect("Field has a name");
        let ty = field.ty.clone();
        let opt_typ = get_option_type(field);
        let (setter, parsed) = get_each_setter(field);
        let inner_type = if opt_typ.is_some() {
            let t = opt_typ.unwrap();
            let t = t.clone();
            Some(t)
        } else if setter.is_some() {
            let t = get_vec_type(field).unwrap().clone();
            Some(t)
        } else {
            None
        };

        Self {
            name,
            ty,
            is_optional: opt_typ.is_some(),
            one_by_one_setter: setter,
            inner_type,
            parsed,
        }
    }
}

impl AnnotatedField {
    /// This function creates individual lines used to define the *Foo*Builder struct.
    /// For example, if we have
    /// ```rust
    /// struct Foo {
    ///     alpha: String,
    ///     beta: Option<u8>,
    ///     gamma: Vec<String>,
    /// }
    /// ```
    /// then this function will generate one of the definition lines for *Foo*Builder, like
    /// ```rust
    /// # struct Foo {
    ///     beta: Option<Option<String>>,
    /// # }
    /// ```
    /// Note that in general the builder will use Options wrapping the actual type.
    /// This is to help the builder know if the user has supplied a value for this
    /// particular field.
    fn get_builder_declaration(&self) -> proc_macro2::TokenStream {
        let name = &self.name;
        let ty = &self.ty;
        quote!(
            #name : std::option::Option<#ty>,
        )
    }

    /// This function creates individual lines used to initialize the *Foo*Builder struct
    /// when the user calls `Builder::builder()`. For example, if we have
    /// ```rust
    /// struct Foo {
    ///     alpha: String,
    ///     beta: Option<u8>,
    ///     gamma: Vec<String>,
    /// }
    /// ```
    /// then this function will generate one of the initialization lines for *Foo*Builder, like
    /// ```rust
    /// # struct Foo {
    /// #     alpha: Option<String>,
    /// # }
    /// # fn t() -> Foo {
    /// # Foo {
    ///     alpha: None,
    /// # }
    /// # }
    /// ```
    /// Note that in general the builder will default to a `None` value, since the builder
    /// wraps fields in an Option to ensure they have been provided.
    fn get_builder_initializer(&self) -> proc_macro2::TokenStream {
        let name = &self.name;
        if self.one_by_one_setter.is_some() {
            quote!(
                #name : std::option::Option::Some(std::vec::Vec::new()),
            )
        } else {
            quote!(
                #name : std::option::Option::None,
            )
        }
    }

    /// This function creates individual setter functions used to set values in the *Foo*Builder struct
    /// when the user calls `Builder::setter()`. For example, if we have
    /// ```rust
    /// struct Foo {
    ///     alpha: String,
    ///     beta: Option<u8>,
    ///     gamma: Vec<String>,
    /// }
    /// ```
    /// then this function will generate one of the setter functions for *Foo*Builder, like
    /// ```rust
    /// # struct FooBuilder {
    /// #     alpha: Option<String>,
    /// # }
    /// impl FooBuilder {
    ///     pub fn alpha(&mut self, alpha: String) -> &mut Self {
    ///         self.alpha = Some(alpha);
    ///         self
    ///     }
    /// }
    /// ```
    /// If the field was also marked with `#[builder(each = baz)`, then the function will
    /// include a setter for one-by-one setting.
    fn get_builder_setter(&self) -> proc_macro2::TokenStream {
        let name = &self.name;
        let ty = &self.ty;
        let it = &self.inner_type;

        let mut q = quote!();

        if self.parsed.is_some() {
            return self.parsed.clone().unwrap().into();
        }

        if let Some(setter_name) = &self.one_by_one_setter {
            // one by one
            let it = it.clone().unwrap();
            q.extend(quote!(
                pub fn #setter_name (&mut self, value: #it) -> &mut Self {
                    self.#name.as_mut().unwrap().push(value);
                    self
                }
            ));
        }

        if self.one_by_one_setter.is_none() || &self.one_by_one_setter.clone().unwrap() != name {
            if self.is_optional {
                let it = self.inner_type.clone().unwrap();
                q.extend(quote!(
                    pub fn #name (&mut self, value: #it) -> &mut Self {
                        self.#name = std::option::Option::Some(std::option::Option::Some(value));
                        self
                    }
                ));
            } else {
                // normal setter
                q.extend(quote!(
                    pub fn #name (&mut self, value: #ty) -> &mut Self {
                        self.#name = std::option::Option::Some(value);
                        self
                    }
                ));
            }
        }

        q
    }

    /// This function creates individual lines used to initialize the *Foo*Builder struct
    /// when the user calls `Builder::build()`. For example, if we have
    /// ```rust
    /// struct Foo {
    ///     alpha: String,
    ///     beta: Option<u8>,
    ///     gamma: Vec<String>,
    /// }
    /// ```
    /// then this function will generate one of the initialization lines for `Builder`, like
    /// ```rust
    /// # struct Foo {
    /// #     alpha: Option<String>,
    /// # }
    /// # fn t() -> Foo {
    /// # Foo {
    ///     alpha: None,
    /// # }
    /// # }
    /// ```
    fn get_build_initializer(&self) -> proc_macro2::TokenStream {
        let mut q = quote!();
        let name = &self.name;

        if self.is_optional {
            q.extend(quote!(
                #name : if self.#name.is_some() {
                    self.#name.take().unwrap()
                } else {
                    std::option::Option::None
                },
            ));
        } else {
            // unwrap the Option and move it
            q.extend(quote!(
                #name : self.#name.take().unwrap(),
            ));
        }

        q
    }
}

fn create_builder_struct(builder_name: &Ident, fields: &Vec<AnnotatedField>) -> TokenStream {
    let mut field_defs = quote!();
    for field in fields {
        field_defs.extend(field.get_builder_declaration());
    }

    TokenStream::from(quote!(
        struct #builder_name {
            #field_defs
        }
    ))
}

fn create_builder_function(
    target_type: &Ident,
    builder_type: &Ident,
    fields: &Vec<AnnotatedField>,
) -> TokenStream {
    let mut initializers = quote!();
    for field in fields {
        initializers.extend(field.get_builder_initializer());
    }

    TokenStream::from(quote!(
        impl #target_type {
            pub fn builder() -> #builder_type {
                #builder_type {
                    #initializers
                }
            }
        }
    ))
}

fn create_setter_fns(builder_type: &Ident, fields: &Vec<AnnotatedField>) -> TokenStream {
    let mut setters = quote!();
    for field in fields {
        setters.extend(field.get_builder_setter());
    }

    TokenStream::from(quote!(
        impl #builder_type {
            #setters
        }
    ))
}

fn create_build_fn(
    target_type: &Ident,
    builder_type: &Ident,
    fields: &Vec<AnnotatedField>,
) -> TokenStream {
    let mut initializers = quote!();
    for field in fields {
        initializers.extend(field.get_build_initializer());
    }

    TokenStream::from(quote!(
        impl #builder_type {
            pub fn build(&mut self) -> std::option::Option<#target_type> {
                std::option::Option::Some( #target_type {
                    #initializers
                } )
            }
        }
    ))
}

#[allow(clippy::missing_panics_doc, clippy::too_many_lines)]
#[proc_macro_derive(Builder, attributes(builder))]
pub fn derive(input: TokenStream) -> TokenStream {
    let mut res = TokenStream::new();

    //eprintln!("Derive input is: {input:#?}");

    let derive_input = parse_macro_input!(input as DeriveInput);

    let struct_name = derive_input.ident;
    let builder = Ident::new(&format!("{struct_name}Builder"), Span::call_site().into());

    let mut annotated_fields: Vec<AnnotatedField> = vec![];
    #[allow(clippy::single_match)]
    match &derive_input.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(fields) => {
                for f in &fields.named {
                    annotated_fields.push(f.into());
                }
            }
            _ => (),
        },
        _ => (),
    }

    // create TypeBuilder struct
    res.extend(create_builder_struct(&builder, &annotated_fields));

    // create builder fn
    res.extend(create_builder_function(
        &struct_name,
        &builder,
        &annotated_fields,
    ));

    // create setter functions in original struct
    res.extend(create_setter_fns(&builder, &annotated_fields));

    // create build fn
    res.extend(create_build_fn(&struct_name, &builder, &annotated_fields));

    res
}

fn get_each_setter(f: &syn::Field) -> (Option<Ident>, Option<TokenStream>) {
    for a in &f.attrs {
        //eprintln!("Attr {}", a.to_token_stream());
        if let Some(ident) = a.path().get_ident() {
            if ident == &Ident::new("builder", Span::call_site().into()) {
                //eprintln!("Found an each! {}", a.path().to_token_stream());
                if let Meta::List(list) = &a.meta {
                    //eprintln!("list = {}", list.path.to_token_stream());
                    let mnv = list
                        .parse_args_with(MetaNameValue::parse)
                        .expect("Able to parse each = name");
                    //eprintln!("mnv.path {}", mnv.path.to_token_stream()); // each
                    //eprintln!("mnv.value {}", mnv.value.to_token_stream()); // "arg"
                    if let Some(i) = mnv.path.get_ident() {
                        if i == &Ident::new("each", Span::call_site().into()) {
                            if let Expr::Lit(name) = mnv.value {
                                if let Lit::Str(lstr) = name.lit {
                                    let s = lstr.value();
                                    //eprintln!("each setter is {s}");
                                    return (Some(Ident::new(&s, Span::call_site().into())), None);
                                }
                            }
                        } else {
                            let ts = syn::Error::new_spanned(
                                &a.meta,
                                indoc! {r#"
                                    expected `builder(each = "...")`
                                "#},
                            )
                            .into_compile_error();
                            return (None, Some(ts.into()));
                        }
                    }
                }
            }
        }
    }
    (None, None)
}

fn get_option_type(field: &syn::Field) -> Option<&syn::Type> {
    let typ = &field.ty;

    if let syn::Type::Path(path) = typ {
        #[allow(clippy::collapsible_if)]
        if path.qself.is_none() {
            // only one thing inside the Option (Option takes a single generic argument)
            if path.path.segments.len() == 1 {
                let segment = path
                    .path
                    .segments
                    .first()
                    .expect("path segments has a segment");
                let ident = &segment.ident;
                // are we an Option?
                if ident == &Ident::new("Option", Span::call_site().into()) {
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        let a = args.args.first().expect("args has a generic argument");
                        match a {
                            syn::GenericArgument::Type(t) => {
                                return Some(t);
                            }
                            _ => unimplemented!(),
                        }
                    }
                }
            }
        }
    }
    None
}

fn get_vec_type(field: &syn::Field) -> Option<&syn::Type> {
    let typ = &field.ty;

    if let syn::Type::Path(path) = typ {
        #[allow(clippy::collapsible_if)]
        if path.qself.is_none() {
            // only one thing inside the Vec (Vec takes a single generic argument)
            if path.path.segments.len() == 1 {
                let segment = path
                    .path
                    .segments
                    .first()
                    .expect("path segments has a segment");
                let ident = &segment.ident;
                // are we an Vec?
                if ident == &Ident::new("Vec", Span::call_site().into()) {
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        let a = args.args.first().expect("args has a generic argument");
                        match a {
                            syn::GenericArgument::Type(t) => {
                                return Some(t);
                            }
                            _ => unimplemented!(),
                        }
                    }
                } else {
                    panic!("Did not have a vec!");
                }
            }
        }
    }
    None
}
