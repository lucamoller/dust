use proc_macro::TokenStream;
use quote::quote;

pub fn dust_main(args: TokenStream) -> TokenStream {
    let app = syn::parse_macro_input!(args as syn::Path);

    return quote! {
        #[cfg(feature = "ssr")]
        #[::dust::tokio::main]
        async fn main() {
            ::dust::serve::serve(#app).await;
        }
    }.into();
}