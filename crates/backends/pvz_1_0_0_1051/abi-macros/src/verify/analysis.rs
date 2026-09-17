use super::*;

mod cfg;
mod transfer;

use cfg::traverse;
use transfer::{conditional_write, is_alignment, transfer, writes};

#[cfg(test)]
use cfg::summarize_call;
#[cfg(test)]
use transfer::logical_stack_index;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Gp {
    Eax,
    Ebx,
    Ecx,
    Edx,
    Esi,
    Edi,
}

impl Gp {
    const ALL: [Self; 6] = [Self::Eax, Self::Ebx, Self::Ecx, Self::Edx, Self::Esi, Self::Edi];

    const fn index(self) -> usize {
        self as usize
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Eax => "eax",
            Self::Ebx => "ebx",
            Self::Ecx => "ecx",
            Self::Edx => "edx",
            Self::Esi => "esi",
            Self::Edi => "edi",
        }
    }
}

impl From<GpReg> for Gp {
    fn from(value: GpReg) -> Self {
        match value {
            GpReg::Eax => Self::Eax,
            GpReg::Ebx => Self::Ebx,
            GpReg::Ecx => Self::Ecx,
            GpReg::Edx => Self::Edx,
            GpReg::Esi => Self::Esi,
            GpReg::Edi => Self::Edi,
        }
    }
}

fn gp_register(register: IcedReg) -> Option<(Gp, u32)> {
    Some(match register {
        IcedReg::EAX => (Gp::Eax, FULL),
        IcedReg::AX => (Gp::Eax, 0xffff),
        IcedReg::AL => (Gp::Eax, 0xff),
        IcedReg::AH => (Gp::Eax, 0xff00),
        IcedReg::EBX => (Gp::Ebx, FULL),
        IcedReg::BX => (Gp::Ebx, 0xffff),
        IcedReg::BL => (Gp::Ebx, 0xff),
        IcedReg::BH => (Gp::Ebx, 0xff00),
        IcedReg::ECX => (Gp::Ecx, FULL),
        IcedReg::CX => (Gp::Ecx, 0xffff),
        IcedReg::CL => (Gp::Ecx, 0xff),
        IcedReg::CH => (Gp::Ecx, 0xff00),
        IcedReg::EDX => (Gp::Edx, FULL),
        IcedReg::DX => (Gp::Edx, 0xffff),
        IcedReg::DL => (Gp::Edx, 0xff),
        IcedReg::DH => (Gp::Edx, 0xff00),
        IcedReg::ESI => (Gp::Esi, FULL),
        IcedReg::SI => (Gp::Esi, 0xffff),
        IcedReg::EDI => (Gp::Edi, FULL),
        IcedReg::DI => (Gp::Edi, 0xffff),
        _ => return None,
    })
}

fn x87_index(register: IcedReg) -> Option<usize> {
    match register {
        IcedReg::ST0 => Some(0),
        IcedReg::ST1 => Some(1),
        _ => None,
    }
}

