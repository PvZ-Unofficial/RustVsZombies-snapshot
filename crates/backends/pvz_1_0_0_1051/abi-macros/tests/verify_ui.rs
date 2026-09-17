#![cfg(feature = "verify-exe")]

#[test]
#[cfg_attr(
    not(target_arch = "x86"),
    ignore = "ABI expansion tests require --target i686-pc-windows-msvc"
)]
fn missing_exe_is_an_error_without_hiding_generated_wrappers() {
    // SAFETY: this test controls the child rustc environment before spawning it.
    unsafe { std::env::remove_var("RSVZ_PVZ1051_EXE") };
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/verify/fail/missing_exe.rs");
}
