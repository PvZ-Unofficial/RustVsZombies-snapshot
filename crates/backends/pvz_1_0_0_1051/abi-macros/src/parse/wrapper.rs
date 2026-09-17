use std::collections::BTreeSet;

use syn::{
    Attribute, Ident, Result, Token, braced,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

use crate::{
    ir::{WrapperListSpec, WrapperParam, WrapperReturn, WrapperSpec},
    parse::abi::{ParsedAbiBody, parse_abi_body},
};

impl Parse for WrapperParam {
    fn parse(input: ParseStream) -> Result<Self> {
        if !input.peek(Ident) {
            return Err(input.error("wrapper parameters must be simple identifiers written as `name: Type`"));
        }

        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty = input.parse()?;
        Ok(Self { name, ty })
    }
}

impl Parse for WrapperSpec {
    fn parse(input: ParseStream) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let name: Ident = input.parse()?;

        let content;
        syn::parenthesized!(content in input);
        let params = Punctuated::<WrapperParam, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect::<Vec<_>>();
        validate_params(&params)?;

        let ret_ty = if input.peek(Token![->]) {
            input.parse::<Token![->]>()?;
            Some(input.parse()?)
        } else {
            None
        };

        let body;
        braced!(body in input);
        let ParsedAbiBody { abi, ret } = parse_abi_body(&body)?;

        let output = match (ret_ty, ret) {
            (Some(ty), Some(ret)) => Some(WrapperReturn {
                reg: ret.into_wrapper_return(&ty)?,
                ty,
            }),
            (Some(_), None) => {
                return Err(syn::Error::new(
                    name.span(),
                    "`pvz_abi_fn!` entries with a return type require a `ret:` field",
                ));
            }
            (None, Some(ret)) => {
                return Err(syn::Error::new(
                    ret.span(),
                    "void `pvz_abi_fn!` entries must not declare a `ret:` field",
                ));
            }
            (None, None) => None,
        };

        Ok(Self {
            attrs,
            name,
            params,
            output,
            abi,
        })
    }
}

impl Parse for WrapperListSpec {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut items = Vec::new();

        while !input.is_empty() {
            items.push(input.parse()?);
            if input.peek(Token![;]) {
                input.parse::<Token![;]>()?;
            }
        }

        if items.is_empty() {
            return Err(input.error("pvz_abi_fn! requires at least one wrapper entry"));
        }

        Ok(Self { items })
    }
}

