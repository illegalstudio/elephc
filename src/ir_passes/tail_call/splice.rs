//! Purpose:
//! Builds shared-frame recursive bodies and typed parameter-rebinding blocks.
//!
//! Called from:
//! - `super::eliminate_tail_recursion` for each original ABI entry in a recursive component.
//!
//! Key details:
//! - Every cloned ID and definition site is remapped, including unreachable instructions.
//! - Rebinding uses existing EIR parallel branch moves, preserving argument evaluation order.
//! - Local loads lose any earlier immutability proof before the new loops are optimized.

use std::collections::HashMap;

use crate::ir::{BlockId, Builder, Function, Immediate, InstId, IrType, LocalSlotId, Op, Ownership, Terminator, ValueDef, ValueId};
use crate::types::PhpType;

use super::{eligibility::parameter_slots, TailSite};
use super::super::inline::{remap_immediate_local, remap_terminator};
use super::super::rewrite::neutralize_to_nop;

/// Maps one original body's blocks, instructions and slots into the combined function.
struct BodyMap {
    blocks: HashMap<BlockId, BlockId>,
    instructions: HashMap<InstId, InstId>,
    locals: HashMap<LocalSlotId, LocalSlotId>,
}

/// Returns an identity mapping for the original host body.
fn identity_map(function: &Function) -> BodyMap {
    BodyMap {
        blocks: function.blocks.iter().map(|block| (block.id, block.id)).collect(),
        instructions: (0..function.instructions.len() as u32).map(|raw| {
            let id = InstId::from_raw(raw); (id, id)
        }).collect(),
        locals: function.locals.iter().map(|local| (local.id, local.id)).collect(),
    }
}

/// Appends a complete body with fresh table IDs and unique non-parameter slot names.
fn clone_body(host: &mut Function, source: &Function) -> BodyMap {
    let map = BodyMap {
        blocks: source.blocks.iter().map(|block| (block.id,
            BlockId::from_raw(host.blocks.len() as u32 + block.id.as_raw()))).collect(),
        instructions: (0..source.instructions.len() as u32).map(|raw| (InstId::from_raw(raw),
            InstId::from_raw(host.instructions.len() as u32 + raw))).collect(),
        locals: source.locals.iter().map(|local| (local.id,
            LocalSlotId::from_raw(host.locals.len() as u32 + local.id.as_raw()))).collect(),
    };
    let values: HashMap<_, _> = (0..source.values.len() as u32).map(|raw| (ValueId::from_raw(raw),
        ValueId::from_raw(host.values.len() as u32 + raw))).collect();
    for local in &source.locals {
        host.add_local(Some(format!("__tail_{}_{}", source.name, local.id.as_raw())),
            local.ir_type, local.php_type.clone(), local.kind);
    }
    for old in &source.values {
        let mut value = old.clone();
        value.def = match old.def {
            ValueDef::BlockParam { block, index } => ValueDef::BlockParam { block: map.blocks[&block], index },
            ValueDef::Instruction { block, index, inst } => ValueDef::Instruction {
                block: map.blocks[&block], index, inst: map.instructions[&inst],
            },
        };
        host.values.push(value);
    }
    for old in &source.instructions {
        let mut inst = old.clone();
        inst.operands = old.operands.iter().map(|arg| values[arg]).collect();
        inst.result = old.result.map(|value| values[&value]);
        if let Some(immediate) = &mut inst.immediate { remap_immediate_local(immediate, &map.locals); }
        host.instructions.push(inst);
    }
    for old in &source.blocks {
        let mut block = old.clone();
        block.id = map.blocks[&old.id];
        block.name = format!("tail_{}_{}", source.name, old.name);
        block.params = old.params.iter().map(|param| values[param]).collect();
        block.instructions = old.instructions.iter().map(|id| map.instructions[id]).collect();
        block.terminator = old.terminator.clone().map(|term| remap_terminator(term, &map.blocks, &values));
        host.blocks.push(block);
    }
    map
}

