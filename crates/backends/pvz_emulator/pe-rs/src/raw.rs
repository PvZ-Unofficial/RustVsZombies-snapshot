//! Generated bindings to PE's real object layouts and the narrow RSVZ-owned C bridge.

#![allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unsafe_code,
    clippy::missing_safety_doc,
    reason = "bindgen-generated C declarations inherit safety contracts from the bridge"
)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

pub const PE_RS_STATUS_OK: i32 = pe_rs_status_code_PE_RS_STATUS_OK;
pub const PE_RS_STATUS_NULL_POINTER: i32 = pe_rs_status_code_PE_RS_STATUS_NULL_POINTER;
pub const PE_RS_STATUS_INVALID_ARGUMENT: i32 = pe_rs_status_code_PE_RS_STATUS_INVALID_ARGUMENT;
pub const PE_RS_STATUS_NOT_FOUND: i32 = pe_rs_status_code_PE_RS_STATUS_NOT_FOUND;
pub const PE_RS_STATUS_STD_EXCEPTION: i32 = pe_rs_status_code_PE_RS_STATUS_STD_EXCEPTION;
pub const PE_RS_STATUS_BAD_ALLOC: i32 = pe_rs_status_code_PE_RS_STATUS_BAD_ALLOC;
pub const PE_RS_STATUS_UNKNOWN_EXCEPTION: i32 = pe_rs_status_code_PE_RS_STATUS_UNKNOWN_EXCEPTION;