fn validate_params(params: &[WrapperParam]) -> Result<()> {
    let mut seen = BTreeSet::new();

    for param in params {
        if !seen.insert(param.name.to_string()) {
            return Err(syn::Error::new(
                param.name.span(),
                format!("duplicate wrapper parameter `{}`", param.name),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_wrapper_parse_err_contains(input: &str, expect_err: &str, needle: &str) {
        let err = syn::parse_str::<WrapperSpec>(input).err().expect(expect_err);
        assert!(err.to_string().contains(needle));
    }

    #[test]
    fn parses_single_wrapper_entry() {
        let spec: WrapperSpec =
            syn::parse_str("board_update(board: *mut MainObject) { addr: 0x415d40, this: ecx = board }").unwrap();
        assert!(spec.attrs.is_empty());
        assert_eq!(spec.name.to_string(), "board_update");
        assert_eq!(spec.params.len(), 1);
        assert!(spec.output.is_none());
    }

    #[test]
    fn parses_doc_comment_as_outer_attributes() {
        let spec: WrapperSpec =
            syn::parse_str("/// # Safety\nboard_update(board: *mut MainObject) { addr: 0x415d40, this: ecx = board }")
                .unwrap();
        assert_eq!(spec.attrs.len(), 1);
        assert!(spec.attrs[0].path().is_ident("doc"));
    }

    #[test]
    fn parses_doc_attribute_and_expect_attribute() {
        let spec: WrapperSpec = syn::parse_str(
            "#[doc = \"wrapper\"] #[expect(clippy::missing_safety_doc, reason = \"raw shim\")] board_update(board: *mut MainObject) { addr: 0x415d40, this: ecx = board }",
        )
        .unwrap();
        assert_eq!(spec.attrs.len(), 2);
        assert!(spec.attrs[0].path().is_ident("doc"));
        assert!(spec.attrs[1].path().is_ident("expect"));
    }

    #[test]
    fn parses_multiple_entries() {
        let list: WrapperListSpec = syn::parse_str(
            "board_update(board: *mut MainObject) { addr: 1 } board_stage_has_pool(board: *mut MainObject) -> u8 { addr: 2, ret: al }",
        )
        .unwrap();
        assert_eq!(list.items.len(), 2);
    }

    #[test]
    fn allows_optional_trailing_semicolon_after_entry() {
        let list: WrapperListSpec = syn::parse_str(
            "board_update(board: *mut MainObject) { addr: 1 }; board_pause(board: *mut MainObject, paused: i32) { addr: 2, regs: { eax = paused } };",
        )
        .unwrap();
        assert_eq!(list.items.len(), 2);
    }

    #[test]
    fn parses_adjacent_entries_with_attributes_without_sticking() {
        let list: WrapperListSpec = syn::parse_str(
            "#[doc = \"first\"] board_update(board: *mut MainObject) { addr: 1 } #[expect(clippy::missing_safety_doc, reason = \"raw shim\")] board_stage_has_pool(board: *mut MainObject) -> u8 { addr: 2, ret: al }",
        )
        .unwrap();
        assert_eq!(list.items.len(), 2);
        assert_eq!(list.items[0].attrs.len(), 1);
        assert_eq!(list.items[1].attrs.len(), 1);
        assert!(list.items[0].attrs[0].path().is_ident("doc"));
        assert!(list.items[1].attrs[0].path().is_ident("expect"));
    }

    #[test]
    fn parses_semicolon_separated_entries_with_attributes_without_sticking() {
        let list: WrapperListSpec = syn::parse_str(
            "#[doc = \"first\"] board_update(board: *mut MainObject) { addr: 1 }; #[expect(clippy::missing_safety_doc, reason = \"raw shim\")] board_stage_has_pool(board: *mut MainObject) -> u8 { addr: 2, ret: al };",
        )
        .unwrap();
        assert_eq!(list.items.len(), 2);
        assert_eq!(list.items[0].attrs.len(), 1);
        assert_eq!(list.items[1].attrs.len(), 1);
    }

    #[test]
    fn rejects_invalid_parameter_patterns() {
        assert_wrapper_parse_err_contains(
            "board_update(_: *mut MainObject) { addr: 1 }",
            "underscore parameters should fail",
            "wrapper parameters must be simple identifiers",
        );
    }

    #[test]
    fn rejects_duplicate_parameter_names() {
        assert_wrapper_parse_err_contains(
            "board_update(board: *mut MainObject, board: *mut MainObject) { addr: 1 }",
            "duplicate parameters should fail",
            "duplicate wrapper parameter",
        );
    }

    #[test]
    fn rejects_missing_ret_for_returning_entry() {
        assert_wrapper_parse_err_contains(
            "board_stage_has_pool(board: *mut MainObject) -> u8 { addr: 1 }",
            "missing ret should fail",
            "require a `ret:` field",
        );
    }

    #[test]
    fn rejects_ret_on_void_entry() {
        assert_wrapper_parse_err_contains(
            "board_update(board: *mut MainObject) { addr: 1, ret: eax }",
            "void entry ret should fail",
            "must not declare",
        );
    }

    #[test]
    fn rejects_call_field_inside_wrapper_body() {
        assert_wrapper_parse_err_contains(
            "board_update(board: *mut MainObject) { addr: 1, call: eax }",
            "call field should fail",
            "public ABI DSL no longer accepts `call:`",
        );
    }
}
