use crate::enum_utils::field_to_enum;
use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;

pub enum MacroCallbackType {
    Server,
    Client,
}

enum CallbackArgType {
    Input,
    Output,
}

struct CallbackArg {
    name_ident: syn::Ident,
    arg_type: CallbackArgType,
}

fn get_arg_name_ident(arg: &syn::FnArg) -> syn::Ident {
    if let syn::FnArg::Typed(pat_type) = arg {
        if let syn::Pat::Ident(ref pat_ident) = *pat_type.pat {
            return pat_ident.ident.clone();
        }
    }
    panic!("couldn't extract arg name for {:?}", arg);
}

fn get_arg_type(arg: &syn::FnArg) -> CallbackArgType {
    if let syn::FnArg::Typed(pat_type) = arg {
        if let syn::Type::Path(ref path) = *pat_type.ty {
            if path.path.segments.len() > 0 && path.path.segments[0].ident == "Input" {
                return CallbackArgType::Input;
            }
        }

        if let syn::Type::Reference(ref type_reference) = *pat_type.ty {
            if let syn::Type::Path(ref path) = *type_reference.elem {
                if path.path.segments.len() > 0 && path.path.segments[0].ident == "Output" {
                    return CallbackArgType::Output;
                }
            }
        }
    }
    panic!(
        "couldn't extract arg outer type. Expected 'Input<T>' or '&mut Output<T>', \
        found {:#?}",
        arg
    );
}

fn get_callback_args(item: &syn::Item) -> Vec<CallbackArg> {
    let mut result = Vec::new();
    if let syn::Item::Fn(syn::ItemFn {
        sig: syn::Signature { inputs, .. },
        ..
    }) = item
    {
        for arg in inputs {
            let arg_name = get_arg_name_ident(arg);
            let arg_type = get_arg_type(arg);

            result.push(CallbackArg {
                name_ident: arg_name,
                arg_type: arg_type,
            });
        }
    }
    return result;
}

fn generate_wrapper_fn(
    state_struct: &syn::Ident,
    function_name: &syn::Ident,
    wrapper_name: &syn::Ident,
    callback_args: &Vec<CallbackArg>,
    outputs: &Vec<&CallbackArg>,
) -> proc_macro2::TokenStream {
    let output_variables = outputs.iter().map(|cb| {
        let name_ident = &cb.name_ident;
        quote! {
            let mut #name_ident = Output::new();
        }
    });

    let call_args = callback_args.iter().map(|cb| {
        let name_ident = &cb.name_ident;
        match cb.arg_type {
            CallbackArgType::Input => {
                let enum_ident = field_to_enum(name_ident);
                quote! {
                    Input {
                        value: if let Value::#enum_ident(v) = state.get(
                            &<#state_struct as ::dust::StateTypes>::Identifier::#enum_ident).unwrap().clone() {
                                v
                            }
                        else {
                            panic!("Failed to unwrap value enum for {:?}!", 
                                   <#state_struct as ::dust::StateTypes>::Identifier::#enum_ident)
                        },
                    }
                }
            }
            CallbackArgType::Output => {
                quote! {&mut #name_ident}
            }
        }
    });

    let collect_updates = if outputs.len() > 0 {
        let output_updates = outputs.iter().map(|cb| {
            let name_ident = &cb.name_ident;
            let enum_ident = field_to_enum(name_ident);
            quote! {
                match #name_ident.state {
                    ::dust::OutputState::NoChange => None,
                    ::dust::OutputState::Updated(value) => Some(
                        <#state_struct as ::dust::StateTypes>::Value::#enum_ident(value.clone())
                    ),
                }
            }
        });

        quote! {
            vec![
                #(#output_updates,)*
            ].iter().filter_map(|x| { x.clone() }).collect()
        }
    } else {
        quote! {
            vec![]
        }
    };

    return quote! {
        fn #wrapper_name(
            state: &::std::collections::HashMap<<#state_struct as ::dust::StateTypes>::Identifier,
                                                <#state_struct as ::dust::StateTypes>::Value>
        ) -> Vec<<#state_struct as ::dust::StateTypes>::Value> {
            type Value = <#state_struct as ::dust::StateTypes>::Value;

            #(#output_variables)*
            #function_name(#(
                #call_args,
            )*);
            return #collect_updates;
        }
    };
}

