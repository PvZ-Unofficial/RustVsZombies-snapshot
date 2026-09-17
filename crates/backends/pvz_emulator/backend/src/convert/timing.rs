use crate::{PeBackendError, Result};

pub(crate) fn i32_from_u32(field: &'static str, value: u32) -> Result<i32> {
    i32::try_from(value).map_err(|_err| PeBackendError::NumericOutOfRange(field))
}
