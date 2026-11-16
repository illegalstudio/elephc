//! Purpose:
//! Identifies stable physical payload types for local reference-cell pointers.
//!
//! Called from:
//! - LocalSlotAnalysis construction before target assembly emission.
//!
//! Key details:
//! - The binding instruction records the payload type before eval or control flow widens the slot.
//! - Conflicting bindings fall back to the final local type instead of guessing one ABI layout.
//! - Object class refinements and container element refinements share one physical payload shape.
//! - Heterogeneous rebindings retain a per-instruction proof only where all CFG predecessors agree.

use super::*;

/// Proves current payloads for slots whose retired bindings prevent a function-wide ABI proof.
pub(super) fn payload_types_before_inst(
    function: &Function,
    may_ref: &HashSet<(InstId, LocalSlotId)>,
    ever_ref: &HashSet<LocalSlotId>,
    stable: &HashMap<LocalSlotId, PhpType>,
) -> HashMap<(InstId, LocalSlotId), PhpType> {
    let needed = ever_ref.iter().filter(|slot| !stable.contains_key(*slot)).copied().collect::<HashSet<_>>();
    if needed.is_empty() || function.blocks.is_empty() { return HashMap::new(); }
    let initial = function.params.iter().enumerate().filter(|(_, param)| param.by_ref)
        .map(|(index, param)| (LocalSlotId::from_raw(index as u32), param.php_type.codegen_repr()))
        .collect::<HashMap<_, _>>();
    let mut inputs = vec![None::<HashMap<LocalSlotId, PhpType>>; function.blocks.len()];
    let mut outputs = inputs.clone();
    let entry = function.entry.as_raw() as usize;
    if entry >= inputs.len() { return HashMap::new(); }
    inputs[entry] = Some(initial);
    let mut pending = VecDeque::from([function.entry]);
    // An implicit exception edge can observe any earlier binding. Start with no additional
    // proof there, leaving the established conservative payload fallback in charge.
    for handler in exception_handler_blocks(function) {
        let Some(input) = inputs.get_mut(handler.as_raw() as usize) else { continue; };
        *input = Some(HashMap::new());
        if handler != function.entry { pending.push_back(handler); }
    }
    while let Some(id) = pending.pop_front() {
        let index = id.as_raw() as usize;
        let Some(mut state) = inputs[index].clone() else { continue; };
        let block = &function.blocks[index];
        for inst in &block.instructions { transfer_payload(function, *inst, may_ref, stable, &mut state); }
        if outputs[index].as_ref() == Some(&state) { continue; }
        outputs[index] = Some(state.clone());
        let Some(terminator) = &block.terminator else { continue; };
        for successor in terminator_successors(terminator) {
            let Some(input) = inputs.get_mut(successor.as_raw() as usize) else { continue; };
            let changed = if let Some(input) = input {
                let before = input.clone();
                input.retain(|slot, ty| state.get(slot).is_some_and(|other|
                    payload_types_are_compatible(ty, other)));
                *input != before
            } else { *input = Some(state.clone()); true };
            if changed { pending.push_back(successor); }
        }
    }
    let mut result = HashMap::new();
    for block in &function.blocks {
        let Some(mut state) = inputs[block.id.as_raw() as usize].clone() else { continue; };
        for inst in &block.instructions {
            result.extend(state.iter().filter(|(slot, _)| needed.contains(*slot))
                .map(|(slot, ty)| ((*inst, *slot), ty.clone())));
            transfer_payload(function, *inst, may_ref, stable, &mut state);
        }
    }
    result
}

/// Replaces a binding's payload proof without allowing an idempotent promotion to retype it.
fn transfer_payload(
    function: &Function, id: InstId, may_ref: &HashSet<(InstId, LocalSlotId)>,
    stable: &HashMap<LocalSlotId, PhpType>, state: &mut HashMap<LocalSlotId, PhpType>,
) {
    let Some(inst) = function.instruction(id) else { return; };
    if matches!(inst.op, Op::EvalLiteralCall | Op::LanguageConstructCall) {
        // Eval can replace a caller-visible binding without an explicit alias opcode.
        state.clear();
        return;
    }
    let Some(slot) = ref_cell_target_slot(function, inst.op, inst.immediate.as_ref(), &inst.operands) else {
        if let (Op::UnsetLocal, Some(Immediate::LocalSlot(slot))) = (inst.op, &inst.immediate) {
            state.remove(slot);
        }
        return;
    };
    let payload = match (inst.op, inst.immediate.as_ref()) {
        (Op::PromoteLocalRefCell, _) => {
            if may_ref.contains(&(id, slot)) { return; }
            function.locals.get(slot.as_raw() as usize).map(|local| local.php_type.codegen_repr())
        }
        (Op::AliasLocalRefCell, Some(Immediate::LocalSlotPair { second, .. })) =>
            state.get(second).or_else(|| stable.get(second)).cloned(),
        (Op::BindRefCellPtr | Op::AdoptRefCellPtr, _) => Some(inst.result_php_type.codegen_repr()),
        _ => None,
    };
    if let Some(ty) = payload { state.insert(slot, ty); } else { state.remove(&slot); }
}