/// Builds one activation boundary: clear scalar locals, bind parameters, enter the body.
fn rebind_block(host: &mut Function, source: &Function, map: &BodyMap) -> BlockId {
    let slots = parameter_slots(source).expect("eligibility checked parameter slots");
    let mut builder = Builder::new(host);
    let header = builder.create_named_block(format!("tail_rebind_{}", source.name),
        source.params.iter().map(|param| (param.ir_type, param.php_type.clone())).collect());
    builder.position_at_end(header);
    for local in &source.locals {
        if !slots.contains(&local.id) {
            builder.emit(Op::ZeroLocalSlot, vec![], Some(Immediate::LocalSlot(map.locals[&local.id])),
                IrType::Void, PhpType::Void, Ownership::NonHeap);
        }
    }
    for (index, slot) in slots.iter().enumerate() {
        let arg = builder.block_param(header, index);
        builder.emit_store_local(map.locals[slot], arg);
    }
    builder.terminate(Terminator::Br { target: map.blocks[&source.entry], args: vec![] });
    header
}

/// Lowers a recursive component into this host, retaining its original public signature.
pub(super) fn lower_component(
    host: &mut Function,
    host_index: usize,
    component: &[usize],
    sources: &[Function],
    sites: &[Vec<TailSite>],
) {
    let mut maps = HashMap::new();
    maps.insert(host_index, identity_map(host));
    for &index in component {
        if index != host_index { maps.insert(index, clone_body(host, &sources[index])); }
    }
    let headers: HashMap<_, _> = component.iter().map(|&index| {
        (index, rebind_block(host, &sources[index], &maps[&index]))
    }).collect();
    // The ABI prologue runs exactly once. Subsequent activations enter a rebinding block.
    let slots = parameter_slots(&sources[host_index]).expect("checked parameter slots");
    let mut builder = Builder::new(host);
    let initial = builder.create_named_block("tail_initial", vec![]);
    builder.set_entry(initial);
    builder.position_at_end(initial);
    let args = slots.iter().zip(&sources[host_index].params).map(|(&slot, param)| {
        builder.emit_load_local(slot, param.ir_type, param.php_type.clone())
    }).collect();
    builder.terminate(Terminator::Br { target: headers[&host_index], args });
    for &index in component {
        for site in &sites[index] {
            let Some(&target) = headers.get(&site.callee) else { continue; };
            let map = &maps[&index];
            let call_id = map.instructions[&site.call];
            let mut args = host.instructions[call_id.as_raw() as usize].operands.clone();
            let call_span = host.instructions[call_id.as_raw() as usize].span;
            let block = map.blocks[&site.block];
            // Materialize the same numeric coercion as the ordinary call ABI while its
            // argument owners are still live, then retire those owners before rebinding.
            let position = host.blocks[block.as_raw() as usize].instructions.iter()
                .position(|&id| id == call_id).expect("call belongs to mapped block");
            let suffix = host.blocks[block.as_raw() as usize].instructions.split_off(position);
            let terminator = host.blocks[block.as_raw() as usize].terminator.take();
            let mut builder = Builder::new(host);
            builder.position_at_end(block);
            for (arg, param) in args.iter_mut().zip(&sources[site.callee].params) {
                if builder.function().value(*arg).expect("argument exists").php_type != param.php_type {
                    *arg = builder.emit_with_effects(Op::Cast, vec![*arg],
                        Some(Immediate::CastTarget(param.ir_type)), param.ir_type, param.php_type.clone(),
                        Ownership::NonHeap, Op::Cast.default_effects(), call_span).expect("cast result");
                }
            }
            host.blocks[block.as_raw() as usize].instructions.extend(suffix);
            host.blocks[block.as_raw() as usize].terminator = terminator;
            // Inserting coercions shifts the definition indices of the original suffix.
            for (index, &id) in host.blocks[block.as_raw() as usize].instructions.iter().enumerate() {
                if let Some(result) = host.instructions[id.as_raw() as usize].result {
                    host.values[result.as_raw() as usize].def = ValueDef::Instruction { block, index: index as u32, inst: id };
                }
            }
            neutralize_to_nop(&mut host.instructions[call_id.as_raw() as usize]);
            for id in &site.erase {
                neutralize_to_nop(&mut host.instructions[map.instructions[id].as_raw() as usize]);
            }
            host.blocks[map.blocks[&site.block].as_raw() as usize].terminator =
                Some(Terminator::Br { target, args });
        }
    }
    for inst in &mut host.instructions {
        if inst.op == Op::LoadLocal { inst.effects |= Op::LoadLocal.default_effects(); }
    }
}
