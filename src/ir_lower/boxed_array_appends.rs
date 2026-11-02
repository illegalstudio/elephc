//! Purpose:
//! Normalizes array mutations after the complete function has determined local storage.
//!
//! Called from:
//! - Module lowering before effect refinement and EIR validation.
//!
//! Key details:
//! - A later reference argument can widen an earlier native array local to boxed storage.
//! - Append mutates that box directly instead of leaking an owned unbox and its replaced box.
//! - Hoisted element-layout conversions release the old box before storing their replacement.
//! - The receiver is fetched after RHS evaluation, so RHS replacement of the local is observed.

use crate::ir::{
    Function, Immediate, InstId, Instruction, IrHeapKind, IrType, LocalKind, Module, Op,
    Ownership, ValueDef, LocalSlotId,
};
use crate::types::PhpType;

/// Repairs every function's append receivers using final local storage contracts.
pub(super) fn normalize_module(module: &mut Module) {
    for function in module.functions.iter_mut()
        .chain(module.class_methods.iter_mut())
        .chain(module.closures.iter_mut())
    {
        normalize_function(function);
        release_boxed_conversion_sources(function);
    }
}

/// Replaces single-use native local reads with adjacent borrowed boxed reads for append.
fn normalize_function(function: &mut Function) {
    // Static locals cannot enter this state: the checker excludes them from by-reference
    // promotion to boxed Mixed storage, and their static slot keeps its initializer layout.
    if !function.locals.iter().any(|local| local.kind == LocalKind::PhpLocal
        && matches!(local.php_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_)))
        || !function.instructions.iter().any(|inst| matches!(inst.op, Op::ArrayPush | Op::HashAppend))
    { return; }
    let uses = function.value_use_counts();
    let native_layouts = observed_native_array_layouts(function);
    for block in &mut function.blocks {
        let appends: Vec<_> = block.instructions.iter().copied().filter(|id| {
            matches!(function.instructions[id.as_raw() as usize].op, Op::ArrayPush | Op::HashAppend)
        }).collect();
        let mut changed = false;
        for append_id in appends {
            let append_index = append_id.as_raw() as usize;
            let receiver = function.instructions[append_index].operands[0];
            if uses.get(&receiver).copied() != Some(1) { continue; }
            let ValueDef::Instruction { block: source_block, inst: load_id, .. } =
                function.values[receiver.as_raw() as usize].def else { continue; };
            if source_block != block.id { continue; }
            let load = &function.instructions[load_id.as_raw() as usize];
            if !matches!(load.op, Op::LoadLocal | Op::LoadRefCell) { continue; }
            let Some(Immediate::LocalSlot(slot)) = load.immediate else { continue; };
            let local = &function.locals[slot.as_raw() as usize];
            if local.kind != LocalKind::PhpLocal
                || !matches!(local.php_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
                || !matches!(load.result_php_type.codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. })
                || native_layouts.contains(&slot)
            { continue; }

            let load = &mut function.instructions[load_id.as_raw() as usize];
            load.result_type = IrType::Heap(IrHeapKind::Mixed);
            load.result_php_type = PhpType::Mixed;
            load.result_ownership = Ownership::Borrowed;
            let value = &mut function.values[receiver.as_raw() as usize];
            value.ir_type = IrType::Heap(IrHeapKind::Mixed);
            value.php_type = PhpType::Mixed;
            value.ownership = Ownership::Borrowed;
            let append = &mut function.instructions[append_index];
            append.op = Op::MixedArrayAppend;
            append.effects = Op::MixedArrayAppend.default_effects();

            block.instructions.retain(|id| *id != load_id);
            let position = block.instructions.iter().position(|id| *id == append_id).unwrap();
            block.instructions.insert(position, load_id);
            changed = true;
        }
        if changed {
            for (index, id) in block.instructions.iter().copied().enumerate() {
                if let Some(result) = function.instructions[id.as_raw() as usize].result {
                    function.values[result.as_raw() as usize].def = ValueDef::Instruction {
                        block: block.id, index: index as u32, inst: id,
                    };
                }
            }
        }
    }
}

