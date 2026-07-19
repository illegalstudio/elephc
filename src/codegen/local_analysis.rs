//! Purpose:
//! Precomputes local-slot facts consumed repeatedly by EIR assembly lowering.
//! Tracks explicit stores, ref-cell representation changes, and owned parameter slots.
//!
//! Called from:
//! - `crate::codegen::context::FunctionContext::new()`.
//!
//! Key details:
//! - Ref-cell state is a forward may-analysis, so later promotions never affect earlier ops.
//! - `UnsetLocal` returns a promoted slot to raw local storage on subsequent paths.
//! - Closure, iterator, alias, and explicit binding operations all update representation state.
//! - `CatchBind` counts as a store: the catch slot takes ownership of the in-flight
//!   exception and must join epilogue cleanup / prologue zero-init (issue #448).

use std::collections::{HashSet, VecDeque};

use crate::ir::{
    BlockId, Function, Immediate, InstId, LocalSlotId, Op, Terminator, ValueDef, ValueId,
};
use crate::types::PhpType;

/// Cached local-slot facts for one EIR function.
pub(super) struct LocalSlotAnalysis {
    stored_slots: HashSet<LocalSlotId>,
    ever_ref_cell_slots: HashSet<LocalSlotId>,
    dynamic_ref_cell_slots: HashSet<LocalSlotId>,
    ref_cell_slots_before_inst: HashSet<(InstId, LocalSlotId)>,
    release_ops_with_possible_ref_cell: HashSet<InstId>,
    owned_parameter_slots: HashSet<LocalSlotId>,
}

impl LocalSlotAnalysis {
    /// Computes all local-slot facts with one instruction scan plus CFG propagation.
    pub(super) fn new(function: &Function) -> Self {
        let initially_ref_cell_slots = initially_ref_cell_slots(function);
        let mut stored_slots = HashSet::new();
        let mut ever_ref_cell_slots = initially_ref_cell_slots.clone();
        for inst in &function.instructions {
            // CatchBind moves the in-flight exception into the slot (issue #448), so it
            // must be treated like StoreLocal for epilogue cleanup and prologue zero-init.
            if matches!(inst.op, Op::StoreLocal | Op::CatchBind) {
                if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
                    stored_slots.insert(slot);
                }
            }
            if let Some(slot) =
                ref_cell_target_slot(function, inst.op, inst.immediate.as_ref(), &inst.operands)
            {
                ever_ref_cell_slots.insert(slot);
            }
        }

        let (release_ops_with_possible_ref_cell, ref_cell_slots_before_inst) =
            analyze_release_local_slot_states(function, &initially_ref_cell_slots);
        let owned_parameter_slots = owned_parameter_slots(
            function,
            &stored_slots,
            &ever_ref_cell_slots,
        );
        let dynamic_ref_cell_slots = dynamic_ref_cell_slots(function, &ever_ref_cell_slots);
        Self {
            stored_slots,
            ever_ref_cell_slots,
            dynamic_ref_cell_slots,
            ref_cell_slots_before_inst,
            release_ops_with_possible_ref_cell,
            owned_parameter_slots,
        }
    }

    /// Returns whether this slot receives an owned value via `StoreLocal` or `CatchBind`.
    pub(super) fn has_store(&self, slot: LocalSlotId) -> bool {
        self.stored_slots.contains(&slot)
    }

    /// Returns whether any instruction can rewrite this slot to a ref-cell pointer.
    pub(super) fn ever_stores_ref_cell_pointer(&self, slot: LocalSlotId) -> bool {
        self.ever_ref_cell_slots.contains(&slot)
    }

    /// Iterates slots whose runtime representation can switch between a raw value and a cell.
    pub(super) fn dynamic_ref_cell_slots(&self) -> impl Iterator<Item = LocalSlotId> + '_ {
        self.dynamic_ref_cell_slots.iter().copied()
    }

    /// Returns whether cleanup must inspect this slot's runtime representation flag.
    pub(super) fn has_dynamic_ref_cell_state(&self, slot: LocalSlotId) -> bool {
        self.dynamic_ref_cell_slots.contains(&slot)
    }

    /// Returns whether a deferred raw-slot release may execute after ref-cell promotion.
    pub(super) fn release_may_observe_ref_cell(&self, inst: InstId) -> bool {
        self.release_ops_with_possible_ref_cell.contains(&inst)
    }

    /// Returns whether a slot may already be represented by a cell at one instruction.
    pub(super) fn inst_may_observe_ref_cell(&self, inst: InstId, slot: LocalSlotId) -> bool {
        self.ref_cell_slots_before_inst.contains(&(inst, slot))
    }

    /// Returns whether the frame owns this by-value parameter for its whole lifetime.
    pub(super) fn owns_parameter_slot(&self, slot: LocalSlotId) -> bool {
        self.owned_parameter_slots.contains(&slot)
    }
}

