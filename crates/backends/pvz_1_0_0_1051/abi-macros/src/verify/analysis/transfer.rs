use super::*;

pub(super) fn transfer(
    instruction: &Instruction, flow: FlowControl, call_summary: Option<CallSummary>, state: &mut State,
    facts: &mut Facts, info_factory: &mut InstructionInfoFactory,
) {
    let info = info_factory.info(instruction);
    let save_only = is_save_only(instruction, state);
    let register_reads_defined = info
        .used_registers()
        .iter()
        .all(|used| !reads(used.access()) || register_read_is_defined(used.register(), state));
    let read_flags = instruction.rflags_read();
    let flags_read_defined = state.defined_flags & read_flags == read_flags;
    let inputs_defined = register_reads_defined && flags_read_defined;
    let flags_input_independent = input_independent_flags(instruction, info, state);
    for used in info.used_registers() {
        if reads(used.access())
            && !save_only
            && !input_independent_write(instruction, used.register())
            && !is_register_nop(instruction, used.register())
            && let Some((reg, mask)) = gp_register(used.register())
        {
            for origin in Gp::ALL {
                let input_mask = mask & state.regs[reg.index()].origins[origin.index()];
                if input_mask != 0 {
                    facts.input_reads[origin.index()] |= input_mask;
                    facts.input_evidence[origin.index()].get_or_insert_with(|| instruction.ip32());
                }
            }
        }
        if reads(used.access()) && is_x87(used.register()) {
            if let Some(index) = x87_index(used.register()) {
                match state.x87[index] {
                    X87Value {
                        origin: Some(X87Reg::St0),
                        ..
                    } => facts.x87_input_reads[0] = true,
                    X87Value {
                        origin: Some(X87Reg::St1),
                        ..
                    } => facts.x87_input_reads[1] = true,
                    X87Value {
                        state: X87State::Empty, ..
                    } => {
                        facts.missing_x87_inputs.insert(index);
                    }
                    X87Value {
                        state: X87State::Unknown,
                        ..
                    } => facts.unknown_x87_read = true,
                    _ => {}
                }
            } else {
                facts.unknown_x87_read = true;
            }
        }
    }
    for operand in 0..instruction.op_count() {
        if instruction.op_kind(operand) != OpKind::Memory {
            continue;
        }
        let access = info.op_access(operand);
        if reads(access) {
            record_stack_access(instruction, state, facts, false);
        }
        if writes(access) {
            record_stack_access(instruction, state, facts, true);
        }
    }
    invalidate_written_stack(instruction, state, info);

    if matches!(flow, FlowControl::Call | FlowControl::IndirectCall) {
        if flow == FlowControl::Call
            && let Some(summary) = call_summary
        {
            record_call_stack_inputs(summary, state, facts, instruction.ip32());
        }
        expose_pushed_registers(state);
        if flow == FlowControl::IndirectCall
            || call_summary.is_none_or(|summary| summary.cleanup.is_none() || summary.unresolved_inputs)
        {
            facts.unknown_call_contract = true;
        }
        if let Some(summary) = call_summary {
            for reg in Gp::ALL {
                match summary.changes[reg.index()] {
                    Change::Preserved => {}
                    change => state.regs[reg.index()].apply_call(change),
                }
            }
        } else {
            for reg in Gp::ALL {
                state.regs[reg.index()].make_unknown();
            }
        }
        state.defined_flags = 0;
        if flow == FlowControl::Call
            && let Some(cleanup) = call_summary.and_then(|summary| summary.cleanup)
        {
            move_esp_up(state, cleanup as i32);
        } else {
            state.esp = None;
            state.logical_stack = None;
        }
        state.x87 = [X87Value::UNKNOWN; 2];
        return;
    }

    match instruction.mnemonic() {
        Mnemonic::Push => {
            let saved = (instruction.op0_kind() == OpKind::Register)
                .then(|| gp_register(instruction.op0_register()))
                .flatten()
                .filter(|(_, mask)| *mask == FULL)
                .map(|(reg, _)| state.regs[reg.index()]);
            let value = saved.map_or(StackValue::Other, |value| StackValue::Register {
                value,
                exposed_to_call: false,
            });
            if let Some(stack) = &mut state.logical_stack {
                stack.push(value);
            }
            if let Some(esp) = state.esp {
                let new_esp = esp - 4;
                state.stack.insert(new_esp, value);
                state.esp = Some(new_esp);
            }
            return;
        }
        Mnemonic::Pop => {
            let logical = pop_logical_stack(state);
            if let Some(esp) = state.esp {
                let saved = state.stack.get(&esp).copied().or(logical);
                if instruction.op0_kind() == OpKind::Register
                    && let Some((reg, mask)) = gp_register(instruction.op0_register())
                    && mask == FULL
                {
                    let restored = stack_value_for_register(saved, reg);
                    if saved.is_some_and(is_exposed_input) && restored.change != Change::Preserved {
                        state.ambiguous_push_input = true;
                    }
                    state.regs[reg.index()] = restored;
                } else {
                    discard_stack_value(state, saved);
                }
                state.stack.remove(&esp);
                state.esp = Some(esp + 4);
            } else if instruction.op0_kind() == OpKind::Register
                && let Some((reg, mask)) = gp_register(instruction.op0_register())
                && mask == FULL
            {
                if let Some(value) = logical {
                    let restored = stack_value_for_register(Some(value), reg);
                    if is_exposed_input(value) && restored.change != Change::Preserved {
                        state.ambiguous_push_input = true;
                    }
                    state.regs[reg.index()] = restored;
                } else {
                    state.regs[reg.index()].make_unknown();
                }
            } else {
                discard_stack_value(state, logical);
            }
            if instruction.op0_kind() == OpKind::Register && instruction.op0_register() == IcedReg::EBP {
                state.ebp = None;
                state.ebp_logical_depth = None;
            }
            return;
        }
        Mnemonic::Leave => {
            reset_logical_to_ebp(state);
            let _ = pop_logical_stack(state);
            if let Some(ebp) = state.ebp {
                state.esp = Some(ebp + 4);
            } else {
                state.esp = None;
            }
            state.ebp = None;
            state.ebp_logical_depth = None;
            return;
        }
        _ => {}
    }

    let fpu = instruction.fpu_stack_increment_info();
    if fpu.writes_top() {
        if fpu.conditional() {
            state.x87 = [X87Value::UNKNOWN; 2];
        } else {
            match fpu.increment() {
                -1 => {
                    let pushed = pushed_x87_value(instruction, state);
                    let old_st0 = state.x87[0];
                    state.x87[1] = old_st0;
                    state.x87[0] = pushed;
                }
                1 => {
                    if instruction.mnemonic() == Mnemonic::Fstp
                        && instruction.op0_kind() == OpKind::Register
                        && instruction.op0_register() == IcedReg::ST1
                    {
                        state.x87[1] = X87Value::UNKNOWN;
                    } else {
                        state.x87 = [X87Value::UNKNOWN; 2];
                    }
                }
                2 => {
                    state.x87 = [X87Value::UNKNOWN; 2];
                }
                0 if matches!(instruction.mnemonic(), Mnemonic::Fninit | Mnemonic::Finit) => {
                    state.x87 = [X87Value::EMPTY; 2];
                }
                _ => {
                    state.x87 = [X87Value::UNKNOWN; 2];
                }
            }
        }
    } else {
        let reads_known = info.used_registers().iter().all(|used| {
            !reads(used.access())
                || !is_x87(used.register())
                || x87_index(used.register()).is_some_and(|index| state.x87[index].state == X87State::Known)
        });
        let mut unknown_x87_write = false;
        for used in info.used_registers() {
            if writes(used.access()) && is_x87(used.register()) {
                if let Some(index) = x87_index(used.register()) {
                    state.x87[index] = X87Value {
                        state: if reads_known && reads(used.access()) {
                            X87State::Known
                        } else {
                            X87State::Unknown
                        },
                        origin: None,
                    };
                } else {
                    unknown_x87_write = true;
                }
            }
        }
        if unknown_x87_write {
            state.x87 = [X87Value::UNKNOWN; 2];
        }
    }

    if instruction.op0_kind() == OpKind::Register
        && instruction.op0_register() == IcedReg::EBP
        && !(instruction.mnemonic() == Mnemonic::Mov
            && instruction.op1_kind() == OpKind::Register
            && instruction.op1_register() == IcedReg::ESP)
    {
        state.ebp = None;
        state.ebp_logical_depth = None;
    }

    if instruction.mnemonic() == Mnemonic::Mov {
        if instruction.op0_kind() == OpKind::Register && instruction.op1_kind() == OpKind::Register {
            if instruction.op0_register() == IcedReg::EBP && instruction.op1_register() == IcedReg::ESP {
                state.ebp = state.esp;
                state.ebp_logical_depth = state.logical_stack.as_ref().map(Vec::len);
            } else if instruction.op0_register() == IcedReg::ESP && instruction.op1_register() == IcedReg::EBP {
                state.esp = state.ebp;
                reset_logical_to_ebp(state);
            } else if let (Some((dst, FULL)), Some((src, FULL))) = (
                gp_register(instruction.op0_register()),
                gp_register(instruction.op1_register()),
            ) {
                let mut value = state.regs[src.index()];
                value.rebase_for(dst);
                state.regs[dst.index()] = value;
                return;
            }
        }

        if instruction.op0_kind() == OpKind::Memory && instruction.op1_kind() == OpKind::Register {
            if let Some((src, FULL)) = gp_register(instruction.op1_register()) {
                let value = StackValue::Register {
                    value: state.regs[src.index()],
                    exposed_to_call: false,
                };
                set_logical_stack_value(instruction, state, value);
                if let Some(offset) = instruction_stack_offset(instruction, state)
                    && offset <= 0
                {
                    state.stack.insert(offset, value);
                }
            }
        } else if instruction.op0_kind() == OpKind::Memory {
            if let Some(offset) = instruction_stack_offset(instruction, state)
                && offset <= 0
            {
                state.stack.insert(offset, StackValue::Other);
            }
        } else if instruction.op0_kind() == OpKind::Register
            && instruction.op1_kind() == OpKind::Memory
            && let Some((dst, FULL)) = gp_register(instruction.op0_register())
            && let Some(offset) = instruction_stack_offset(instruction, state)
            && let Some(saved) = state.stack.get(&offset).copied()
        {
            state.regs[dst.index()] = stack_value_for_register(Some(saved), dst);
            return;
        }

        if instruction.op0_kind() == OpKind::Register
            && instruction.op1_kind() == OpKind::Memory
            && let Some((dst, FULL)) = gp_register(instruction.op0_register())
            && let Some(saved) = logical_stack_value(instruction, state)
        {
            state.regs[dst.index()] = stack_value_for_register(Some(saved), dst);
            return;
        }
    }

    update_stack_pointer(instruction, state);
    for used in info.used_registers() {
        if writes(used.access())
            && !preserves_register_value(instruction, used.register())
            && let Some((reg, mask)) = gp_register(used.register())
        {
            state.regs[reg.index()].write(
                mask,
                used.access(),
                inputs_defined,
                input_independent_write(instruction, used.register()),
            );
        }
    }
    let modified_flags = instruction.rflags_modified();
    state.defined_flags &= !modified_flags;
    if inputs_defined || flags_input_independent {
        state.defined_flags |= instruction.rflags_written() | instruction.rflags_cleared() | instruction.rflags_set();
    }
}

