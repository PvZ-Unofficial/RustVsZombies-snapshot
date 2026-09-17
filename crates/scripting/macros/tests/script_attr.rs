#[test]
fn script_attribute_accepts_supported_forms() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/pass_plain_script.rs");
    cases.pass("tests/ui/pass_fallible_script.rs");
    cases.pass("tests/ui/pass_raw_escape_hatch.rs");
    cases.pass("tests/ui/pass_rsvz_path.rs");
    cases.pass("tests/ui/pass_auto_prelude.rs");
    cases.pass("tests/ui/pass_dsl_without_question_mark.rs");
    cases.pass("tests/ui/pass_raw_connect_escape_hatch.rs");
    cases.pass("tests/ui/pass_private_script_export.rs");
    cases.pass("tests/ui/pass_maid_cheats_helper_script.rs");
    cases.pass("tests/ui/pass_raw_maid_cheats_script.rs");
    cases.pass("tests/ui/pass_lowdsl_placement_constructors.rs");
    cases.pass("tests/ui/pass_unrelated_dancing_method.rs");
    cases.pass("tests/ui/pass_state_hooks.rs");
}

#[test]
fn script_attribute_rejects_unsupported_function_shapes() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail_params.rs");
    cases.compile_fail("tests/ui/fail_generics.rs");
    cases.compile_fail("tests/ui/fail_async.rs");
    cases.compile_fail("tests/ui/fail_unsafe.rs");
    cases.compile_fail("tests/ui/fail_extern.rs");
    cases.compile_fail("tests/ui/fail_const.rs");
    cases.compile_fail("tests/ui/fail_legacy_zombies_setup.rs");
    cases.compile_fail("tests/ui/fail_removed_script_options.rs");
    cases.compile_fail("tests/ui/fail_backend_options.rs");
    cases.compile_fail("tests/ui/fail_state_hooks_script_option.rs");
    cases.compile_fail("tests/ui/fail_duplicate_state_hooks.rs");
}
