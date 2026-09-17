use proc_macro2::Span;
use syn::{Attribute, Expr, Ident, Type};

use crate::register::{ByteReg, FloatKind, GpReg, ReturnReg, X87Reg};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PointerChain {
    pub base: u32,
    pub offsets: Box<[u32]>,
}

#[derive(Clone)]
pub(crate) enum ValueSource {
    Expr(Expr),
    PointerChain(PointerChain),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum BindingSlot {
    Gp(GpReg),
    X87(X87Reg),
}

#[derive(Clone)]
pub(crate) struct GpBinding {
    pub reg: GpReg,
    pub source: ValueSource,
}

impl GpBinding {
    pub(crate) fn slot(&self) -> BindingSlot {
        BindingSlot::Gp(self.reg)
    }

    pub(crate) fn canonical_name(&self) -> &'static str {
        self.reg.canonical_name()
    }
}

#[derive(Clone)]
pub(crate) enum InputBinding {
    Gp(GpBinding),
    Byte {
        reg: ByteReg,
        source: ValueSource,
    },
    X87 {
        reg: X87Reg,
        kind: FloatKind,
        source: ValueSource,
    },
}

impl InputBinding {
    pub(crate) fn slot(&self) -> BindingSlot {
        match self {
            Self::Gp(binding) => binding.slot(),
            Self::Byte { reg, .. } => BindingSlot::Gp(reg.carrier()),
            Self::X87 { reg, .. } => BindingSlot::X87(*reg),
        }
    }

    pub(crate) fn canonical_name(&self) -> &'static str {
        match self {
            Self::Gp(binding) => binding.canonical_name(),
            Self::Byte { reg, .. } => reg.canonical_name(),
            Self::X87 { reg, .. } => reg.canonical_name(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct AbiSpec {
    pub addr: u32,
    pub this: Option<GpBinding>,
    pub inputs: Vec<InputBinding>,
    pub stack: Vec<Expr>,
    pub stack_this: Option<ValueSource>,
    pub cleanup: u32,
    pub clobbers: Vec<GpReg>,
}

#[derive(Clone)]
pub(crate) struct CapturedReturn {
    pub reg: ReturnReg,
    pub target: Expr,
}

#[derive(Clone)]
pub(crate) struct CallSpec {
    pub span: Span,
    pub abi: AbiSpec,
    pub ret: Option<CapturedReturn>,
}

#[derive(Clone)]
pub(crate) struct WrapperParam {
    pub name: Ident,
    pub ty: Type,
}

#[derive(Clone)]
pub(crate) struct WrapperReturn {
    pub ty: Type,
    pub reg: ReturnReg,
}

#[derive(Clone)]
pub(crate) struct WrapperSpec {
    pub attrs: Vec<Attribute>,
    pub name: Ident,
    pub params: Vec<WrapperParam>,
    pub output: Option<WrapperReturn>,
    pub abi: AbiSpec,
}

#[derive(Clone)]
pub(crate) struct WrapperListSpec {
    pub items: Vec<WrapperSpec>,
}