fn pushed_x87_value(instruction: &Instruction, state: &State) -> X87Value {
    if instruction.op0_kind() == OpKind::Memory {
        return X87Value {
            state: X87State::Known,
            origin: None,
        };
    }
    if instruction.mnemonic() == Mnemonic::Fld && instruction.op0_kind() == OpKind::Register {
        return x87_index(instruction.op0_register()).map_or(X87Value::UNKNOWN, |index| state.x87[index]);
    }
    if matches!(
        instruction.mnemonic(),
        Mnemonic::Fld1
            | Mnemonic::Fldl2t
            | Mnemonic::Fldl2e
            | Mnemonic::Fldpi
            | Mnemonic::Fldlg2
            | Mnemonic::Fldln2
            | Mnemonic::Fldz
    ) {
        return X87Value {
            state: X87State::Known,
            origin: None,
        };
    }
    X87Value::UNKNOWN
}

fn is_save_only(instruction: &Instruction, state: &State) -> bool {
    if instruction.mnemonic() == Mnemonic::Push {
        return instruction.op0_kind() == OpKind::Register
            && gp_register(instruction.op0_register()).is_some_and(|(_, mask)| mask == FULL);
    }
    instruction.mnemonic() == Mnemonic::Mov
        && instruction.op0_kind() == OpKind::Memory
        && instruction.op1_kind() == OpKind::Register
        && instruction_stack_offset(instruction, state).is_some_and(|offset| offset <= 0)
}

