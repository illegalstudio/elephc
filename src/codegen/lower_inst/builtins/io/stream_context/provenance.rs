//! Purpose:
//! Recovers literal notification producers through compiler-owned temporary roots.
//!
//! Called from:
//! - The stream-context callback discovery in the parent module.
//!
//! Key details:
//! - Only uniquely assigned compiler temporaries are transparent, never PHP locals.
//! - Clearing writes and exceptional entry paths invalidate a temporary's known source.

use super::{CodegenIrError, Result, expect_operand};
use crate::ir::{BlockId, Function, Immediate, InstId, LocalKind, LocalSlotId, Op, ValueDef, ValueId};

/// Follows ownership forwarding and definitely initialized, single-source temporary loads.
pub(super) fn transparent_callback_source(function: &Function, mut value: ValueId) -> Result<ValueId> {
    let mut seen = std::collections::HashSet::new();
    while seen.insert(value) {
        let metadata = function.value(value)
            .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
        let ValueDef::Instruction { block, inst, .. } = metadata.def else { break; };
        let instruction = function.instruction(inst)
            .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
        match instruction.op {
            Op::Acquire | Op::Move | Op::Borrow => value = expect_operand(instruction, 0)?,
            Op::LoadLocal => {
                let Some(Immediate::LocalSlot(slot)) = instruction.immediate else { break; };
                let Some(source) = temporary_source(function, slot, block, inst)? else { break; };
                value = source;
            }
            _ => break,
        }
    }
    Ok(value)
}

/// Returns a temporary's unique stored value only when it is still defined at this load.
fn temporary_source(
    function: &Function, slot: LocalSlotId, block: BlockId, load: InstId,
) -> Result<Option<ValueId>> {
    if !function.locals.get(slot.as_raw() as usize).is_some_and(|local|
        matches!(local.kind, LocalKind::HiddenTemp | LocalKind::OwnedTemp | LocalKind::NamedArgTemp))
    { return Ok(None); }
    let mut stores = function.instructions.iter().filter(|instruction|
        instruction.op == Op::StoreLocal && instruction.immediate == Some(Immediate::LocalSlot(slot)));
    let Some(store) = stores.next() else { return Ok(None); };
    if stores.next().is_some() || !initialized_at_load(function, slot, block, load) {
        return Ok(None);
    }
    Ok(Some(expect_operand(store, 0)?))
}

/// Computes definite assignment with handler entries treated as independent unknown roots.
fn initialized_at_load(function: &Function, slot: LocalSlotId, block: BlockId, load: InstId) -> bool {
    let mut predecessors = vec![Vec::new(); function.blocks.len()];
    let mut roots = vec![false; function.blocks.len()];
    roots[function.entry.as_raw() as usize] = true;
    for instruction in &function.instructions {
        if instruction.op != Op::TryPushHandler { continue; }
        if let Some(Immediate::I64(token)) = instruction.immediate {
            if let Ok(index) = usize::try_from(token) {
                if let Some(root) = roots.get_mut(index) { *root = true; }
            }
        }
    }
    for current in &function.blocks {
        if let Some(terminator) = &current.terminator {
            for successor in crate::ir_passes::successors(terminator) {
                predecessors[successor.as_raw() as usize].push(current.id.as_raw() as usize);
            }
        }
    }
    let entry_state = |index: usize, states: &[bool]| !roots[index]
        && !predecessors[index].is_empty() && predecessors[index].iter().all(|&pred| states[pred]);
    let mut states = vec![true; function.blocks.len()];
    loop {
        let mut changed = false;
        for current in &function.blocks {
            let index = current.id.as_raw() as usize;
            let next = transfer(function, &current.instructions, slot, entry_state(index, &states), None);
            changed |= states[index] != next;
            states[index] = next;
        }
        if !changed { break; }
    }
    let index = block.as_raw() as usize;
    transfer(function, &function.blocks[index].instructions, slot, entry_state(index, &states), Some(load))
}

