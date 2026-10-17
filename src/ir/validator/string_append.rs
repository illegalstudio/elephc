//! Purpose:
//! Validates consuming string append's adjacent local-owner handoff.
//!
//! Called from:
//! - `super::validate_function` after ordinary instruction and dominance checks.
//!
//! Key details:
//! - The old owner is loaded immediately before append and the replacement is stored next.
//! - No observer may retain the consumed load or duplicate the replacement owner.

use super::*;

/// Rejects append graphs that separate consumption from local owner publication.
pub(super) fn validate_transfers(function: &Function) -> Result<(), ValidationError> {
    if !function.instructions.iter().any(|inst| inst.op == Op::StrAppend) { return Ok(()); }
    let uses = function.value_use_counts();
    for block in &function.blocks {
        let active: Vec<_> = block.instructions.iter().copied().filter(|id| {
            function.instruction(*id).is_some_and(|inst| inst.op != Op::Nop)
        }).collect();
        for (index, &id) in active.iter().enumerate() {
            let inst = function.instruction(id).unwrap();
            if inst.op != Op::StrAppend { continue; }
            let bad = || ValidationError::InvalidStringAppendTransfer(id);
            let Some(Immediate::LocalSlot(slot)) = inst.immediate else { return Err(bad()); };
            let Some(local) = function.locals.get(slot.as_raw() as usize) else { return Err(bad()); };
            let Some(result) = inst.result else { return Err(bad()); };
            let Some(value) = function.value(result) else { return Err(bad()); };
            if local.kind != LocalKind::PhpLocal || local.ir_type != IrType::Str
                || local.php_type != PhpType::Str || value.ownership != Ownership::Owned
                || value.ir_type != IrType::Str || value.php_type != PhpType::Str
            { return Err(bad()); }
            let previous = index.checked_sub(1).and_then(|i| function.instruction(active[i]));
            let next = active.get(index + 1).and_then(|id| function.instruction(*id));
            let Some(load) = previous else { return Err(bad()); };
            let Some(store) = next else { return Err(bad()); };
            if load.op != Op::LoadLocal || load.immediate != Some(Immediate::LocalSlot(slot))
                || load.result != inst.operands.first().copied()
                || store.op != Op::StoreLocal || store.immediate != Some(Immediate::LocalSlot(slot))
                || store.operands != [result]
            { return Err(bad()); }
            if uses.get(&result) != Some(&1) || uses.get(&inst.operands[0]) != Some(&1) {
                return Err(bad());
            }
        }
    }
    Ok(())
}