fn stack_value_for_register(value: Option<StackValue>, destination: Gp) -> RegState {
    match value {
        Some(StackValue::Register { mut value, .. }) => {
            value.rebase_for(destination);
            value
        }
        Some(StackValue::Other) => RegState {
            origins: [0; 6],
            defined: FULL,
            unknown: 0,
            change: Change::Changed,
        },
        Some(StackValue::Unknown | StackValue::Alignment) | None => RegState {
            origins: [0; 6],
            defined: 0,
            unknown: FULL,
            change: Change::Unknown,
        },
    }
}

fn expose_pushed_registers(state: &mut State) {
    for value in state.stack.values_mut().chain(state.logical_stack.iter_mut().flatten()) {
        if let StackValue::Register { exposed_to_call, .. } = value {
            *exposed_to_call = true;
        }
    }
}

fn record_call_stack_inputs(summary: CallSummary, state: &State, facts: &mut Facts, address: u32) {
    for index in 0..64 {
        if summary.stack_inputs & (1u64 << index) == 0 {
            continue;
        }
        let logical = state
            .logical_stack
            .as_ref()
            .and_then(|stack| stack.len().checked_sub(index + 1).and_then(|slot| stack.get(slot)))
            .copied();
        let absolute = state.esp.and_then(|esp| {
            let offset = esp.checked_add(i32::try_from(index).ok()?.checked_mul(4)?)?;
            state.stack.get(&offset).copied()
        });
        match logical.or(absolute) {
            Some(StackValue::Register { value, .. }) => {
                for origin in Gp::ALL {
                    let mask = value.origins[origin.index()];
                    if mask != 0 {
                        facts.input_reads[origin.index()] |= mask;
                        facts.input_evidence[origin.index()].get_or_insert(address);
                    }
                }
            }
            Some(StackValue::Other) => {}
            Some(StackValue::Unknown | StackValue::Alignment) | None => {
                facts.unknown_call_contract = true;
            }
        }
    }
}

