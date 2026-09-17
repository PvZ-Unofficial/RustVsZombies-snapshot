use super::*;
use crate::ir::CallSpec;

const BASE: u32 = 0x401000;

fn check(bytes: &[u8], declaration: &str) -> Report {
    let spec: CallSpec = syn::parse_str(declaration).unwrap();
    analyze(
        &TextImage::synthetic(BASE, bytes),
        &spec.abi,
        spec.ret.as_ref().map(|ret| ret.reg),
        &mut AnalysisCache::default(),
    )
}

#[test]
fn follows_branches_and_checks_all_return_cleanup() {
    let report = check(
        b"\x85\xC9\x74\x03\xC2\x04\x00\xC2\x04\x00",
        "addr: 0x401000, this: ecx = x, stack: [x]",
    );
    assert!(report.contradictions.is_empty(), "{:?}", report.contradictions);
}

#[test]
fn follows_direct_tail_jumps() {
    let report = check(
        b"\xE9\x00\x00\x00\x00\x8B\x44\x24\x04\xC2\x04\x00",
        "addr: 0x401000, stack: [x], ret: eax => out",
    );
    assert!(report.contradictions.is_empty(), "{:?}", report.contradictions);
}

#[test]
fn tracks_ebp_stack_frames() {
    let report = check(
        b"\x55\x8B\xEC\x8B\x45\x08\x5D\xC2\x04\x00",
        "addr: 0x401000, stack: [x], ret: eax => out",
    );
    assert!(report.contradictions.is_empty(), "{:?}", report.contradictions);
    assert!(!report.unknown.iter().any(|item| item.contains("stack slots")));
}

#[test]
fn restored_push_pop_register_is_not_a_missing_clobber() {
    let report = check(b"\x53\xBB\x01\x00\x00\x00\x5B\xC3", "addr: 0x401000");
    assert!(report.contradictions.is_empty(), "{:?}", report.contradictions);
}

#[test]
fn detects_extra_stack_slots_and_different_return_cleanup() {
    let extra = check(
        b"\x8B\x44\x24\x08\xC2\x04\x00",
        "addr: 0x401000, stack: [x], ret: eax => out",
    );
    assert!(extra.contradictions.iter().any(|item| item.contains("[esp+0x8]")));

    let cleanup = check(
        b"\x85\xC9\x74\x03\xC2\x04\x00\xC2\x08\x00",
        "addr: 0x401000, this: ecx = x, stack: [x]",
    );
    assert!(cleanup.contradictions.iter().any(|item| item.contains("cleans 0x8")));
}

#[test]
fn return_width_is_directional() {
    let eax_to_al = check(b"\xB8\x01\x00\x00\x00\xC3", "addr: 0x401000, ret: al => out");
    assert!(eax_to_al.contradictions.is_empty(), "{:?}", eax_to_al.contradictions);

    let al_to_eax = check(b"\xB0\x01\xC3", "addr: 0x401000, ret: eax => out");
    assert!(al_to_eax.contradictions.iter().any(|item| item.contains("eax return")));
}

#[test]
fn recognizes_st0_returns() {
    let report = check(b"\xD9\xE8\xC3", "addr: 0x401000, ret: st0<f32> => out");
    assert!(report.contradictions.is_empty(), "{:?}", report.contradictions);
}

#[test]
fn constant_register_idioms_do_not_require_an_input() {
    let report = check(b"\x83\xCF\xFF\xC3", "addr: 0x401000, clobber: [edi]");
    assert!(report.contradictions.is_empty(), "{:?}", report.contradictions);
}

#[test]
fn partial_writes_preserve_unwritten_input_bits() {
    let report = check(b"\xB0\x01\x85\xC0\xC3", "addr: 0x401000, clobber: [eax]");
    assert!(report.contradictions.iter().any(|item| item.contains("reads eax")));
}

