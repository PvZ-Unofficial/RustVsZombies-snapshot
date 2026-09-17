use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::Span;
use syn::{
    Expr, Ident, LitInt, Result, Token, Type, braced, bracketed,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

use crate::{
    ir::{AbiSpec, BindingSlot, CapturedReturn, GpBinding, InputBinding, PointerChain, ValueSource},
    register::{ByteReg, FloatKind, GpReg, RegisterToken, ReturnReg, X87Reg},
};

macro_rules! define_fields {
    ($($variant:ident => $kw:ident as $label:literal),* $(,)?) => {
        mod keyword {
            syn::custom_keyword!(call);
            $(syn::custom_keyword!($kw);)*
        }

        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        enum FieldName {
            $($variant,)*
        }

        impl FieldName {
            fn parse(input: ParseStream) -> Result<Self> {
                if input.peek(keyword::call) {
                    let span = input.span();
                    input.parse::<keyword::call>()?;
                    return Err(syn::Error::new(
                        span,
                        "public ABI DSL no longer accepts `call:`; the macro selects the call-target scratch register internally",
                    ));
                }
                $(if input.peek(keyword::$kw) {
                    input.parse::<keyword::$kw>()?;
                    return Ok(Self::$variant);
                })*
                Err(input.error("unknown field"))
            }

            fn label(self) -> &'static str {
                match self {
                    $(Self::$variant => $label,)*
                }
            }
        }
    };
}

define_fields! {
    Addr      => addr      as "addr",
    This      => this      as "this",
    Regs      => regs      as "regs",
    Stack     => stack     as "stack",
    StackThis => stack_this as "stack_this",
    Ret       => ret       as "ret",
    Cleanup   => cleanup   as "cleanup",
    Clobber   => clobber   as "clobber",
}

#[derive(Clone)]
pub(crate) struct ParsedAbiBody {
    pub abi: AbiSpec,
    pub ret: Option<ParsedReturn>,
}

#[derive(Clone)]
pub(crate) struct ParsedReturn {
    kind: ParsedReturnKind,
    target: Option<Expr>,
    span: Span,
}

#[derive(Clone, Copy)]
enum ParsedReturnKind {
    Gp(GpReg),
    Ax,
    Byte(ByteReg),
    X87(Option<FloatKind>),
}

impl ParsedReturn {
    pub(crate) fn span(&self) -> Span {
        self.span
    }

    pub(crate) fn into_call_return(self) -> Result<CapturedReturn> {
        let target = self.target.ok_or_else(|| {
            syn::Error::new(
                self.span,
                "pvz_abi_call! requires `ret: <reg> => <place>` when a return value is declared",
            )
        })?;

        let reg = match self.kind {
            ParsedReturnKind::Gp(reg) => ReturnReg::Gp(reg),
            ParsedReturnKind::Ax => ReturnReg::Ax,
            ParsedReturnKind::Byte(reg) => ReturnReg::Byte(reg),
            ParsedReturnKind::X87(Some(kind)) => ReturnReg::X87(kind),
            ParsedReturnKind::X87(None) => {
                return Err(syn::Error::new(
                    self.span,
                    "pvz_abi_call! requires `ret: st0<f32> => place` or `ret: st0<f64> => place` for x87 returns",
                ));
            }
        };

        Ok(CapturedReturn { reg, target })
    }