/// Returns by-reference parameter slots, whose incoming representation is already a cell pointer.
fn initially_ref_cell_slots(function: &Function) -> HashSet<LocalSlotId> {
    function
        .params
        .iter()
        .enumerate()
        .filter(|(_, param)| param.by_ref)
        .filter_map(|(index, _)| {
            let slot = LocalSlotId::from_raw(index as u32);
            function.locals.get(index).map(|_| slot)
        })
        .collect()
}

/// Returns the local slot promoted or bound by one ref-cell-producing instruction.
fn ref_cell_target_slot(
    function: &Function,
    op: Op,
    immediate: Option<&Immediate>,
    operands: &[ValueId],
) -> Option<LocalSlotId> {
    match (op, immediate) {
        (
            Op::PromoteLocalRefCell | Op::AliasLocalRefCell,
            Some(Immediate::LocalSlotPair { first, .. }),
        ) => Some(*first),
        (
            Op::BindRefCellPtr | Op::IterCurrentValueRef,
            Some(Immediate::LocalSlot(slot)),
        ) => Some(*slot),
        (Op::ClosureCapture, Some(Immediate::I64(1))) => operands
            .first()
            .copied()
            .and_then(|value| loaded_local_slot(function, value)),
        _ => None,
    }
}

/// Resolves a local-load SSA value back to its frame slot.
fn loaded_local_slot(function: &Function, value: ValueId) -> Option<LocalSlotId> {
    let value = function.value(value)?;
    let ValueDef::Instruction { inst, .. } = value.def else {
        return None;
    };
    let inst = function.instruction(inst)?;
    match (inst.op, inst.immediate.as_ref()) {
        (Op::LoadLocal | Op::LoadRefCell, Some(Immediate::LocalSlot(slot))) => Some(*slot),
        _ => None,
    }
}

/// Computes release instructions whose slot may already contain a ref-cell pointer.
fn analyze_release_local_slot_states(
    function: &Function,
    initially_ref_cell_slots: &HashSet<LocalSlotId>,
) -> (HashSet<InstId>, HashSet<(InstId, LocalSlotId)>) {
    if function.blocks.is_empty() {
        return (HashSet::new(), HashSet::new());
    }
    let mut block_inputs = vec![None::<HashSet<LocalSlotId>>; function.blocks.len()];
    let mut block_outputs = vec![None::<HashSet<LocalSlotId>>; function.blocks.len()];
    let entry_index = function.entry.as_raw() as usize;
    if entry_index >= function.blocks.len() {
        return (HashSet::new(), HashSet::new());
    }
    block_inputs[entry_index] = Some(initially_ref_cell_slots.clone());
    let mut worklist = VecDeque::from([function.entry]);
    while let Some(block_id) = worklist.pop_front() {
        let block_index = block_id.as_raw() as usize;
        let Some(mut state) = block_inputs[block_index].clone() else {
            continue;
        };
        let block = &function.blocks[block_index];
        for inst_id in &block.instructions {
            apply_ref_cell_transfer(function, *inst_id, &mut state);
        }
        if block_outputs[block_index].as_ref() == Some(&state) {
            continue;
        }
        block_outputs[block_index] = Some(state.clone());
        let Some(terminator) = block.terminator.as_ref() else {
            continue;
        };
        for successor in terminator_successors(terminator) {
            let successor_index = successor.as_raw() as usize;
            if successor_index >= block_inputs.len() {
                continue;
            }
            let changed = if let Some(input) = &mut block_inputs[successor_index] {
                let old_len = input.len();
                input.extend(state.iter().copied());
                input.len() != old_len
            } else {
                block_inputs[successor_index] = Some(state.clone());
                true
            };
            if changed {
                worklist.push_back(successor);
            }
        }
    }

    let mut releases = HashSet::new();
    let mut ref_cell_slots_before_inst = HashSet::new();
    for block in &function.blocks {
        let Some(mut state) = block_inputs[block.id.as_raw() as usize].clone() else {
            continue;
        };
        for inst_id in &block.instructions {
            ref_cell_slots_before_inst.extend(state.iter().map(|slot| (*inst_id, *slot)));
            let Some(inst) = function.instruction(*inst_id) else {
                continue;
            };
            if inst.op == Op::ReleaseLocalSlot {
                if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
                    if state.contains(&slot) {
                        releases.insert(*inst_id);
                    }
                }
            }
            apply_ref_cell_transfer(function, *inst_id, &mut state);
        }
    }
    (releases, ref_cell_slots_before_inst)
}