#[test]
fn identity_writes_preserve_registers() {
    let report = check(b"\x21\xC0\xC3", "addr: 0x401000, regs: { eax = x }");
    assert!(report.contradictions.is_empty(), "{:?}", report.contradictions);

    for bytes in [b"\x8B\xFF\xC3".as_slice(), b"\x8D\x3F\xC3".as_slice()] {
        let nop = check(bytes, "addr: 0x401000");
        assert!(nop.contradictions.is_empty(), "{:?}", nop.contradictions);
    }

    let flag_setting = check(b"\x21\xC0\xC3", "addr: 0x401000");
    assert!(
        flag_setting
            .contradictions
            .iter()
            .any(|item| item.contains("reads eax"))
    );
}

#[test]
fn wide_stack_reads_cover_every_word() {
    let report = check(
        b"\xDD\x44\x24\x04\xC2\x04\x00",
        "addr: 0x401000, stack: [x], ret: st0<f64> => out",
    );
    assert!(report.contradictions.iter().any(|item| item.contains("[esp+0x8]")));

    let pushed = check(b"\xFF\x74\x24\x04\xC2\x04\x00", "addr: 0x401000, stack: [x]");
    assert!(!pushed.unknown.iter().any(|item| item.contains("stack slots")));
}

#[test]
fn direct_call_summaries_do_not_invent_defined_returns() {
    let report = check(
        b"\xE8\x01\x00\x00\x00\xC3\xBE\x01\x00\x00\x00\xC3",
        "addr: 0x401000, clobber: [esi], ret: esi => out",
    );
    assert!(report.contradictions.is_empty(), "{:?}", report.contradictions);
    assert!(report.unknown.iter().any(|item| item.contains("esi return")));

    let partial = check(
        b"\xE8\x01\x00\x00\x00\xC3\xB0\x01\xC3",
        "addr: 0x401000, ret: eax => out",
    );
    assert!(partial.unknown.iter().any(|item| item.contains("eax return")));

    let wide = check(
        b"\xE8\x01\x00\x00\x00\xC3\xB8\x01\x00\x00\x00\xC3",
        "addr: 0x401000, ret: al => out",
    );
    assert!(wide.contradictions.is_empty(), "{:?}", wide.contradictions);
    assert!(wide.unknown.iter().any(|item| item.contains("eax return")));

    for dependent in [
        b"\xE8\x01\x00\x00\x00\xC3\x83\xC0\x01\xC3".as_slice(),
        b"\xE8\x01\x00\x00\x00\xC3\x0F\xB6\xC1\xC3".as_slice(),
    ] {
        let report = check(dependent, "addr: 0x401000, ret: eax => out");
        assert!(report.unknown.iter().any(|item| item.contains("eax return")));
    }

    for defined_at_call in [
        b"\xB8\x01\x00\x00\x00\xE8\x01\x00\x00\x00\xC3\x83\xC0\x01\xC3".as_slice(),
        b"\xB9\x01\x00\x00\x00\xE8\x01\x00\x00\x00\xC3\x0F\xB6\xC1\xC3".as_slice(),
    ] {
        let report = check(defined_at_call, "addr: 0x401000, clobber: [ecx], ret: eax => out");
        assert!(report.contradictions.is_empty(), "{:?}", report.contradictions);
        assert!(report.unknown.iter().any(|item| item.contains("eax return")));
    }

    let overwritten_by_unknown = check(
        b"\xB8\x01\x00\x00\x00\xE8\x01\x00\x00\x00\xC3\x03\xC1\xC3",
        "addr: 0x401000, ret: eax => out",
    );
    assert!(
        overwritten_by_unknown
            .unknown
            .iter()
            .any(|item| item.contains("eax return"))
    );

    let unknown_value_still_clobbers = check(
        b"\xB8\x01\x00\x00\x00\xE8\x03\x00\x00\x00\x8B\xF8\xC3\x03\xC1\xC3",
        "addr: 0x401000, clobber: [eax]",
    );
    assert!(
        unknown_value_still_clobbers
            .contradictions
            .iter()
            .any(|item| item.contains("edi"))
    );
}