/// Keeps native element layouts when another consumer observes a concrete array view.
/// Boxed append converts every element to Mixed, which cannot feed a typed sort or builtin.
fn observed_native_array_layouts(function: &Function) -> std::collections::HashSet<LocalSlotId> {
    let observed: std::collections::HashSet<_> = function.instructions.iter()
        .filter(|inst| !matches!(inst.op, Op::ArrayPush | Op::Release))
        .flat_map(|inst| inst.operands.iter().copied()).collect();
    function.instructions.iter().filter_map(|load| {
        if !matches!(load.op, Op::LoadLocal | Op::LoadRefCell)
            || !load.result.is_some_and(|value| observed.contains(&value))
        { return None; }
        let PhpType::Array(element) = load.result_php_type.codegen_repr() else { return None; };
        if matches!(element.codegen_repr(), PhpType::Mixed | PhpType::Never | PhpType::Void) {
            return None;
        }
        match load.immediate {
            Some(Immediate::LocalSlot(slot)) => Some(slot),
            _ => None,
        }
    }).collect()
}

/// Releases a final Mixed local's old box before an in-place element-layout conversion.
///
/// Statement fixed-point lowering can hoist `ArrayToMixed` before a loop while a later reference
/// call widens the same frame slot only after that statement has been lowered. The conversion load
/// then owns the unboxed array, but the following `StoreLocal` would overwrite the old Mixed box
/// without retiring it. Insert the cleanup once the function's final slot type is authoritative.
fn release_boxed_conversion_sources(function: &mut Function) {
    let mut insertions = Vec::new();
    for (block_index, block) in function.blocks.iter().enumerate() {
        for (store_position, store_id) in block.instructions.iter().copied().enumerate() {
            let store = &function.instructions[store_id.as_raw() as usize];
            if store.op != Op::StoreLocal { continue; }
            let Some(Immediate::LocalSlot(slot)) = store.immediate else { continue; };
            if !matches!(
                function.locals[slot.as_raw() as usize].php_type.codegen_repr(),
                PhpType::Mixed | PhpType::Union(_),
            ) { continue; }
            let Some(&stored) = store.operands.first() else { continue; };
            let ValueDef::Instruction {
                block: conversion_block,
                index: conversion_position,
                inst: conversion_id,
            } = function.values[stored.as_raw() as usize].def else { continue; };
            if conversion_block != block.id || conversion_position as usize >= store_position {
                continue;
            }
            let conversion = &function.instructions[conversion_id.as_raw() as usize];
            if conversion.op != Op::ArrayToMixed { continue; }
            let Some(&source) = conversion.operands.first() else { continue; };
            let ValueDef::Instruction {
                block: load_block,
                index: load_position,
                inst: load_id,
            } = function.values[source.as_raw() as usize].def else { continue; };
            if load_block != block.id || load_position >= conversion_position { continue; }
            let load = &function.instructions[load_id.as_raw() as usize];
            if load.op != Op::LoadLocal
                || load.immediate != Some(Immediate::LocalSlot(slot))
            {
                continue;
            }
            let already_released = block.instructions
                [load_position as usize + 1..conversion_position as usize]
                .iter()
                .any(|id| {
                    let inst = &function.instructions[id.as_raw() as usize];
                    inst.op == Op::ReleaseLocalSlot
                        && inst.immediate == Some(Immediate::LocalSlot(slot))
                });
            if !already_released {
                insertions.push((block_index, conversion_position as usize, slot, conversion.span));
            }
        }
    }
    for (block_index, position, slot, span) in insertions.into_iter().rev() {
        let id = InstId::from_raw(function.instructions.len() as u32);
        function.instructions.push(Instruction::new(
            Op::ReleaseLocalSlot,
            Vec::new(),
            Some(Immediate::LocalSlot(slot)),
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            Op::ReleaseLocalSlot.default_effects(),
            span,
        ));
        function.blocks[block_index].instructions.insert(position, id);
    }
    for block in &function.blocks {
        for (index, id) in block.instructions.iter().copied().enumerate() {
            if let Some(result) = function.instructions[id.as_raw() as usize].result {
                function.values[result.as_raw() as usize].def = ValueDef::Instruction {
                    block: block.id,
                    index: index as u32,
                    inst: id,
                };
            }
        }
    }
}
