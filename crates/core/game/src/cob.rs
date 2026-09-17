//! Current cannon operations and explicitly shared cannon lists.

pub use crate::logic::cob::CobManager;
use crate::logic::cob::{CobSequentialMode, current_cob_manager};
use crate::runtime::{RuntimeError, RuntimeResult};

#[doc(hidden)]
pub fn report_fire(operation: &'static str, result: RuntimeResult<Option<i32>>) -> Option<i32> {
    match result {
        Ok(Some(index)) => Some(index),
        Ok(None) => {
            crate::diagnostics::report_operation_error(RuntimeError::new(format!(
                "{operation}失败：没有发射玉米加农炮"
            )));
            None
        }
        Err(error) => {
            crate::diagnostics::report_operation_error(error);
            None
        }
    }
}

#[doc(hidden)]
pub fn report_fire_many(operation: &'static str, result: RuntimeResult<Vec<Option<i32>>>) -> Vec<Option<i32>> {
    match result {
        Ok(indices) => {
            for (target, index) in indices.iter().enumerate() {
                if index.is_none() {
                    crate::diagnostics::report_operation_error(RuntimeError::new(format!(
                        "{operation}第 {} 个目标失败：没有发射玉米加农炮",
                        target + 1
                    )));
                }
            }
            indices
        }
        Err(error) => {
            crate::diagnostics::report_operation_error(error);
            Vec::new()
        }
    }
}

mod current;
pub use current::*;

#[doc(hidden)]
pub mod impact;