fn is_exposed_input(value: StackValue) -> bool {
    matches!(
        value,
        StackValue::Register {
            value: RegState { origins, .. },
            exposed_to_call: true,
        } if origins.into_iter().any(|mask| mask != 0)
    )
}

fn discard_stack_value(state: &mut State, value: Option<StackValue>) {
    if value.is_some_and(is_exposed_input) {
        state.ambiguous_push_input = true;
    }
}

pub(super) fn is_alignment(value: StackValue) -> bool {
    value == StackValue::Alignment
}

fn pop_logical_stack(state: &mut State) -> Option<StackValue> {
    let value = state.logical_stack.as_mut()?.pop()?;
    if is_alignment(value) {
        state.logical_stack = None;
        None
    } else {
        Some(value)
    }
}

fn reset_logical_to_ebp(state: &mut State) {
    let Some(depth) = state.ebp_logical_depth else {
        state.logical_stack = None;
        return;
    };
    let Some(stack) = &mut state.logical_stack else {
        return;
    };
    if depth <= stack.len() {
        if stack[depth..].iter().copied().any(is_exposed_input) {
            state.ambiguous_push_input = true;
        }
        stack.truncate(depth);
    } else {
        state.logical_stack = None;
    }
}

fn input_independent_write(instruction: &Instruction, register: IcedReg) -> bool {
    if instruction.op0_kind() != OpKind::Register || instruction.op0_register() != register {
        return false;
    }
    match instruction.mnemonic() {
        Mnemonic::And => immediate_i32(instruction) == Some(0),
        Mnemonic::Or => immediate_is_all_ones(instruction, register),
        Mnemonic::Sub | Mnemonic::Xor => {
            instruction.op1_kind() == OpKind::Register && instruction.op1_register() == register
        }
        _ => false,
    }
}

fn register_read_is_defined(register: IcedReg, state: &State) -> bool {
    if let Some((reg, mask)) = gp_register(register) {
        let value = state.regs[reg.index()];
        return value.defined & !value.unknown & mask == mask;
    }
    if let Some(index) = x87_index(register) {
        return state.x87[index].state == X87State::Known;
    }
    if is_x87(register) {
        return false;
    }
    match register {
        IcedReg::ESP | IcedReg::SP => true,
        IcedReg::EBP | IcedReg::BP => state.ebp.is_some(),
        IcedReg::EIP | IcedReg::ES | IcedReg::CS | IcedReg::SS | IcedReg::DS | IcedReg::FS | IcedReg::GS => true,
        _ => false,
    }
}

