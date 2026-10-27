//! Purpose:
//! Removes redundant method receiver leases backed by unchanged concrete object parameters.
//!
//! Called from:
//! - The EIR driver's preparation phase, before cross-function inlining.
//!
//! Key details:
//! - The parameter's frame owner survives the call, including exceptional exits.
//! - Only direct parameter loads used by a method receiver and its root store qualify.
//! - Eval, reference aliases, rebinding and non-object storage retain their original guards.

use std::collections::{HashMap, HashSet};

use crate::ir::{DataPool, Function, Immediate, LocalKind, Module, Op, ValueDef, ValueId};
use crate::ir::parameter_roots::borrowed_parameter_roots;
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::{neutralize_to_nop, replace_all_uses};

/// Canonicalizes proven parameter borrows before the inliner creates new receiver bindings.
pub(super) struct BorrowedReceiverRoots {
    /// Dynamic scope operations can write parameters without a visible local-slot operation.
    enabled: bool,
}

impl BorrowedReceiverRoots {
    /// Keeps modules with interpreted scope access on the conservative ownership path.
    pub(super) fn new(module: &Module) -> Self {
        Self { enabled: !module.required_runtime_features.eval_bridge }
    }
}

impl IrPass for BorrowedReceiverRoots {
    /// Identifies the ownership preparation pass in validation diagnostics.
    fn name(&self) -> &'static str { "borrowed-receiver-roots" }

    /// Runs before inlining can merge a receiver lease with another ownership graph.
    fn run_before_inlining(&self) -> bool { true }

    /// Avoids analysis when interpreted scope writes or the absence of leases rules it out.
    fn is_applicable(&self, function: &Function) -> bool {
        self.enabled && function.instructions.iter().any(|inst| {
            inst.op == Op::Acquire && matches!(inst.result_php_type, PhpType::Object(_))
        })
    }

    /// Replaces only the receiver's extra retain and its matching publication/retirement.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        if !self.enabled { return false; }
        let roots = borrowed_parameter_roots(function);
        let mut removed_slots = HashSet::new();
        let mut acquires = Vec::new();
        let mut replacements = HashMap::new();
        for store in &function.instructions {
            let Some(Immediate::LocalSlot(slot)) = store.immediate else { continue; };
            if store.op != Op::StoreLocal || !roots.contains(&slot) { continue; }
            let Some(&rooted) = store.operands.first() else { continue; };
            let Some(value) = function.value(rooted) else { continue; };
            let ValueDef::Instruction { inst: acquire_id, .. } = value.def else { continue; };
            let Some(acquire) = function.instruction(acquire_id) else { continue; };
            if acquire.op != Op::Acquire || acquire.immediate.is_some()
                || !matches!(value.php_type, PhpType::Object(_))
            { continue; }
            let Some(&source) = acquire.operands.first() else { continue; };
            let Some(source_value) = function.value(source) else { continue; };
            let ValueDef::Instruction { inst: load_id, .. } = source_value.def else { continue; };
            let Some(load) = function.instruction(load_id) else { continue; };
            let Some(Immediate::LocalSlot(parameter)) = load.immediate else { continue; };
            let Some(local) = function.locals.get(parameter.as_raw() as usize) else { continue; };
            if load.op != Op::LoadLocal || local.kind != LocalKind::PhpLocal
                || local.php_type != source_value.php_type || value.php_type != source_value.php_type
                || !matches!(local.php_type, PhpType::Object(_))
                || !receiver_uses_only(function, rooted, slot)
            { continue; }
            replacements.insert(rooted, source);
            acquires.push(acquire_id);
            removed_slots.insert(slot);
        }
        if replacements.is_empty() { return false; }
        replace_all_uses(function, &replacements);
        for id in acquires { neutralize_to_nop(&mut function.instructions[id.as_raw() as usize]); }
        for instruction in &mut function.instructions {
            if matches!(instruction.immediate, Some(Immediate::LocalSlot(slot)) if removed_slots.contains(&slot)) {
                neutralize_to_nop(instruction);
            }
        }
        for slot in removed_slots {
            // Unused slots must not keep a destructor obligation that prevents later inlining.
            function.locals[slot.as_raw() as usize].kind = LocalKind::BorrowedTemp;
        }
        true
    }
}

/// Rejects every ownership escape except the root store and ordinary method receiver borrows.
fn receiver_uses_only(function: &Function, value: ValueId, root: crate::ir::LocalSlotId) -> bool {
    let mut receiver = false;
    for instruction in &function.instructions {
        for (index, &operand) in instruction.operands.iter().enumerate() {
            if operand != value { continue; }
            match instruction.op {
                Op::MethodCall if index == 0 => receiver = true,
                Op::StoreLocal if index == 0 && instruction.immediate == Some(Immediate::LocalSlot(root)) => {}
                _ => return false,
            }
        }
    }
    receiver && function.blocks.iter().all(|block| {
        block.terminator.as_ref().is_none_or(|term| !term.used_values().contains(&value))
    })
}
