use crate::raw;

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("null pointer passed across the PvZ-Portable bridge")]
    NullPointer,
    #[error("invalid PvZ-Portable bridge argument ({detail})")]
    InvalidArgument { detail: i32 },
    #[error("PvZ-Portable object not found ({detail})")]
    NotFound { detail: i32 },
    #[error("unsupported PvZ-Portable operation ({detail})")]
    Unsupported { detail: i32 },
    #[error("PvZ-Portable threw a C++ exception ({detail})")]
    CppException { detail: i32 },
    #[error("PvZ-Portable allocation failed")]
    BadAlloc,
    #[error("PvZ-Portable threw an unknown exception")]
    UnknownException,
    #[error("unknown PvZ-Portable bridge status {code} ({detail})")]
    UnknownStatus { code: i32, detail: i32 },
    #[error("PvZ-Portable native and Rust object layouts do not match")]
    LayoutMismatch,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn from_status(status: raw::pvzp_rs_status) -> Result<()> {
        match status.code {
            raw::STATUS_OK => Ok(()),
            raw::STATUS_NULL_POINTER => Err(Self::NullPointer),
            raw::STATUS_INVALID_ARGUMENT => Err(Self::InvalidArgument { detail: status.detail }),
            raw::STATUS_NOT_FOUND => Err(Self::NotFound { detail: status.detail }),
            raw::STATUS_UNSUPPORTED => Err(Self::Unsupported { detail: status.detail }),
            raw::STATUS_STD_EXCEPTION => Err(Self::CppException { detail: status.detail }),
            raw::STATUS_BAD_ALLOC => Err(Self::BadAlloc),
            raw::STATUS_UNKNOWN_EXCEPTION => Err(Self::UnknownException),
            code => Err(Self::UnknownStatus {
                code,
                detail: status.detail,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_every_versioned_status_code() {
        let cases = [
            (raw::STATUS_NULL_POINTER, Error::NullPointer),
            (raw::STATUS_INVALID_ARGUMENT, Error::InvalidArgument { detail: 17 }),
            (raw::STATUS_NOT_FOUND, Error::NotFound { detail: 17 }),
            (raw::STATUS_UNSUPPORTED, Error::Unsupported { detail: 17 }),
            (raw::STATUS_STD_EXCEPTION, Error::CppException { detail: 17 }),
            (raw::STATUS_BAD_ALLOC, Error::BadAlloc),
            (raw::STATUS_UNKNOWN_EXCEPTION, Error::UnknownException),
        ];
        assert_eq!(
            Error::from_status(raw::pvzp_rs_status {
                code: raw::STATUS_OK,
                detail: 17
            }),
            Ok(())
        );
        for (code, expected) in cases {
            assert_eq!(
                Error::from_status(raw::pvzp_rs_status { code, detail: 17 }),
                Err(expected)
            );
        }
        assert_eq!(
            Error::from_status(raw::pvzp_rs_status { code: 999, detail: 17 }),
            Err(Error::UnknownStatus { code: 999, detail: 17 })
        );
    }
}