fn input_independent_flags(instruction: &Instruction, info: &iced_x86::InstructionInfo, state: &State) -> bool {
    info.used_registers().iter().all(|used| {
        !reads(used.access())
            || gp_register(used.register()).map_or_else(
                || register_read_is_defined(used.register(), state),
                |_| input_independent_write(instruction, used.register()),
            )
    })
}

fn is_register_nop(instruction: &Instruction, register: IcedReg) -> bool {
    instruction.op0_kind() == OpKind::Register
        && instruction.op0_register() == register
        && match instruction.mnemonic() {
            Mnemonic::Mov => instruction.op1_kind() == OpKind::Register && instruction.op1_register() == register,
            Mnemonic::Lea => {
                instruction.op1_kind() == OpKind::Memory
                    && instruction.memory_base() == register
                    && instruction.memory_index() == IcedReg::None
                    && instruction.memory_displacement64() == 0
            }
            _ => false,
        }
}

fn preserves_register_value(instruction: &Instruction, register: IcedReg) -> bool {
    if instruction.op0_kind() != OpKind::Register || instruction.op0_register() != register {
        return false;
    }
    match instruction.mnemonic() {
        Mnemonic::And | Mnemonic::Or
            if instruction.op1_kind() == OpKind::Register && instruction.op1_register() == register =>
        {
            true
        }
        Mnemonic::Add | Mnemonic::Or | Mnemonic::Sub | Mnemonic::Xor => immediate_i32(instruction) == Some(0),
        Mnemonic::And => immediate_is_all_ones(instruction, register),
        Mnemonic::Lea => {
            instruction.op1_kind() == OpKind::Memory
                && instruction.memory_base() == register
                && instruction.memory_index() == IcedReg::None
                && instruction.memory_displacement64() == 0
        }
        _ => false,
    }
}

fn immediate_is_all_ones(instruction: &Instruction, register: IcedReg) -> bool {
    let Some((_, mask)) = gp_register(register) else {
        return false;
    };
    let value = match instruction.op1_kind() {
        OpKind::Immediate8 => u32::from(instruction.immediate8()),
        OpKind::Immediate8to16 => instruction.immediate8to16() as u16 as u32,
        OpKind::Immediate8to32 => instruction.immediate8to32() as u32,
        OpKind::Immediate16 => u32::from(instruction.immediate16()),
        OpKind::Immediate32 => instruction.immediate32(),
        _ => return false,
    };
    let immediate_mask = mask >> mask.trailing_zeros();
    value & immediate_mask == immediate_mask
}

fn update_stack_pointer(instruction: &Instruction, state: &mut State) {
    if instruction.op0_kind() != OpKind::Register || instruction.op0_register() != IcedReg::ESP {
        return;
    }
    match instruction.mnemonic() {
        Mnemonic::Add => {
            if let Some(value) = immediate_i32(instruction) {
                move_esp_up(state, value);
            } else {
                state.esp = None;
            }
        }
        Mnemonic::Sub => {
            if let Some(value) = immediate_i32(instruction) {
                adjust_logical_esp(state, -value);
                state.esp = state.esp.and_then(|esp| esp.checked_sub(value));
            } else {
                state.esp = None;
                state.logical_stack = None;
            }
        }
        Mnemonic::Lea
            if instruction.op1_kind() == OpKind::Memory
                && instruction.memory_base() == IcedReg::ESP
                && instruction.memory_index() == IcedReg::None =>
        {
            move_esp_up(state, instruction.memory_displacement64() as u32 as i32);
        }
        Mnemonic::Mov if instruction.op1_kind() == OpKind::Register && instruction.op1_register() == IcedReg::EBP => {
            reset_logical_to_ebp(state);
            set_esp(state, state.ebp);
        }
        Mnemonic::And => {
            if let Some(stack) = &mut state.logical_stack {
                stack.push(StackValue::Alignment);
            }
            state.esp = None;
        }
        Mnemonic::Mov => {
            state.esp = None;
            state.logical_stack = None;
        }
        _ => {
            state.esp = None;
            state.logical_stack = None;
        }
    }
}

