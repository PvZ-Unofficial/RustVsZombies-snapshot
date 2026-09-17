use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type};

fn normalized_ident_text(ident: &Ident) -> String {
    ident.to_string().to_ascii_lowercase()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum GpReg {
    Eax,
    Ebx,
    Ecx,
    Edx,
    Esi,
    Edi,
}

impl GpReg {
    pub(crate) fn canonical_name(self) -> &'static str {
        match self {
            Self::Eax => "eax",
            Self::Ebx => "ebx",
            Self::Ecx => "ecx",
            Self::Edx => "edx",
            Self::Esi => "esi",
            Self::Edi => "edi",
        }
    }

    pub(crate) fn asm_name(self) -> &'static str {
        match self {
            Self::Eax => "%eax",
            Self::Ebx => "%ebx",
            Self::Ecx => "%ecx",
            Self::Edx => "%edx",
            Self::Esi => "%esi",
            Self::Edi => "%edi",
        }
    }

    pub(crate) fn is_callee_saved_i686(self) -> bool {
        matches!(self, Self::Ebx | Self::Esi | Self::Edi)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ByteReg {
    Al,
    Bl,
    Cl,
    Dl,
}

impl ByteReg {
    pub(crate) fn canonical_name(self) -> &'static str {
        match self {
            Self::Al => "al",
            Self::Bl => "bl",
            Self::Cl => "cl",
            Self::Dl => "dl",
        }
    }

    pub(crate) fn asm_name(self) -> &'static str {
        match self {
            Self::Al => "%al",
            Self::Bl => "%bl",
            Self::Cl => "%cl",
            Self::Dl => "%dl",
        }
    }

    pub(crate) fn carrier(self) -> GpReg {
        match self {
            Self::Al => GpReg::Eax,
            Self::Bl => GpReg::Ebx,
            Self::Cl => GpReg::Ecx,
            Self::Dl => GpReg::Edx,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum X87Reg {
    St0,
    St1,
}

impl X87Reg {
    pub(crate) fn canonical_name(self) -> &'static str {
        match self {
            Self::St0 => "st0",
            Self::St1 => "st1",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RegisterToken {
    Gp(GpReg),
    Ebp,
    Ax,
    Byte(ByteReg),
    X87(X87Reg),
    Esp,
}

impl RegisterToken {
    pub(crate) fn from_ident(ident: &Ident) -> Option<Self> {
        match normalized_ident_text(ident).as_str() {
            "eax" => Some(Self::Gp(GpReg::Eax)),
            "ebx" => Some(Self::Gp(GpReg::Ebx)),
            "ecx" => Some(Self::Gp(GpReg::Ecx)),
            "edx" => Some(Self::Gp(GpReg::Edx)),
            "esi" => Some(Self::Gp(GpReg::Esi)),
            "edi" => Some(Self::Gp(GpReg::Edi)),
            "ebp" => Some(Self::Ebp),
            "ax" => Some(Self::Ax),
            "al" => Some(Self::Byte(ByteReg::Al)),
            "bl" => Some(Self::Byte(ByteReg::Bl)),
            "cl" => Some(Self::Byte(ByteReg::Cl)),
            "dl" => Some(Self::Byte(ByteReg::Dl)),
            "st0" => Some(Self::X87(X87Reg::St0)),
            "st1" => Some(Self::X87(X87Reg::St1)),
            "esp" => Some(Self::Esp),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FloatKind {
    F32,
    F64,
}

impl FloatKind {
    pub(crate) fn from_type(ty: &Type) -> Option<Self> {
        let Type::Path(path) = ty else {
            return None;
        };

        if path.qself.is_some() {
            return None;
        }

        match path.path.segments.last()?.ident.to_string().as_str() {
            "f32" => Some(Self::F32),
            "f64" => Some(Self::F64),
            _ => None,
        }
    }

    pub(crate) fn rust_type_tokens(self) -> TokenStream {
        match self {
            Self::F32 => quote!(f32),
            Self::F64 => quote!(f64),
        }
    }

    pub(crate) fn zero_value(self) -> TokenStream {
        match self {
            Self::F32 => quote!(0.0f32),
            Self::F64 => quote!(0.0f64),
        }
    }

    pub(crate) fn load_mnemonic(self) -> &'static str {
        match self {
            Self::F32 => "flds",
            Self::F64 => "fldl",
        }
    }

    pub(crate) fn store_mnemonic(self) -> &'static str {
        match self {
            Self::F32 => "fstps",
            Self::F64 => "fstpl",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReturnReg {
    Gp(GpReg),
    Ax,
    Byte(ByteReg),
    X87(FloatKind),
}

impl ReturnReg {
    pub(crate) fn carrier(self) -> Option<GpReg> {
        match self {
            Self::Gp(reg) => Some(reg),
            Self::Ax => Some(GpReg::Eax),
            Self::Byte(reg) => Some(reg.carrier()),
            Self::X87(_) => None,
        }
    }

    pub(crate) fn explicit_name(self) -> Option<&'static str> {
        match self {
            Self::Gp(reg) => Some(reg.canonical_name()),
            Self::Ax => Some("ax"),
            Self::Byte(reg) => Some(reg.canonical_name()),
            Self::X87(_) => None,
        }
    }
}
