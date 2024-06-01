// extern crate proc_macro;

mod define_callback;
mod derive_state;
mod dust_lib;
mod dust_main;
mod enum_utils;

use define_callback::MacroCallbackType;
use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn dust_define_server_callback(args: TokenStream, input: TokenStream) -> TokenStream {
   return define_callback::define_callback(args, input, MacroCallbackType::Server);
}

#[proc_macro_attribute]
pub fn dust_define_client_callback(args: TokenStream, input: TokenStream) -> TokenStream {
   return define_callback::define_callback(args, input, MacroCallbackType::Client);
}

#[proc_macro_derive(
    DustState,
    attributes(
        dust_register_callback,
    )
)]
pub fn derive_dust_state(input: TokenStream) -> TokenStream {
    return derive_state::derive_state(input);
}

#[proc_macro]
pub fn dust_lib(args: TokenStream) -> TokenStream {
    return dust_lib::dust_lib(args);
}

#[proc_macro]
pub fn dust_main(args: TokenStream) -> TokenStream {
    return dust_main::dust_main(args);
}


// #[proc_macro_attribute]
// pub fn dust_main(args: TokenStream, input: TokenStream) -> TokenStream {
//     return dust_main::dust_main(args, input);
// }