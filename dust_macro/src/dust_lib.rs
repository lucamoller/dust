use proc_macro::TokenStream;
use quote::quote;

pub fn dust_lib(args: TokenStream) -> TokenStream {
    let app = syn::parse_macro_input!(args as syn::Path);

    return quote! {
        #[cfg(feature = "hydrate")]
        #[::dust::wasm_bindgen::prelude::wasm_bindgen]
        pub fn hydrate() {
            ::dust::console_error_panic_hook::set_once();
            leptos::mount_to_body(#app);
        }

    }.into();
}