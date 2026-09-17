#![warn(
    clippy::doc_link_code,
    clippy::expect_used,
    clippy::fn_params_excessive_bools,
    clippy::future_not_send,
    clippy::indexing_slicing,
    clippy::missing_assert_message,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_ref_mut,
    clippy::needless_pass_by_value,
    clippy::option_option,
    clippy::panic_in_result_fn,
    clippy::ref_option,
    clippy::ref_option_ref,
    clippy::return_self_not_must_use,
    clippy::string_slice,
    clippy::unnecessary_wraps,
    clippy::unwrap_in_result,
    clippy::unwrap_used,
    clippy::use_self,
    clippy::wildcard_enum_match_arm
)]

mod script;
mod state_hooks;

use proc_macro::TokenStream;

/// Wraps a plain Rust function body as an `rsvz` script entry.
#[proc_macro_attribute]
pub fn script(attr: TokenStream, item: TokenStream) -> TokenStream {
    script::expand(attr.into(), item.into()).into()
}

/// Marks the single session state-hook installer in an `rsvz` user crate.
#[proc_macro_attribute]
pub fn state_hooks(attr: TokenStream, item: TokenStream) -> TokenStream {
    state_hooks::expand(attr.into(), item.into()).into()
}
