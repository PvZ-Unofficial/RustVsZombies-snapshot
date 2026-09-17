use std::ffi::c_void;

#[unsafe(no_mangle)]
pub extern "system" fn rsvz_initialize(context: *mut c_void) -> u32 {
    rsvz_backend_1051::host::initialize(context, user_script::__rsvz_dispatch)
}

#[unsafe(no_mangle)]
pub extern "system" fn rsvz_request_unload(context: *mut c_void) -> u32 {
    rsvz_backend_1051::host::request_unload(context)
}
