#[cfg(not(feature = "verify-exe"))]
#[test]
#[cfg_attr(
    not(target_arch = "x86"),
    ignore = "ABI expansion tests require --target i686-pc-windows-msvc"
)]
fn ui() {
    let tests = trybuild::TestCases::new();
    tests.pass("tests/ui/pass/*.rs");
    tests.compile_fail("tests/ui/fail/*.rs");
}
