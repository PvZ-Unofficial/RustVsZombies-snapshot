use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Expr, Result};

use crate::{
    ir::{AbiSpec, CapturedReturn, GpBinding, InputBinding, PointerChain, ValueSource},
    register::{ByteReg, FloatKind, GpReg, ReturnReg, X87Reg},
};

const CALL_TARGET_PRIORITY: [GpReg; 5] = [GpReg::Eax, GpReg::Edx, GpReg::Ecx, GpReg::Ebx, GpReg::Edi];

#[derive(Clone, Copy)]
enum StagedSet {
    Gp(GpReg),
    Byte(ByteReg),
}

impl StagedSet {
    fn carrier(self) -> GpReg {
        match self {
            Self::Gp(reg) => reg,
            Self::Byte(reg) => reg.carrier(),
        }
    }
}

#[derive(Clone, Copy)]
enum ManualReturn {
    Gp(GpReg),
    Byte(ByteReg),
}

impl ManualReturn {
    fn carrier(self) -> GpReg {
        match self {
            Self::Gp(reg) => reg,
            Self::Byte(reg) => reg.carrier(),
        }
    }

    fn capture_line(self) -> String {
        match self {
            Self::Gp(reg) => format!("movl {}, %eax", reg.asm_name()),
            Self::Byte(reg) => format!("movb {}, %al", reg.asm_name()),
        }
    }
}

fn normalize_output(mut output: Option<CapturedReturn>) -> (Option<CapturedReturn>, Option<ManualReturn>) {
    let manual_return = match output.as_mut() {
        Some(output) => match output.reg {
            ReturnReg::Gp(reg) if reg.is_callee_saved_i686() => {
                output.reg = ReturnReg::Gp(GpReg::Eax);
                Some(ManualReturn::Gp(reg))
            }
            ReturnReg::Byte(reg) if reg.carrier().is_callee_saved_i686() => {
                output.reg = ReturnReg::Byte(ByteReg::Al);
                Some(ManualReturn::Byte(reg))
            }
            _ => None,
        },
        None => None,
    };
    (output, manual_return)
}

struct StagedItem {
    asm_line: String,
    set: StagedSet,
}

struct AsmPlan {
    setup: Vec<TokenStream>,
    stack_lines: Vec<String>,
    pre_manual_lines: Vec<String>,
    staged_items: Vec<StagedItem>,
    late_lines: Vec<String>,
    post_restore_lines: Vec<String>,
    operands: Vec<TokenStream>,
    post: Vec<TokenStream>,
    clobbers: BTreeSet<GpReg>,
    saved_regs: BTreeSet<GpReg>,
    pending_output: Option<CapturedReturn>,
    manual_return: Option<ManualReturn>,
    touches_x87: bool,
    output_carrier: Option<GpReg>,
    output_operand_carrier: Option<GpReg>,
    operand_index: usize,
}

impl AsmPlan {
    fn new(abi: &AbiSpec, output: Option<CapturedReturn>) -> Self {
        let output_carrier = output.as_ref().and_then(|ret| ret.reg.carrier());
        let (pending_output, manual_return) = normalize_output(output);
        let output_operand_carrier = pending_output.as_ref().and_then(|ret| ret.reg.carrier());

        let mut clobbers = BTreeSet::new();
        let mut saved_regs = BTreeSet::new();
        for reg in &abi.clobbers {
            if Some(*reg) == output_operand_carrier {
                continue;
            }
            if reg.is_callee_saved_i686() {
                saved_regs.insert(*reg);
            } else {
                clobbers.insert(*reg);
            }
        }
        if let Some(manual_return) = manual_return {
            saved_regs.insert(manual_return.carrier());
        }

        Self {
            setup: Vec::new(),
            stack_lines: Vec::new(),
            pre_manual_lines: Vec::new(),
            staged_items: Vec::new(),
            late_lines: Vec::new(),
            post_restore_lines: Vec::new(),
            operands: Vec::new(),
            post: Vec::new(),
            clobbers,
            saved_regs,
            pending_output,
            manual_return,
            touches_x87: false,
            output_carrier,
            output_operand_carrier,
            operand_index: 0,
        }
    }

    fn carrier_is_reserved(&self, reg: GpReg) -> bool {
        self.output_uses_carrier(reg) || self.clobbers.contains(&reg)
    }

