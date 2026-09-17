//! Helpers for precise plant effect timing.

/// Error returned by precise active-time helpers.
#[derive(Debug, thiserror::Error)]
pub enum ActiveTimeError {
    #[error("active-time delay must be at least 10 frames")]
    InvalidDelay,
}

pub fn active_time_normalize_delay(delay: i32) -> Result<i32, ActiveTimeError> {
    if delay < 10 {
        return Err(ActiveTimeError::InvalidDelay);
    }
    Ok(delay - 10)
}
