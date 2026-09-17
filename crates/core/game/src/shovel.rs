//! Current semantic shovel operations.
use crate::logic::shovel::{IntoShovelOp, IntoShovelTarget, ShovelContext};
use rsvz_current::CurrentBackend;

pub fn shovel(row: i32, col: i32)
where
    CurrentBackend: ShovelContext,
{
    if let Err(error) = try_shovel(row, col) {
        crate::diagnostics::report_operation_error(error);
    }
}

pub fn shovel_target<T>(row: i32, col: i32, target: T)
where
    CurrentBackend: ShovelContext,
    T: IntoShovelTarget,
{
    if let Err(error) = try_shovel_target(row, col, target) {
        crate::diagnostics::report_operation_error(error);
    }
}

pub fn shovel_ops<I, O>(ops: I)
where
    CurrentBackend: ShovelContext,
    I: IntoIterator<Item = O>,
    O: IntoShovelOp,
{
    if let Err(error) = try_shovel_ops(ops) {
        crate::diagnostics::report_operation_error(error);
    }
}

pub use crate::logic::shovel::{
    shovel as try_shovel, shovel_ops as try_shovel_ops, shovel_target as try_shovel_target,
};