#[test]
fn flag_definedness_does_not_cross_calls() {
    let unknown = check(
        b"\xE8\x01\x00\x00\x00\xC3\x83\xF9\x00\x0F\x94\xC0\xC3",
        "addr: 0x401000, clobber: [ecx], ret: al => out",
    );
    assert!(unknown.unknown.iter().any(|item| item.contains("eax return")));

    let defined = check(
        b"\xB9\x01\x00\x00\x00\xE8\x01\x00\x00\x00\xC3\x83\xF9\x00\x0F\x94\xC0\xC3",
        "addr: 0x401000, clobber: [ecx], ret: al => out",
    );
    assert!(defined.contradictions.is_empty(), "{:?}", defined.contradictions);
    assert!(defined.unknown.iter().any(|item| item.contains("eax return")));

    let local = check(
        b"\xB9\x01\x00\x00\x00\x83\xF9\x00\x0F\x94\xC0\xC3",
        "addr: 0x401000, clobber: [ecx], ret: al => out",
    );
    assert!(local.contradictions.is_empty(), "{:?}", local.contradictions);
    assert!(!local.unknown.iter().any(|item| item.contains("eax return")));
}

#[test]
fn code_after_calls_can_resolve_clobber_state() {
    let changed = check(b"\xE8\x06\x00\x00\x00\xBE\x01\x00\x00\x00\xC3\xC3", "addr: 0x401000");
    assert!(changed.contradictions.iter().any(|item| item.contains("esi")));

    let restored = check(
        b"\x55\x8B\xEC\x53\xE8\x07\x00\x00\x00\x8B\x5D\xFC\x8B\xE5\x5D\xC3\xC3",
        "addr: 0x401000, clobber: [eax, ecx, edx, esi, edi]",
    );
    assert!(restored.contradictions.is_empty(), "{:?}", restored.contradictions);
    assert!(!restored.unknown.iter().any(|item| item.contains("clobber set")));
}

#[test]
fn direct_return_cleanup_keeps_saved_register_slots_aligned() {
    let callee_cleanup = check(
        b"\x53\x6A\x01\xE8\x02\x00\x00\x00\x5B\xC3\xC2\x04\x00",
        "addr: 0x401000, clobber: [eax, ecx, edx, esi, edi]",
    );
    assert!(
        callee_cleanup.contradictions.is_empty(),
        "{:?}",
        callee_cleanup.contradictions
    );
    assert!(!callee_cleanup.unknown.iter().any(|item| item.contains("clobber")));

    let caller_cleanup = check(
        b"\x53\x6A\x01\xE8\x05\x00\x00\x00\x83\xC4\x04\x5B\xC3\xC3",
        "addr: 0x401000, clobber: [eax, ecx, edx, esi, edi]",
    );
    assert!(
        caller_cleanup.contradictions.is_empty(),
        "{:?}",
        caller_cleanup.contradictions
    );
    assert!(!caller_cleanup.unknown.iter().any(|item| item.contains("clobber")));
}

#[test]
fn logical_stack_tracks_saves_across_dynamic_alignment() {
    let report = check(
        b"\x55\x8B\xEC\x83\xE4\xF8\x53\x6A\x01\xE8\x05\x00\x00\x00\x5B\x8B\xE5\x5D\xC3\xBB\x01\x00\x00\x00\xC2\x04\x00",
        "addr: 0x401000",
    );
    assert!(report.contradictions.is_empty(), "{:?}", report.contradictions);
    assert!(!report.unknown.iter().any(|item| item.contains("clobber")));
}

#[test]
fn logical_stack_invalidates_modified_saves_after_alignment() {
    let report = check(
        b"\x55\x8B\xEC\x83\xE4\xF8\x53\x83\x04\x24\x01\x5B\x8B\xE5\x5D\xC3",
        "addr: 0x401000",
    );
    assert!(report.contradictions.iter().any(|item| item.contains("ebx")));
}

