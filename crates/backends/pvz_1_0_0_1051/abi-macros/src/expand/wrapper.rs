use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Result, parse_quote};

use crate::{
    expand::asm,
    ir::{CapturedReturn, WrapperListSpec, WrapperReturn, WrapperSpec},
};

pub(crate) fn expand_wrapper(spec: WrapperSpec) -> Result<TokenStream> {
    let WrapperSpec {
        attrs,
        name,
        params,
        output,
        abi,
    } = spec;

    let params = params
        .iter()
        .map(|param| {
            let name = &param.name;
            let ty = &param.ty;
            quote! { #name: #ty }
        })
        .collect::<Vec<_>>();

    let (ret_arrow, x86_body) = match output {
        Some(WrapperReturn { ty, reg }) => {
            let ret_ident: Ident = parse_quote!(__rsvz_asm_ret);
            let ret_expr: syn::Expr = parse_quote!(#ret_ident);
            let asm = asm::expand_abi_call(&abi, Some(CapturedReturn { reg, target: ret_expr }), name.span())?;
            (
                quote!(-> #ty),
                quote! {
                    let #ret_ident: #ty;
                    unsafe { #asm }
                    #ret_ident
                },
            )
        }
        None => {
            let asm = asm::expand_abi_call(&abi, None, name.span())?;
            (TokenStream::new(), quote! { unsafe { #asm } })
        }
    };

    Ok(quote! {
        #(#attrs)*
        #[inline(always)]
        pub unsafe fn #name(#(#params),*) #ret_arrow {
            #x86_body
        }
    })
}

pub(crate) fn expand_wrapper_list(spec: WrapperListSpec) -> Result<TokenStream> {
    spec.items.into_iter().map(expand_wrapper).collect()
}

#[cfg(test)]
mod tests {
    use quote::ToTokens;
    use syn::ItemMod;

    use crate::ir::{WrapperListSpec, WrapperSpec};

    use super::*;

    #[test]
    fn expands_void_wrapper() {
        let spec: WrapperSpec =
            syn::parse_str("board_update(board: *mut MainObject) { addr: 0x415d40, this: ecx = board }").unwrap();
        let tokens = expand_wrapper(spec).unwrap().to_string();
        assert!(tokens.contains("pub unsafe fn board_update"));
        assert!(tokens.contains("inline"));
        assert!(tokens.matches("unsafe").count() >= 2);
        assert!(!tokens.contains("cfg"));
        assert!(!tokens.contains("panic"));
    }

    #[test]
    fn expands_returning_wrapper() {
        let spec: WrapperSpec = syn::parse_str(
            "board_stage_has_pool(board: *mut MainObject) -> u8 { addr: 0x41c0d0, this: eax = board, ret: al }",
        )
        .unwrap();
        let tokens = expand_wrapper(spec).unwrap().to_string();
        assert!(tokens.contains("__rsvz_asm_ret"));
        assert!(tokens.contains("-> u8"));
        assert!(tokens.matches("unsafe").count() >= 2);
    }

    #[test]
    fn expands_ax_wrapper() {
        let spec: WrapperSpec =
            syn::parse_str("read_short(buf: *mut Buffer) -> i16 { addr: 0x5d67d0, this: ecx = buf, ret: ax }").unwrap();
        let tokens = expand_wrapper(spec).unwrap().to_string();
        assert!(tokens.contains("\"ax\""));
        assert!(tokens.contains("-> i16"));
    }

    #[test]
    fn expands_x87_wrapper_with_inferred_type() {
        let spec: WrapperSpec = syn::parse_str(
            "board_row_y(board: *mut MainObject, row: i32) -> f32 { addr: 0x41c6c0, this: ecx = board, regs: { eax = row }, ret: st0 }",
        )
        .unwrap();
        let tokens = expand_wrapper(spec).unwrap().to_string();
        assert!(tokens.contains("fstps"));
        assert!(tokens.contains("__rsvz_asm_ret"));
    }

    #[test]
    fn expands_x87_wrapper_with_explicit_hint() {
        let spec: WrapperSpec =
            syn::parse_str("curve_s(v: f64) -> MyFloat { addr: 0x5118c0, regs: { st0<f64> = v }, ret: st0<f64> }")
                .unwrap();
        let tokens = expand_wrapper(spec).unwrap().to_string();
        assert!(tokens.contains("fstpl"));
    }

    #[test]
    fn expands_multiple_wrappers_from_one_block() {
        let list: WrapperListSpec = syn::parse_str(
            "board_update(board: *mut MainObject) { addr: 0x415d40, this: ecx = board } board_stage_has_pool(board: *mut MainObject) -> u8 { addr: 0x41c0d0, this: eax = board, ret: al }",
        )
        .unwrap();
        let tokens = expand_wrapper_list(list).unwrap().to_string();
        assert!(tokens.contains("pub unsafe fn board_update"));
        assert!(tokens.contains("pub unsafe fn board_stage_has_pool"));
    }

    #[test]
    fn pre_new_game_wrapper_uses_macro_generated_hazard_free_asm() {
        let spec: WrapperSpec = syn::parse_str(
            "lawn_app_pre_new_game(look_for_saved_game: i32, game_mode: i32) { addr: 0x44f560, this: esi = [0x6a9ec0], stack: [look_for_saved_game, game_mode], clobber: [eax, ecx, edx] }",
        )
        .unwrap();
        let tokens = expand_wrapper(spec).unwrap().to_string();

        assert!(tokens.contains("pub unsafe fn lawn_app_pre_new_game"));
        assert!(tokens.contains("pushl %esi"));
        assert!(tokens.contains("popl %esi"));
        assert!(!tokens.contains("\"esi\""));
    }

    #[test]
    fn multiple_wrappers_keep_independent_operand_indices() {
        let list: WrapperListSpec = syn::parse_str(
            "first(app: usize, arg: i32) { addr: 0x44f560, this: esi = app, stack: [arg] } second(app: usize, arg: i32) { addr: 0x44f560, this: esi = app, stack: [arg] }",
        )
        .unwrap();
        let tokens = expand_wrapper_list(list).unwrap().to_string();

        assert_eq!(tokens.matches("__rsvz_asm_operand_0").count(), 4);
        assert!(tokens.contains("pub unsafe fn first"));
        assert!(tokens.contains("pub unsafe fn second"));
    }

    #[test]
    fn forwards_outer_attributes_to_generated_wrapper() {
        let spec: WrapperSpec = syn::parse_str(
            "#[doc = \"wrapper\"] #[expect(clippy::missing_safety_doc, reason = \"raw shim\")] board_update(board: *mut MainObject) { addr: 0x415d40, this: ecx = board }",
        )
        .unwrap();
        let tokens = expand_wrapper(spec).unwrap();
        let module: ItemMod = syn::parse2(quote! {
            mod generated {
                #tokens
            }
        })
        .unwrap();
        let (_, items) = module.content.unwrap();
        let funcs = items
            .into_iter()
            .map(|item| match item {
                syn::Item::Fn(func) => func,
                _ => panic!("expected generated functions"),
            })
            .collect::<Vec<_>>();
        assert_eq!(funcs.len(), 1);
        for item in funcs {
            assert_eq!(item.attrs.len(), 3);
            assert!(item.attrs[0].path().is_ident("doc"));
            assert!(item.attrs[1].path().is_ident("expect"));
            assert!(item.attrs[2].path().is_ident("inline"));
            assert_eq!(item.sig.ident, "board_update");
        }
    }

    #[test]
    fn forwards_doc_comments_across_multiple_wrappers() {
        let list: WrapperListSpec = syn::parse_str(
            "/// first\nboard_update(board: *mut MainObject) { addr: 0x415d40, this: ecx = board } /// second\nboard_stage_has_pool(board: *mut MainObject) -> u8 { addr: 0x41c0d0, this: eax = board, ret: al }",
        )
        .unwrap();
        let tokens = expand_wrapper_list(list).unwrap();
        let module: ItemMod = syn::parse2(quote! {
            mod generated {
                #tokens
            }
        })
        .unwrap();
        let (_, items) = module.content.unwrap();
        let docs = items
            .into_iter()
            .map(|item| match item {
                syn::Item::Fn(func) => func,
                _ => panic!("expected generated functions"),
            })
            .map(|func| {
                func.attrs
                    .into_iter()
                    .find(|attr| attr.path().is_ident("doc"))
                    .expect("doc attribute should be preserved")
                    .meta
                    .to_token_stream()
                    .to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(docs.len(), 2);
        assert!(docs[0].contains("first"));
        assert!(docs[1].contains("second"));
    }
}
