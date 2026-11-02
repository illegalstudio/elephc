//! Purpose:
//! Reuses numeric frame storage after a transient cell's last payload observation.
//!
//! Called from:
//! - `super::plan()` after the complete numeric ownership proof succeeds.
//!
//! Key details:
//! - A cell stored in any local always retains dedicated storage.
//! - Sharing stays inside one block and follows transparent Acquire/Borrow users.
//! - Numeric emitters read both inputs before writing, allowing reuse at the last use.

use super::*;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Assigns nonoverlapping transient intervals to cells without aliasing any retained local.
pub(super) fn storage_groups(proof: &Proof<'_>, cells: &[ValueId]) -> Vec<Vec<ValueId>> {
    let selected: HashSet<_> = cells.iter().copied().collect();
    let mut groups: Vec<Vec<ValueId>> = Vec::new();
    let mut assigned = HashSet::new();
    for block in &proof.function.blocks {
        let mut pool: BinaryHeap<Reverse<(usize, usize)>> = BinaryHeap::new();
        for (position, &id) in block.instructions.iter().enumerate() {
            let Some(value) = proof.function.instructions[id.as_raw() as usize].result else { continue; };
            if !selected.contains(&value) { continue; }
            assigned.insert(value);
            let end = transient_end(proof, value, block.id, position);
            let reusable = end.is_some() && pool.peek().is_some_and(|Reverse((end, _))| *end <= position);
            let group = if reusable {
                pool.pop().unwrap().0.1
            } else {
                groups.push(Vec::new());
                groups.len() - 1
            };
            groups[group].push(value);
            if let Some(end) = end { pool.push(Reverse((end, group))); }
        }
    }
    // Keep unlocated values conservative and preserve the previous dedicated-cell ordering.
    for &value in cells {
        if !assigned.contains(&value) { groups.push(vec![value]); }
    }
    let order: HashMap<_, _> = cells.iter().enumerate().map(|(i, &value)| (value, i)).collect();
    groups.sort_by_key(|group| group.iter().map(|value| order[value]).min().unwrap());
    groups
}

/// Finds the last scalar read through all pointer copies, refusing every local store.
fn transient_end(proof: &Proof<'_>, value: ValueId, block: crate::ir::BlockId, start: usize) -> Option<usize> {
    let mut pending = vec![value];
    let mut seen = HashSet::new();
    let mut end = start;
    while let Some(value) = pending.pop() {
        if !seen.insert(value) { continue; }
        for &user in proof.uses.get(&value)? {
            let &(user_block, position) = proof.locations.get(&user)?;
            if user_block != block || position < start { return None; }
            let inst = &proof.function.instructions[user.as_raw() as usize];
            match inst.op {
                Op::Release => {}
                Op::Acquire | Op::Borrow if inst.immediate.is_none() => pending.push(inst.result?),
                _ if scalar_observation(inst) || tagged_producer(inst) => end = end.max(position),
                _ => return None,
            }
        }
    }
    Some(end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Builder, MixedNumericOp, Terminator};

    /// Emits one checked value whose original operands already use the integer ABI.
    fn checked_value(builder: &mut Builder<'_>, one: ValueId) -> ValueId {
        builder.emit(Op::ICheckedAdd, vec![one, one], None,
            IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap()
    }

    /// Emits a tagged consumer which reads both operands before writing its result cell.
    fn sum(builder: &mut Builder<'_>, left: ValueId, right: ValueId) -> ValueId {
        builder.emit(Op::MixedNumericBinop, vec![left, right],
            Some(Immediate::MixedNumericOp(MixedNumericOp::Add)),
            IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap()
    }

    /// Observes a payload without returning its pointer outside this activation.
    fn scalar(builder: &mut Builder<'_>, value: ValueId) -> ValueId {
        builder.emit(Op::Cast, vec![value], Some(Immediate::CastTarget(IrType::I64)),
            IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap()
    }

    /// A chain through transparent ownership copies shares one cell at read-before-write sites.
    #[test]
    fn numeric_transient_chain_reuses_one_cell() {
        let mut function = Function::new("chain_storage".into(), IrType::I64, PhpType::Int);
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry); builder.position_at_end(entry);
        let one = builder.emit_const_i64(1);
        let a = checked_value(&mut builder, one);
        let owner = builder.emit(Op::Acquire, vec![a], None,
            IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
        let borrowed = builder.emit(Op::Borrow, vec![owner], None,
            IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Borrowed).unwrap();
        let b = sum(&mut builder, borrowed, one);
        let c = sum(&mut builder, b, one);
        let result = scalar(&mut builder, c);
        builder.emit(Op::Release, vec![a], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
        builder.terminate(Terminator::Return { value: Some(result) });
        crate::ir::validate_function(&function).unwrap();
        let frame = super::super::plan(&function);
        assert_eq!(frame.cells, vec![a, b, c]);
        assert_eq!(frame.cell_groups, vec![vec![a, b, c]]);
    }

    /// A second read keeps a shared input live until both consumers have copied its payload.
    #[test]
    fn numeric_transient_fanout_keeps_overlapping_cells_separate() {
        let mut function = Function::new("fanout_storage".into(), IrType::I64, PhpType::Int);
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry); builder.position_at_end(entry);
        let one = builder.emit_const_i64(1);
        let a = checked_value(&mut builder, one);
        let b = sum(&mut builder, a, one);
        let c = sum(&mut builder, b, a);
        let result = scalar(&mut builder, c);
        builder.terminate(Terminator::Return { value: Some(result) });
        crate::ir::validate_function(&function).unwrap();
        let groups = super::super::plan(&function).cell_groups;
        assert_eq!(groups, vec![vec![a, c], vec![b]]);
    }

    /// Retained locals and distinct blocks never borrow one another's transient storage.
    #[test]
    fn numeric_transient_storage_excludes_local_owners_and_other_blocks() {
        let mut function = Function::new("private_storage".into(), IrType::I64, PhpType::Int);
        let slot = function.add_local(Some("retained".into()), IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed, LocalKind::PhpLocal);
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let other = builder.create_named_block("other", vec![]);
        builder.set_entry(entry); builder.position_at_end(entry);
        let one = builder.emit_const_i64(1);
        let retained = checked_value(&mut builder, one);
        builder.emit_store_local(slot, retained);
        let a = checked_value(&mut builder, one);
        scalar(&mut builder, a);
        let b = checked_value(&mut builder, one);
        scalar(&mut builder, b);
        builder.terminate(Terminator::Br { target: other, args: vec![] });
        builder.position_at_end(other);
        let c = checked_value(&mut builder, one);
        scalar(&mut builder, c);
        let result = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        builder.terminate(Terminator::Return { value: Some(result) });
        crate::ir::validate_function(&function).unwrap();
        let groups = super::super::plan(&function).cell_groups;
        assert_eq!(groups, vec![vec![retained], vec![a, b], vec![c]]);
    }
}