    fn output_uses_carrier(&self, reg: GpReg) -> bool {
        Some(reg) == self.output_carrier || Some(reg) == self.output_operand_carrier
    }

    fn take_gp_output_for_input(&mut self, reg: GpReg) -> Option<Expr> {
        let Some(CapturedReturn {
            reg: ReturnReg::Gp(output_reg),
            ..
        }) = &self.pending_output
        else {
            return None;
        };
        if *output_reg != reg {
            return None;
        }
        self.pending_output.take().map(|output| output.target)
    }

    fn mark_explicit_inout(&mut self, reg: GpReg) {
        self.clobbers.remove(&reg);
    }

    fn save_if_needed(&mut self, reg: GpReg) {
        if reg.is_callee_saved_i686() {
            self.saved_regs.insert(reg);
        }
    }

    fn reg_needs_staging(&self, reg: GpReg) -> bool {
        reg.is_callee_saved_i686() || self.output_uses_carrier(reg)
    }

    fn push_named_reg_operand(&mut self, value: TokenStream) -> String {
        let name = format_ident!("__rsvz_asm_operand_{}", self.operand_index);
        self.operand_index += 1;
        self.operands.push(quote! { #name = in(reg) #value });
        format!("{{{name}}}")
    }

    fn stage_set(&mut self, set: StagedSet, source: TokenStream) {
        self.save_if_needed(set.carrier());
        let placeholder = self.push_named_reg_operand(source);
        self.staged_items.push(StagedItem {
            asm_line: format!("pushl {placeholder}"),
            set,
        });
    }

    fn emit_input(&mut self, binding: &InputBinding) {
        match binding {
            InputBinding::Gp(binding) => self.emit_gp_input(binding),
            InputBinding::Byte { reg, source } => self.emit_byte_input(*reg, source),
            InputBinding::X87 { reg, kind, source } => self.emit_x87_input(*reg, *kind, source),
        }
    }

    fn emit_gp_input(&mut self, binding: &GpBinding) {
        let reg = binding.reg;
        let source = source_tokens(&binding.source);
        if let Some(target) = self.take_gp_output_for_input(reg) {
            self.mark_explicit_inout(reg);
            let reg_name = syn::LitStr::new(reg.canonical_name(), Span::call_site());
            self.operands.push(quote! { inout(#reg_name) #source => #target });
        } else if self.reg_needs_staging(reg) {
            self.stage_set(StagedSet::Gp(reg), source);
        } else if self.clobbers.contains(&reg) {
            self.mark_explicit_inout(reg);
            let reg_name = syn::LitStr::new(reg.canonical_name(), Span::call_site());
            self.operands.push(quote! { inout(#reg_name) #source => _ });
        } else {
            let reg_name = syn::LitStr::new(reg.canonical_name(), Span::call_site());
            self.operands.push(quote! { in(#reg_name) #source });
        }
    }

    fn emit_byte_input(&mut self, reg: ByteReg, source: &ValueSource) {
        let source = source_tokens(source);
        if reg.carrier().is_callee_saved_i686() || self.carrier_is_reserved(reg.carrier()) {
            self.stage_set(StagedSet::Byte(reg), quote! { ((#source) as u32) });
        } else {
            let reg_name = syn::LitStr::new(reg.canonical_name(), Span::call_site());
            self.operands.push(quote! { in(#reg_name) ((#source) as u8) });
        }
    }

    fn emit_x87_input(&mut self, reg: X87Reg, kind: FloatKind, source: &ValueSource) {
        let suffix = reg.canonical_name();
        let temp_ident = format_ident!("__rsvz_asm_{suffix}_value");
        let ptr_ident = format_ident!("__rsvz_asm_{suffix}_ptr");
        let source = source_tokens(source);
        let ty = kind.rust_type_tokens();
        let load_line = format!("{} ({{{ptr_ident}}})", kind.load_mnemonic());

        self.setup.push(quote! {
            let #temp_ident: #ty = (#source) as #ty;
        });
        self.pre_manual_lines.push(load_line);
        self.operands.push(quote! { #ptr_ident = in(reg) &#temp_ident });
        self.touches_x87 = true;
    }

    fn emit_stack_source(&mut self, source: &ValueSource) {
        let source = source_tokens(source);
        let placeholder = self.push_named_reg_operand(source);
        self.stack_lines.push(format!("pushl {placeholder}"));
    }

    fn emit_stack_expr(&mut self, expr: &Expr) {
        let placeholder = self.push_named_reg_operand(quote! { #expr });
        self.stack_lines.push(format!("pushl {placeholder}"));
    }

    fn emit_call_target(&mut self, reg: GpReg, addr: u32) {
        if reg.is_callee_saved_i686() || self.carrier_is_reserved(reg) {
            self.save_if_needed(reg);
            self.late_lines.push(format!("movl ${:#x}, {}", addr, reg.asm_name()));
        } else {
            let reg_name = syn::LitStr::new(reg.canonical_name(), Span::call_site());
            self.operands.push(quote! { in(#reg_name) #addr });
        }
    }

    fn configure_return(&mut self) {
        if let Some(manual_return) = self.manual_return.take() {
            self.late_lines.push(manual_return.capture_line());
        }

        let (kind, target) = match self.pending_output.take() {
            Some(CapturedReturn {
                reg: ReturnReg::X87(kind),
                target,
            }) => (kind, target),
            pending_output => {
                self.pending_output = pending_output;
                return;
            }
        };
        let temp_ident = format_ident!("__rsvz_asm_x87_ret");
        let ptr_ident = format_ident!("__rsvz_asm_x87_ret_ptr");
        let ty = kind.rust_type_tokens();
        let zero = kind.zero_value();
        let store_line = format!("{} ({{{ptr_ident}}})", kind.store_mnemonic());

        self.setup.push(quote! {
            let mut #temp_ident: #ty = #zero;
        });
        self.post_restore_lines.push(store_line);
        // The pointer is consumed after the native call, so it must stay live across every
        // declared register clobber. A plain `in(reg)` may overlap a `lateout` clobber and
        // leave `fstp` writing through the native function's clobbered register value.
        self.operands
            .push(quote! { #ptr_ident = inout(reg) &mut #temp_ident => _ });
        self.post.push(quote! {
            #target = #temp_ident;
        });
        self.touches_x87 = true;
    }

    fn rendered_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();

        lines.extend(self.saved_regs.iter().map(|reg| format!("pushl {}", reg.asm_name())));
        lines.extend(self.stack_lines.iter().cloned());
        lines.extend(self.pre_manual_lines.iter().cloned());
        lines.extend(self.staged_items.iter().map(|item| item.asm_line.clone()));
        for item in self.staged_items.iter().rev() {
            match item.set {
                StagedSet::Gp(reg) => lines.push(format!("popl {}", reg.asm_name())),
                StagedSet::Byte(reg) => {
                    lines.push(format!("movb (%esp), {}", reg.asm_name()));
                    lines.push("addl $4, %esp".to_string());
                }
            }
        }
        lines.extend(self.late_lines.iter().cloned());
        lines.extend(
            self.saved_regs
                .iter()
                .rev()
                .map(|reg| format!("popl {}", reg.asm_name())),
        );
        lines.extend(self.post_restore_lines.iter().cloned());

        lines
    }
}

fn source_tokens(source: &ValueSource) -> TokenStream {
    match source {
        ValueSource::Expr(expr) => quote!(#expr),
        ValueSource::PointerChain(chain) => pointer_chain_tokens(chain),
    }
}

fn pointer_chain_tokens(chain: &PointerChain) -> TokenStream {
    let base = chain.base as usize;
    let offsets = chain.offsets.iter().map(|offset| *offset as usize);
    quote! {{
        let mut __rsvz_asm_ptr = core::ptr::read_unaligned(#base as *const usize);
        #(
            __rsvz_asm_ptr = core::ptr::read_unaligned(
                __rsvz_asm_ptr.wrapping_add(#offsets) as *const usize,
            );
        )*
        __rsvz_asm_ptr
    }}
}

fn collect_occupied_gp_slots(abi: &AbiSpec, output: Option<&CapturedReturn>) -> BTreeMap<GpReg, String> {
    let mut occupied = BTreeMap::new();

    if let Some(this) = &abi.this {
        occupied.insert(this.reg, format!("the `this` binding `{}`", this.canonical_name()));
    }

    for input in &abi.inputs {
        if let crate::ir::BindingSlot::Gp(reg) = input.slot() {
            occupied.insert(reg, format!("the `regs` binding `{}`", input.canonical_name()));
        }
    }

    if let Some(carrier) = output.and_then(|output| output.reg.carrier()) {
        occupied
            .entry(carrier)
            .or_insert_with(|| format!("the `ret` carrier `{}`", carrier.canonical_name()));
    }

    occupied
}

pub(crate) fn choose_call_reg(abi: &AbiSpec, output: Option<&CapturedReturn>, error_span: Span) -> Result<GpReg> {
    let occupied = collect_occupied_gp_slots(abi, output);

    for reg in CALL_TARGET_PRIORITY {
        if !occupied.contains_key(&reg) {
            return Ok(reg);
        }
    }

    let occupied_list = CALL_TARGET_PRIORITY
        .into_iter()
        .map(|reg| {
            let source = occupied
                .get(&reg)
                .expect("all call target registers should be occupied here");
            format!("{} by {}", reg.canonical_name(), source)
        })
        .collect::<Vec<_>>()
        .join(", ");

    Err(syn::Error::new(
        error_span,
        format!(
            "could not choose a call-target scratch register automatically because all safe i686 call-target registers are already occupied: {occupied_list}",
        ),
    ))
}

fn build_plan(abi: &AbiSpec, output: Option<CapturedReturn>, call_reg: GpReg) -> AsmPlan {
    let mut plan = AsmPlan::new(abi, output);

    if let Some(binding) = &abi.this {
        plan.emit_gp_input(binding);
    }

    let mut st0 = None;
    let mut st1 = None;
    for binding in &abi.inputs {
        match binding {
            InputBinding::X87 { reg: X87Reg::St0, .. } => st0 = Some(binding),
            InputBinding::X87 { reg: X87Reg::St1, .. } => st1 = Some(binding),
            _ => plan.emit_input(binding),
        }
    }

    for binding in [st1, st0].into_iter().flatten() {
        plan.emit_input(binding);
    }

    if let Some(source) = &abi.stack_this {
        plan.emit_stack_source(source);
    }
    for expr in &abi.stack {
        plan.emit_stack_expr(expr);
    }

    plan.emit_call_target(call_reg, abi.addr);
    plan.late_lines.push(format!("call *{}", call_reg.asm_name()));

    if abi.cleanup > 0 {
        plan.late_lines.push(format!("addl ${}, %esp", abi.cleanup));
    }

    plan.configure_return();
    plan
}

#[cfg(test)]
pub(crate) fn build_asm_template(abi: &AbiSpec) -> Result<String> {
    let call_reg = choose_call_reg(abi, None, Span::call_site())?;
    Ok(build_plan(abi, None, call_reg).rendered_lines().join(";\n"))
}

pub(crate) fn expand_abi_call(abi: &AbiSpec, output: Option<CapturedReturn>, error_span: Span) -> Result<TokenStream> {
    let call_reg = choose_call_reg(abi, output.as_ref(), error_span)?;
    let mut plan = build_plan(abi, output, call_reg);

    let asm_lines = plan
        .rendered_lines()
        .into_iter()
        .map(|line| syn::LitStr::new(&line, Span::call_site()))
        .collect::<Vec<_>>();

    if let Some(CapturedReturn { reg, target }) = plan.pending_output.take() {
        let reg_name = syn::LitStr::new(
            reg.explicit_name()
                .expect("non-x87 outputs always have explicit register names"),
            Span::call_site(),
        );
        plan.operands.push(quote! { lateout(#reg_name) #target });
    }

    if plan.touches_x87 {
        plan.operands.push(quote! { out("st") _ });
    }

    for reg in plan.clobbers {
        let reg_name = syn::LitStr::new(reg.canonical_name(), Span::call_site());
        plan.operands.push(quote! { lateout(#reg_name) _ });
    }

    let setup = plan.setup;
    let operands = plan.operands;
    let post = plan.post;

    Ok(quote! {{
        #(#setup)*
        core::arch::asm!(
            #(#asm_lines,)*
            #(#operands,)*
            options(att_syntax),
        );
        #(#post)*
    }})
}

#[cfg(test)]
mod tests {
    use crate::ir::CallSpec;

    use super::*;

    fn parse_call(input: &str) -> CallSpec {
        syn::parse_str(input).unwrap()
    }

    fn find_in(asm: &str, needle: &str) -> usize {
        asm.find(needle)
            .unwrap_or_else(|| panic!("expected `{needle}` in `{asm}`"))
    }

    fn assert_before(asm: &str, before: &str, after: &str) {
        assert!(
            find_in(asm, before) < find_in(asm, after),
            "expected `{before}` before `{after}` in `{asm}`",
        );
    }

    #[test]
    fn builds_template_for_chain_and_stack_this() {
        let spec = parse_call("addr: 0x5518f0, this: ecx = [0x6a9ec0], stack_this: [0x6a9ec0]");
        let asm = build_asm_template(&spec.abi).unwrap();
        assert!(asm.contains("call *%eax"));
        assert!(asm.contains("pushl {__rsvz_asm_operand_"));
    }

    #[test]
    fn builds_template_with_cleanup() {
        let spec = parse_call("addr: 0x481fe0, this: ecx = board, stack: [file], cleanup: 0x4");
        let asm = build_asm_template(&spec.abi).unwrap();
        assert!(asm.contains("addl $4, %esp"));
    }

    #[test]
    fn builds_template_for_x87_bindings() {
        let spec = parse_call("addr: 0x6398b0, regs: { st1<f64> = x, st0<f64> = y }");
        let asm = build_asm_template(&spec.abi).unwrap();
        assert!(asm.contains("fldl"));
        assert!(asm.contains("call *%eax"));
    }

    #[test]
    fn x87_inputs_keep_st1_then_st0_order() {
        let spec = parse_call("addr: 0x6398b0, regs: { st1<f64> = x, st0<f64> = y }");
        let asm = build_asm_template(&spec.abi).unwrap();
        let st1 = asm.find("__rsvz_asm_st1_ptr").unwrap();
        let st0 = asm.find("__rsvz_asm_st0_ptr").unwrap();
        assert!(st1 < st0);
    }

    #[test]
    fn auto_selects_eax_when_free() {
        let spec = parse_call("addr: 0x415d40");
        let asm = build_asm_template(&spec.abi).unwrap();
        assert!(asm.contains("call *%eax"));
    }

    #[test]
    fn auto_selects_next_free_register() {
        let spec = parse_call("addr: 0x415d40, this: eax = board");
        let asm = build_asm_template(&spec.abi).unwrap();
        assert!(asm.contains("call *%edx"));
    }

    #[test]
    fn auto_selection_ignores_clobbers_and_return_carriers() {
        let spec = parse_call("addr: 0x41c0d0, regs: { eax = row }, clobber: [edx], ret: eax => out");
        let tokens = expand_abi_call(&spec.abi, spec.ret, spec.span).unwrap().to_string();
        assert!(tokens.contains("movl $0x41c0d0, %edx"));
        assert!(tokens.contains("call *%edx"));
    }

    #[test]
    fn byte_bindings_use_movb_when_carrier_is_reserved() {
        let spec = parse_call("addr: 0x40e020, regs: { al = flag }, clobber: [eax]");
        let tokens = expand_abi_call(&spec.abi, spec.ret, spec.span).unwrap().to_string();
        assert!(tokens.contains("movb"));
        assert!(tokens.contains("movb (%esp), %al"));
        assert!(!tokens.contains("reg_byte"));
    }

    #[test]
    fn clobbers_are_emitted() {
        let spec = parse_call("addr: 0x41dae0, this: edi = board, clobber: [edi]");
        let tokens = expand_abi_call(&spec.abi, spec.ret, spec.span).unwrap().to_string();
        assert!(tokens.contains("pushl %edi"));
        assert!(tokens.contains("popl %edi"));
        assert!(!tokens.contains("\"edi\""));
    }

    #[test]
    fn saved_gp_return_is_captured_before_restore() {
        let spec = parse_call("addr: 1, regs: { esi = value }, clobber: [eax, esi], ret: esi => out");
        let tokens = expand_abi_call(&spec.abi, spec.ret, spec.span).unwrap().to_string();

        assert_before(&tokens, "call *%eax", "movl %esi, %eax");
        let capture = find_in(&tokens, "movl %esi, %eax");
        let restore = tokens.rfind("popl %esi").expect("expected esi restore");
        assert!(capture < restore, "unexpected order in `{tokens}`");
        assert!(tokens.contains("lateout (\"eax\") out"));
        assert!(!tokens.contains("lateout (\"esi\")"));
    }

    #[test]
    fn saved_byte_return_is_captured_before_restore() {
        let spec = parse_call("addr: 1, regs: { bl = value }, clobber: [eax, ebx], ret: bl => out");
        let tokens = expand_abi_call(&spec.abi, spec.ret, spec.span).unwrap().to_string();

        assert_before(&tokens, "call *%eax", "movb %bl, %al");
        assert_before(&tokens, "movb %bl, %al", "popl %ebx");
        assert!(tokens.contains("lateout (\"al\") out"));
        assert!(!tokens.contains("lateout (\"bl\")"));
    }

    #[test]
    fn template_for_saved_registers_is_stable() {
        let spec = parse_call("addr: 0x41c680, clobber: [esi], regs: { ecx = board, eax = col, esi = row }");
        let asm = build_asm_template(&spec.abi).unwrap();
        assert!(asm.contains("call *%edx"));
        assert!(asm.contains("pushl %esi"));
        assert!(asm.contains("popl %esi"));
    }

    #[test]
    fn manual_esi_reg_without_stack_is_staged_before_set() {
        let spec = parse_call("addr: 0x415d40, regs: { esi = row }");
        let tokens = expand_abi_call(&spec.abi, spec.ret, spec.span).unwrap().to_string();
        assert!(tokens.contains("pushl %esi"));
        assert!(tokens.contains("pushl {__rsvz_asm_operand_0}"));
        assert!(tokens.contains("popl %esi"));
        assert!(!tokens.contains("movl {__rsvz_asm_operand_0}, %esi"));
        assert!(!tokens.contains("\"esi\""));
    }

    #[test]
    fn manual_esi_this_consumes_stack_before_setting_esi() {
        let spec = parse_call(
            "addr: 0x44f560, this: esi = app, stack: [look_for_saved_game, game_mode], clobber: [eax, ecx, edx]",
        );
        let asm = build_asm_template(&spec.abi).unwrap();

        assert!(asm.contains("pushl %esi"));
        assert_before(&asm, "pushl {__rsvz_asm_operand_1}", "popl %esi");
        assert_before(&asm, "pushl {__rsvz_asm_operand_2}", "popl %esi");
        assert_before(&asm, "popl %esi", "call *%eax");

        let tokens = expand_abi_call(&spec.abi, spec.ret, spec.span).unwrap().to_string();
        assert!(!tokens.contains("\"esi\""));
    }

    #[test]
    fn manual_edi_and_ebx_regs_do_not_clobber_each_other() {
        let spec = parse_call("addr: 0x426620, regs: { edi = row, ebx = col }, stack: [challenge]");
        let asm = build_asm_template(&spec.abi).unwrap();

        assert_before(&asm, "pushl {__rsvz_asm_operand_2}", "popl %ebx");
        assert_before(&asm, "pushl {__rsvz_asm_operand_1}", "popl %edi");
        assert_before(&asm, "pushl {__rsvz_asm_operand_0}", "pushl {__rsvz_asm_operand_1}");
        assert_before(&asm, "pushl %ebx", "popl %ebx");
        assert_before(&asm, "pushl %edi", "popl %edi");
    }

    #[test]
    fn stack_this_order_is_preserved_with_manual_this() {
        let spec = parse_call("addr: 0x5518f0, this: esi = app, stack_this: hidden, stack: [a, b]");
        let asm = build_asm_template(&spec.abi).unwrap();

        assert_before(&asm, "pushl {__rsvz_asm_operand_1}", "pushl {__rsvz_asm_operand_2}");
        assert_before(&asm, "pushl {__rsvz_asm_operand_2}", "pushl {__rsvz_asm_operand_3}");
        assert_before(&asm, "pushl {__rsvz_asm_operand_3}", "popl %esi");
    }

    #[test]
    fn byte_binding_on_saved_carrier_is_hazard_free() {
        let spec = parse_call("addr: 0x1234, regs: { bl = flag }, stack: [arg]");
        let asm = build_asm_template(&spec.abi).unwrap();

        assert_before(&asm, "pushl {__rsvz_asm_operand_0}", "movb (%esp), %bl");
        assert_before(&asm, "pushl {__rsvz_asm_operand_1}", "movb (%esp), %bl");
        assert_before(&asm, "movb (%esp), %bl", "call *%eax");
    }

    #[test]
    fn x87_inputs_remain_st1_then_st0_with_manual_gp_regs() {
        let spec = parse_call("addr: 0x6398b0, this: esi = app, regs: { st1<f64> = x, st0<f64> = y }");
        let asm = build_asm_template(&spec.abi).unwrap();

        assert_before(&asm, "__rsvz_asm_st1_ptr", "__rsvz_asm_st0_ptr");
        assert_before(&asm, "__rsvz_asm_st0_ptr", "popl %esi");
    }

    #[test]
    fn x87_return_pointer_is_not_hazarded_by_manual_saved_regs() {
        let spec = parse_call("addr: 0x41c6c0, this: esi = board, ret: st0<f32> => ret");
        let tokens = expand_abi_call(&spec.abi, spec.ret, spec.span).unwrap().to_string();

        assert_before(&tokens, "popl %esi", "fstps");
        assert!(tokens.contains("__rsvz_asm_x87_ret_ptr = inout (reg)"));
    }

    #[test]
    fn x87_return_pointer_stays_live_across_volatile_clobbers() {
        let spec = parse_call(
            "addr: 0x531880, this: eax = zombie, stack: [row], clobber: [eax, ecx, edx], ret: st0<f32> => ret",
        );
        let tokens = expand_abi_call(&spec.abi, spec.ret, spec.span).unwrap().to_string();

        assert!(tokens.contains("__rsvz_asm_x87_ret_ptr = inout (reg)"));
        assert!(!tokens.contains("__rsvz_asm_x87_ret_ptr = in (reg)"));
    }

    #[test]
    fn cleanup_occurs_after_call_before_restore() {
        let spec = parse_call("addr: 0x481fe0, this: edi = app, stack: [file], cleanup: 0x4");
        let asm = build_asm_template(&spec.abi).unwrap();

        let call = find_in(&asm, "call *%eax");
        let cleanup = find_in(&asm, "addl $4, %esp");
        let restore = asm.rfind("popl %edi").expect("expected edi restore");
        assert!(call < cleanup && cleanup < restore, "unexpected order in `{asm}`");
    }

    #[test]
    fn clobbered_volatile_inputs_use_inout_not_generic_moves() {
        let spec = parse_call(
            "addr: 0x41c740, this: ebx = board, regs: { ecx = col, eax = row }, clobber: [eax, ecx, edx], ret: eax => out",
        );
        let tokens = expand_abi_call(&spec.abi, spec.ret, spec.span).unwrap().to_string();

        assert!(tokens.contains("inout (\"eax\") row => out"));
        assert!(tokens.contains("inout (\"ecx\") col => _"));
        assert!(!tokens.contains("movl {__rsvz_asm_operand_1}, %ecx"));
        assert!(!tokens.contains("movl {__rsvz_asm_operand_2}, %eax"));
    }

    #[test]
    fn reports_when_no_call_target_register_is_available() {
        let spec = parse_call("addr: 1, this: eax = this_arg, regs: { ebx = a, ecx = b, edx = c, esi = d, edi = e }");
        let err = expand_abi_call(&spec.abi, spec.ret, spec.span).expect_err("fully occupied gp set should fail");
        let text = err.to_string();
        assert!(text.contains("could not choose a call-target scratch register automatically"));
        assert!(text.contains("eax by the `this` binding `eax`"));
        assert!(text.contains("edi by the `regs` binding `edi`"));
    }

    #[test]
    fn choose_call_reg_never_selects_esi_or_ebp() {
        let spec = parse_call("addr: 1, this: eax = this_arg, regs: { ebx = a, ecx = b, edx = c, edi = e }");
        let err = expand_abi_call(&spec.abi, spec.ret, spec.span)
            .expect_err("esi must not be used as the fallback call target");
        let text = err.to_string();

        assert!(text.contains("all safe i686 call-target registers are already occupied"));
        assert!(!text.contains("esi by"));
        assert!(!text.contains("ebp by"));
    }

    #[test]
    fn ebp_carriers_are_rejected() {
        for input in [
            "addr: 1, this: ebp = app",
            "addr: 1, regs: { ebp = value }",
            "addr: 1, ret: ebp => out",
            "addr: 1, clobber: [ebp]",
        ] {
            let err = syn::parse_str::<CallSpec>(input).err().expect("ebp should be rejected");
            let text = err.to_string();
            assert!(text.contains("`ebp` is not supported"));
            assert!(text.contains("frame/base pointer"));
        }
    }
}
