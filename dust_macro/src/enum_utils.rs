use convert_case::{Case, Casing};

pub fn field_to_enum(ident: &syn::Ident) -> syn::Ident {
    syn::Ident::new(
        format!("{}", ident)
            .as_str()
            .to_case(Case::UpperCamel)
            .as_str(),
        ident.span(),
    )
}