    pub(crate) fn into_wrapper_return(self, wrapper_ret_ty: &Type) -> Result<ReturnReg> {
        if self.target.is_some() {
            return Err(syn::Error::new(
                self.span,
                "pvz_abi_fn! uses `ret: <reg>` without `=> <place>`; the wrapper captures the return value for you",
            ));
        }

        match self.kind {
            ParsedReturnKind::Gp(reg) => Ok(ReturnReg::Gp(reg)),
            ParsedReturnKind::Ax => Ok(ReturnReg::Ax),
            ParsedReturnKind::Byte(reg) => Ok(ReturnReg::Byte(reg)),
            ParsedReturnKind::X87(Some(kind)) => {
                let inferred = FloatKind::from_type(wrapper_ret_ty);
                if inferred.is_some_and(|inferred| inferred != kind) {
                    return Err(syn::Error::new(
                        self.span,
                        "the wrapper return type does not match the x87 return hint",
                    ));
                }
                Ok(ReturnReg::X87(kind))
            }
            ParsedReturnKind::X87(None) => {
                let Some(kind) = FloatKind::from_type(wrapper_ret_ty) else {
                    return Err(syn::Error::new(
                        self.span,
                        "x87 returns require `ret: st0<f32>` or `ret: st0<f64>` unless the wrapper return type is literally `f32` or `f64`",
                    ));
                };
                Ok(ReturnReg::X87(kind))
            }
        }
    }
}

impl Parse for ValueSource {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(syn::token::Bracket) {
            Ok(Self::PointerChain(parse_pointer_chain(input)?))
        } else {
            Ok(Self::Expr(input.parse()?))
        }
    }
}

pub(crate) fn parse_abi_body(input: ParseStream) -> Result<ParsedAbiBody> {
    let mut addr = None;
    let mut this = None;
    let mut inputs = Vec::new();
    let mut stack = Vec::new();
    let mut stack_this = None;
    let mut ret = None;
    let mut cleanup = 0;
    let mut clobbers = Vec::new();
    let mut seen = BTreeSet::new();

    while !input.is_empty() {
        let field_span = input.span();
        let field = FieldName::parse(input)?;
        input.parse::<Token![:]>()?;

        if !seen.insert(field) {
            return Err(syn::Error::new(
                field_span,
                format!("duplicate {} field", field.label()),
            ));
        }

        match field {
            FieldName::Addr => {
                let lit: LitInt = input.parse()?;
                addr = Some(parse_u32(&lit)?);
            }
            FieldName::This => {
                this = Some(parse_this_binding(input)?);
            }
            FieldName::Regs => {
                inputs = parse_input_bindings(input)?;
            }
            FieldName::Stack => {
                stack = parse_stack(input)?;
            }
            FieldName::StackThis => {
                stack_this = Some(input.parse()?);
            }
            FieldName::Ret => {
                ret = Some(parse_return(input)?);
            }
            FieldName::Cleanup => {
                let lit: LitInt = input.parse()?;
                cleanup = parse_u32(&lit)?;
            }
            FieldName::Clobber => {
                clobbers = parse_clobbers(input)?;
            }
        }

        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
    }

    validate_binding_slots(this.as_ref(), &inputs)?;

    Ok(ParsedAbiBody {
        abi: AbiSpec {
            addr: addr.ok_or_else(|| input.error("missing addr field"))?,
            this: this.map(|binding| binding.binding),
            inputs: inputs.into_iter().map(|binding| binding.binding).collect(),
            stack,
            stack_this,
            cleanup,
            clobbers,
        },
        ret,
    })
}

struct ParsedThisBinding {
    binding: GpBinding,
    span: Span,
}

struct ParsedInputBinding {
    binding: InputBinding,
    span: Span,
}

impl Parse for ParsedInputBinding {
    fn parse(input: ParseStream) -> Result<Self> {
        parse_input_binding(input)
    }
}

fn parse_this_binding(input: ParseStream) -> Result<ParsedThisBinding> {
    let (token, span) = parse_register_token(input)?;
    let reg = match token {
        RegisterToken::Gp(reg) => reg,
        RegisterToken::Ebp => return reject_ebp(span, "the `this` binding"),
        _ => {
            return Err(syn::Error::new(
                span,
                "this requires a 32-bit general-purpose register other than esp",
            ));
        }
    };

    reject_float_hint(parse_float_hint(input)?, span)?;
    input.parse::<Token![=]>()?;
    let source = input.parse()?;
    Ok(ParsedThisBinding {
        binding: GpBinding { reg, source },
        span,
    })
}