fn move_esp_up(state: &mut State, amount: i32) {
    adjust_logical_esp(state, amount);
    let new_esp = state.esp.and_then(|esp| esp.checked_add(amount));
    set_esp(state, new_esp);
}

fn adjust_logical_esp(state: &mut State, amount: i32) {
    if amount % 4 != 0 {
        state.logical_stack = None;
        return;
    }
    if amount > 0 {
        for _ in 0..amount / 4 {
            let value = pop_logical_stack(state);
            if value.is_none() {
                state.logical_stack = None;
                break;
            }
            discard_stack_value(state, value);
        }
    } else if amount < 0
        && let Some(stack) = &mut state.logical_stack
    {
        stack.extend(std::iter::repeat_n(StackValue::Other, (-amount / 4) as usize));
    }
}

fn set_esp(state: &mut State, new_esp: Option<i32>) {
    if let (Some(old), Some(new)) = (state.esp, new_esp)
        && new > old
    {
        state.stack.retain(|offset, _| *offset < old || *offset >= new);
    }
    state.esp = new_esp;
}

fn immediate_i32(instruction: &Instruction) -> Option<i32> {
    Some(match instruction.op1_kind() {
        OpKind::Immediate8 => i32::from(instruction.immediate8()),
        OpKind::Immediate8to16 => i32::from(instruction.immediate8to16()),
        OpKind::Immediate8to32 => instruction.immediate8to32(),
        OpKind::Immediate16 => i32::from(instruction.immediate16()),
        OpKind::Immediate32 => instruction.immediate32() as i32,
        _ => return None,
    })
}

fn instruction_stack_offset(instruction: &Instruction, state: &State) -> Option<i32> {
    stack_offset(
        state,
        instruction.memory_base(),
        instruction.memory_index(),
        instruction.memory_displacement64(),
    )
}

fn record_stack_access(instruction: &Instruction, state: &State, facts: &mut Facts, write: bool) {
    let Some(offset) = instruction_stack_offset(instruction, state) else {
        if matches!(instruction.memory_base(), IcedReg::ESP | IcedReg::EBP) && !logical_stack_covers(instruction, state)
        {
            facts.unknown_stack = true;
        }
        return;
    };
    let size = instruction.memory_size().size();
    if size == 0 {
        facts.unknown_stack = true;
        return;
    }
    let end = offset.saturating_add(size as i32 - 1);
    if end < 0 || (!write && end == 0) {
        return;
    }
    if offset % 4 != 0 {
        facts.unknown_stack = true;
        return;
    }
    for index in 0..size.div_ceil(4) {
        let slot = offset.saturating_add(index as i32 * 4);
        if (write && slot >= 0) || (!write && slot > 0) {
            let accesses = if write {
                &mut facts.stack_writes
            } else {
                &mut facts.stack_reads
            };
            accesses.entry(slot as u32).or_insert_with(|| instruction.ip32());
        }
    }
}

fn logical_stack_covers(instruction: &Instruction, state: &State) -> bool {
    let size = instruction.memory_size().size();
    let displacement = instruction.memory_displacement64() as u32 as i32;
    size != 0
        && displacement % 4 == 0
        && (0..size.div_ceil(4)).all(|word| {
            logical_stack_index(
                state,
                instruction.memory_base(),
                instruction.memory_index(),
                displacement.saturating_add(word as i32 * 4),
            )
            .is_some()
        })
}

fn invalidate_written_stack(instruction: &Instruction, state: &mut State, info: &iced_x86::InstructionInfo) {
    let Some(access) = (0..instruction.op_count())
        .filter(|operand| instruction.op_kind(*operand) == OpKind::Memory)
        .map(|operand| info.op_access(operand))
        .find(|access| writes(*access))
    else {
        return;
    };
    let replacement = if conditional_write(access) {
        StackValue::Unknown
    } else {
        StackValue::Other
    };
    invalidate_logical_stack_write(instruction, state, replacement);
    let Some(offset) = instruction_stack_offset(instruction, state) else {
        return;
    };
    let size = instruction.memory_size().size();
    if size == 0 {
        if let Some(value) = state.stack.get_mut(&offset) {
            *value = StackValue::Unknown;
        }
        return;
    }
    let end = offset.saturating_add(size as i32);
    for (&slot, value) in &mut state.stack {
        if offset < slot.saturating_add(4) && slot < end {
            *value = replacement;
        }
    }
}