/// Stops at the requested load and invalidates known sources when a slot is cleared.
fn transfer(function: &Function, instructions: &[InstId], slot: LocalSlotId, mut known: bool, stop: Option<InstId>) -> bool {
    for &id in instructions {
        if Some(id) == stop { return known; }
        let Some(instruction) = function.instruction(id) else { continue; };
        if instruction.immediate != Some(Immediate::LocalSlot(slot)) { continue; }
        match instruction.op {
            Op::StoreLocal => known = true,
            Op::ZeroLocalSlot | Op::ReleaseLocalSlot | Op::UnsetLocal => known = false,
            _ => {}
        }
    }
    known
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Builder, IrType, Ownership, Terminator};
    use crate::types::PhpType;

    /// Builds one stored descriptor, optionally clearing or assigning its slot again.
    fn staged(kind: LocalKind, clear: bool, duplicate: bool) -> (Function, ValueId, ValueId) {
        let mut function = Function::new("staged_notification".into(), IrType::Void, PhpType::Void);
        let (source, load) = {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let slot = builder.add_local(None, IrType::I64, PhpType::Callable, kind);
            let source = builder.emit(Op::ClosureNew, Vec::new(), None,
                IrType::I64, PhpType::Callable, Ownership::Owned).unwrap();
            builder.emit_store_local(slot, source);
            if duplicate { builder.emit_store_local(slot, source); }
            if clear {
                let _ = builder.emit(Op::ReleaseLocalSlot, Vec::new(), Some(Immediate::LocalSlot(slot)),
                    IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            let load = builder.emit_load_local(slot, IrType::I64, PhpType::Callable);
            builder.terminate(Terminator::Return { value: None });
            (source, load)
        };
        (function, source, load)
    }

    /// A unique live compiler root preserves producer identity, but PHP slots do not.
    #[test]
    fn notification_temporary_sources_require_one_live_compiler_store() {
        for kind in [LocalKind::HiddenTemp, LocalKind::OwnedTemp, LocalKind::NamedArgTemp] {
            let (function, source, load) = staged(kind, false, false);
            assert_eq!(transparent_callback_source(&function, load).unwrap(), source);
        }
        for (kind, clear, duplicate) in [(LocalKind::PhpLocal, false, false),
            (LocalKind::HiddenTemp, true, false), (LocalKind::HiddenTemp, false, true)]
        {
            let (function, _, load) = staged(kind, clear, duplicate);
            assert_eq!(transparent_callback_source(&function, load).unwrap(), load);
        }
    }

    /// Builds a branch merge with either a dominating store, a missing path or a handler entry.
    fn branched(conditional_store: bool, handler: bool) -> (Function, ValueId, ValueId) {
        let mut function = Function::new("branched_notification".into(), IrType::Void, PhpType::Void);
        let (source, load) = {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            let left = builder.create_named_block("left", Vec::new());
            let right = builder.create_named_block("right", Vec::new());
            let merge = builder.create_named_block("merge", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let slot = builder.add_local(None, IrType::I64, PhpType::Callable, LocalKind::HiddenTemp);
            let source = builder.emit(Op::ClosureNew, Vec::new(), None,
                IrType::I64, PhpType::Callable, Ownership::Owned).unwrap();
            if !conditional_store { builder.emit_store_local(slot, source); }
            if handler {
                let _ = builder.emit(Op::TryPushHandler, Vec::new(), Some(Immediate::I64(right.as_raw() as i64)),
                    IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            let cond = builder.emit(Op::ConstBool, Vec::new(), Some(Immediate::Bool(true)),
                IrType::I64, PhpType::Bool, Ownership::NonHeap).unwrap();
            builder.terminate(Terminator::CondBr { cond, then_target: left, then_args: Vec::new(),
                else_target: right, else_args: Vec::new() });
            builder.position_at_end(left);
            if conditional_store { builder.emit_store_local(slot, source); }
            builder.terminate(Terminator::Br { target: merge, args: Vec::new() });
            builder.position_at_end(right);
            builder.terminate(Terminator::Br { target: merge, args: Vec::new() });
            builder.position_at_end(merge);
            let load = builder.emit_load_local(slot, IrType::I64, PhpType::Callable);
            builder.terminate(Terminator::Return { value: None });
            (source, load)
        };
        (function, source, load)
    }

    /// Branches preserve only definitely assigned roots; exception entry cannot inherit them.
    #[test]
    fn notification_temporary_sources_respect_branch_and_handler_paths() {
        let (function, source, load) = branched(false, false);
        assert_eq!(transparent_callback_source(&function, load).unwrap(), source);
        for (conditional, handler) in [(true, false), (false, true)] {
            let (function, _, load) = branched(conditional, handler);
            assert_eq!(transparent_callback_source(&function, load).unwrap(), load);
        }
    }
}