fn parse_input_binding(input: ParseStream) -> Result<ParsedInputBinding> {
    let (token, span) = parse_register_token(input)?;
    let float = parse_float_hint(input)?;
    input.parse::<Token![=]>()?;
    let source = input.parse()?;

    let binding = match token {
        RegisterToken::Gp(reg) => {
            reject_float_hint(float, span)?;
            InputBinding::Gp(GpBinding { reg, source })
        }
        RegisterToken::Ebp => return reject_ebp(span, "a `regs` binding"),
        RegisterToken::Byte(reg) => {
            reject_float_hint(float, span)?;
            InputBinding::Byte { reg, source }
        }
        RegisterToken::X87(reg) => {
            let kind = float.ok_or_else(|| {
                syn::Error::new(
                    span,
                    "x87 register bindings require an explicit `<f32>` or `<f64>` type hint",
                )
            })?;
            InputBinding::X87 { reg, kind, source }
        }
        RegisterToken::Ax | RegisterToken::Esp => {
            return Err(syn::Error::new(
                span,
                "regs only supports 32-bit general-purpose registers other than esp, low-byte registers, or x87 registers",
            ));
        }
    };

    Ok(ParsedInputBinding { binding, span })
}

fn parse_input_bindings(input: ParseStream) -> Result<Vec<ParsedInputBinding>> {
    let content;
    braced!(content in input);
    Ok(Punctuated::<ParsedInputBinding, Token![,]>::parse_terminated(&content)?
        .into_iter()
        .collect())
}

fn parse_stack(input: ParseStream) -> Result<Vec<Expr>> {
    let content;
    bracketed!(content in input);
    let list: Punctuated<Expr, Token![,]> = Punctuated::parse_terminated(&content)?;
    Ok(list.into_iter().collect())
}

fn parse_return(input: ParseStream) -> Result<ParsedReturn> {
    let (token, span) = parse_register_token(input)?;
    let float = parse_float_hint(input)?;
    let kind = match token {
        RegisterToken::Gp(reg) => {
            reject_float_hint(float, span)?;
            ParsedReturnKind::Gp(reg)
        }
        RegisterToken::Ebp => return reject_ebp(span, "a `ret` binding"),
        RegisterToken::Ax => {
            reject_float_hint(float, span)?;
            ParsedReturnKind::Ax
        }
        RegisterToken::Byte(reg) => {
            reject_float_hint(float, span)?;
            ParsedReturnKind::Byte(reg)
        }
        RegisterToken::X87(X87Reg::St0) => ParsedReturnKind::X87(float),
        RegisterToken::X87(X87Reg::St1) | RegisterToken::Esp => {
            return Err(syn::Error::new(
                span,
                "ret only supports 32-bit general-purpose registers other than esp, ax, low-byte registers, or st0",
            ));
        }
    };

    let target = if input.peek(Token![=>]) {
        input.parse::<Token![=>]>()?;
        Some(input.parse()?)
    } else {
        None
    };

    Ok(ParsedReturn { kind, target, span })
}

fn parse_clobbers(input: ParseStream) -> Result<Vec<GpReg>> {
    let content;
    bracketed!(content in input);
    let list: Punctuated<Ident, Token![,]> = Punctuated::parse_terminated(&content)?;
    let mut seen = BTreeSet::new();
    let mut regs = Vec::new();

    for ident in list {
        let span = ident.span();
        let reg = match RegisterToken::from_ident(&ident) {
            Some(RegisterToken::Gp(reg)) => reg,
            Some(RegisterToken::Ebp) => return reject_ebp(span, "the `clobber` list"),
            _ => {
                return Err(syn::Error::new(
                    span,
                    "clobber only supports 32-bit general-purpose registers other than esp",
                ));
            }
        };
        if !seen.insert(reg) {
            return Err(syn::Error::new(
                span,
                format!("duplicate clobber register `{}`", reg.canonical_name()),
            ));
        }
        regs.push(reg);
    }

    Ok(regs)
}

