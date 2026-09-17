use super::*;

pub(super) fn traverse(
    image: &TextImage, entry: u32, initial: State, cache: &mut AnalysisCache, call_depth: usize,
) -> Facts {
    let mut facts = Facts::default();
    let mut states = BTreeMap::from([(entry, initial)]);
    let mut decoded = BTreeMap::<u32, Instruction>::new();
    let mut incoming = BTreeMap::<u32, BTreeSet<u32>>::new();
    let mut jump_table_proofs = BTreeMap::<u32, Vec<(u32, u32)>>::new();
    let mut queue = VecDeque::from([entry]);
    let mut info_factory = InstructionInfoFactory::new();
    let mut edges = 0usize;
    let mut propagations = 0usize;

    while let Some(address) = queue.pop_front() {
        if propagations >= MAX_PROPAGATIONS {
            facts.budget_exhausted = true;
            break;
        }
        propagations += 1;
        let Some(mut state) = states.get(&address).cloned() else {
            continue;
        };

        let instruction = if let Some(instruction) = decoded.get(&address) {
            *instruction
        } else {
            if decoded.len() >= MAX_INSTRUCTIONS {
                facts.budget_exhausted = true;
                break;
            }
            let Some(instruction) = image.decode(address) else {
                facts.unknown_control = true;
                continue;
            };
            if overlaps_existing(&decoded, address, instruction.len() as u32) {
                facts.unknown_control = true;
                continue;
            }
            decoded.insert(address, instruction);
            instruction
        };

        let flow = instruction.flow_control();
        let call_target = (flow == FlowControl::Call)
            .then(|| direct_target(&instruction))
            .flatten();
        let call_summary = call_target.map(|target| summarize_call(image, target, cache, call_depth + 1));
        transfer(
            &instruction,
            flow,
            call_summary,
            &mut state,
            &mut facts,
            &mut info_factory,
        );

        if flow == FlowControl::Return {
            let Some(cleanup) = return_cleanup(&instruction) else {
                facts.unknown_control = true;
                continue;
            };
            facts
                .exits
                .entry(address)
                .and_modify(|(old, _)| {
                    old.join(&state);
                })
                .or_insert((state, cleanup));
            continue;
        }

        let next = instruction.next_ip32();
        let successors = match flow {
            FlowControl::Next | FlowControl::Call | FlowControl::IndirectCall => vec![next],
            FlowControl::ConditionalBranch => {
                direct_target(&instruction).map_or_else(Vec::new, |target| vec![next, target])
            }
            FlowControl::UnconditionalBranch => {
                direct_target(&instruction).map_or_else(Vec::new, |target| vec![target])
            }
            FlowControl::IndirectBranch => {
                if let Some(table) = resolve_jump_table(image, &decoded, &instruction) {
                    jump_table_proofs.insert(address, table.expected_incoming);
                    table.targets
                } else {
                    Vec::new()
                }
            }
            FlowControl::Interrupt | FlowControl::XbeginXabortXend | FlowControl::Exception => Vec::new(),
            FlowControl::Return => unreachable!(),
        };
        if successors.is_empty() {
            facts.unknown_control = true;
            continue;
        }
        for successor in successors {
            if edges >= MAX_EDGES {
                facts.budget_exhausted = true;
                break;
            }
            edges += 1;
            incoming.entry(successor).or_default().insert(address);
            let changed = match states.get_mut(&successor) {
                Some(old) => old.join(&state),
                None => {
                    states.insert(successor, state.clone());
                    true
                }
            };
            if changed {
                queue.push_back(successor);
            }
        }
        if facts.budget_exhausted {
            break;
        }
    }

    if !jump_table_proofs_are_valid(&jump_table_proofs, &incoming, entry) {
        facts.unknown_control = true;
    }

    facts
}

fn overlaps_existing(decoded: &BTreeMap<u32, Instruction>, address: u32, len: u32) -> bool {
    let end = address.saturating_add(len);
    decoded.iter().any(|(other, instruction)| {
        let other_end = other.saturating_add(instruction.len() as u32);
        address < other_end && *other < end
    })
}

fn direct_target(instruction: &Instruction) -> Option<u32> {
    matches!(
        instruction.op0_kind(),
        OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64
    )
    .then(|| instruction.near_branch_target() as u32)
}

