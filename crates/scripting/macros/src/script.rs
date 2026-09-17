//! `#[rsvz::script]` expansion.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Error, FnArg, ItemFn, Meta, ReturnType, Signature, Token, parse2};

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    if let Err(error) = validate_options(attr) {
        return error.to_compile_error();
    }

    let input = match parse2::<ItemFn>(item) {
        Ok(input) => input,
        Err(error) => return error.to_compile_error(),
    };

    match validate_signature(&input.sig, "#[rsvz::script]", true) {
        Ok(()) => expand_script(input),
        Err(error) => error.to_compile_error(),
    }
}

fn validate_options(attr: TokenStream) -> Result<(), Error> {
    if attr.is_empty() {
        return Ok(());
    }

    let metas = Punctuated::<Meta, Token![,]>::parse_terminated.parse2(attr)?;
    match metas.into_iter().next() {
        Some(Meta::NameValue(name_value)) if name_value.path.is_ident("state_hooks") => Err(Error::new_spanned(
            name_value,
            "#[rsvz::script] no longer accepts state_hooks; mark one installer with #[rsvz::state_hooks]",
        )),
        Some(Meta::Path(path)) => {
            let Some(ident) = path.get_ident() else {
                return Err(Error::new_spanned(path, "unsupported #[rsvz::script] option"));
            };
            let message = if ident == "fast_forward" {
                "#[rsvz::script(fast_forward)] is no longer supported; fast-forward DSL operations are enabled by default"
            } else if ident == "maid_cheats" {
                "#[rsvz::script(maid_cheats)] is no longer supported; MaidCheats DSL operations are enabled by default"
            } else {
                "unsupported #[rsvz::script] option; final runners select backends with Cargo features"
            };
            Err(Error::new_spanned(ident, message))
        }
        Some(Meta::List(unsupported)) => Err(Error::new_spanned(unsupported, "unsupported #[rsvz::script] option")),
        Some(Meta::NameValue(unsupported)) => {
            Err(Error::new_spanned(unsupported, "unsupported #[rsvz::script] option"))
        }
        None => Ok(()),
    }
}

pub(crate) fn validate_signature(sig: &Signature, attribute: &str, allow_return: bool) -> Result<(), Error> {
    if sig.constness.is_some() {
        return Err(Error::new_spanned(
            sig.constness,
            format!("{attribute} does not support const fn"),
        ));
    }
    if sig.asyncness.is_some() {
        return Err(Error::new_spanned(
            sig.asyncness,
            format!("{attribute} does not support async fn"),
        ));
    }
    if sig.unsafety.is_some() {
        return Err(Error::new_spanned(
            sig.unsafety,
            format!("{attribute} does not support unsafe fn"),
        ));
    }
    if let Some(abi) = &sig.abi {
        let span_target = quote_spanned!(abi.extern_token.span=> extern);
        return Err(Error::new_spanned(
            span_target,
            format!("{attribute} does not support extern fn"),
        ));
    }
    if let Some(param) = sig.generics.params.iter().next() {
        return Err(Error::new_spanned(
            param,
            format!("{attribute} functions cannot declare generics"),
        ));
    }
    if let Some(where_clause) = &sig.generics.where_clause {
        return Err(Error::new_spanned(
            where_clause,
            format!("{attribute} functions cannot declare where clauses"),
        ));
    }
    if let Some(input) = sig.inputs.iter().next() {
        let span_target = match input {
            FnArg::Receiver(receiver) => quote_spanned!(receiver.self_token.span=> self),
            FnArg::Typed(typed) => quote_spanned!(typed.pat.span()=> #typed),
        };
        return Err(Error::new_spanned(
            span_target,
            format!("{attribute} functions cannot take parameters"),
        ));
    }
    if !allow_return && !matches!(sig.output, ReturnType::Default) {
        return Err(Error::new_spanned(
            &sig.output,
            format!("{attribute} functions cannot declare an explicit return type"),
        ));
    }

    Ok(())
}

fn expand_script(input: ItemFn) -> TokenStream {
    let attrs = input.attrs;
    let ident = input.sig.ident;
    let fallible = !matches!(input.sig.output, ReturnType::Default);
    let body = input.block;
    let body_tokens = if fallible {
        quote! { #body }
    } else {
        quote! {
            #body;
            Ok(())
        }
    };

    quote! {
        #(#attrs)*
        pub fn #ident() -> ::rsvz::core::runtime::RuntimeResult<()> {
            ::rsvz::__private::run_script(|| {
                use ::rsvz::dsl::prelude::*;
                #body_tokens
            })
        }

        #[doc(hidden)]
        pub(crate) struct __RsvzStateHookInstaller;

        #[doc(hidden)]
        pub(crate) trait __RsvzDefaultStateHookInstaller: Sized {
            fn __rsvz_install(self) -> ::rsvz::core::runtime::RuntimeResult<()> {
                Ok(())
            }
        }

        impl __RsvzDefaultStateHookInstaller for __RsvzStateHookInstaller {}

        #[doc(hidden)]
        pub fn __rsvz_install_state_hooks() -> ::rsvz::core::runtime::RuntimeResult<()> {
            ::rsvz::__private::install_framework_state_hooks()?;
            #[allow(unused_imports)]
            use self::__RsvzDefaultStateHookInstaller as _;
            __RsvzStateHookInstaller.__rsvz_install()
        }

        #[doc(hidden)]
        pub fn __rsvz_dispatch(
            backend: &mut ::rsvz::__private::CurrentBackend,
            input: ::rsvz::__private::DispatchInput,
        ) -> ::rsvz::__private::DispatchResult {
            ::rsvz::__private::runtime_dispatch(
                backend,
                input,
                #ident,
                __rsvz_install_state_hooks,
            )
        }
    }
}