/// Applies one instruction's local representation change to the forward state.
fn apply_ref_cell_transfer(
    function: &Function,
    inst_id: InstId,
    state: &mut HashSet<LocalSlotId>,
) {
    let Some(inst) = function.instruction(inst_id) else {
        return;
    };
    if inst.op == Op::AliasLocalRefCell {
        if let Some(Immediate::LocalSlotPair { first, second }) = inst.immediate {
            // Alias lowering guarantees the source is promoted on the runtime
            // raw path before the target receives the shared cell pointer.
            state.insert(first);
            state.insert(second);
            return;
        }
    }
    if let Some(slot) =
        ref_cell_target_slot(function, inst.op, inst.immediate.as_ref(), &inst.operands)
    {
        state.insert(slot);
        return;
    }
    if inst.op == Op::UnsetLocal {
        if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
            state.remove(&slot);
        }
    }
}

/// Returns all CFG successors named by one terminator.
fn terminator_successors(terminator: &Terminator) -> Vec<BlockId> {
    match terminator {
        Terminator::Br { target, .. } => vec![*target],
        Terminator::CondBr {
            then_target,
            else_target,
            ..
        } => vec![*then_target, *else_target],
        Terminator::Switch { cases, default, .. } => {
            let mut successors = cases.iter().map(|case| case.target).collect::<Vec<_>>();
            successors.push(*default);
            successors
        }
        Terminator::GeneratorSuspend { resume, .. } => vec![*resume],
        Terminator::Return { .. }
        | Terminator::Throw { .. }
        | Terminator::Fatal { .. }
        | Terminator::Unreachable => Vec::new(),
    }
}

/// Returns by-value parameter slots that must own incoming or subsequently stored values.
fn owned_parameter_slots(
    function: &Function,
    stored_slots: &HashSet<LocalSlotId>,
    ever_ref_cell_slots: &HashSet<LocalSlotId>,
) -> HashSet<LocalSlotId> {
    function
        .params
        .iter()
        .enumerate()
        .filter(|(_, param)| !param.by_ref)
        .filter_map(|(index, param)| {
            let slot = LocalSlotId::from_raw(index as u32);
            let local = function.locals.get(index)?;
            let local_ty = local.php_type.codegen_repr();
            if !local_type_needs_cleanup(&local_ty) {
                return None;
            }
            let prologue_boxes_owned_mixed = local_ty == PhpType::Mixed
                && param.php_type.codegen_repr() != PhpType::Mixed;
            (stored_slots.contains(&slot)
                || ever_ref_cell_slots.contains(&slot)
                || prologue_boxes_owned_mixed)
                .then_some(slot)
        })
        .collect()
}

/// Returns whether a local representation has an implemented frame cleanup path.
fn local_type_needs_cleanup(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Str | PhpType::Callable) || ty.is_refcounted()
}

