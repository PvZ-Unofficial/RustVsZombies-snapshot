use syn::{
    Result,
    parse::{Parse, ParseStream},
};

use crate::{
    ir::CallSpec,
    parse::abi::{ParsedAbiBody, ParsedReturn, parse_abi_body},
};

impl Parse for CallSpec {
    fn parse(input: ParseStream) -> Result<Self> {
        let span = input.span();
        let ParsedAbiBody { abi, ret } = parse_abi_body(input)?;
        Ok(Self {
            span,
            abi,
            ret: ret.map(ParsedReturn::into_call_return).transpose()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;

    use crate::{
        ir::{InputBinding, ValueSource},
        register::{ByteReg, FloatKind, ReturnReg, X87Reg},
    };

    fn parse_call(input: &str) -> CallSpec {
        syn::parse_str(input).unwrap()
    }

    fn assert_call_parse_err_contains(input: &str, expect_err: &str, needle: &str) {
        let err = syn::parse_str::<CallSpec>(input).err().expect(expect_err);
        assert!(err.to_string().contains(needle));
    }

    #[test]
    fn parses_minimal_block() {
        let spec = parse_call("addr: 0x415d40");
        assert_eq!(spec.abi.addr, 0x415d40);
        assert!(spec.abi.this.is_none());
    }

    #[test]
    fn parses_this_and_stack() {
        let spec = parse_call("addr: 0x411f20, this: ecx = board, stack: [click_count, y, x]");
        assert_eq!(spec.abi.stack.len(), 3);
        assert_eq!(spec.abi.this.as_ref().unwrap().reg, crate::register::GpReg::Ecx);
    }

    #[test]
    fn parses_chain_sources() {
        let spec = parse_call("addr: 0x5518f0, this: ecx = [0x6a9ec0], stack_this: [0x6a9ec0]");
        let ValueSource::PointerChain(chain) = &spec.abi.this.unwrap().source else {
            panic!("expected chain source");
        };
        assert_eq!(chain.base, 0x6a9ec0);
        assert!(chain.offsets.is_empty());

        let ValueSource::PointerChain(chain) = spec.abi.stack_this.unwrap() else {
            panic!("expected chain stack_this");
        };
        assert_eq!(chain.base, 0x6a9ec0);
    }

    #[test]
    fn parses_expr_stack_this() {
        let spec = parse_call("addr: 0x5518f0, stack_this: hidden_this");
        let ValueSource::Expr(expr) = spec.abi.stack_this.unwrap() else {
            panic!("expected expr stack_this");
        };
        assert_eq!(expr.to_token_stream().to_string(), "hidden_this");
    }

    #[test]
    fn parses_return_register_and_target() {
        let spec = parse_call("addr: 0x41c0d0, this: eax = board, ret: al => ret");
        let ret = spec.ret.unwrap();
        assert_eq!(ret.reg, ReturnReg::Byte(ByteReg::Al));
    }

    #[test]
    fn parses_ax_and_x87_registers() {
        let spec = parse_call("addr: 0x6398b0, regs: { st1<f64> = x, st0<f64> = y, al = flag }, ret: st0<f64> => out");
        assert!(matches!(
            spec.abi.inputs[0],
            InputBinding::X87 {
                reg: X87Reg::St1,
                kind: FloatKind::F64,
                ..
            }
        ));
        assert!(matches!(
            spec.abi.inputs[1],
            InputBinding::X87 {
                reg: X87Reg::St0,
                kind: FloatKind::F64,
                ..
            }
        ));
        assert!(matches!(
            spec.abi.inputs[2],
            InputBinding::Byte { reg: ByteReg::Al, .. }
        ));
        assert_eq!(spec.ret.unwrap().reg, ReturnReg::X87(FloatKind::F64));

        let spec = parse_call("addr: 0x5d67d0, ret: ax => out");
        assert_eq!(spec.ret.unwrap().reg, ReturnReg::Ax);
    }

    #[test]
    fn rejects_call_field() {
        assert_call_parse_err_contains(
            "addr: 1, call: eax",
            "call field should be rejected",
            "public ABI DSL no longer accepts `call:`",
        );
    }

    #[test]
    fn duplicates_are_rejected() {
        assert_call_parse_err_contains("addr: 1, addr: 2", "duplicate addr should fail", "duplicate addr");
    }

    #[test]
    fn raw_call_requires_target_for_ret() {
        assert_call_parse_err_contains(
            "addr: 0x41c0d0, ret: al",
            "ret without target should fail",
            "=> <place>",
        );
    }

    #[test]
    fn raw_call_rejects_untyped_x87_return() {
        assert_call_parse_err_contains(
            "addr: 0x41c6c0, ret: st0 => out",
            "untyped x87 return should fail",
            "st0<f32>",
        );
    }

    #[test]
    fn x87_bindings_require_st0() {
        assert_call_parse_err_contains(
            "addr: 0x6398b0, regs: { st1<f64> = x }",
            "st1 without st0 should fail",
            "st1 requires",
        );
    }

    #[test]
    fn aliasing_gp_and_byte_bindings_are_rejected() {
        assert_call_parse_err_contains(
            "addr: 1, this: eax = board, regs: { al = flag }",
            "aliased byte/gp bindings should fail",
            "conflicts",
        );
    }

    #[test]
    fn esp_is_rejected_in_clobber() {
        assert_call_parse_err_contains("addr: 1, clobber: [esp]", "esp clobber should fail", "general-purpose");
    }
}