/// Finds slots whose every possible reference binding uses one compatible physical payload type.
pub(super) fn stable_payload_types(
    function: &Function,
    definite_ref_cell_slots_before_inst: &HashSet<(InstId, LocalSlotId)>,
) -> HashMap<LocalSlotId, PhpType> {
    let mut candidates = HashMap::new();
    let mut conflicts = HashSet::new();
    for (index, param) in function.params.iter().enumerate().filter(|(_, param)| param.by_ref) {
        let slot = LocalSlotId::from_raw(index as u32);
        if function.locals.get(index).is_some() {
            record_payload_type(&mut candidates, &mut conflicts, slot, param.php_type.clone());
        }
    }
    for (index, inst) in function.instructions.iter().enumerate() {
        let Some(slot) =
            ref_cell_target_slot(function, inst.op, inst.immediate.as_ref(), &inst.operands)
        else {
            continue;
        };
        let payload_ty = match (inst.op, inst.immediate.as_ref()) {
            // A promotion allocates from the owner slot only when the target can still be raw.
            // Once every incoming path already stores a cell, it is idempotent and must not
            // conflict with a narrower payload ABI established by an external binding.
            (
                Op::PromoteLocalRefCell,
                Some(Immediate::LocalSlotPair { .. }),
            ) => (!definite_ref_cell_slots_before_inst
                .contains(&(InstId::from_raw(index as u32), slot)))
                .then(|| function.locals.get(slot.as_raw() as usize).map(|local| local.php_type.clone()))
                .flatten(),
            (Op::AliasLocalRefCell, Some(Immediate::LocalSlotPair { second, .. })) => {
                if conflicts.contains(second) {
                    conflicts.insert(slot);
                    candidates.remove(&slot);
                    None
                } else {
                    candidates
                        .get(second)
                        .cloned()
                        .or_else(|| Some(inst.result_php_type.clone()))
                }
            }
            (Op::BindRefCellPtr | Op::AdoptRefCellPtr, _) => {
                Some(inst.result_php_type.clone())
            }
            // Capturing an existing cell adds an owner without changing its payload ABI. Iterator
            // bindings expose external element storage, but their void instruction metadata does
            // not describe that storage's payload type, so they retain the conservative fallback.
            (Op::ClosureCapture | Op::IterCurrentValueRef, _) => None,
            _ => None,
        };
        if let Some(payload_ty) = payload_ty {
            record_payload_type(&mut candidates, &mut conflicts, slot, payload_ty);
        }
    }
    for slot in conflicts {
        candidates.remove(&slot);
    }
    candidates
}

/// Records one binding while rejecting slots whose bindings need different load or boxing rules.
fn record_payload_type(
    candidates: &mut HashMap<LocalSlotId, PhpType>,
    conflicts: &mut HashSet<LocalSlotId>,
    slot: LocalSlotId,
    ty: PhpType,
) {
    if conflicts.contains(&slot) {
        return;
    }
    let ty = ty.codegen_repr();
    let Some(existing) = candidates.get(&slot) else {
        candidates.insert(slot, ty);
        return;
    };
    if payload_types_are_compatible(existing, &ty) {
        return;
    }
    candidates.remove(&slot);
    conflicts.insert(slot);
}