#[test]
fn ambiguous_aligned_stack_writes_invalidate_absolute_saves() {
    let report = check(
        b"\x55\x8B\xEC\x53\x83\xE4\xF8\xC7\x44\x24\x04\x00\x00\x00\x00\x8B\x5D\xFC\x8B\xE5\x5D\xC3",
        "addr: 0x401000",
    );
    assert!(report.contradictions.is_empty(), "{:?}", report.contradictions);
    assert!(report.unknown.iter().any(|item| item.contains("clobber")));
}

#[test]
fn logical_stack_distinguishes_locals_from_unknown_entry_slots() {
    let local = check(
        b"\x55\x8B\xEC\x83\xE4\xF8\x83\xEC\x04\x8B\x04\x24\x8B\xE5\x5D\xC3",
        "addr: 0x401000, clobber: [eax]",
    );
    assert!(!local.unknown.iter().any(|item| item.contains("stack layout")));

    let beyond = check(
        b"\x55\x8B\xEC\x83\xE4\xF8\x83\xEC\x04\x8B\x44\x24\x0C\x8B\xE5\x5D\xC3",
        "addr: 0x401000, clobber: [eax]",
    );
    assert!(beyond.unknown.iter().any(|item| item.contains("stack layout")));

    let alignment_dependent = check(
        b"\x55\x8B\xEC\x83\xE4\xF8\x8B\x44\x24\x04\x8B\xE5\x5D\xC3",
        "addr: 0x401000, clobber: [eax]",
    );
    assert!(
        alignment_dependent
            .unknown
            .iter()
            .any(|item| item.contains("stack layout"))
    );
}

#[test]
fn logical_stack_alignment_barriers_survive_joins() {
    let mut aligned = State::entry([0; 6], [false; 2]);
    aligned.logical_stack = Some(vec![StackValue::Alignment]);
    let mut unaligned = State::entry([0; 6], [false; 2]);
    unaligned.logical_stack = Some(vec![StackValue::Other]);
    aligned.join(&unaligned);
    assert_eq!(aligned.logical_stack, None);

    let mut framed = State::entry([0; 6], [false; 2]);
    framed.logical_stack = Some(vec![StackValue::Other, StackValue::Alignment, StackValue::Other]);
    framed.ebp_logical_depth = Some(1);
    assert_eq!(logical_stack_index(&framed, IcedReg::EBP, IcedReg::None, 0), Some(0));
    assert_eq!(logical_stack_index(&framed, IcedReg::EBP, IcedReg::None, -4), None);
    assert_eq!(logical_stack_index(&framed, IcedReg::EBP, IcedReg::None, -8), None);
}

#[test]
fn inconsistent_return_cleanup_has_no_summary() {
    let image = TextImage::synthetic(BASE, b"\x85\xC0\x74\x03\xC3\x90\x90\xC2\x04\x00");
    assert_eq!(
        summarize_call(&image, BASE, &mut AnalysisCache::default(), 1).cleanup,
        None
    );
}

#[test]
fn summarizes_cleanup_through_bounded_jump_tables() {
    let table = BASE + 0x20;
    let mut bytes = vec![0x83, 0xf8, 0x01, 0x77, 0x0d, 0xff, 0x24, 0x85];
    bytes.extend_from_slice(&table.to_le_bytes());
    bytes.extend_from_slice(&[0xc2, 0x04, 0x00, 0xc2, 0x04, 0x00, 0xc2, 0x04, 0x00]);
    bytes.resize(0x20, 0xcc);
    bytes.extend_from_slice(&(BASE + 12).to_le_bytes());
    bytes.extend_from_slice(&(BASE + 15).to_le_bytes());

    assert_eq!(
        summarize_call(
            &TextImage::synthetic(BASE, &bytes),
            BASE,
            &mut AnalysisCache::default(),
            1,
        )
        .cleanup,
        Some(4)
    );
}

#[test]
fn recursively_composes_direct_clobber_summaries() {
    let report = check(
        b"\xE8\x01\x00\x00\x00\xC3\xE8\x01\x00\x00\x00\xC3\xBE\x01\x00\x00\x00\xC3",
        "addr: 0x401000",
    );
    assert!(report.contradictions.iter().any(|item| item.contains("esi")));
}

