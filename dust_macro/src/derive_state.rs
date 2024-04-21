use convert_case::{Case, Casing};
use proc_macro::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{parse_macro_input, DeriveInput};
use crate::enum_utils::field_to_enum;
use std::collections::HashSet;
use once_cell::sync::Lazy;

static INCREMENTABLE_TYPES: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    HashSet::from(["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64"])
});

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

pub fn derive_state(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let attributes = DustStateAttributes::from_input(&ast);

    let state_struct = &ast.ident;
    let internal_mod = syn::Ident::new(
        format!("{}Internal", state_struct).as_str().to_case(Case::Snake).as_str(), 
        state_struct.span()
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

    //
    // Identifier Enum
    //
    let identifier_enum_entries = fields.iter().map(|field| {
        let entry_ident = field_to_enum(&field.ident.clone().unwrap());
        quote! {
            #entry_ident
        }
    });

    let dust_identifier_enum = quote! {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
        pub enum Identifier {
            #(#identifier_enum_entries,)*
        }
    };

    //
    // Value Enum
    //
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

    let identifier_to_value_from_signal_entries = fields.iter().map(|field| {
        let field_ident = &field.ident;
        let entry_ident = field_to_enum(&field.ident.clone().unwrap());
        quote! {
            Identifier::#entry_ident => Value::#entry_ident(self.#field_ident.get_untracked())
        }
    });

    let dust_value_enum = quote! {
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

    //
    // Context
    //
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

    let signal_fields_setter_getter = fields.iter().map(|field| {
        let field_ident = &field.ident;
        let field_type = &field.ty;
        // let getter_ident = syn::Ident::new(
        //     &format!("get_{}", field_ident.clone().unwrap()),
        //     field_ident.span(),
        // );
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

    let dust_context = quote! {
        #[derive(Clone, Debug)]
        struct ContextInternalState {
            initialized: std::cell::Cell<bool>,
        }

        #[derive(Clone, Debug)]
        pub struct DustContext {
            #(#signal_fields)*

            context_internal_state: ContextInternalState,
        }

        impl DustContext {
            pub fn from_default_state() -> Self {
                let state = super::#state_struct::default();
                #(#signal_variables_definition)*
                Self {
                    #(#signal_fields_initialization)*

                    context_internal_state: ContextInternalState {
                        initialized: std::cell::Cell::new(false),
                    },
                }
            }

            pub fn get_values_from_identifiers(
                &self, identifiers: &std::collections::HashSet<Identifier>
            ) -> Vec<Value> {
                identifiers.iter().map(|value_ident| {
                    match *value_ident {
                        #(#identifier_to_value_from_signal_entries,)*
                    }
                }).collect()
            }

            pub fn apply_updates(&self, updates: Vec<Value>) {
                for update in updates {
                    match update {
                        #(#signal_fields_update,)*
                    }
                }
            }

            #(#signal_fields_setter_getter)*

            pub fn initialize_state(self: &std::rc::Rc<Self>){
                ::dust::leptos::logging::log!("initialize_state");
                self.context_internal_state.initialized.set(true);
                self.handle_updates(
                    self.get_values_from_identifiers(
                        &EXECUTOR.get_required_initialization_inputs()
                    )
                );
            }

            pub fn handle_updates(self: &std::rc::Rc<Self>, input_updates: Vec<Value>) {
                let updated_inputs = input_updates.iter().map(|v| v.to_identifier()).collect();
                let execution_plan = EXECUTOR.get_execution_plan(&updated_inputs);
                let required_state = EXECUTOR.get_required_state(&updated_inputs, &execution_plan);
                let required_state_values = self.get_values_from_identifiers(&required_state);

                ::dust::leptos::logging::log!("handle_updates call");
                ::dust::leptos::logging::log!("  input_updates: {:?}", input_updates);
                ::dust::leptos::logging::log!("  execution_plan: {:?}", execution_plan);
                ::dust::leptos::logging::log!("  required_state_values: {:?}", required_state_values);

                let state = self.clone();
                ::dust::leptos::spawn_local(async move {
                    let response = server_callback(input_updates, required_state_values).await;
                    match response {
                        Ok(output_updates) => {
                            ::dust::leptos::logging::log!("    server output_updates: {:?}", output_updates);
                            state.apply_updates(output_updates);
                        }
                        Err(e) => {
                            ::dust::leptos::logging::log!("server_callback error: {}", e);
                        }
                    }
                });
            }

        }
    };

    //
    // CallbackInfo
    //
    let dust_callback_info = quote! {
        #[derive(Clone)]
        pub struct CallbackInfo {
            pub name: &'static str,
            pub cb: fn(&mut super::#state_struct) -> Vec<Value>,
            pub inputs: Vec<Identifier>,
            pub outputs: Vec<Identifier>,
        }

        impl CallbackInfo {
            pub fn new(
                name: &'static str,
                cb: fn(&mut super::#state_struct) -> Vec<Value>,
                inputs: Vec<Identifier>,
                outputs: Vec<Identifier>
            ) -> CallbackInfo {
                CallbackInfo{
                    name, cb, inputs, outputs
                }
            }
        }
    };

    let registered_callbacks = attributes.callbacks.iter().map(|callback_tokens| {
        let callback_get_info_ident = syn::Ident::new(
            &format!("{}_get_info", callback_tokens),
            callback_tokens.span(),
        );

        quote! {
            #callback_get_info_ident()
        }
    });

    //
    // Apply Updates
    //
    let apply_updates_enum_update_match = fields.iter().map(|field| {
        // eprintln!("field: {:#?}", field);
        let field_ident = &field.ident;
        let enum_ident = field_to_enum(&field.ident.clone().unwrap());
        quote! {
            #internal_mod::Value::#enum_ident(v) => {self.#field_ident = v.clone();}
        }
    });

    quote! {
        mod #internal_mod {
            use ::dust::*;  // Get traits in scope.
            use ::dust::leptos::*;  // Get traits in scope.

            #dust_identifier_enum

            #dust_value_enum

            #dust_context

            #dust_callback_info

            pub static EXECUTOR: ::dust::once_cell::sync::Lazy<
                ::dust::Executor<Identifier, Value, super::#state_struct>,
            > = ::dust::once_cell::sync::Lazy::new(|| {
                let mut app = ::dust::Executor::new();
                for callback in super::#state_struct::get_registered_callbacks() {
                    app.register_callback(callback);
                }
                app.init_callbacks();
                app
            });

            #[::leptos::server(ServerCallback, "/server_callback", "Cbor")]
            pub async fn server_callback(
                input_updates: Vec<Value>,
                required_state: Vec<Value>,
            ) -> Result<Vec<Value>, ::dust::leptos::ServerFnError> {
                println!(
                    "server_callback input_updates: {:?} required_state {:?}",
                    input_updates, required_state
                );

                let output_updates = EXECUTOR.process_updates(input_updates, required_state);
                Ok(output_updates)
            }
        }

        impl ::dust::ApplyUpdates<#internal_mod::Value> for #state_struct {
            fn apply_updates(&mut self, updates: &Vec<#internal_mod::Value>) {
                for update in updates.iter() {
                    match update {
                        #(#apply_updates_enum_update_match,)*
                    };
                }
            }
        }

        impl #state_struct {
            pub fn get_registered_callbacks() -> Vec<::dust::StateCallback<#internal_mod::Identifier, #internal_mod::Value, #state_struct>> {
                return vec![#(#registered_callbacks,)*];
            }
        }

        impl ::dust::StateTypes for #state_struct {
            // Associated type definition
            type Identifier = #internal_mod::Identifier;
            type Value = #internal_mod::Value;
            type CallbackInfo = ::dust::StateCallback<
                #internal_mod::Identifier, 
                #internal_mod::Value, 
                #state_struct
            >;
            type Context = std::rc::Rc<#internal_mod::DustContext>;
        }

        impl #state_struct {
            pub fn provide_and_initiaze_context() {
                let state = std::rc::Rc::new(#internal_mod::DustContext::from_default_state());
                ::dust::leptos::provide_context(state.clone());
                ::dust::leptos::create_effect(move |_| {
                    log!("Initializing state...");
                    state.initialize_state();
                });
            }

            pub fn expect_context() -> std::rc::Rc<#internal_mod::DustContext> {
                return ::dust::leptos::expect_context::<std::rc::Rc<#internal_mod::DustContext>>();
            }
        }
    }
    .into()
}