fn parse_pointer_chain(input: ParseStream) -> Result<PointerChain> {
    let content;
    bracketed!(content in input);
    let mut numbers = Punctuated::<LitInt, Token![,]>::parse_terminated(&content)?.into_iter();
    let Some(base) = numbers.next() else {
        return Err(content.error("chain cannot be empty"));
    };
    Ok(PointerChain {
        base: parse_u32(&base)?,
        offsets: numbers
            .map(|offset| parse_u32(&offset))
            .collect::<Result<Vec<_>>>()?
            .into_boxed_slice(),
    })
}

fn parse_register_token(input: ParseStream) -> Result<(RegisterToken, Span)> {
    let ident: Ident = input.parse()?;
    let span = ident.span();
    let token = RegisterToken::from_ident(&ident).ok_or_else(|| syn::Error::new(span, "unknown register"))?;
    Ok((token, span))
}

fn parse_float_hint(input: ParseStream) -> Result<Option<FloatKind>> {
    if !input.peek(Token![<]) {
        return Ok(None);
    }

    input.parse::<Token![<]>()?;
    let ty: Type = input.parse()?;
    input.parse::<Token![>]>()?;
    let kind = FloatKind::from_type(&ty)
        .ok_or_else(|| syn::Error::new_spanned(ty, "x87 register types must be `f32` or `f64`"))?;
    Ok(Some(kind))
}

fn reject_float_hint(float: Option<FloatKind>, span: Span) -> Result<()> {
    if float.is_some() {
        return Err(syn::Error::new(
            span,
            "only x87 registers accept a `<f32>` or `<f64>` type hint",
        ));
    }
    Ok(())
}

fn reject_ebp<T>(span: Span, role: &str) -> Result<T> {
    Err(syn::Error::new(
        span,
        format!(
            "`ebp` is not supported in {role}: the 1051 calling-convention table has no `ebp = ...` ABI carrier, and `ebp` is an i686 frame/base pointer register. If a future ABI really needs `ebp`, evaluate a dedicated trampoline or global asm wrapper instead of the generic macro",
        ),
    ))
}

fn parse_u32(lit: &LitInt) -> Result<u32> {
    let text = lit.to_string();
    let parsed = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .map_or_else(|| text.parse(), |rest| u32::from_str_radix(rest, 16));

    parsed.map_err(|err| syn::Error::new_spanned(lit, format!("invalid integer literal: {err}")))
}

fn validate_binding_slots(this: Option<&ParsedThisBinding>, inputs: &[ParsedInputBinding]) -> Result<()> {
    let mut used = BTreeMap::<BindingSlot, String>::new();

    if let Some(this) = this {
        reserve_slot(
            &mut used,
            this.binding.slot(),
            this.binding.canonical_name(),
            this.span,
            format!("the `this` binding `{}`", this.binding.canonical_name()),
        )?;
    }

    let mut has_st0 = false;
    let mut st1_span = None;
    for input in inputs {
        match input.binding.slot() {
            BindingSlot::X87(X87Reg::St0) => has_st0 = true,
            BindingSlot::X87(X87Reg::St1) => st1_span = Some(input.span),
            _ => {}
        }

        reserve_slot(
            &mut used,
            input.binding.slot(),
            input.binding.canonical_name(),
            input.span,
            format!("the `regs` binding `{}`", input.binding.canonical_name()),
        )?;
    }

    if let Some(span) = st1_span
        && !has_st0
    {
        return Err(syn::Error::new(
            span,
            "st1 requires an accompanying st0 binding so the x87 stack is well-formed",
        ));
    }

    Ok(())
}

fn reserve_slot(
    used: &mut BTreeMap<BindingSlot, String>, slot: BindingSlot, raw_name: &'static str, span: Span,
    description: String,
) -> Result<()> {
    if let Some(existing) = used.get(&slot) {
        return Err(syn::Error::new(
            span,
            format!("binding for `{raw_name}` conflicts with {existing}"),
        ));
    }

    used.insert(slot, description);
    Ok(())
}
