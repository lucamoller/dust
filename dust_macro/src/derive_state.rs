use crate::enum_utils::field_to_enum;
use convert_case::{Case, Casing};
use once_cell::sync::Lazy;
use proc_macro::TokenStream;
use quote::quote;
use std::collections::HashSet;
use syn::spanned::Spanned;
use syn::{parse_macro_input, DeriveInput};

static INCREMENTABLE_TYPES: Lazy<HashSet<&'static str>> =
    Lazy::new(|| HashSet::from(["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64"]));

type Fields = syn::punctuated::Punctuated<syn::Field, syn::token::Comma>;

struct DustStateAttributes {
    callbacks: Vec<proc_macro2::TokenStream>,
}

impl DustStateAttributes {
    fn from_input(input: &syn::DeriveInput) -> DustStateAttributes {
        let mut result = DustStateAttributes {
            callbacks: Vec::new(),
        };
        for attr in input.attrs.iter() {
            if attr.meta.path().segments.len() != 1 {
                panic!(
                    "Expecting single path segment on attr path for: {:#?}",
                    attr
                );
            }

            if format!("{}", attr.meta.path().segments[0].ident) == "dust_register_callback" {
                result
                    .callbacks
                    .push(attr.meta.require_list().unwrap().tokens.clone());
            }
        }
        return result;
    }
}

fn generate_identifier_enum(fields: &Fields) -> proc_macro2::TokenStream {
    let identifier_enum_entries = fields.iter().map(|field| {
        let entry_ident = field_to_enum(&field.ident.clone().unwrap());
        quote! {
            #entry_ident
        }
    });

    return quote! {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
        pub enum Identifier {
            #(#identifier_enum_entries,)*
        }
    };
}

fn generate_value_enum(fields: &Fields) -> proc_macro2::TokenStream {
    let value_enum_entries = fields.iter().map(|field| {
        let entry_ident = field_to_enum(&field.ident.clone().unwrap());
        let entry_inner_type = &field.ty;
        quote! {
            #entry_ident(#entry_inner_type)
        }
    });

    let value_to_identifier_match_entries = fields.iter().map(|field| {
        let entry_ident = field_to_enum(&field.ident.clone().unwrap());
        quote! {
            Value::#entry_ident(_) => Identifier::#entry_ident
        }
    });

    return quote! {
        #[derive(Clone, Debug, ::dust::serde::Serialize, ::dust::serde::Deserialize)]
        pub enum Value {
            #(#value_enum_entries,)*
        }

        impl ::dust::ValueToIdentifier<Identifier> for Value {
            fn to_identifier(&self) -> Identifier {
                match self {
                    #(#value_to_identifier_match_entries,)*
                }
            }
        }
    };
}

fn generate_executor(state_struct: &syn::Ident) -> proc_macro2::TokenStream {
    return quote! {
        pub static EXECUTOR: ::dust::once_cell::sync::Lazy<
            ::dust::Executor<Identifier, Value>,
        > = ::dust::once_cell::sync::Lazy::new(|| {
            let mut app = ::dust::Executor::new();
            for callback in super::#state_struct::get_registered_callbacks() {
                app.register_callback(callback);
            }
            app.init_callbacks();
            app
        });

        #[::leptos::server(ServerCallback, "/server_callback", "Cbor")]
        pub async fn execute_server_callbacks(
            execution_args: Vec<::dust::ExecutionArg<Value>>,
            server_plan: Vec<::dust::CallbackId>,
        ) -> Result<Vec<::dust::ExecutionArg<Value>>, ::dust::leptos::ServerFnError> {
            println!(
                "server_callback execution_args: {:#?} server_plan {:?}",
                execution_args, server_plan
            );

            let output_updates = EXECUTOR.execute_plan(&execution_args, &server_plan);
            Ok(output_updates)
        }
    };
}

