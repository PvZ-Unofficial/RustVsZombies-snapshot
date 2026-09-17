//! Bindgen declarations for PvZ-Portable's real object layouts and narrow bridge.

#![allow(
    dead_code,
    improper_ctypes,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    clippy::allow_attributes_without_reason,
    clippy::derive_partial_eq_without_eq,
    clippy::missing_safety_doc,
    reason = "bindgen-generated declarations inherit their safety contracts from pvzp-rs"
)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

pub const STATUS_OK: i32 = pvzp_rs_status_code_PVZP_RS_STATUS_OK;
pub const STATUS_NULL_POINTER: i32 = pvzp_rs_status_code_PVZP_RS_STATUS_NULL_POINTER;
pub const STATUS_INVALID_ARGUMENT: i32 = pvzp_rs_status_code_PVZP_RS_STATUS_INVALID_ARGUMENT;
pub const STATUS_NOT_FOUND: i32 = pvzp_rs_status_code_PVZP_RS_STATUS_NOT_FOUND;
pub const STATUS_UNSUPPORTED: i32 = pvzp_rs_status_code_PVZP_RS_STATUS_UNSUPPORTED;
pub const STATUS_STD_EXCEPTION: i32 = pvzp_rs_status_code_PVZP_RS_STATUS_STD_EXCEPTION;
pub const STATUS_BAD_ALLOC: i32 = pvzp_rs_status_code_PVZP_RS_STATUS_BAD_ALLOC;
pub const STATUS_UNKNOWN_EXCEPTION: i32 = pvzp_rs_status_code_PVZP_RS_STATUS_UNKNOWN_EXCEPTION;