struct ResolvedJumpTable {
    targets: Vec<u32>,
    expected_incoming: Vec<(u32, u32)>,
}

fn resolve_jump_table(
    image: &TextImage, decoded: &BTreeMap<u32, Instruction>, jump: &Instruction,
) -> Option<ResolvedJumpTable> {
    if jump.op0_kind() != OpKind::Memory
        || jump.memory_base() != IcedReg::None
        || jump.memory_index() == IcedReg::None
        || jump.memory_index_scale() != 4
    {
        return None;
    }

    let predecessors = linear_predecessors(decoded, jump.ip32(), 8);
    let (values, compare_index) = if predecessors.first()?.flow_control() == FlowControl::ConditionalBranch {
        let (max, compare_index) = compared_max(&predecessors, 0, jump.memory_index())?;
        ((0..=max).collect::<Vec<_>>(), compare_index)
    } else {
        let remap = predecessors.first()?;
        if remap.mnemonic() != Mnemonic::Movzx
            || remap.op0_kind() != OpKind::Register
            || remap.op0_register() != jump.memory_index()
            || remap.op1_kind() != OpKind::Memory
            || remap.memory_size().size() != 1
            || remap.memory_index() != IcedReg::None
            || remap.memory_base() == IcedReg::None
        {
            return None;
        }
        let (max, compare_index) = compared_max(&predecessors, 1, remap.memory_base())?;
        let values = (0..=max)
            .map(|index| image.read_u8((remap.memory_displacement64() as u32).checked_add(index)?))
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .map(u32::from)
            .collect();
        (values, compare_index)
    };
    let branch_index = usize::from(predecessors[0].flow_control() != FlowControl::ConditionalBranch);
    let branch_target = direct_target(&predecessors[branch_index])?;
    if branch_target == jump.ip32()
        || predecessors[..=branch_index]
            .iter()
            .any(|instruction| instruction.ip32() == branch_target)
    {
        return None;
    }

    let table = jump.memory_displacement64() as u32;
    let targets = values
        .into_iter()
        .map(|index| image.read_u32(table.checked_add(index.checked_mul(4)?)?))
        .collect::<Option<BTreeSet<_>>>()?;
    if targets.is_empty()
        || targets
            .iter()
            .any(|target| !image.contains(*target) || image.decode(*target).is_none())
    {
        return None;
    }
    let mut expected_incoming = vec![(jump.ip32(), predecessors[0].ip32())];
    expected_incoming.extend(
        predecessors[..=compare_index]
            .windows(2)
            .map(|pair| (pair[0].ip32(), pair[1].ip32())),
    );
    Some(ResolvedJumpTable {
        targets: targets.into_iter().collect(),
        expected_incoming,
    })
}

fn linear_predecessors(decoded: &BTreeMap<u32, Instruction>, mut address: u32, limit: usize) -> Vec<Instruction> {
    let mut result = Vec::new();
    for _ in 0..limit {
        let mut matches = decoded
            .values()
            .filter(|instruction| instruction.next_ip32() == address && has_fallthrough(instruction.flow_control()));
        let Some(previous) = matches.next().copied() else {
            break;
        };
        if matches.next().is_some() {
            break;
        }
        result.push(previous);
        address = previous.ip32();
    }
    result
}

fn has_fallthrough(flow: FlowControl) -> bool {
    matches!(
        flow,
        FlowControl::Next | FlowControl::Call | FlowControl::IndirectCall | FlowControl::ConditionalBranch
    )
}

fn compared_max(predecessors: &[Instruction], branch_index: usize, register: IcedReg) -> Option<(u32, usize)> {
    let branch = predecessors.get(branch_index)?;
    let inclusive = match branch.mnemonic() {
        Mnemonic::Ja => true,
        Mnemonic::Jae => false,
        _ => return None,
    };
    for (index, instruction) in predecessors.iter().enumerate().skip(branch_index + 1) {
        if instruction.mnemonic() == Mnemonic::Cmp
            && instruction.op0_kind() == OpKind::Register
            && instruction.op0_register() == register
        {
            let value = immediate_u32(instruction)?;
            let max = if inclusive { value } else { value.checked_sub(1)? };
            return (max < 256).then_some((max, index));
        }
        if instruction.flow_control() != FlowControl::Next
            || instruction.rflags_modified() != 0
            || instruction_writes_gp(instruction, register)
        {
            return None;
        }
    }
    None
}