/// Returns slots whose raw-value/ref-cell representation can change at runtime.
fn dynamic_ref_cell_slots(
    function: &Function,
    ever_ref_cell_slots: &HashSet<LocalSlotId>,
) -> HashSet<LocalSlotId> {
    function
        .locals
        .iter()
        .filter(|local| ever_ref_cell_slots.contains(&local.id))
        .filter(|local| {
            matches!(
                local.kind,
                crate::ir::LocalKind::PhpLocal
                    | crate::ir::LocalKind::HiddenTemp
                    | crate::ir::LocalKind::OwnedTemp
                    | crate::ir::LocalKind::ClosureCapture
                    | crate::ir::LocalKind::NamedArgTemp
                    | crate::ir::LocalKind::IteratorState
                    | crate::ir::LocalKind::GeneratorState
            )
        })
        .filter(|local| {
            function
                .params
                .get(local.id.as_raw() as usize)
                .is_none_or(|param| !param.by_ref)
        })
        .map(|local| local.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::generate_user_asm_from_ir;
    use crate::codegen::platform::{Arch, Platform, Target};
    use crate::ir::{Builder, FunctionParam, IrType, LocalKind, Module, Ownership};

    /// Verifies a later promotion does not flow backward into an earlier deferred release.
    #[test]
    fn later_promotion_does_not_suppress_earlier_release() {
        let mut function =
            Function::new("later_promotion".to_string(), IrType::Void, PhpType::Void);
        let slot = function.add_local(
            Some("x".to_string()),
            IrType::Heap(crate::ir::IrHeapKind::Mixed),
            PhpType::Mixed,
            LocalKind::PhpLocal,
        );
        let owner = function.add_local(
            None,
            IrType::Heap(crate::ir::IrHeapKind::Mixed),
            PhpType::Mixed,
            LocalKind::RefCell,
        );
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            builder.emit(
                Op::ReleaseLocalSlot,
                Vec::new(),
                Some(Immediate::LocalSlot(slot)),
                IrType::Void,
                PhpType::Mixed,
                Ownership::NonHeap,
            );
            builder.emit(
                Op::PromoteLocalRefCell,
                Vec::new(),
                Some(Immediate::LocalSlotPair {
                    first: slot,
                    second: owner,
                }),
                IrType::Void,
                PhpType::Mixed,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }

        let analysis = LocalSlotAnalysis::new(&function);
        assert!(!analysis.release_may_observe_ref_cell(InstId::from_raw(0)));
        assert!(!analysis.inst_may_observe_ref_cell(InstId::from_raw(0), slot));
        assert!(analysis.has_dynamic_ref_cell_state(slot));
    }

    /// Verifies a promotion on a predecessor path protects the cell pointer from raw cleanup.
    #[test]
    fn prior_promotion_suppresses_later_release() {
        let mut function =
            Function::new("prior_promotion".to_string(), IrType::Void, PhpType::Void);
        let slot = function.add_local(
            Some("x".to_string()),
            IrType::Heap(crate::ir::IrHeapKind::Mixed),
            PhpType::Mixed,
            LocalKind::PhpLocal,
        );
        let owner = function.add_local(
            None,
            IrType::Heap(crate::ir::IrHeapKind::Mixed),
            PhpType::Mixed,
            LocalKind::RefCell,
        );
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            builder.emit(
                Op::PromoteLocalRefCell,
                Vec::new(),
                Some(Immediate::LocalSlotPair {
                    first: slot,
                    second: owner,
                }),
                IrType::Void,
                PhpType::Mixed,
                Ownership::NonHeap,
            );
            builder.emit(
                Op::ReleaseLocalSlot,
                Vec::new(),
                Some(Immediate::LocalSlot(slot)),
                IrType::Void,
                PhpType::Mixed,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }

        let analysis = LocalSlotAnalysis::new(&function);
        assert!(analysis.release_may_observe_ref_cell(InstId::from_raw(1)));
        assert!(analysis.inst_may_observe_ref_cell(InstId::from_raw(1), slot));
    }

    /// Verifies a widened scalar parameter stays owned even if optimization removes its stores.
    #[test]
    fn widened_parameter_owns_prologue_mixed_box_without_remaining_store() {
        let mut function = Function::new(
            "widened_parameter".to_string(),
            IrType::Void,
            PhpType::Void,
        );
        function.params.push(FunctionParam {
            name: "value".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Int,
            by_ref: false,
            variadic: false,
        });
        let slot = function.add_local(
            Some("value".to_string()),
            IrType::Heap(crate::ir::IrHeapKind::Mixed),
            PhpType::Mixed,
            LocalKind::PhpLocal,
        );

        let analysis = LocalSlotAnalysis::new(&function);
        assert!(analysis.owns_parameter_slot(slot));
    }

    /// Verifies incoming by-reference cells remain borrowed and need no raw-value state flag.
    #[test]
    fn by_ref_parameter_is_not_treated_as_dynamic_owned_storage() {
        let mut function = Function::new(
            "by_ref_parameter".to_string(),
            IrType::Void,
            PhpType::Void,
        );
        function.params.push(FunctionParam {
            name: "value".to_string(),
            ir_type: IrType::Heap(crate::ir::IrHeapKind::Mixed),
            php_type: PhpType::Mixed,
            by_ref: true,
            variadic: false,
        });
        let slot = function.add_local(
            Some("value".to_string()),
            IrType::Heap(crate::ir::IrHeapKind::Mixed),
            PhpType::Mixed,
            LocalKind::PhpLocal,
        );

        let analysis = LocalSlotAnalysis::new(&function);
        assert!(analysis.ever_stores_ref_cell_pointer(slot));
        assert!(!analysis.has_dynamic_ref_cell_state(slot));
        assert!(!analysis.owns_parameter_slot(slot));
    }

    /// Verifies AArch64 cleanup guards skip raw release when the runtime slot is a cell.
    #[test]
    fn dynamic_release_emits_aarch64_representation_guard() {
        let asm = dynamic_release_asm(Target::new(Platform::Linux, Arch::AArch64));

        assert!(asm.contains("cbnz x0, _eir_main_raw_local_cleanup_done"), "{asm}");
    }

    /// Verifies x86_64 cleanup guards skip raw release when the runtime slot is a cell.
    #[test]
    fn dynamic_release_emits_x86_64_representation_guard() {
        let asm = dynamic_release_asm(Target::new(Platform::Linux, Arch::X86_64));

        assert!(asm.contains("test rax, rax"), "{asm}");
        assert!(asm.contains("jne _eir_main_raw_local_cleanup_done"), "{asm}");
    }

    /// Builds a conditional promotion followed by deferred cleanup for one target.
    fn dynamic_release_asm(target: Target) -> String {
        let mut module = Module::new(target);
        let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        let slot = function.add_local(
            Some("value".to_string()),
            IrType::Heap(crate::ir::IrHeapKind::Mixed),
            PhpType::Mixed,
            LocalKind::PhpLocal,
        );
        let owner = function.add_local(
            None,
            IrType::Heap(crate::ir::IrHeapKind::Mixed),
            PhpType::Mixed,
            LocalKind::RefCell,
        );
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            let promoted = builder.create_named_block("promoted", Vec::new());
            let raw = builder.create_named_block("raw", Vec::new());
            let merge = builder.create_named_block("merge", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let cond = builder.emit_const_bool(true);
            builder.terminate(Terminator::CondBr {
                cond,
                then_target: promoted,
                then_args: Vec::new(),
                else_target: raw,
                else_args: Vec::new(),
            });
            builder.position_at_end(promoted);
            builder.emit(
                Op::PromoteLocalRefCell,
                Vec::new(),
                Some(Immediate::LocalSlotPair {
                    first: slot,
                    second: owner,
                }),
                IrType::Void,
                PhpType::Mixed,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Br {
                target: merge,
                args: Vec::new(),
            });
            builder.position_at_end(raw);
            builder.terminate(Terminator::Br {
                target: merge,
                args: Vec::new(),
            });
            builder.position_at_end(merge);
            builder.emit(
                Op::ReleaseLocalSlot,
                Vec::new(),
                Some(Immediate::LocalSlot(slot)),
                IrType::Void,
                PhpType::Mixed,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        generate_user_asm_from_ir(&module, false, false)
            .expect("dynamic ReleaseLocalSlot fixture should lower")
    }
}