fn generate_context(state_struct: &syn::Ident, fields: &Fields) -> proc_macro2::TokenStream {
    let signal_fields = fields.iter().map(|field| {
        let field_ident = &field.ident;
        let signal_write_ident = syn::Ident::new(
            &format!("{}_write_signal", field_ident.clone().unwrap()),
            field_ident.span(),
        );
        let field_type = &field.ty;
        quote! {
            pub #field_ident: ::dust::leptos::ReadSignal<#field_type>,
            #signal_write_ident: ::dust::leptos::WriteSignal<#field_type>,
        }
    });

    let signal_variables_definition = fields.iter().map(|field| {
        let field_ident = &field.ident;
        let signal_write_ident = syn::Ident::new(
            &format!("{}_write_signal", field_ident.clone().unwrap()),
            field_ident.span(),
        );
        quote! {
            let (#field_ident, #signal_write_ident) = ::dust::leptos::create_signal(state.#field_ident);
        }
    });

    let signal_fields_initialization = fields.iter().map(|field| {
        let field_ident = &field.ident;
        let signal_write_ident = syn::Ident::new(
            &format!("{}_write_signal", field_ident.clone().unwrap()),
            field_ident.span(),
        );
        quote! {
            #field_ident: #field_ident,
            #signal_write_ident: #signal_write_ident,
        }
    });

    let signal_fields_update = fields.iter().map(|field| {
        let field_ident = &field.ident;
        let signal_write_ident = syn::Ident::new(
            &format!("{}_write_signal", field_ident.clone().unwrap()),
            field_ident.span(),
        );
        let field_literal = syn::LitStr::new(
            &format!("{}", field_ident.clone().unwrap()),
            field_ident.span(),
        );
        let enum_ident = field_to_enum(&field.ident.clone().unwrap());
        quote! {
            Value::#enum_ident(v) => {
                ::dust::leptos::logging::log!("apply_updates setting {}", #field_literal);
                self.#signal_write_ident.set(v);
            }
        }
    });

    let identifier_to_value_from_signal_entries = fields.iter().map(|field| {
        let field_ident = &field.ident;
        let entry_ident = field_to_enum(&field.ident.clone().unwrap());
        quote! {
            Identifier::#entry_ident => Value::#entry_ident(self.#field_ident.get_untracked())
        }
    });

    let signal_fields_setter_getter = fields.iter().map(|field| {
        let field_ident = &field.ident;
        let field_type = &field.ty;
        let setter_ident = syn::Ident::new(
            &format!("set_{}", field_ident.clone().unwrap()),
            field_ident.span(),
        );
        let update_ident = syn::Ident::new(
            &format!("update_{}", field_ident.clone().unwrap()),
            field_ident.span(),
        );
        let signal_write_ident = syn::Ident::new(
            &format!("{}_write_signal", field_ident.clone().unwrap()),
            field_ident.span(),
        );
        let enum_ident = field_to_enum(&field.ident.clone().unwrap());

        use quote::ToTokens;
        let increment_onclick = if INCREMENTABLE_TYPES.contains(field_type.clone().into_token_stream().to_string().as_str())  {
            let increment_onclick_ident = syn::Ident::new(
                &format!("increment_onclick_{}", field_ident.clone().unwrap()),
                field_ident.span(),
            );
            quote! {
                pub fn #increment_onclick_ident(self: &std::rc::Rc<Self>) -> impl Fn(::dust::web_sys::MouseEvent) {
                    let state = self.clone();
                    return move |_| {
                        state.#update_ident(|x| {
                            *x = *x + 1;
                        });
                    };
                }
            }
        } else {
            quote! {}
        };

        quote! {
            pub fn #setter_ident(self: &std::rc::Rc<Self>, v: #field_type) {
                self.#signal_write_ident.set(v);
                self.handle_updates(vec![Value::#enum_ident(self.#field_ident.get_untracked())]);
            }

            pub fn #update_ident(self: &std::rc::Rc<Self>, f: impl FnOnce(&mut #field_type)) {
                self.#signal_write_ident.update(f);
                self.handle_updates(vec![Value::#enum_ident(self.#field_ident.get_untracked())]);
            }

            #increment_onclick
        }
    });

    return quote! {
        #[derive(Debug)]
        pub struct ContextImpl {
            #(#signal_fields)*

            context_manager: ::dust::ContextManager<Identifier>,
        }

        impl ::dust::Context for ContextImpl {
            type I = Identifier;
            type V = Value;

            fn from_default_state() -> Self {
                let state = super::#state_struct::default();
                #(#signal_variables_definition)*
                Self {
                    #(#signal_fields_initialization)*

                    context_manager: ::dust::ContextManager::new(),
                }
            }

            fn update_value(&self, value: Self::V) {
                match value {
                    #(#signal_fields_update,)*
                };
            }

            fn read_value(&self, identifier: &Self::I) -> Self::V {
                return match *identifier {
                    #(#identifier_to_value_from_signal_entries,)*
                };
            }

            fn get_manager(&self) -> &::dust::ContextManager<Self::I> {
                return &self.context_manager;
            }

            fn get_executor() -> &'static ::dust::Executor<Self::I, Self::V> {
                return ::dust::once_cell::sync::Lazy::force(&EXECUTOR);
            }

            fn execute_server_callbacks(
                execution_args: Vec<::dust::ExecutionArg<Self::V>>,
                server_plan: Vec<::dust::CallbackId>,
            ) -> impl std::future::Future<Output = Result<Vec<::dust::ExecutionArg<Self::V>>, ::leptos::ServerFnError>>
                   + Send {
                return execute_server_callbacks(execution_args, server_plan);
            }
        }

        impl ContextImpl {
            #(#signal_fields_setter_getter)*
        }
    };
}