#[test]
fn recursive_call_cycles_are_unknown() {
    let report = check(b"\xE8\x01\x00\x00\x00\xC3\xE8\xF5\xFF\xFF\xFF\xC3", "addr: 0x401000");
    assert!(report.contradictions.is_empty(), "{:?}", report.contradictions);
    assert!(report.unknown.iter().any(|item| item.contains("clobber")));
}

#[test]
fn active_call_addresses_override_cached_contexts() {
    let image = TextImage::synthetic(BASE, b"\xC3");
    let mut cache = AnalysisCache::default();
    cache.call_summaries.insert(
        BASE,
        CallSummary {
            changes: [Change::Preserved; 6],
            cleanup: Some(0),
            stack_inputs: 0,
            unresolved_inputs: false,
        },
    );
    cache.active_calls.insert(BASE);

    let summary = summarize_call(&image, BASE, &mut cache, 1);
    assert!(summary.changes.into_iter().all(|change| change == Change::Unknown));
    assert_eq!(summary.cleanup, None);
}

#[test]
fn x87_reset_empties_st0_and_unused_inputs_are_unknown() {
    let reset = check(b"\xD9\xE8\xDB\xE3\xC3", "addr: 0x401000, ret: st0<f32> => out");
    assert!(reset.contradictions.iter().any(|item| item.contains("ST0 return")));

    let unused = check(b"\xC3", "addr: 0x401000, regs: { st0<f32> = x }");
    assert!(unused.unknown.iter().any(|item| item.contains("st0 input")));
}

#[test]
fn x87_double_pop_cannot_leave_a_proven_return() {
    let report = check(
        b"\xDE\xD9\xC3",
        "addr: 0x401000, regs: { st1<f32> = y, st0<f32> = x }, ret: st0<f32> => out",
    );
    assert!(
        report
            .unknown
            .iter()
            .any(|item| item.contains("ST0 return") || item.contains("x87 stack")),
        "{:?}",
        report.unknown
    );
}

#[test]
fn divergent_saved_stack_values_join_as_unknown() {
    let report = check(
        b"\x53\x31\xC0\x74\x07\xC7\x04\x24\x00\x00\x00\x00\x5B\xC3",
        "addr: 0x401000, clobber: [eax]",
    );
    assert!(report.contradictions.is_empty(), "{:?}", report.contradictions);
    assert!(report.unknown.iter().any(|item| item.contains("clobber")));
}

#[test]
fn narrow_all_ones_and_identity_lea_are_modeled_exactly() {
    let or = check(b"\x80\xC8\xFF\xC3", "addr: 0x401000, clobber: [eax]");
    assert!(or.contradictions.is_empty(), "{:?}", or.contradictions);

    let and = check(b"\x24\xFF\xC3", "addr: 0x401000, regs: { al = x }");
    assert!(and.contradictions.is_empty(), "{:?}", and.contradictions);

    let lea = check(b"\x8D\x00\xC3", "addr: 0x401000, regs: { eax = x }");
    assert!(lea.contradictions.is_empty(), "{:?}", lea.contradictions);
}

#[test]
fn writes_to_saved_stack_slots_invalidate_the_saved_register() {
    let report = check(b"\x53\x83\x04\x24\x01\x5B\xC3", "addr: 0x401000");
    assert!(report.contradictions.iter().any(|item| item.contains("ebx")));

    let partial = check(b"\x53\xB0\x01\x88\x04\x24\x5B\xC3", "addr: 0x401000, clobber: [eax]");
    assert!(partial.contradictions.iter().any(|item| item.contains("ebx")));
}

#[test]
fn fstp_st1_keeps_the_written_value_on_top() {
    let report = check(
        b"\xDD\xD9\xC3",
        "addr: 0x401000, regs: { st0<f32> = x }, ret: st0<f32> => out",
    );
    assert!(report.contradictions.is_empty(), "{:?}", report.contradictions);
}