/// Treats logical refinements as one payload only when loading and boxing remain identical.
fn payload_types_are_compatible(left: &PhpType, right: &PhpType) -> bool {
    left == right
        || matches!(
            (left, right),
            (PhpType::Object(_), PhpType::Object(_))
                | (PhpType::Array(_), PhpType::Array(_))
                | (PhpType::AssocArray { .. }, PhpType::AssocArray { .. })
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Builder, FunctionParam, IrHeapKind, IrType, LocalKind, Ownership};

    /// Creates raw integer and array reference parameters with one boxed logical alias slot.
    fn reference_fixture() -> (Function, LocalSlotId, LocalSlotId, LocalSlotId) {
        let mut function = Function::new("rebound_payload".into(), IrType::Void, PhpType::Void);
        for (name, php_type, ir_type) in [
            ("number", PhpType::Int, IrType::I64),
            ("items", PhpType::Array(Box::new(PhpType::Int)), IrType::Heap(IrHeapKind::Array)),
        ] {
            function.params.push(FunctionParam {
                name: name.into(), php_type: php_type.clone(), ir_type, by_ref: true, variadic: false,
            });
            function.add_local(Some(name.into()), ir_type, php_type, LocalKind::PhpLocal);
        }
        let alias = function.add_local(Some("alias".into()), IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed, LocalKind::PhpLocal);
        (function, LocalSlotId::from_raw(0), LocalSlotId::from_raw(1), alias)
    }

    /// Emits one raw-payload alias and a subsequent logical read without changing cell layout.
    fn alias_and_read(builder: &mut Builder<'_>, alias: LocalSlotId, source: LocalSlotId, ty: PhpType) -> InstId {
        builder.emit(Op::AliasLocalRefCell, vec![],
            Some(Immediate::LocalSlotPair { first: alias, second: source }),
            IrType::Void, ty, Ownership::NonHeap);
        let value = builder.emit(Op::LoadRefCell, vec![], Some(Immediate::LocalSlot(alias)),
            IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Borrowed).unwrap();
        let ValueDef::Instruction { inst, .. } = builder.function().value(value).unwrap().def else { unreachable!() };
        inst
    }

    /// A retired array binding cannot make the next integer reference look like a Mixed pointer.
    #[test]
    fn heterogeneous_rebinding_keeps_each_read_payload() {
        let (mut function, number, items, alias) = reference_fixture();
        let (array_read, int_read);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", vec![]);
            builder.set_entry(entry);
            builder.position_at_end(entry);
            array_read = alias_and_read(&mut builder, alias, items, PhpType::Array(Box::new(PhpType::Int)));
            int_read = alias_and_read(&mut builder, alias, number, PhpType::Int);
            builder.terminate(Terminator::Return { value: None });
        }
        let analysis = LocalSlotAnalysis::new(&function);
        assert_eq!(analysis.stable_ref_cell_payload_type(alias), None);
        assert_eq!(analysis.ref_cell_payload_type_before_inst(array_read, alias),
            Some(&PhpType::Array(Box::new(PhpType::Int))));
        assert_eq!(analysis.ref_cell_payload_type_before_inst(int_read, alias), Some(&PhpType::Int));
    }

    /// Different incoming physical payloads cannot establish a speculative merged ABI.
    #[test]
    fn heterogeneous_predecessors_keep_the_conservative_fallback() {
        let (mut function, number, items, alias) = reference_fixture();
        let read;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", vec![]);
            let integer = builder.create_named_block("integer", vec![]);
            let array = builder.create_named_block("array", vec![]);
            let merge = builder.create_named_block("merge", vec![]);
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let condition = builder.emit(Op::ConstBool, vec![], Some(Immediate::Bool(true)),
                IrType::I64, PhpType::Bool, Ownership::NonHeap).unwrap();
            builder.terminate(Terminator::CondBr { cond: condition,
                then_target: integer, then_args: vec![], else_target: array, else_args: vec![] });
            for (block, source, ty) in [(integer, number, PhpType::Int),
                (array, items, PhpType::Array(Box::new(PhpType::Int)))] {
                builder.position_at_end(block);
                alias_and_read(&mut builder, alias, source, ty);
                builder.terminate(Terminator::Br { target: merge, args: vec![] });
            }
            builder.position_at_end(merge);
            let value = builder.emit(Op::LoadRefCell, vec![], Some(Immediate::LocalSlot(alias)),
                IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Borrowed).unwrap();
            let ValueDef::Instruction { inst, .. } = builder.function().value(value).unwrap().def else { unreachable!() };
            read = inst;
            builder.terminate(Terminator::Return { value: None });
        }
        let analysis = LocalSlotAnalysis::new(&function);
        assert_eq!(analysis.ref_cell_payload_type_before_inst(read, alias), None);
        assert_eq!(analysis.stable_ref_cell_payload_type(alias), None);
    }
}