fn jump_table_proofs_are_valid(
    proofs: &BTreeMap<u32, Vec<(u32, u32)>>, incoming: &BTreeMap<u32, BTreeSet<u32>>, entry: u32,
) -> bool {
    proofs.values().flatten().all(|(address, expected)| {
        *address != entry
            && incoming
                .get(address)
                .is_some_and(|actual| actual.len() == 1 && actual.contains(expected))
    })
}

fn instruction_writes_gp(instruction: &Instruction, register: IcedReg) -> bool {
    let Some((expected, _)) = gp_register(register) else {
        return true;
    };
    InstructionInfoFactory::new()
        .info(instruction)
        .used_registers()
        .iter()
        .any(|used| writes(used.access()) && gp_register(used.register()).is_some_and(|(actual, _)| actual == expected))
}

fn immediate_u32(instruction: &Instruction) -> Option<u32> {
    Some(match instruction.op1_kind() {
        OpKind::Immediate8 => u32::from(instruction.immediate8()),
        OpKind::Immediate8to16 => instruction.immediate8to16() as u16 as u32,
        OpKind::Immediate8to32 => instruction.immediate8to32() as u32,
        OpKind::Immediate16 => u32::from(instruction.immediate16()),
        OpKind::Immediate32 => instruction.immediate32(),
        _ => return None,
    })
}

fn return_cleanup(instruction: &Instruction) -> Option<u32> {
    if instruction.mnemonic() != Mnemonic::Ret {
        return None;
    }
    match instruction.op_count() {
        0 => Some(0),
        1 if instruction.op0_kind() == OpKind::Immediate16 => Some(u32::from(instruction.immediate16())),
        _ => None,
    }
}

pub(super) fn summarize_call(
    image: &TextImage, entry: u32, cache: &mut AnalysisCache, call_depth: usize,
) -> CallSummary {
    if call_depth > MAX_CALL_DEPTH {
        return CallSummary::UNKNOWN;
    }
    if cache.active_calls.contains(&entry) {
        return CallSummary::UNKNOWN;
    }
    if let Some(summary) = cache.call_summaries.get(&entry).copied() {
        return summary;
    }
    cache.active_calls.insert(entry);

    let facts = traverse(image, entry, State::entry([0; 6], [false; 2]), cache, call_depth);
    let summary = if facts.unknown_control
        || facts.unknown_call_contract
        || facts.unknown_x87_read
        || facts.unknown_stack
        || facts.budget_exhausted
        || facts.exits.is_empty()
        || facts.exits.values().any(|(state, _)| state.esp != Some(0))
        || facts.exits.values().any(|(state, _)| state.ambiguous_push_input)
    {
        CallSummary::UNKNOWN
    } else {
        let changes = Gp::ALL.map(|reg| {
            let mut changes = facts.exits.values().map(|(state, _)| state.regs[reg.index()].change);
            let first = changes.next().expect("at least one exit was checked above");
            if changes.all(|change| change == first) {
                first
            } else {
                Change::Unknown
            }
        });
        let mut cleanups = facts.exits.values().map(|(_, cleanup)| *cleanup);
        let first_cleanup = cleanups.next().expect("at least one exit was checked above");
        let stack_inputs = facts.stack_reads.keys().try_fold(0u64, |mask, slot| {
            let index = slot.checked_div(4)?.checked_sub(1)?;
            (index < 64).then_some(mask | 1u64 << index)
        });
        if cleanups.all(|cleanup| cleanup == first_cleanup)
            && let Some(stack_inputs) = stack_inputs
        {
            CallSummary {
                changes,
                cleanup: Some(first_cleanup),
                stack_inputs,
                unresolved_inputs: facts.input_reads.into_iter().any(|mask| mask != 0)
                    || !facts.missing_x87_inputs.is_empty()
                    || !facts.stack_writes.is_empty(),
            }
        } else {
            CallSummary::UNKNOWN
        }
    };
    cache.active_calls.remove(&entry);
    cache.call_summaries.insert(entry, summary);
    summary
}