#[test]
fn high_byte_all_ones_does_not_require_an_input() {
    let report = check(b"\x80\xCC\xFF\xC3", "addr: 0x401000, clobber: [eax]");
    assert!(report.contradictions.is_empty(), "{:?}", report.contradictions);
}

#[test]
fn unmodeled_x87_pop_destinations_are_unknown() {
    let report = check(
        b"\xD8\xC1\xDF\xC1\xC3",
        "addr: 0x401000, regs: { st1<f32> = y, st0<f32> = x }, ret: st0<f32> => out",
    );
    assert!(report.contradictions.is_empty(), "{:?}", report.contradictions);
    assert!(report.unknown.iter().any(|item| item.contains("ST0 return")));
}

#[test]
fn conditional_stack_writes_do_not_prove_a_clobber() {
    let report = check(
        b"\x53\x8B\xC3\xF7\xD0\x31\xC9\x0F\xB1\x0C\x24\x5B\xC3",
        "addr: 0x401000, regs: { ebx = x }, clobber: [eax, ecx]",
    );
    assert!(report.contradictions.is_empty(), "{:?}", report.contradictions);
    assert!(report.unknown.iter().any(|item| item.contains("clobber")));
}

#[test]
fn indirect_jumps_and_instruction_budget_are_unknown() {
    let indirect = check(b"\xFF\xE0", "addr: 0x401000");
    assert!(indirect.unknown.iter().any(|item| item.contains("control flow")));

    let indirect_call = check(
        b"\xFF\xD0\xC3",
        "addr: 0x401000, regs: { eax = target }, clobber: [eax, ebx, ecx, edx, esi, edi]",
    );
    assert!(!indirect_call.unknown.iter().any(|item| item.contains("eax input")));
    let missing_target = check(
        b"\xFF\xD0\xC3",
        "addr: 0x401000, clobber: [eax, ebx, ecx, edx, esi, edi]",
    );
    assert!(
        missing_target
            .contradictions
            .iter()
            .any(|item| item.contains("reads eax"))
    );

    let mut bytes = vec![0x90; MAX_INSTRUCTIONS + 1];
    bytes.push(0xc3);
    let budget = check(&bytes, "addr: 0x401000");
    assert!(budget.unknown.iter().any(|item| item.contains("budget")));
}

#[test]
fn resolves_bounded_direct_jump_tables() {
    let table = BASE + 0x20;
    let mut bytes = vec![0x83, 0xf8, 0x01, 0x77, 0x09, 0xff, 0x24, 0x85];
    bytes.extend_from_slice(&table.to_le_bytes());
    bytes.extend_from_slice(&[0xc3, 0xc3, 0xc3]);
    bytes.resize(0x20, 0xcc);
    bytes.extend_from_slice(&(BASE + 12).to_le_bytes());
    bytes.extend_from_slice(&(BASE + 13).to_le_bytes());

    let report = check(&bytes, "addr: 0x401000, regs: { eax = index }");
    assert!(report.contradictions.is_empty(), "{:?}", report.contradictions);
    assert!(!report.unknown.iter().any(|item| item.contains("control flow")));
}

#[test]
fn resolves_bounded_byte_remap_jump_tables() {
    let remap = BASE + 0x30;
    let table = BASE + 0x34;
    let mut bytes = vec![0x83, 0xff, 0x02, 0x77, 0x10, 0x0f, 0xb6, 0x87];
    bytes.extend_from_slice(&remap.to_le_bytes());
    bytes.extend_from_slice(&[0xff, 0x24, 0x85]);
    bytes.extend_from_slice(&table.to_le_bytes());
    bytes.extend_from_slice(&[0xc3, 0xc3, 0xc3]);
    bytes.resize(0x30, 0xcc);
    bytes.extend_from_slice(&[1, 0, 1, 0]);
    bytes.extend_from_slice(&(BASE + 19).to_le_bytes());
    bytes.extend_from_slice(&(BASE + 20).to_le_bytes());

    let report = check(&bytes, "addr: 0x401000, regs: { edi = index }, clobber: [eax]");
    assert!(report.contradictions.is_empty(), "{:?}", report.contradictions);
    assert!(!report.unknown.iter().any(|item| item.contains("control flow")));
}

