//! Purpose:
//! Copies already lowered loop blocks while preserving source and lexical metadata.
//!
//! Called from:
//! - Guarded counter versioning after its complete use inventory succeeds.
//!
//! Key details:
//! - Other locals are shared, while the integer counter has private scalar storage.
//! - Every edge leaving the fast copy publishes its final counter to the original slot.

use super::*;
use crate::ir::BasicBlock;

/// Rejects SSA observations outside the copied region, including exit arguments and returns.
pub(super) fn values_escape(function: &Function, lp: &NaturalLoop) -> bool {
    let defined: HashSet<_> = lp.blocks.iter().flat_map(|b| {
        let block = &function.blocks[b.as_raw() as usize];
        block.params.iter().copied().chain(block.instructions.iter().filter_map(|id| function.instructions[id.as_raw() as usize].result))
    }).collect();
    function.blocks.iter().any(|block| {
        if lp.contains(block.id) {
            let Some(term) = &block.terminator else { return true; };
            if matches!(term, Terminator::Return { .. } | Terminator::Throw { .. } | Terminator::GeneratorSuspend { .. }) { return true; }
            let mut term = term.clone();
            return crate::ir_passes::cfg::successor_edges(&mut term).into_iter()
                .any(|(target, args)| !lp.contains(*target) && !args.is_empty());
        }
        block.instructions.iter().any(|id| function.instructions[id.as_raw() as usize].operands.iter().any(|v| defined.contains(v)))
            || block.terminator.as_ref().is_some_and(|t| t.used_values().iter().any(|v| defined.contains(v)))
    })
}

/// Copies blocks with fresh IDs and connects their exits through representation boundaries.
pub(super) fn copy_loop(function: &mut Function, plan: &Plan, slot: LocalSlotId, bound: ValueId) -> BlockId {
    let source = &plan.scalarized;
    let mut blocks = HashMap::new();
    let mut values = HashMap::from([(plan.bound_placeholder, bound)]);
    let mut instructions = Vec::new();
    for &old in &plan.blocks {
        let original = &source.blocks[old.as_raw() as usize];
        let block = BlockId::from_raw(function.blocks.len() as u32);
        blocks.insert(old, block);
        let mut params = Vec::new();
        for (index, &param) in original.params.iter().enumerate() {
            let result = ValueId::from_raw(function.values.len() as u32);
            let mut metadata = source.values[param.as_raw() as usize].clone();
            metadata.def = ValueDef::BlockParam { block, index: index as u16 };
            function.values.push(metadata); values.insert(param, result); params.push(result);
        }
        function.blocks.push(BasicBlock::new(block, format!("{}.counter.fast", original.name), params));
    }
    for &old in &plan.blocks {
        let block = blocks[&old];
        for (index, &id) in source.blocks[old.as_raw() as usize].instructions.iter().enumerate() {
            let new_id = InstId::from_raw(function.instructions.len() as u32);
            let mut inst = source.instructions[id.as_raw() as usize].clone();
            if let Some(result) = inst.result {
                let new_result = ValueId::from_raw(function.values.len() as u32);
                let mut metadata = source.values[result.as_raw() as usize].clone();
                metadata.def = ValueDef::Instruction { block, index: index as u32, inst: new_id };
                function.values.push(metadata); values.insert(result, new_result); inst.result = Some(new_result);
            }
            if inst.immediate == Some(Immediate::LocalSlot(plan.slot)) { inst.immediate = Some(Immediate::LocalSlot(slot)); }
            if let Some(scope) = source.inlined_scopes.get(&id) { function.inlined_scopes.insert(new_id, scope.clone()); }
            function.instructions.push(inst);
            function.blocks[block.as_raw() as usize].instructions.push(new_id);
            instructions.push(new_id);
        }
    }
    for id in instructions {
        for operand in &mut function.instructions[id.as_raw() as usize].operands {
            if let Some(replacement) = values.get(operand) { *operand = *replacement; }
        }
    }
    let mut exits = HashMap::new();
    for &old in &plan.blocks {
        let original = source.blocks[old.as_raw() as usize].terminator.clone().unwrap();
        let mut term = crate::ir_passes::inline::remap_terminator(original, &blocks, &values);
        for (target, _) in crate::ir_passes::cfg::successor_edges(&mut term) {
            if blocks.values().any(|block| *block == *target) { continue; }
            let exit = *exits.entry(*target).or_insert_with(|| {
                let mut builder = Builder::new(function);
                let exit = builder.create_named_block("counter.integer.exit", vec![]);
                builder.position_at_end(exit);
                let value = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
                let boxed = emit(&mut builder, Op::MixedBox, vec![value], None, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
                builder.emit(Op::ReleaseLocalSlot, vec![], Some(Immediate::LocalSlot(plan.slot)), IrType::Void, PhpType::Void, Ownership::NonHeap);
                builder.emit_store_local(plan.slot, boxed);
                builder.terminate(Terminator::Br { target: *target, args: vec![] });
                exit
            });
            *target = exit;
        }
        function.blocks[blocks[&old].as_raw() as usize].terminator = Some(term);
    }
    blocks[&plan.header]
}
