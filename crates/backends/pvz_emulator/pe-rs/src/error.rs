use std::fmt;

use crate::raw;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    NullPointer,
    NullHandle,
    InvalidArgument { detail: i32 },
    NotFound { detail: i32 },
    CppException { detail: i32 },
    BadAlloc,
    UnknownException,
    Bridge { code: i32, detail: i32 },
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NullPointer => f.write_str("PE bridge received a null pointer"),
            Self::NullHandle => f.write_str("PE bridge returned a null world handle"),
            Self::InvalidArgument { detail } => write!(f, "PE bridge invalid argument ({detail})"),
            Self::NotFound { detail } => write!(f, "PE bridge object not found ({detail})"),
            Self::CppException { detail } => {
                write!(f, "PE bridge caught std::exception ({detail})")
            }
            Self::BadAlloc => f.write_str("PE bridge caught std::bad_alloc"),
            Self::UnknownException => f.write_str("PE bridge caught an unknown C++ exception"),
            Self::Bridge { code, detail } => write!(f, "PE bridge error {code} ({detail})"),
        }
    }
}

impl std::error::Error for Error {}

impl Error {
    pub(crate) fn from_status(status: raw::pe_rs_status) -> Result<()> {
        match status.code {
            raw::PE_RS_STATUS_OK => Ok(()),
            raw::PE_RS_STATUS_NULL_POINTER => Err(Self::NullPointer),
            raw::PE_RS_STATUS_INVALID_ARGUMENT => Err(Self::InvalidArgument { detail: status.detail }),
            raw::PE_RS_STATUS_NOT_FOUND => Err(Self::NotFound { detail: status.detail }),
            raw::PE_RS_STATUS_STD_EXCEPTION => Err(Self::CppException { detail: status.detail }),
            raw::PE_RS_STATUS_BAD_ALLOC => Err(Self::BadAlloc),
            raw::PE_RS_STATUS_UNKNOWN_EXCEPTION => Err(Self::UnknownException),
            code => Err(Self::Bridge {
                code,
                detail: status.detail,
            }),
        }
    }
}