#[test]
fn rejects_jump_bounds_when_the_index_changes_after_cmp() {
    let table = BASE + 0x20;
    let mut bytes = vec![
        0x83, 0xf8, 0x01, 0xb8, 0x00, 0x00, 0x00, 0x00, 0x77, 0x09, 0xff, 0x24, 0x85,
    ];
    bytes.extend_from_slice(&table.to_le_bytes());
    bytes.extend_from_slice(&[0xc3, 0xc3, 0xc3]);
    bytes.resize(0x20, 0xcc);
    bytes.extend_from_slice(&(BASE + 17).to_le_bytes());
    bytes.extend_from_slice(&(BASE + 18).to_le_bytes());

    let report = check(&bytes, "addr: 0x401000, regs: { eax = index }");
    assert!(report.unknown.iter().any(|item| item.contains("control flow")));
}

#[test]
fn rejects_jump_tables_with_bound_bypasses() {
    let table = BASE + 0x20;
    let mut bytes = vec![0x83, 0xf8, 0x01, 0x77, 0x0a, 0xff, 0x24, 0x85];
    bytes.extend_from_slice(&table.to_le_bytes());
    bytes.extend_from_slice(&[0xeb, 0xf7, 0xc3, 0xc3]);
    bytes.resize(0x20, 0xcc);
    bytes.extend_from_slice(&(BASE + 12).to_le_bytes());
    bytes.extend_from_slice(&(BASE + 14).to_le_bytes());

    let image = TextImage::synthetic(BASE, &bytes);
    let report = check(&bytes, "addr: 0x401000, regs: { eax = index }");
    assert!(report.unknown.iter().any(|item| item.contains("control flow")));
    assert_eq!(
        summarize_call(&image, BASE, &mut AnalysisCache::default(), 1).cleanup,
        None
    );

    let mut same_source = vec![0x83, 0xf8, 0x01, 0x77, 0x00, 0xff, 0x24, 0x85];
    same_source.extend_from_slice(&table.to_le_bytes());
    same_source.extend_from_slice(&[0xc3, 0xc3]);
    same_source.resize(0x20, 0xcc);
    same_source.extend_from_slice(&(BASE + 12).to_le_bytes());
    same_source.extend_from_slice(&(BASE + 13).to_le_bytes());
    let report = check(&same_source, "addr: 0x401000, regs: { eax = index }");
    assert!(report.unknown.iter().any(|item| item.contains("control flow")));
}