fn is_x87(register: IcedReg) -> bool {
    matches!(
        register,
        IcedReg::ST0
            | IcedReg::ST1
            | IcedReg::ST2
            | IcedReg::ST3
            | IcedReg::ST4
            | IcedReg::ST5
            | IcedReg::ST6
            | IcedReg::ST7
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RegState {
    origins: [u32; 6],
    defined: u32,
    unknown: u32,
    change: Change,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Change {
    Preserved,
    Changed,
    Unknown,
}

impl RegState {
    fn entry(reg: Gp, defined: u32) -> Self {
        let mut origins = [0; 6];
        origins[reg.index()] = FULL;
        Self {
            origins,
            defined,
            unknown: 0,
            change: Change::Preserved,
        }
    }

    fn write(&mut self, mask: u32, access: OpAccess, inputs_defined: bool, independent: bool) {
        let old_defined = self.defined & !self.unknown & mask == mask;
        let result_defined = inputs_defined || independent;
        if conditional_write(access) {
            // The untaken write keeps the old value, including its entry-input origins.
            if !(old_defined && result_defined) {
                self.defined &= !mask;
                self.unknown |= mask;
            }
            if self.change == Change::Preserved {
                self.change = Change::Unknown;
            }
        } else {
            for origin in &mut self.origins {
                *origin &= !mask;
            }
            if result_defined {
                self.defined |= mask;
                self.unknown &= !mask;
            } else {
                self.defined &= !mask;
                self.unknown |= mask;
            }
            if mask == FULL || self.change == Change::Preserved {
                self.change = Change::Changed;
            }
        }
    }

    fn make_unknown(&mut self) {
        self.origins = [0; 6];
        self.defined = 0;
        self.unknown = FULL;
        self.change = Change::Unknown;
    }

    fn apply_call(&mut self, change: Change) {
        self.origins = [0; 6];
        self.defined = 0;
        self.unknown = FULL;
        self.change = change;
    }

    fn rebase_for(&mut self, destination: Gp) {
        self.change = if self.origins[destination.index()] == FULL
            && Gp::ALL
                .into_iter()
                .all(|origin| origin == destination || self.origins[origin.index()] == 0)
        {
            Change::Preserved
        } else if self.change != Change::Unknown {
            Change::Changed
        } else {
            Change::Unknown
        };
    }

    fn join(&mut self, other: Self) -> bool {
        let before = *self;
        for origin in Gp::ALL {
            self.origins[origin.index()] &= other.origins[origin.index()];
        }
        self.defined &= other.defined;
        self.unknown |= other.unknown;
        if self.change != other.change {
            self.change = Change::Unknown;
        }
        *self != before
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum X87State {
    Empty,
    Known,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct X87Value {
    state: X87State,
    origin: Option<X87Reg>,
}

impl X87Value {
    const EMPTY: Self = Self {
        state: X87State::Empty,
        origin: None,
    };
    const UNKNOWN: Self = Self {
        state: X87State::Unknown,
        origin: None,
    };

    fn input(reg: X87Reg) -> Self {
        Self {
            state: X87State::Known,
            origin: Some(reg),
        }
    }

    fn join(&mut self, other: Self) {
        if self.state != other.state {
            self.state = X87State::Unknown;
        }
        if self.origin != other.origin {
            self.origin = None;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StackValue {
    Register { value: RegState, exposed_to_call: bool },
    Other,
    Unknown,
    Alignment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    esp: Option<i32>,
    ebp: Option<i32>,
    regs: [RegState; 6],
    defined_flags: u32,
    stack: BTreeMap<i32, StackValue>,
    logical_stack: Option<Vec<StackValue>>,
    ebp_logical_depth: Option<usize>,
    ambiguous_push_input: bool,
    x87: [X87Value; 2],
}

impl State {
    fn entry(input_masks: [u32; 6], x87_inputs: [bool; 2]) -> Self {
        Self {
            esp: Some(0),
            ebp: None,
            regs: Gp::ALL.map(|reg| RegState::entry(reg, input_masks[reg.index()])),
            defined_flags: 0,
            stack: BTreeMap::new(),
            logical_stack: Some(Vec::new()),
            ebp_logical_depth: None,
            ambiguous_push_input: false,
            x87: [
                if x87_inputs[0] {
                    X87Value::input(X87Reg::St0)
                } else {
                    X87Value::EMPTY
                },
                if x87_inputs[1] {
                    X87Value::input(X87Reg::St1)
                } else {
                    X87Value::EMPTY
                },
            ],
        }
    }

    fn join(&mut self, other: &Self) -> bool {
        let before = self.clone();
        if self.esp != other.esp {
            self.esp = None;
        }
        if self.ebp != other.ebp {
            self.ebp = None;
        }
        if self.ebp_logical_depth != other.ebp_logical_depth {
            self.ebp_logical_depth = None;
        }
        self.ambiguous_push_input |= other.ambiguous_push_input;
        for reg in Gp::ALL {
            self.regs[reg.index()].join(other.regs[reg.index()]);
        }
        self.defined_flags &= other.defined_flags;
        let offsets = self
            .stack
            .keys()
            .chain(other.stack.keys())
            .copied()
            .collect::<BTreeSet<_>>();
        self.stack = offsets
            .into_iter()
            .map(|offset| {
                let value = match (self.stack.get(&offset), other.stack.get(&offset)) {
                    (Some(left), Some(right)) if left == right => *left,
                    _ => StackValue::Unknown,
                };
                (offset, value)
            })
            .collect();
        match (&mut self.logical_stack, &other.logical_stack) {
            (Some(left), Some(right)) if left.len() == right.len() => {
                if left
                    .iter()
                    .zip(right)
                    .any(|(left, right)| left != right && (is_alignment(*left) || is_alignment(*right)))
                {
                    self.logical_stack = None;
                } else {
                    for (left, right) in left.iter_mut().zip(right) {
                        if left != right {
                            *left = StackValue::Unknown;
                        }
                    }
                }
            }
            (Some(_), _) => self.logical_stack = None,
            (None, _) => {}
        }
        for index in 0..2 {
            self.x87[index].join(other.x87[index]);
        }
        *self != before
    }
}

#[derive(Default)]
struct Facts {
    input_reads: [u32; 6],
    input_evidence: [Option<u32>; 6],
    stack_reads: BTreeMap<u32, u32>,
    stack_writes: BTreeMap<u32, u32>,
    x87_input_reads: [bool; 2],
    missing_x87_inputs: BTreeSet<usize>,
    exits: BTreeMap<u32, (State, u32)>,
    unknown_control: bool,
    unknown_call_contract: bool,
    unknown_x87_read: bool,
    unknown_stack: bool,
    budget_exhausted: bool,
}

#[derive(Default)]
pub(super) struct Report {
    pub(super) contradictions: Vec<String>,
    pub(super) unknown: Vec<String>,
}

#[derive(Default)]
pub(super) struct AnalysisCache {
    call_summaries: BTreeMap<u32, CallSummary>,
    active_calls: BTreeSet<u32>,
}

#[derive(Clone, Copy)]
struct CallSummary {
    changes: [Change; 6],
    cleanup: Option<u32>,
    stack_inputs: u64,
    unresolved_inputs: bool,
}

impl CallSummary {
    const UNKNOWN: Self = Self {
        changes: [Change::Unknown; 6],
        cleanup: None,
        stack_inputs: 0,
        unresolved_inputs: true,
    };
}

pub(super) fn analyze(
    image: &TextImage, abi: &AbiSpec, output: Option<ReturnReg>, cache: &mut AnalysisCache,
) -> Report {
    if image.decode(abi.addr).is_none() {
        return Report {
            contradictions: vec![format!("address {:#010x} is outside decodable .text code", abi.addr)],
            unknown: Vec::new(),
        };
    }
    let (input_masks, x87_inputs) = declared_inputs(abi);
    let facts = traverse(image, abi.addr, State::entry(input_masks, x87_inputs), cache, 0);
    let mut report = Report::default();

    if facts.exits.is_empty() && !facts.unknown_control && !facts.budget_exhausted {
        report.contradictions.push("no reachable return instruction".to_owned());
    }

    let pushed = (abi.stack.len() + usize::from(abi.stack_this.is_some())) as u32 * 4;
    let mut unknown_exit_esp = false;
    for (address, (state, callee_cleanup)) in &facts.exits {
        let Some(esp) = state.esp else {
            unknown_exit_esp = true;
            continue;
        };
        if esp != 0 {
            report.contradictions.push(format!(
                "return at {address:#010x} is reached with ESP offset {esp:#x} instead of the entry return address"
            ));
            continue;
        }
        if callee_cleanup.saturating_add(abi.cleanup) != pushed {
            report.contradictions.push(format!(
                "return at {address:#010x} cleans {callee_cleanup:#x} bytes and the wrapper cleans {:#x}, but it pushes {pushed:#x}",
                abi.cleanup
            ));
        }
    }
    if facts.unknown_control || facts.budget_exhausted || facts.exits.is_empty() || unknown_exit_esp {
        report.unknown.push("cleanup on every exit".to_owned());
    }

    for (&slot, &address) in &facts.stack_reads {
        if slot > pushed {
            report.contradictions.push(format!(
                "instruction at {address:#010x} reads entry stack slot [esp+{slot:#x}], beyond the {pushed:#x} bytes supplied by the wrapper"
            ));
        }
    }
    for (&slot, &address) in &facts.stack_writes {
        if slot == 0 {
            report.contradictions.push(format!(
                "instruction at {address:#010x} writes the entry return address"
            ));
        } else if slot > pushed {
            report.contradictions.push(format!(
                "instruction at {address:#010x} writes entry stack slot [esp+{slot:#x}], beyond the {pushed:#x} bytes supplied by the wrapper"
            ));
        }
    }
    let unread = (1..=abi.stack.len() + usize::from(abi.stack_this.is_some()))
        .map(|index| index as u32 * 4)
        .filter(|slot| !facts.stack_reads.contains_key(slot))
        .collect::<Vec<_>>();
    if !unread.is_empty() {
        report.unknown.push(format!(
            "unobserved declared stack slots {}",
            unread
                .iter()
                .map(|slot| format!("{slot:#x}"))
                .collect::<Vec<_>>()
                .join("/")
        ));
    }
    if facts.unknown_stack || unknown_exit_esp {
        report.unknown.push("stack layout".to_owned());
    }

    for reg in Gp::ALL {
        let declared = input_masks[reg.index()];
        let observed = facts.input_reads[reg.index()];
        if observed & !declared != 0 {
            if declared == 0 {
                report.contradictions.push(format!(
                    "instruction at {:#010x} reads {} before defining it, but the wrapper supplies no value",
                    facts.input_evidence[reg.index()].unwrap_or(abi.addr),
                    reg.name()
                ));
            } else {
                report.contradictions.push(format!(
                    "instruction at {:#010x} reads more of {} than the wrapper initializes",
                    facts.input_evidence[reg.index()].unwrap_or(abi.addr),
                    reg.name()
                ));
            }
        } else if declared != 0 && observed == 0 {
            report
                .unknown
                .push(format!("{} input is not observably read", reg.name()));
        }
    }

    for (index, name) in ["st0", "st1"].into_iter().enumerate() {
        if facts.missing_x87_inputs.contains(&index) {
            report.contradictions.push(format!(
                "reachable code reads {name} before defining it, but the wrapper supplies no value"
            ));
        } else if x87_inputs[index] && !facts.x87_input_reads[index] {
            report.unknown.push(format!("{name} input is not observably read"));
        }
    }
    if facts.unknown_x87_read {
        report.unknown.push("unmodeled or unknown x87 input".to_owned());
    }
    if facts.unknown_call_contract {
        report.unknown.push("indirect or unresolved call contract".to_owned());
    }
    if facts.exits.values().any(|(state, _)| state.ambiguous_push_input) {
        report.unknown.push("ambiguous pushed register input".to_owned());
    }
    check_return(&facts, output, input_masks, &mut report);
    check_clobbers(&facts, abi, output, &mut report);

    if facts.budget_exhausted {
        report.unknown.push("analysis budget".to_owned());
    } else if facts.unknown_control {
        report.unknown.push("indirect or unsupported control flow".to_owned());
    }

    report.contradictions.sort();
    report.contradictions.dedup();
    report.unknown.sort();
    report.unknown.dedup();
    report
}

fn declared_inputs(abi: &AbiSpec) -> ([u32; 6], [bool; 2]) {
    let mut masks = [0; 6];
    let mut x87 = [false; 2];
    if let Some(binding) = &abi.this {
        masks[Gp::from(binding.reg).index()] = FULL;
    }
    for input in &abi.inputs {
        match input {
            InputBinding::Gp(binding) => masks[Gp::from(binding.reg).index()] = FULL,
            InputBinding::Byte { reg, .. } => masks[Gp::from(reg.carrier()).index()] = 0xff,
            InputBinding::X87 { reg: X87Reg::St0, .. } => x87[0] = true,
            InputBinding::X87 { reg: X87Reg::St1, .. } => x87[1] = true,
        }
    }
    (masks, x87)
}

fn check_return(facts: &Facts, output: Option<ReturnReg>, input_masks: [u32; 6], report: &mut Report) {
    let Some(output) = output else {
        return;
    };
    match output {
        ReturnReg::X87(_) => {
            if facts
                .exits
                .values()
                .any(|(state, _)| state.x87[0].state == X87State::Empty)
            {
                report
                    .contradictions
                    .push("declared ST0 return is not defined on every known return path".to_owned());
            }
            if facts.unknown_control
                || facts.budget_exhausted
                || facts.exits.is_empty()
                || facts
                    .exits
                    .values()
                    .any(|(state, _)| state.x87[0].state == X87State::Unknown)
            {
                report.unknown.push("ST0 return on every exit".to_owned());
            }
        }
        ReturnReg::Gp(reg) => check_gp_return(facts, Gp::from(reg), FULL, input_masks, report),
        ReturnReg::Ax => check_gp_return(facts, Gp::Eax, 0xffff, input_masks, report),
        ReturnReg::Byte(reg) => check_gp_return(facts, Gp::from(reg.carrier()), byte_mask(reg), input_masks, report),
    }
}

fn byte_mask(reg: ByteReg) -> u32 {
    match reg {
        ByteReg::Al | ByteReg::Bl | ByteReg::Cl | ByteReg::Dl => 0xff,
    }
}

fn check_gp_return(facts: &Facts, reg: Gp, mask: u32, input_masks: [u32; 6], report: &mut Report) {
    let mut uncertain = facts.unknown_control || facts.budget_exhausted || facts.exits.is_empty();
    for (state, _) in facts.exits.values() {
        let value = state.regs[reg.index()];
        if value.unknown & mask != 0 {
            uncertain = true;
        } else if (value.defined | input_masks[reg.index()]) & mask != mask {
            report.contradictions.push(format!(
                "declared {} return is not defined on every known return path",
                reg.name()
            ));
        }
    }
    if uncertain {
        report.unknown.push(format!("{} return on every exit", reg.name()));
    }
}

fn check_clobbers(facts: &Facts, abi: &AbiSpec, output: Option<ReturnReg>, report: &mut Report) {
    if facts.unknown_control || facts.budget_exhausted || facts.exits.is_empty() {
        report.unknown.push("complete clobber set".to_owned());
        return;
    }
    let declared = abi.clobbers.iter().copied().map(Gp::from).collect::<BTreeSet<_>>();
    let return_carrier = output.and_then(ReturnReg::carrier).map(Gp::from);
    let mut unknown = Vec::new();
    for reg in Gp::ALL {
        if declared.contains(&reg) || return_carrier == Some(reg) {
            continue;
        }
        let all_changed = facts
            .exits
            .values()
            .all(|(state, _)| state.regs[reg.index()].change == Change::Changed);
        let all_preserved = facts
            .exits
            .values()
            .all(|(state, _)| state.regs[reg.index()].change == Change::Preserved);
        if all_changed {
            report.contradictions.push(format!(
                "{} is changed on every return path but is absent from clobber",
                reg.name()
            ));
        } else if !all_preserved {
            unknown.push(reg.name());
        }
    }
    if !unknown.is_empty() {
        report
            .unknown
            .push(format!("unresolved clobbers {}", unknown.join("/")));
    }
}

#[cfg(test)]
mod tests;