fn generate_get_registered_callbacks(
    state_struct: &syn::Ident,
    internal_mod: &syn::Ident,
    attributes: &DustStateAttributes,
) -> proc_macro2::TokenStream {
    let registered_callbacks = attributes.callbacks.iter().map(|callback_tokens| {
        let callback_get_info_ident = syn::Ident::new(
            &format!("{}_get_info", callback_tokens),
            callback_tokens.span(),
        );

        quote! {
            #callback_get_info_ident()
        }
    });

    return quote! {
        impl #state_struct {
            pub fn get_registered_callbacks() -> Vec<::dust::Callback<#internal_mod::Identifier,
                                                                      #internal_mod::Value>> {
                return vec![#(#registered_callbacks,)*];
            }
        }
    };
}

pub fn derive_state(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let attributes = DustStateAttributes::from_input(&ast);

    let state_struct = &ast.ident;
    let internal_mod = syn::Ident::new(
        format!("{}Internal", state_struct)
            .as_str()
            .to_case(Case::Snake)
            .as_str(),
        state_struct.span(),
    );

    let fields = if let syn::Data::Struct(syn::DataStruct {
        fields: syn::Fields::Named(syn::FieldsNamed { ref named, .. }),
        ..
    }) = ast.data
    {
        named
    } else {
        unimplemented!();
    };

    let identifier_enum = generate_identifier_enum(fields);
    let value_enum = generate_value_enum(fields);
    let executor = generate_executor(state_struct);
    let context = generate_context(state_struct, fields);

    let get_registered_callbacks =
        generate_get_registered_callbacks(state_struct, &internal_mod, &attributes);

    quote! {
        mod #internal_mod {
            use ::dust::*;  // Get traits in scope.
            use ::dust::leptos::*;  // Get traits in scope.

            #identifier_enum

            #value_enum

            #executor

            #context
        }

        #get_registered_callbacks

        impl ::dust::StateTypes for #state_struct {
            // Associated type definition
            type Identifier = #internal_mod::Identifier;
            type Value = #internal_mod::Value;
        }

        impl ::dust::ContextProvider for #state_struct {
            type C = #internal_mod::ContextImpl;
        }

        // Expose ContextProvider methods directly to avoid requiring importing the trait.
        impl #state_struct {
            pub fn provide_and_initialize_context() {
                <Self as ::dust::ContextProvider>::provide_and_initialize_context();
            }

            pub fn expect_context() -> std::rc::Rc<#internal_mod::ContextImpl> {
                return <Self as ::dust::ContextProvider>::expect_context();
            }
        }
    }
    .into()
}