#[test]
fn rejects_wrong_executable_identity_before_decoding() {
    assert!(TextImage::from_pe(b"not a PE").unwrap_err().contains("MZ"));

    let mut pe = vec![0; 0x200 + TEXT_SIZE];
    pe[..2].copy_from_slice(b"MZ");
    pe[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
    pe[0x80..0x84].copy_from_slice(b"PE\0\0");
    pe[0x84..0x86].copy_from_slice(&0x014cu16.to_le_bytes());
    pe[0x86..0x88].copy_from_slice(&1u16.to_le_bytes());
    pe[0x94..0x96].copy_from_slice(&0x00e0u16.to_le_bytes());
    pe[0x98..0x9a].copy_from_slice(&0x010bu16.to_le_bytes());
    pe[0xb4..0xb8].copy_from_slice(&IMAGE_BASE.to_le_bytes());
    let section = 0x178;
    pe[section..section + 5].copy_from_slice(b".text");
    pe[section + 8..section + 12].copy_from_slice(&(TEXT_SIZE as u32).to_le_bytes());
    pe[section + 12..section + 16].copy_from_slice(&0x1000u32.to_le_bytes());
    pe[section + 16..section + 20].copy_from_slice(&(TEXT_SIZE as u32).to_le_bytes());
    pe[section + 20..section + 24].copy_from_slice(&0x200u32.to_le_bytes());
    assert!(TextImage::from_pe(&pe).unwrap_err().contains("SHA-256"));
}

#[test]
fn rejects_addresses_outside_text() {
    let report = check(b"\xC3", "addr: 0x402000");
    assert!(
        report
            .contradictions
            .iter()
            .any(|item| item.contains("outside decodable .text"))
    );
}

#[test]
fn rejects_unbalanced_returns_and_out_of_bounds_stack_writes() {
    let unbalanced = check(b"\x53\xC3", "addr: 0x401000");
    assert!(unbalanced.contradictions.iter().any(|item| item.contains("ESP offset")));

    let write = check(b"\xC7\x44\x24\x04\x00\x00\x00\x00\xC3", "addr: 0x401000");
    assert!(
        write
            .contradictions
            .iter()
            .any(|item| item.contains("writes entry stack"))
    );
}

#[test]
fn mixed_clobbers_and_stack_aliases_are_unknown() {
    let mixed = check(
        b"\x85\xC0\x74\x06\xBE\x01\x00\x00\x00\xC3\xC3",
        "addr: 0x401000, regs: { eax = condition }",
    );
    assert!(mixed.unknown.iter().any(|item| item.contains("clobber")));

    let alias = check(
        b"\x56\x8D\x04\x24\xC7\x00\x00\x00\x00\x00\x5E\xC3",
        "addr: 0x401000, clobber: [eax]",
    );
    assert!(alias.unknown.iter().any(|item| item.contains("clobber")));
}

#[test]
fn pushes_and_call_summaries_do_not_hide_missing_inputs() {
    let memory_push = check(b"\xFF\x30\x83\xC4\x04\xC3", "addr: 0x401000");
    assert!(memory_push.contradictions.iter().any(|item| item.contains("reads eax")));

    let stack_return = check(
        b"\x50\xE8\x01\x00\x00\x00\xC3\x8B\x44\x24\x04\xC2\x04\x00",
        "addr: 0x401000, ret: eax => out",
    );
    assert!(stack_return.unknown.iter().any(|item| item.contains("eax return")));

    let gp_prerequisite = check(b"\xE8\x01\x00\x00\x00\xC3\x85\xF6\xC3", "addr: 0x401000");
    assert!(
        gp_prerequisite
            .unknown
            .iter()
            .any(|item| item.contains("call contract"))
    );
}

#[test]
fn unknown_indirect_calls_and_x87_reads_stay_visible() {
    let indirect = check(
        b"\xFF\xD0\xC3",
        "addr: 0x401000, regs: { eax = target }, clobber: [eax, ebx, ecx, edx, esi, edi]",
    );
    assert!(indirect.unknown.iter().any(|item| item.contains("call contract")));

    let x87 = check(b"\xD9\xC2\xC3", "addr: 0x401000");
    assert!(x87.unknown.iter().any(|item| item.contains("x87")));
}

#[test]
fn conditional_moves_do_not_hide_old_destination_inputs() {
    // When ECX is nonzero, CMOVE leaves the undeclared entry EAX in place.
    let report = check(
        b"\x85\xC9\x0F\x44\xC2\x85\xC0\xC3",
        "addr: 0x401000, regs: { ecx = condition, edx = value }, clobber: [eax]",
    );
    assert!(
        report.contradictions.iter().any(|item| item.contains("reads eax")),
        "{:?}",
        report.contradictions
    );

    let declared = check(
        b"\x85\xC9\x0F\x44\xC2\x85\xC0\xC3",
        "addr: 0x401000, regs: { eax = old, ecx = condition, edx = value }, clobber: [eax]",
    );
    assert!(declared.contradictions.is_empty(), "{:?}", declared.contradictions);

    let overwritten = check(
        b"\x85\xC9\x0F\x44\xC2\x31\xC0\x85\xC0\xC3",
        "addr: 0x401000, regs: { ecx = condition, edx = value }, clobber: [eax]",
    );
    assert!(
        overwritten.contradictions.is_empty(),
        "{:?}",
        overwritten.contradictions
    );
    assert!(overwritten.unknown.is_empty(), "{:?}", overwritten.unknown);
}
