#[unsafe(no_mangle)]
pub extern "C" fn pvzp_plugin_abi_version() -> u32 {
    rsvz_pvz_portable_backend::plugin_abi_version()
}

#[unsafe(no_mangle)]
pub extern "C" fn pvzp_plugin_initialize() -> i32 {
    rsvz_pvz_portable_backend::plugin_initialize(user_script::__rsvz_dispatch)
}

#[unsafe(no_mangle)]
pub extern "C" fn pvzp_plugin_shutdown() -> i32 {
    rsvz_pvz_portable_backend::plugin_shutdown()
}
