//! `#[rsvz::state_hooks]` expansion.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Error, ItemFn, parse2};

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return Error::new_spanned(attr, "#[rsvz::state_hooks] does not accept options").to_compile_error();
    }

    let input = match parse2::<ItemFn>(item) {
        Ok(input) => input,
        Err(error) => return error.to_compile_error(),
    };
    if let Err(error) = crate::script::validate_signature(&input.sig, "#[rsvz::state_hooks]", false) {
        return error.to_compile_error();
    }

    let ident = &input.sig.ident;
    quote! {
        #input

        impl __RsvzStateHookInstaller {
            #[doc(hidden)]
            pub(crate) fn __rsvz_install(self) -> ::rsvz::core::runtime::RuntimeResult<()> {
                ::rsvz::__private::run_state_hook_installer(|| {
                    #ident();
                    Ok(())
                })
            }
        }
    }
}