fn invalidate_logical_stack_write(instruction: &Instruction, state: &mut State, replacement: StackValue) {
    let size = instruction.memory_size().size();
    let displacement = instruction.memory_displacement64() as u32 as i32;
    if size == 0 || displacement % 4 != 0 {
        invalidate_possible_stack_write(instruction, state);
        if matches!(instruction.memory_base(), IcedReg::ESP | IcedReg::EBP) {
            state.logical_stack = None;
        }
        return;
    }
    for offset in (0..size.div_ceil(4)).map(|word| displacement.saturating_add(word as i32 * 4)) {
        let Some(index) = logical_stack_index(state, instruction.memory_base(), instruction.memory_index(), offset)
        else {
            invalidate_possible_stack_write(instruction, state);
            continue;
        };
        if let Some(stack) = &mut state.logical_stack {
            stack[index] = replacement;
        }
    }
}

fn invalidate_possible_stack_write(instruction: &Instruction, state: &mut State) {
    let position_unknown = match instruction.memory_base() {
        IcedReg::ESP => state.esp.is_none() || instruction.memory_index() != IcedReg::None,
        IcedReg::EBP => state.ebp.is_none() || instruction.memory_index() != IcedReg::None,
        base => gp_register(base).is_some() || gp_register(instruction.memory_index()).is_some(),
    };
    if position_unknown {
        forget_saved_stack(state);
    }
}

fn forget_saved_stack(state: &mut State) {
    state.logical_stack = None;
    for value in state.stack.values_mut() {
        *value = StackValue::Unknown;
    }
}

fn logical_stack_value(instruction: &Instruction, state: &State) -> Option<StackValue> {
    if instruction.memory_size().size() != 4 {
        return None;
    }
    let index = logical_stack_index(
        state,
        instruction.memory_base(),
        instruction.memory_index(),
        instruction.memory_displacement64() as u32 as i32,
    )?;
    state.logical_stack.as_ref()?.get(index).copied()
}

fn set_logical_stack_value(instruction: &Instruction, state: &mut State, value: StackValue) {
    if instruction.memory_size().size() != 4 {
        return;
    }
    let Some(index) = logical_stack_index(
        state,
        instruction.memory_base(),
        instruction.memory_index(),
        instruction.memory_displacement64() as u32 as i32,
    ) else {
        return;
    };
    if let Some(stack) = &mut state.logical_stack {
        stack[index] = value;
    }
}

pub(super) fn logical_stack_index(state: &State, base: IcedReg, index: IcedReg, displacement: i32) -> Option<usize> {
    if index != IcedReg::None || displacement % 4 != 0 {
        return None;
    }
    let logical = state.logical_stack.as_ref()?;
    let index = match base {
        IcedReg::ESP if displacement >= 0 => {
            let index = logical.len() as i64 - 1 - i64::from(displacement / 4);
            if logical
                .iter()
                .rposition(|value| is_alignment(*value))
                .is_some_and(|alignment| index <= alignment as i64)
            {
                return None;
            }
            index
        }
        IcedReg::EBP => {
            let index = i64::try_from(state.ebp_logical_depth?).ok()? - 1 - i64::from(displacement / 4);
            if logical
                .iter()
                .position(|value| is_alignment(*value))
                .is_some_and(|alignment| index >= alignment as i64)
            {
                return None;
            }
            index
        }
        _ => return None,
    };
    (index >= 0 && index < logical.len() as i64).then_some(index as usize)
}

fn stack_offset(state: &State, base: IcedReg, index: IcedReg, displacement: u64) -> Option<i32> {
    if index != IcedReg::None {
        return None;
    }
    let base = match base {
        IcedReg::ESP => state.esp?,
        IcedReg::EBP => state.ebp?,
        _ => return None,
    };
    Some(base.wrapping_add(displacement as u32 as i32))
}

fn reads(access: OpAccess) -> bool {
    matches!(
        access,
        OpAccess::Read | OpAccess::CondRead | OpAccess::ReadWrite | OpAccess::ReadCondWrite
    )
}

pub(super) fn writes(access: OpAccess) -> bool {
    matches!(
        access,
        OpAccess::Write | OpAccess::CondWrite | OpAccess::ReadWrite | OpAccess::ReadCondWrite
    )
}

pub(super) fn conditional_write(access: OpAccess) -> bool {
    matches!(access, OpAccess::CondWrite | OpAccess::ReadCondWrite)
}