fn generate_get_info_fn(
    state_struct: &syn::Ident,
    function_name: &syn::Ident,
    wrapper_name: &syn::Ident,
    inputs: &Vec<&CallbackArg>,
    outputs: &Vec<&CallbackArg>,
    callback_type: &MacroCallbackType,
) -> proc_macro2::TokenStream {
    let input_entries = inputs.iter().map(|arg| {
        let input_ident = &arg.name_ident;
        let input_enum = field_to_enum(input_ident);
        quote! {
            <#state_struct as ::dust::StateTypes>::Identifier::#input_enum
        }
    });
    let output_entries = outputs.iter().map(|arg| {
        let output_ident = &arg.name_ident;
        let output_enum = field_to_enum(output_ident);
        quote! {
            <#state_struct as ::dust::StateTypes>::Identifier::#output_enum
        }
    });

    let get_info_name =
        syn::Ident::new(&format!("{}_get_info", function_name), function_name.span());
    let function_name_str = format!("{}", function_name);

    return match *callback_type {
        MacroCallbackType::Server => {
            quote! {
                fn #get_info_name() -> ::dust::Callback<<#state_struct as ::dust::StateTypes>::Identifier,
                                                        <#state_struct as ::dust::StateTypes>::Value> {
                    ::dust::Callback::new(
                        #function_name_str,
                        // Server callbacks should only have wrappers available in the server binary.
                        #[cfg(feature = "ssr")]
                        Some(#wrapper_name),
                        #[cfg(not(feature = "ssr"))]
                        None,
                        vec![#(#input_entries,)*],
                        vec![#(#output_entries,)*],
                        ::dust::CallbackType::Server,
                    )
                }
            }
        }
        MacroCallbackType::Client => {
            quote! {
                fn #get_info_name() ->  ::dust::Callback<<#state_struct as ::dust::StateTypes>::Identifier,
                                                         <#state_struct as ::dust::StateTypes>::Value>{
                    ::dust::Callback::new(
                        #function_name_str,
                        Some(#wrapper_name),
                        vec![#(#input_entries,)*],
                        vec![#(#output_entries,)*],
                        ::dust::CallbackType::Client,
                    )
                }
            }
        }
    };
}

pub fn define_callback(
    args: TokenStream,
    input: TokenStream,
    callback_type: MacroCallbackType,
) -> TokenStream {
    let state_struct = parse_macro_input!(args as syn::Ident);
    let function = parse_macro_input!(input as syn::Item);

    let function_name = if let syn::Item::Fn(syn::ItemFn { ref sig, .. }) = function {
        sig.ident.clone()
    } else {
        panic!("missing function name")
    };

    let callback_args = get_callback_args(&function);
    let inputs: Vec<&CallbackArg> = callback_args
        .iter()
        .filter_map(|cb| match cb.arg_type {
            CallbackArgType::Input => Some(cb),
            _ => None,
        })
        .collect();
    let outputs: Vec<&CallbackArg> = callback_args
        .iter()
        .filter_map(|cb| match cb.arg_type {
            CallbackArgType::Output => Some(cb),
            _ => None,
        })
        .collect();

    let wrapper_name = syn::Ident::new(&format!("{}_wrapper", function_name), function_name.span());
    let wrapper_fn = generate_wrapper_fn(
        &state_struct,
        &function_name,
        &wrapper_name,
        &callback_args,
        &outputs,
    );

    let get_info_fn = generate_get_info_fn(
        &state_struct,
        &function_name,
        &wrapper_name,
        &inputs,
        &outputs,
        &callback_type,
    );

    return match callback_type {
        MacroCallbackType::Server => {
            quote! {
                // Server callbacks should only have their function and wrapper compiled in the server binary.
                #[cfg(feature = "ssr")]
                #function

                #[cfg(feature = "ssr")]
                #wrapper_fn

                #get_info_fn
            }
            .into()
        }
        MacroCallbackType::Client => quote! {
            #function

            #wrapper_fn

            #get_info_fn
        }
        .into(),
    };
}
