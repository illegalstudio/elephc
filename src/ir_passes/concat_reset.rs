//! Purpose:
//! Concat scratch-state optimization over EIR functions. Removes or coalesces
//! statement-boundary `concat_reset` instructions when CFG dataflow proves the
//! concat scratch cursor `_concat_off` is already at the frame base or will be
//! reset before any instruction can allocate, read, modify, or observe it.
//!
//! Called from:
//! - The fixed-point pass driver in `crate::ir_passes::driver`.
//!
//! Key details:
//! - Every function, method, closure, main entry, and web handler prologue
//!   captures `_concat_off` into its stack frame's `concat_base_offset`, and
//!   `Op::ConcatReset` restores `_concat_off` to that frame base. Lowering
//!   persists any value that outlives a statement (`Op::StrPersist` / `Op::Acquire`)
//!   before emitting the statement-boundary `Op::ConcatReset`.
//! - Phase 1 (forward clean-state elimination) computes a greatest-fixed-point
//!   `in_clean` / `out_clean` fact over reachable blocks: `_concat_off` equals
//!   `concat_base_offset` at function entry and immediately after `Op::ConcatReset`,
//!   and remains clean across instructions on a strict whitelist of concat-inert
//!   operations. Any `Op::ConcatReset` reached when the state is already clean on
//!   all incoming paths is neutralized to `nop`. This eliminates statement-boundary
//!   resets across scalar/arithmetic/container-only straight-line code and loop
//!   bodies while keeping any needed reset before the loop.
//! - Phase 2 (backward unobserved-reset coalescing) computes a greatest-fixed-point
//!   `in_will_reset` / `out_will_reset` fact: when every path from an earlier
//!   `Op::ConcatReset` reaches a later `Op::ConcatReset` before any instruction
//!   that can allocate, read, modify, or observe concat scratch (or throw/exit),
//!   the earlier reset is coalesced into the later one by neutralizing the earlier
//!   instruction to `nop`.
//! - Exception handler targets named by `Op::TryPushHandler` are seeded as dirty
//!   on entry because `__rt_throw_current` rewinds `_concat_off` and unwinding may
//!   leave an arbitrary scratch offset from inside the `try` region.

use crate::ir::{
    BlockId, DataPool, Effects, Function, Immediate, InstId, Instruction, IrType, LocalSlotId, Op,
    Terminator, ValueId,
};
use crate::types::PhpType;

use super::cfg::{predecessors, reverse_postorder, successors};
use super::driver::IrPass;
use super::rewrite::neutralize_to_nop;

/// Concat scratch-state optimization pass.
pub struct ConcatResetElim;

impl IrPass for ConcatResetElim {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "concat-reset"
    }

    /// Skips functions with no `concat_reset` or single-block functions where
    /// every `concat_reset` is already separated by an unconditional barrier.
    fn is_applicable(&self, function: &Function) -> bool {
        if let [block] = function.blocks.as_slice() {
            if !block
                .terminator
                .as_ref()
                .is_some_and(terminator_preserves_concat_state)
            {
                let mut dirty_since_last_reset = false;
                for &inst_id in &block.instructions {
                    let Some(inst) = function.instruction(inst_id) else {
                        continue;
                    };
                    if inst.op == Op::ConcatReset {
                        if !dirty_since_last_reset {
                            return true;
                        }
                        dirty_since_last_reset = false;
                    } else if is_unconditional_concat_barrier(inst) {
                        dirty_since_last_reset = true;
                    }
                }
                return false;
            }
        }
        function
            .instructions
            .iter()
            .any(|inst| inst.op == Op::ConcatReset)
    }

    /// Eliminates redundant clean-state `concat_reset` instructions and coalesces
    /// unobserved back-to-back resets across basic blocks.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        if function.blocks.is_empty() {
            return false;
        }
        let inert = ConcatInertAnalysis::new(function).compute_all();
        let handlers = collect_handler_blocks(function);
        let reachable = collect_reachable_blocks(function, &handlers);
        let mut changed = false;
        changed |= eliminate_clean_concat_resets(function, &inert, &handlers, &reachable);
        changed |= coalesce_postdominated_concat_resets(function, &inert, &reachable);
        changed
    }
}

/// Three-state memoization entry for destructor-freeness queries over values and local slots.
#[derive(Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Unvisited,
    Visiting,
    Done(bool),
}

/// Per-function memoized analysis of concat-inert instructions and destructor-free values/slots.
struct ConcatInertAnalysis<'a> {
    function: &'a Function,
    has_dynamic_eval: bool,
    value_state: Vec<VisitState>,
    slot_state: Vec<VisitState>,
    slot_never_ref_cell: Vec<bool>,
    slot_saw_store: Vec<bool>,
    slot_disqualified: Vec<bool>,
    slot_value_deps: Vec<Vec<ValueId>>,
    alloc_disqualified: Vec<bool>,
    alloc_value_deps: Vec<Vec<ValueId>>,
    block_param_disqualified: Vec<bool>,
    block_param_incoming: Vec<Vec<ValueId>>,
}

/// Collects a dense boolean mask of block identifiers targeted by `Op::TryPushHandler` tokens.
///
/// Handler blocks are reached via implicit runtime `longjmp` edges from throwing
/// instructions and cannot assume `_concat_off` equals the frame base on entry.
fn collect_handler_blocks(function: &Function) -> Vec<bool> {
    let mut handlers = vec![false; function.blocks.len()];
    for inst in &function.instructions {
        if inst.op == Op::TryPushHandler {
            if let Some(Immediate::I64(token)) = inst.immediate {
                if let Ok(raw) = usize::try_from(token) {
                    if let Some(entry) = handlers.get_mut(raw) {
                        *entry = true;
                    }
                }
            }
        }
    }
    handlers
}

/// Computes a dense boolean mask of blocks reachable from `function.entry` or from any
/// installed exception handler block.
fn collect_reachable_blocks(function: &Function, handlers: &[bool]) -> Vec<bool> {
    let mut reachable = vec![false; function.blocks.len()];
    for block_id in reverse_postorder(function) {
        if let Some(entry) = reachable.get_mut(block_id.as_raw() as usize) {
            *entry = true;
        }
    }
    let mut stack: Vec<BlockId> = handlers
        .iter()
        .enumerate()
        .filter_map(|(idx, &is_handler)| is_handler.then(|| BlockId::from_raw(idx as u32)))
        .collect();
    while let Some(block_id) = stack.pop() {
        let idx = block_id.as_raw() as usize;
        let Some(entry) = reachable.get_mut(idx) else {
            continue;
        };
        if *entry {
            continue;
        }
        *entry = true;
        if let Some(block) = function.block(block_id) {
            if let Some(term) = &block.terminator {
                for succ in successors(term) {
                    if !reachable.get(succ.as_raw() as usize).copied().unwrap_or(true) {
                        stack.push(succ);
                    }
                }
            }
        }
    }
    reachable
}

/// Neutralizes `Op::ConcatReset` instructions that are reached when `_concat_off`
/// is already guaranteed to equal the current frame's `concat_base_offset`.
fn eliminate_clean_concat_resets(
    function: &mut Function,
    inert: &[bool],
    handlers: &[bool],
    reachable: &[bool],
) -> bool {
    let num_blocks = function.blocks.len();
    if num_blocks == 0 {
        return false;
    }

    let preds = predecessors(function);

    // Precompute block-level transfer summaries (`out_clean` when `in_clean` is false vs true)
    // so the fixed-point loop operates strictly over CFG edges in O(B + E) per sweep.
    let mut block_generates_clean = vec![false; num_blocks];
    let mut block_preserves_clean = vec![false; num_blocks];
    for block in &function.blocks {
        let idx = block.id.as_raw() as usize;
        if !reachable.get(idx).copied().unwrap_or(false) {
            continue;
        }
        let mut clean_from_false = false;
        let mut clean_from_true = true;
        for &inst_id in &block.instructions {
            let inst = function
                .instruction(inst_id)
                .expect("block references a valid instruction");
            let is_inert = inert.get(inst_id.as_raw() as usize).copied().unwrap_or(false);
            clean_from_false = transfer_forward_clean(inst, is_inert, clean_from_false);
            clean_from_true = transfer_forward_clean(inst, is_inert, clean_from_true);
        }
        if let Some(term) = &block.terminator {
            if !terminator_preserves_concat_state(term) {
                clean_from_false = false;
                clean_from_true = false;
            }
        }
        block_generates_clean[idx] = clean_from_false;
        block_preserves_clean[idx] = clean_from_true;
    }

    let mut in_clean = vec![true; num_blocks];
    let mut out_clean = vec![true; num_blocks];
    for idx in 0..num_blocks {
        if !reachable[idx] || handlers[idx] {
            in_clean[idx] = false;
            out_clean[idx] = false;
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for block in &function.blocks {
            let idx = block.id.as_raw() as usize;
            if !reachable[idx] {
                continue;
            }

            let new_in = if handlers[idx] {
                false
            } else {
                let mut clean = true;
                for &pred in &preds[idx] {
                    let pred_idx = pred.as_raw() as usize;
                    if reachable.get(pred_idx).copied().unwrap_or(false) && !out_clean[pred_idx] {
                        clean = false;
                        break;
                    }
                }
                clean
            };

            let clean = if new_in {
                block_preserves_clean[idx]
            } else {
                block_generates_clean[idx]
            };

            if new_in != in_clean[idx] || clean != out_clean[idx] {
                in_clean[idx] = new_in;
                out_clean[idx] = clean;
                changed = true;
            }
        }
    }

    let mut redundant: Vec<InstId> = Vec::new();
    for block in &function.blocks {
        let idx = block.id.as_raw() as usize;
        if !reachable[idx] {
            continue;
        }
        let mut clean = in_clean[idx];
        for &inst_id in &block.instructions {
            let inst = function
                .instruction(inst_id)
                .expect("block references a valid instruction");
            if inst.op == Op::ConcatReset && clean {
                redundant.push(inst_id);
            }
            let is_inert = inert.get(inst_id.as_raw() as usize).copied().unwrap_or(false);
            clean = transfer_forward_clean(inst, is_inert, clean);
        }
    }

    if redundant.is_empty() {
        return false;
    }
    for inst_id in redundant {
        if let Some(inst) = function.instruction_mut(inst_id) {
            neutralize_to_nop(inst);
        }
    }
    true
}

/// Computes the forward clean-state transfer for a single instruction.
fn transfer_forward_clean(inst: &Instruction, is_inert: bool, clean: bool) -> bool {
    if inst.op == Op::ConcatReset {
        return true;
    }
    clean && is_inert
}

/// Coalesces an earlier `Op::ConcatReset` into a later `Op::ConcatReset` when
/// every path from the earlier reset reaches a later reset before any instruction
/// can observe or modify `_concat_off`, throw to a handler, or exit the function.
fn coalesce_postdominated_concat_resets(
    function: &mut Function,
    inert: &[bool],
    reachable: &[bool],
) -> bool {
    let num_blocks = function.blocks.len();
    if num_blocks == 0 {
        return false;
    }

    // Precompute block-level backward transfer summaries (`in_will_reset` when
    // `out_will_reset` is false vs true).
    let mut block_generates_reset = vec![false; num_blocks];
    let mut block_preserves_reset = vec![false; num_blocks];
    for block in &function.blocks {
        let idx = block.id.as_raw() as usize;
        if !reachable.get(idx).copied().unwrap_or(false) {
            continue;
        }
        let mut reset_from_false = false;
        let mut reset_from_true = true;
        for &inst_id in block.instructions.iter().rev() {
            let inst = function
                .instruction(inst_id)
                .expect("block references a valid instruction");
            let is_inert = inert.get(inst_id.as_raw() as usize).copied().unwrap_or(false);
            reset_from_false = transfer_backward_will_reset(inst, is_inert, reset_from_false);
            reset_from_true = transfer_backward_will_reset(inst, is_inert, reset_from_true);
        }
        block_generates_reset[idx] = reset_from_false;
        block_preserves_reset[idx] = reset_from_true;
    }

    let mut in_will_reset = vec![false; num_blocks];
    let mut out_will_reset = vec![false; num_blocks];
    for idx in 0..num_blocks {
        if reachable[idx] {
            in_will_reset[idx] = true;
            out_will_reset[idx] = true;
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for block in function.blocks.iter().rev() {
            let idx = block.id.as_raw() as usize;
            if !reachable[idx] {
                continue;
            }

            let new_out = block_out_will_reset(block, &in_will_reset, reachable);
            let will_reset = if new_out {
                block_preserves_reset[idx]
            } else {
                block_generates_reset[idx]
            };

            if new_out != out_will_reset[idx] || will_reset != in_will_reset[idx] {
                out_will_reset[idx] = new_out;
                in_will_reset[idx] = will_reset;
                changed = true;
            }
        }
    }

    let mut redundant: Vec<InstId> = Vec::new();
    for block in &function.blocks {
        let idx = block.id.as_raw() as usize;
        if !reachable[idx] {
            continue;
        }
        let mut will_reset = out_will_reset[idx];
        for &inst_id in block.instructions.iter().rev() {
            let inst = function
                .instruction(inst_id)
                .expect("block references a valid instruction");
            if inst.op == Op::ConcatReset && will_reset {
                redundant.push(inst_id);
                // The later reset on all outgoing paths remains active, so
                // `will_reset` stays true for any even earlier reset in the block.
                continue;
            }
            let is_inert = inert.get(inst_id.as_raw() as usize).copied().unwrap_or(false);
            will_reset = transfer_backward_will_reset(inst, is_inert, will_reset);
        }
    }

    if redundant.is_empty() {
        return false;
    }
    for inst_id in redundant {
        if let Some(inst) = function.instruction_mut(inst_id) {
            neutralize_to_nop(inst);
        }
    }
    true
}

/// Computes whether all outgoing edges from `block` are guaranteed to hit a
/// `ConcatReset` before any concat-observing operation or function exit.
fn block_out_will_reset(
    block: &crate::ir::BasicBlock,
    in_will_reset: &[bool],
    reachable: &[bool],
) -> bool {
    let Some(term) = block.terminator.as_ref() else {
        return false;
    };
    if !terminator_preserves_concat_state(term) {
        return false;
    }
    let succs = successors(term);
    if succs.is_empty() {
        return false;
    }
    let mut saw_reachable_succ = false;
    for succ in succs {
        let succ_idx = succ.as_raw() as usize;
        if reachable.get(succ_idx).copied().unwrap_or(false) {
            saw_reachable_succ = true;
            if !in_will_reset[succ_idx] {
                return false;
            }
        }
    }
    saw_reachable_succ
}

/// Computes the backward `will_reset` transfer across a single instruction.
///
/// Any instruction that may throw (`Effects::MAY_THROW`) or fatal (`Effects::MAY_FATAL`)
/// is treated as a barrier even when the current function installs no `try` handler,
/// because unwinding or error reporting can exit the frame before the post-dominating
/// `ConcatReset` is reached.
fn transfer_backward_will_reset(inst: &Instruction, is_inert: bool, will_reset: bool) -> bool {
    if inst.op == Op::ConcatReset {
        return true;
    }
    if !will_reset {
        return false;
    }
    if inst
        .effects
        .intersects(Effects::MAY_THROW | Effects::MAY_FATAL)
    {
        return false;
    }
    is_inert
}

/// Returns true when a terminator cannot observe or modify `_concat_off` on its
/// outgoing control-flow edges.
fn terminator_preserves_concat_state(term: &Terminator) -> bool {
    matches!(
        term,
        Terminator::Br { .. } | Terminator::CondBr { .. } | Terminator::Switch { .. }
    )
}

/// Returns whether `inst` is unconditionally non-inert regardless of operand or slot types.
fn is_unconditional_concat_barrier(inst: &Instruction) -> bool {
    !inst
        .effects
        .intersection(Effects::ALLOC_CONCAT | Effects::OUTPUT | Effects::MAY_WARN)
        .is_empty()
        || matches!(
            inst.op,
            Op::Call
                | Op::FunctionVariantCall
                | Op::LanguageConstructCall
                | Op::EvalLiteralCall
                | Op::EvalFunctionCall
                | Op::EvalFunctionCallArray
                | Op::EvalStaticMethodCall
                | Op::RuntimeCall
                | Op::ExternCall
                | Op::MethodCall
                | Op::NullsafeMethodCall
                | Op::StaticMethodCall
                | Op::ClosureCall
                | Op::ExprCall
                | Op::CallableDescriptorInvoke
                | Op::PipeCall
                | Op::IteratorMethodCall
                | Op::SplRuntimeCall
                | Op::FiberRuntimeCall
        )
}

impl<'a> ConcatInertAnalysis<'a> {
    /// Creates a new memoized analysis context for `function`, pre-indexing local slot,
    /// array/hash allocation, and block-parameter dependencies in a single linear pass.
    fn new(function: &'a Function) -> Self {
        let num_values = function.values.len();
        let num_slots = function.locals.len();

        let mut slot_never_ref_cell = vec![true; num_slots];
        for (idx, local) in function.locals.iter().enumerate() {
            if function.params.get(idx).is_some_and(|param| param.by_ref)
                || matches!(
                    local.kind,
                    crate::ir::LocalKind::RefCell | crate::ir::LocalKind::ReturnRefCell
                )
            {
                slot_never_ref_cell[idx] = false;
            }
        }

        let mut slot_saw_store = vec![false; num_slots];
        let mut slot_disqualified = vec![false; num_slots];
        let mut slot_value_deps = vec![Vec::new(); num_slots];
        let mut alloc_disqualified = vec![false; num_values];
        let mut alloc_value_deps = vec![Vec::new(); num_values];

        for inst in &function.instructions {
            match (inst.op, inst.immediate.as_ref()) {
                (
                    Op::PromoteLocalRefCell
                    | Op::AliasLocalRefCell
                    | Op::RetainLocalRefCell
                    | Op::BindRefCellPtr
                    | Op::AdoptRefCellPtr,
                    Some(Immediate::LocalSlotPair { first, second }),
                ) => {
                    if let Some(entry) = slot_never_ref_cell.get_mut(first.as_raw() as usize) {
                        *entry = false;
                    }
                    if let Some(entry) = slot_never_ref_cell.get_mut(second.as_raw() as usize) {
                        *entry = false;
                    }
                }
                (
                    Op::BindRefCellPtr
                    | Op::AdoptRefCellPtr
                    | Op::IterCurrentValueRef
                    | Op::StoreRefCell
                    | Op::LoadRefCell
                    | Op::ReleaseLocalRefCell,
                    Some(Immediate::LocalSlot(s)),
                ) => {
                    if let Some(entry) = slot_never_ref_cell.get_mut(s.as_raw() as usize) {
                        *entry = false;
                    }
                }
                (Op::ClosureCapture, Some(Immediate::I64(1))) => {
                    if let Some(&op) = inst.operands.first() {
                        if let Some(load) = super::rewrite::defining_instruction(function, op) {
                            if matches!(load.op, Op::LoadLocal | Op::LoadRefCell) {
                                if let Some(Immediate::LocalSlot(s)) = load.immediate {
                                    if let Some(entry) =
                                        slot_never_ref_cell.get_mut(s.as_raw() as usize)
                                    {
                                        *entry = false;
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }

            if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
                let s_idx = slot.as_raw() as usize;
                if s_idx < num_slots {
                    if inst.op == Op::StoreLocal {
                        slot_saw_store[s_idx] = true;
                        if let Some(&stored_val) = inst.operands.first() {
                            slot_value_deps[s_idx].push(stored_val);
                        } else {
                            slot_disqualified[s_idx] = true;
                        }
                    } else if !matches!(
                        inst.op,
                        Op::LoadLocal
                            | Op::ReleaseLocalSlot
                            | Op::ZeroLocalSlot
                            | Op::PushCallOperandOwner
                            | Op::PopCallOperandOwner
                    ) {
                        slot_disqualified[s_idx] = true;
                    }
                }
            }

            if is_tracked_container_write_op(inst.op) {
                if let Some(&container) = inst.operands.first() {
                    record_container_write_deps(
                        function,
                        container,
                        &inst.operands[1..],
                        &mut alloc_value_deps,
                        &mut slot_value_deps,
                    );
                }
            } else if inst_may_mutate_or_alias_operand(inst) {
                for &op_val in &inst.operands {
                    disqualify_container_chain(
                        function,
                        op_val,
                        &mut alloc_disqualified,
                        &mut slot_disqualified,
                    );
                }
            }
        }

        let mut block_param_disqualified = vec![false; num_values];
        let mut block_param_incoming = vec![Vec::new(); num_values];
        let mut record_edge = |target: BlockId, args: &[ValueId]| {
            let Some(target_block) = function.block(target) else {
                return;
            };
            for (param_idx, &param_val) in target_block.params.iter().enumerate() {
                let p_idx = param_val.as_raw() as usize;
                if p_idx >= num_values {
                    continue;
                }
                if let Some(&arg_val) = args.get(param_idx) {
                    block_param_incoming[p_idx].push(arg_val);
                } else {
                    block_param_disqualified[p_idx] = true;
                }
            }
        };
        for pred_block in &function.blocks {
            let Some(term) = &pred_block.terminator else {
                continue;
            };
            match term {
                Terminator::Br { target, args } => record_edge(*target, args),
                Terminator::CondBr {
                    then_target,
                    then_args,
                    else_target,
                    else_args,
                    ..
                } => {
                    record_edge(*then_target, then_args);
                    record_edge(*else_target, else_args);
                }
                Terminator::Switch {
                    cases,
                    default,
                    default_args,
                    ..
                } => {
                    for case in cases {
                        record_edge(case.target, &case.args);
                    }
                    record_edge(*default, default_args);
                }
                _ => {}
            }
        }

        Self {
            function,
            has_dynamic_eval: super::local_scope::has_dynamic_eval(function),
            value_state: vec![VisitState::Unvisited; num_values],
            slot_state: vec![VisitState::Unvisited; num_slots],
            slot_never_ref_cell,
            slot_saw_store,
            slot_disqualified,
            slot_value_deps,
            alloc_disqualified,
            alloc_value_deps,
            block_param_disqualified,
            block_param_incoming,
        }
    }

    /// Precomputes concat-inertness for every instruction in `self.function`.
    fn compute_all(&mut self) -> Vec<bool> {
        let mut inert = Vec::with_capacity(self.function.instructions.len());
        for inst in &self.function.instructions {
            inert.push(self.instruction_is_concat_inert(inst));
        }
        inert
    }

    /// Returns true when `inst` is guaranteed not to allocate, read, modify, or
    /// observe `_concat_off`, nor invoke any callee, destructor, output routine,
    /// warning formatter, or `eval` path that can observe or reuse concat scratch.
    fn instruction_is_concat_inert(&mut self, inst: &Instruction) -> bool {
        // Handle opcodes whose default effects are conservatively broader than their
        // concrete operand/slot specialization before the generic effect filter.
        match inst.op {
            Op::ConcatReset => return true,
            Op::ReleaseLocalSlot | Op::ReleaseLocalRefCell => {
                return self.release_slot_is_concat_inert(inst);
            }
            Op::Cast => return self.cast_is_concat_inert(inst),
            Op::IterStart
            | Op::IterNext
            | Op::IterCurrentKey
            | Op::IterCurrentValue
            | Op::IterCurrentValueRef
            | Op::IterEnd => return self.iter_op_is_concat_inert(inst),
            _ => {}
        }

        if inst
            .effects
            .intersects(Effects::ALLOC_CONCAT | Effects::OUTPUT | Effects::MAY_WARN)
        {
            return false;
        }

        match inst.op {
            Op::Nop
            | Op::ConstI64
            | Op::ConstF64
            | Op::ConstStr
            | Op::ConstNull
            | Op::ConstBool
            | Op::ConstClassName
            | Op::ConstEnumCase
            | Op::LoadCalledClassId
            | Op::DataAddr
            | Op::IAdd
            | Op::ISub
            | Op::IMul
            | Op::ICheckedAdd
            | Op::ICheckedSub
            | Op::ICheckedMul
            | Op::ICheckedAddToInt
            | Op::ICheckedSubToInt
            | Op::ICheckedMulToInt
            | Op::ICheckedNumericChainToInt
            | Op::ICheckedPow
            | Op::IDiv
            | Op::ISDiv
            | Op::ISMod
            | Op::IPow
            | Op::INeg
            | Op::IBitAnd
            | Op::IBitOr
            | Op::IBitXor
            | Op::IBitNot
            | Op::IShl
            | Op::IShrA
            | Op::FAdd
            | Op::FSub
            | Op::FMul
            | Op::FDiv
            | Op::FPow
            | Op::FNeg
            | Op::ICmp
            | Op::FCmp
            | Op::IToF
            | Op::StrToI
            | Op::StrToF
            | Op::StrLen
            | Op::StrCmp
            | Op::MixedTagOf
            | Op::MixedBox
            | Op::MixedClone
            | Op::MixedUnbox
            | Op::MixedCastInt
            | Op::MixedCastFloat
            | Op::IsNull
            | Op::TypePredicate
            | Op::InstanceOf
            | Op::Move
            | Op::Borrow
            | Op::StrPersist
            | Op::StrAppend
            | Op::TryPushHandler
            | Op::TryPopHandler
            | Op::FinallyEnter
            | Op::FinallyExit
            | Op::ErrorSuppressBegin
            | Op::ErrorSuppressEnd
            | Op::PushCallOperandOwner
            | Op::PopCallOperandOwner
            | Op::PromoteLocalRefCell
            | Op::AliasLocalRefCell
            | Op::RetainLocalRefCell
            | Op::BindRefCellPtr
            | Op::AdoptRefCellPtr
            | Op::UnsetLocal
            | Op::ZeroLocalSlot
            | Op::StaticLocalUninitialized
            | Op::StaticPropInitialized
            | Op::ReflectionStaticPropertyInitialized
            | Op::PropInitialized
            | Op::PtrCast
            | Op::PtrOffset
            | Op::PtrCheckNonnull
            | Op::PtrRead
            | Op::PtrWrite
            | Op::BufferNew
            | Op::BufferLen
            | Op::BufferGet
            | Op::BufferSet
            | Op::BufferFree
            | Op::ArrayNew
            | Op::HashNew
            | Op::ArrayLen
            | Op::HashLen
            | Op::ArrayToMixed
            | Op::HashToMixed
            | Op::ArrayCloneShallow
            | Op::HashCloneShallow
            | Op::ArrayEnsureUnique
            | Op::HashEnsureUnique
            | Op::ArrayToHash => true,

            // `Op::FToI` with `Immediate::FloatKeyDiagnostic` or `Immediate::Bool(true)`
            // formats a warning into `_concat_buf` (`__rt_float_key_to_int`) or invokes
            // `__rt_diag_warning`. Only plain truncation (`immediate.is_none()`) is inert.
            Op::FToI => inst.immediate.is_none(),

            Op::MixedNumericBinop => {
                !inst.operands.is_empty()
                    && inst
                        .operands
                        .iter()
                        .all(|&op| self.value_is_statically_destructor_free(op))
            }

            Op::ArrayIsset
            | Op::HashIsset
            | Op::ArrayGetSilent
            | Op::HashGetSilent => inst
                .operands
                .get(1)
                .is_some_and(|&key| key_operand_is_concat_inert(self.function, key)),

            Op::ArrayKeyExists | Op::DescriptorArgKeyExists => inst
                .operands
                .first()
                .is_some_and(|&key| key_operand_is_concat_inert(self.function, key)),

            Op::LoadLocal | Op::LoadRefCell => load_local_is_concat_inert(self.function, inst),
            Op::StoreLocal => self.store_local_is_concat_inert(inst),
            Op::Acquire => true,
            Op::Release | Op::ReleaseUnlessAliases => self.release_is_concat_inert(inst),
            Op::IsTruthy => is_truthy_is_concat_inert(self.function, inst),
            Op::StrictEq | Op::StrictNotEq => self.strict_eq_is_concat_inert(inst),
            Op::ArraySet
            | Op::ArrayPush
            | Op::MixedArrayAppend
            | Op::HashSet
            | Op::DescriptorArgSet
            | Op::HashAppend
            | Op::SlotDetach => self.container_write_is_concat_inert(inst),

            _ => false,
        }
    }

    /// Returns true when `Op::StoreLocal` cannot retire a previous ref-cell occupant or
    /// invoke `reset_concat_to_frame_base` internally.
    ///
    /// Codegen's `lower_store_local`:
    /// 1. Calls `reset_concat_to_frame_base` itself when `inst.span` has `line > 0` and the
    ///    stored value is `Acquire(StrConcat(...))`. Treating such a store as dirty ensures
    ///    Phase 2 never coalesces a preceding `ConcatReset` across the `StrConcat` that feeds it.
    /// 2. Retires the previous payload when the target slot has `RefCell` or `Dynamic`
    ///    representation (`store_value_to_ref_cell_local`), which can invoke a destructor if
    ///    the slot is not statically destructor-free.
    fn store_local_is_concat_inert(&mut self, inst: &Instruction) -> bool {
        let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
            return false;
        };
        let Some(&value) = inst.operands.first() else {
            return false;
        };
        if value_is_acquire_of_str_concat(self.function, value) {
            return false;
        }
        if self.slot_is_never_ref_cell(slot) {
            return true;
        }
        self.slot_is_statically_destructor_free(slot)
    }

    /// Returns true when `slot` is never a by-reference parameter and never promoted or
    /// bound to a reference cell anywhere in `self.function`.
    fn slot_is_never_ref_cell(&self, slot: LocalSlotId) -> bool {
        self.slot_never_ref_cell
            .get(slot.as_raw() as usize)
            .copied()
            .unwrap_or(false)
    }

    /// Returns true when `Op::Release` or `Op::ReleaseUnlessAliases` cannot invoke a user destructor.
    fn release_is_concat_inert(&mut self, inst: &Instruction) -> bool {
        let Some(&value) = inst.operands.first() else {
            return false;
        };
        self.value_is_statically_destructor_free(value)
    }

    /// Returns true when `Op::ReleaseLocalSlot` or `Op::ReleaseLocalRefCell` cannot invoke a user destructor.
    fn release_slot_is_concat_inert(&mut self, inst: &Instruction) -> bool {
        let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
            return false;
        };
        self.slot_is_statically_destructor_free(slot)
    }

    /// Returns true when a local slot's stored values are all statically destructor-free.
    ///
    /// Cycles through `slot_state` fail closed (`false`) so self-referential local
    /// copies (`$x = $x`) without an external non-cyclic base case cannot be proven
    /// destructor-free vacuously.
    fn slot_is_statically_destructor_free(&mut self, slot: LocalSlotId) -> bool {
        if self.has_dynamic_eval {
            return false;
        }
        let idx = slot.as_raw() as usize;
        let Some(local) = self.function.locals.get(idx) else {
            return false;
        };
        if php_type_is_scalar_destructor_free(&local.php_type) {
            return true;
        }
        let statically_typed_destructor_free = php_type_is_destructor_free(&local.php_type);
        if !statically_typed_destructor_free {
            // Parameter slots have an initial incoming value from the caller that is not
            // represented by an in-body `StoreLocal`.
            if idx < self.function.params.len() {
                return false;
            }
            if !matches!(
                local.php_type.codegen_repr(),
                PhpType::Mixed | PhpType::Union(_) | PhpType::Array(_) | PhpType::AssocArray { .. }
            ) {
                return false;
            }
        }
        match self.slot_state.get(idx).copied() {
            Some(VisitState::Done(ans)) => return ans,
            Some(VisitState::Visiting) | None => return false,
            Some(VisitState::Unvisited) => {
                self.slot_state[idx] = VisitState::Visiting;
            }
        }

        let mut all_ok = !self.slot_disqualified[idx]
            && (statically_typed_destructor_free || self.slot_saw_store[idx]);
        if all_ok {
            let dep_len = self.slot_value_deps[idx].len();
            for dep_idx in 0..dep_len {
                let dep_val = self.slot_value_deps[idx][dep_idx];
                if !self.value_is_statically_destructor_free(dep_val) {
                    all_ok = false;
                    break;
                }
            }
        }
        self.slot_state[idx] = VisitState::Done(all_ok);
        all_ok
    }

    /// Returns true when an SSA value is statically guaranteed never to hold an object with a destructor.
    ///
    /// Any cycle in `value_state` (such as a loop block-parameter phi cycle) fails closed
    /// (`false`) rather than assuming induction holds without a base case.
    fn value_is_statically_destructor_free(&mut self, value: ValueId) -> bool {
        let idx = value.as_raw() as usize;
        let Some(meta) = self.function.value(value) else {
            return false;
        };
        if php_type_is_scalar_destructor_free(&meta.php_type) {
            return true;
        }
        match self.value_state.get(idx).copied() {
            Some(VisitState::Done(ans)) => return ans,
            Some(VisitState::Visiting) | None => return false,
            Some(VisitState::Unvisited) => {
                self.value_state[idx] = VisitState::Visiting;
            }
        }

        if self.alloc_disqualified.get(idx).copied().unwrap_or(true)
            || !self.array_allocation_is_destructor_free(value)
        {
            self.value_state[idx] = VisitState::Done(false);
            return false;
        }

        if php_type_is_destructor_free(&meta.php_type) {
            self.value_state[idx] = VisitState::Done(true);
            return true;
        }
        if !matches!(
            meta.php_type.codegen_repr(),
            PhpType::Mixed | PhpType::Union(_) | PhpType::Array(_) | PhpType::AssocArray { .. }
        ) {
            self.value_state[idx] = VisitState::Done(false);
            return false;
        }

        let result = match meta.def {
            crate::ir::ValueDef::Instruction { inst: inst_id, .. } => {
                if let Some(inst) = self.function.instruction(inst_id) {
                    match inst.op {
                        Op::ICheckedAdd
                        | Op::ICheckedSub
                        | Op::ICheckedMul
                        | Op::ICheckedPow
                        | Op::MixedNumericBinop
                        | Op::IterCurrentKey
                        | Op::ConstI64
                        | Op::ConstF64
                        | Op::ConstStr
                        | Op::ConstBool
                        | Op::ConstNull
                        | Op::ArrayNew
                        | Op::HashNew => true,
                        Op::IterCurrentValue => self.iter_op_is_concat_inert(inst),
                        Op::Acquire
                        | Op::Borrow
                        | Op::Move
                        | Op::EnsureOwned
                        | Op::Cast
                        | Op::MixedBox
                        | Op::MixedClone
                        | Op::MixedUnbox
                        | Op::ArrayCloneShallow
                        | Op::HashCloneShallow
                        | Op::ArrayEnsureUnique
                        | Op::HashEnsureUnique
                        | Op::ArrayToMixed
                        | Op::HashToMixed
                        | Op::ArrayToHash => inst
                            .operands
                            .first()
                            .is_some_and(|&src| self.value_is_statically_destructor_free(src)),
                        Op::ArrayUnion
                        | Op::HashUnion
                        | Op::ArrayHashUnion
                        | Op::HashArrayUnion => {
                            !inst.operands.is_empty()
                                && inst
                                    .operands
                                    .iter()
                                    .all(|&op| self.value_is_statically_destructor_free(op))
                        }
                        Op::LoadLocal => {
                            if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
                                self.slot_is_statically_destructor_free(slot)
                            } else {
                                false
                            }
                        }
                        _ => false,
                    }
                } else {
                    false
                }
            }
            crate::ir::ValueDef::BlockParam { block, .. } => {
                // Entry block parameters are function parameters; if their type is Mixed/Union,
                // the caller can pass an arbitrary object.
                if block == self.function.entry
                    || self.block_param_disqualified[idx]
                    || self.block_param_incoming[idx].is_empty()
                {
                    false
                } else {
                    let incoming_len = self.block_param_incoming[idx].len();
                    let mut all_ok = true;
                    for in_idx in 0..incoming_len {
                        let arg_val = self.block_param_incoming[idx][in_idx];
                        if !self.value_is_statically_destructor_free(arg_val) {
                            all_ok = false;
                            break;
                        }
                    }
                    all_ok
                }
            }
        };
        self.value_state[idx] = VisitState::Done(result);
        result
    }

    /// Returns true when every in-place write to a newly allocated `ArrayNew` or `HashNew`
    /// container `alloc_val` stores a destructor-free value.
    fn array_allocation_is_destructor_free(&mut self, alloc_val: ValueId) -> bool {
        let idx = alloc_val.as_raw() as usize;
        if self.alloc_disqualified.get(idx).copied().unwrap_or(true) {
            return false;
        }
        let Some(deps_len) = self.alloc_value_deps.get(idx).map(Vec::len) else {
            return true;
        };
        for dep_idx in 0..deps_len {
            let op = self.alloc_value_deps[idx][dep_idx];
            if !self.value_is_statically_destructor_free(op) {
                return false;
            }
        }
        true
    }

    /// Returns true when `Op::Cast` is an inline scalar cast that never touches `_concat_off`.
    fn cast_is_concat_inert(&self, inst: &Instruction) -> bool {
        if matches!(inst.immediate, Some(Immediate::StringOffsetCast)) {
            return false;
        }
        let Some(Immediate::CastTarget(target)) = inst.immediate else {
            return false;
        };
        if !matches!(target, IrType::I64 | IrType::F64) {
            return false;
        }
        if inst.result_php_type.codegen_repr() == PhpType::Bool {
            return is_truthy_is_concat_inert(self.function, inst);
        }
        let Some(&value) = inst.operands.first() else {
            return false;
        };
        let Some(raw_ty) = self.function.value(value).map(|v| &v.php_type) else {
            return false;
        };
        if matches!(raw_ty, PhpType::Resource(_)) {
            return false;
        }
        // Float-to-int cast (`emit_explicit_float_result_to_int`) can emit the PHP 8.5
        // non-representable float deprecation warning, which formats into `_concat_buf`.
        if target == IrType::I64 && matches!(raw_ty.codegen_repr(), PhpType::Float) {
            return false;
        }
        matches!(
            raw_ty.codegen_repr(),
            PhpType::Int
                | PhpType::Float
                | PhpType::Bool
                | PhpType::False
                | PhpType::Void
                | PhpType::Never
                | PhpType::TaggedScalar
                | PhpType::Str
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
        )
    }

    /// Returns true when `Op::StrictEq` / `Op::StrictNotEq` compares scalar, string, object,
    /// pointer, or destructor-free `Mixed` operands without invoking a destructor.
    ///
    /// `__rt_mixed_strict_eq` and `__rt_array_strict_eq` are read-only and never touch
    /// `_concat_off`. When one operand is `Mixed` and the other is concrete, codegen boxes
    /// the concrete operand into a temporary `Mixed` cell and releases that temporary box
    /// after the comparison (`decref_mixed_temp_at`), so both operands must be statically
    /// destructor-free to guarantee the post-comparison decref cannot run `__destruct`.
    fn strict_eq_is_concat_inert(&mut self, inst: &Instruction) -> bool {
        if inst.operands.len() != 2 {
            return false;
        }
        let lhs_val = inst.operands[0];
        let rhs_val = inst.operands[1];
        let (Some(lhs), Some(rhs)) = (
            value_php_type(self.function, lhs_val),
            value_php_type(self.function, rhs_val),
        ) else {
            return false;
        };
        let is_simple = |ty: &PhpType| {
            matches!(
                ty,
                PhpType::Int
                    | PhpType::Float
                    | PhpType::Bool
                    | PhpType::False
                    | PhpType::Void
                    | PhpType::Never
                    | PhpType::Str
                    | PhpType::Object(_)
                    | PhpType::Pointer(_)
            )
        };
        if is_simple(&lhs) && is_simple(&rhs) {
            return true;
        }
        self.value_is_statically_destructor_free(lhs_val)
            && self.value_is_statically_destructor_free(rhs_val)
    }

    /// Returns true when an array or hash mutation cannot overwrite an entry holding a
    /// destructor-bearing object, emit a float-key warning, or coerce a `Mixed` value to `Str`.
    fn container_write_is_concat_inert(&mut self, inst: &Instruction) -> bool {
        let Some(&container) = inst.operands.first() else {
            return false;
        };
        let Some(container_meta) = self.function.value(container) else {
            return false;
        };
        if matches!(inst.op, Op::ArraySet | Op::HashSet | Op::DescriptorArgSet) {
            let Some(&key) = inst.operands.get(1) else {
                return false;
            };
            if !key_operand_is_concat_inert(self.function, key) {
                return false;
            }
        }
        if matches!(inst.op, Op::ArrayPush | Op::MixedArrayAppend | Op::HashAppend) {
            if !matches!(
                &container_meta.php_type,
                PhpType::Array(_) | PhpType::AssocArray { .. }
            ) {
                return false;
            }
        } else if !self.value_is_statically_destructor_free(container) {
            return false;
        }
        // Check that no value operand is a `Mixed` being coerced into a `Str` container element.
        let container_holds_raw_str = matches!(
            &container_meta.php_type,
            PhpType::Array(elem) if **elem == PhpType::Str
        ) || matches!(
            &container_meta.php_type,
            PhpType::AssocArray { value, .. } if **value == PhpType::Str
        );
        for &operand in inst.operands.iter().skip(1) {
            let Some(op_ty) = value_php_type(self.function, operand) else {
                return false;
            };
            if container_holds_raw_str && matches!(op_ty, PhpType::Mixed | PhpType::Union(_)) {
                return false;
            }
            if !self.value_is_statically_destructor_free(operand) {
                return false;
            }
        }
        true
    }

    /// Returns true when an iterator operation iterates by value over a concrete indexed or
    /// associative array with destructor-free elements rather than a user `Iterator` /
    /// `IteratorAggregate` object, a dynamic `Mixed` / `Iterable` source, or a by-reference
    /// `foreach` that can replace an existing container owner on `IterStart`.
    fn iter_op_is_concat_inert(&mut self, inst: &Instruction) -> bool {
        if inst.op == Op::IterEnd {
            return true;
        }
        let Some(&first_op) = inst.operands.first() else {
            return false;
        };
        let iter_start_inst = if inst.op == Op::IterStart {
            inst
        } else {
            let Some(def_inst) = super::rewrite::defining_instruction(self.function, first_op)
            else {
                return false;
            };
            if def_inst.op != Op::IterStart {
                return false;
            }
            def_inst
        };
        let Some(Immediate::IterStart(metadata)) = iter_start_inst.immediate.as_ref() else {
            return false;
        };
        if metadata.is_by_ref() || metadata.owner().is_some() || metadata.origin().is_some() {
            return false;
        }
        let Some(&source_val) = iter_start_inst.operands.first() else {
            return false;
        };
        let Some(source_meta) = self.function.value(source_val) else {
            return false;
        };
        if !matches!(
            &source_meta.php_type,
            PhpType::Array(_) | PhpType::AssocArray { .. }
        ) {
            return false;
        }
        self.value_is_statically_destructor_free(source_val)
    }
}

/// Returns the PHP type of `value` in `function`, if present.
fn value_php_type(function: &Function, value: ValueId) -> Option<PhpType> {
    function.value(value).map(|v| v.php_type.codegen_repr())
}

/// Returns the PHP type of `slot` in `function`, if present.
fn slot_php_type(function: &Function, slot: LocalSlotId) -> Option<PhpType> {
    function
        .locals
        .get(slot.as_raw() as usize)
        .map(|local| local.php_type.codegen_repr())
}

/// Returns true when an array or hash key operand cannot trigger `__rt_float_key_to_int`
/// or an illegal-offset warning during key materialization.
fn key_operand_is_concat_inert(function: &Function, key: ValueId) -> bool {
    let Some(ty) = value_php_type(function, key) else {
        return false;
    };
    matches!(
        ty,
        PhpType::Int
            | PhpType::Bool
            | PhpType::False
            | PhpType::Str
            | PhpType::TaggedScalar
            | PhpType::Void
            | PhpType::Never
    )
}

/// Returns true when a `LoadLocal` or `LoadRefCell` cannot invoke `__rt_mixed_cast_string`.
///
/// Codegen calls `__rt_mixed_cast_string` only when coercing a `Mixed` local slot into a
/// `PhpType::Str` SSA result.
fn load_local_is_concat_inert(function: &Function, inst: &Instruction) -> bool {
    if inst.result_php_type.codegen_repr() != PhpType::Str {
        return true;
    }
    let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
        return false;
    };
    let Some(source_ty) = slot_php_type(function, slot) else {
        return false;
    };
    crate::ir::local_load_types_share_storage(&source_ty, &PhpType::Str)
}

/// Returns true when `value` is `Acquire(StrConcat(...))`.
fn value_is_acquire_of_str_concat(function: &Function, value: ValueId) -> bool {
    let Some(acquire_inst) = super::rewrite::defining_instruction(function, value) else {
        return false;
    };
    if acquire_inst.op != Op::Acquire {
        return false;
    }
    let Some(&source) = acquire_inst.operands.first() else {
        return false;
    };
    super::rewrite::defining_instruction(function, source)
        .is_some_and(|src_inst| src_inst.op == Op::StrConcat)
}

/// Returns true when `op` is a tracked in-place container write whose target container is
/// `operands[0]` and whose written key/value/spread operands are `operands[1..]`.
fn is_tracked_container_write_op(op: Op) -> bool {
    matches!(
        op,
        Op::ArrayPush
            | Op::MixedArrayAppend
            | Op::HashAppend
            | Op::ArraySet
            | Op::ArraySetMixedKey
            | Op::HashSet
            | Op::DescriptorArgSet
            | Op::HashSpread
    )
}

/// Returns the underlying source `ValueId` when `inst` forwards, clones, boxes, or converts
/// a single container/value operand without introducing a new independent container root.
fn container_forwarding_source(inst: &Instruction) -> Option<ValueId> {
    if matches!(
        inst.op,
        Op::Acquire
            | Op::Borrow
            | Op::Move
            | Op::EnsureOwned
            | Op::Cast
            | Op::MixedBox
            | Op::MixedClone
            | Op::MixedUnbox
            | Op::ArrayCloneShallow
            | Op::HashCloneShallow
            | Op::ArrayEnsureUnique
            | Op::HashEnsureUnique
            | Op::ArrayToMixed
            | Op::HashToMixed
            | Op::ArrayToHash
    ) {
        inst.operands.first().copied()
    } else {
        None
    }
}

/// Records `written_values` as dependencies of `container` and every value or local slot
/// along its forwarding/conversion chain.
fn record_container_write_deps(
    function: &Function,
    container: ValueId,
    written_values: &[ValueId],
    alloc_value_deps: &mut [Vec<ValueId>],
    slot_value_deps: &mut [Vec<ValueId>],
) {
    let mut cur = container;
    for _ in 0..=function.values.len() {
        let c_idx = cur.as_raw() as usize;
        if let Some(deps) = alloc_value_deps.get_mut(c_idx) {
            deps.extend_from_slice(written_values);
        }
        let Some(def) = super::rewrite::defining_instruction(function, cur) else {
            break;
        };
        if matches!(def.op, Op::LoadLocal | Op::LoadRefCell) {
            if let Some(Immediate::LocalSlot(slot)) = def.immediate {
                if let Some(slot_deps) = slot_value_deps.get_mut(slot.as_raw() as usize) {
                    slot_deps.extend_from_slice(written_values);
                }
            }
            break;
        }
        if let Some(prev) = container_forwarding_source(def) {
            cur = prev;
        } else {
            break;
        }
    }
}

/// Marks `value` and every container value or local slot along its forwarding chain as
/// disqualified when `value` escapes into an instruction that may mutate it or take its address.
fn disqualify_container_chain(
    function: &Function,
    value: ValueId,
    alloc_disqualified: &mut [bool],
    slot_disqualified: &mut [bool],
) {
    let mut cur = value;
    for _ in 0..=function.values.len() {
        let c_idx = cur.as_raw() as usize;
        if let Some(flag) = alloc_disqualified.get_mut(c_idx) {
            *flag = true;
        }
        let Some(def) = super::rewrite::defining_instruction(function, cur) else {
            break;
        };
        if matches!(def.op, Op::LoadLocal | Op::LoadRefCell) {
            if let Some(Immediate::LocalSlot(slot)) = def.immediate {
                if let Some(flag) = slot_disqualified.get_mut(slot.as_raw() as usize) {
                    *flag = true;
                }
            }
            break;
        }
        if let Some(prev) = container_forwarding_source(def) {
            cur = prev;
        } else {
            break;
        }
    }
}

/// Returns true when `inst` may mutate a container operand in place outside
/// `is_tracked_container_write_op` or bind a `LoadLocal` operand by reference.
fn inst_may_mutate_or_alias_operand(inst: &Instruction) -> bool {
    matches!(
        inst.op,
        Op::Call
            | Op::FunctionVariantCall
            | Op::LanguageConstructCall
            | Op::EvalLiteralCall
            | Op::EvalFunctionCall
            | Op::EvalFunctionCallArray
            | Op::EvalStaticMethodCall
            | Op::RuntimeCall
            | Op::ExternCall
            | Op::MethodCall
            | Op::NullsafeMethodCall
            | Op::StaticMethodCall
            | Op::ClosureCall
            | Op::ExprCall
            | Op::CallableDescriptorInvoke
            | Op::PipeCall
            | Op::IteratorMethodCall
            | Op::SplRuntimeCall
            | Op::FiberRuntimeCall
            | Op::CoreBuiltin
            | Op::ObjectNew
            | Op::EvalObjectNew
            | Op::DynamicObjectNew
            | Op::DynamicObjectNewMixed
            | Op::DynamicObjectNewWithoutConstructorMixed
            | Op::DynamicPdoStatementConstructorCall
            | Op::DynamicPdoStatementInitialize
            | Op::InvokerRefArg
            | Op::AcquireRefCell
            | Op::LoadArrayElemRefCell
            | Op::LoadArrayElemRefCellExisting
            | Op::ArrayElemAddr
            | Op::ArrayGetForWrite
            | Op::HashGetForWrite
            | Op::MixedArrayGetForWrite
            | Op::IterCurrentValueRef
    ) || (inst.op == Op::ClosureCapture && matches!(inst.immediate, Some(Immediate::I64(1))))
        || (inst.op == Op::IterStart
            && (matches!(inst.immediate, Some(Immediate::IterStart(ref meta)) if meta.is_by_ref())
                || matches!(inst.immediate, Some(Immediate::I64(1)))))
}

/// Returns true when `ty` is a non-container scalar, pointer, buffer, or packed value
/// that cannot hold elements or objects with destructors.
fn php_type_is_scalar_destructor_free(ty: &PhpType) -> bool {
    matches!(
        ty,
        PhpType::Int
            | PhpType::Float
            | PhpType::Str
            | PhpType::Bool
            | PhpType::False
            | PhpType::Void
            | PhpType::Never
            | PhpType::TaggedScalar
            | PhpType::Pointer(_)
            | PhpType::Buffer(_)
            | PhpType::Packed(_)
    )
}

/// Returns true when `PhpType` is guaranteed never to hold an object with a user `__destruct`
/// or a closure/callable descriptor whose release can drop captured objects.
///
/// `PhpType::Callable` and `PhpType::Resource` are intentionally excluded: releasing a
/// callable descriptor (`__rt_callable_descriptor_release`) decrefs captured payloads or
/// bound `$this` objects (which can run `__destruct`), and `PhpType::Resource` in PHP 8
/// can represent object-backed handles or custom stream wrappers. Empty-array placeholder
/// element types (`Void`, `Never`) are excluded on containers so `ArrayNew` / `HashNew`
/// allocations are always checked against their recorded in-place writes.
fn php_type_is_destructor_free(ty: &PhpType) -> bool {
    if php_type_is_scalar_destructor_free(ty) {
        return true;
    }
    match ty {
        PhpType::Array(elem) => {
            !matches!(elem.as_ref(), PhpType::Void | PhpType::Never)
                && php_type_is_destructor_free(elem)
        }
        PhpType::AssocArray { key, value } => {
            !matches!(key.as_ref(), PhpType::Void | PhpType::Never)
                && !matches!(value.as_ref(), PhpType::Void | PhpType::Never)
                && php_type_is_destructor_free(key)
                && php_type_is_destructor_free(value)
        }
        PhpType::Union(members) => {
            !members.is_empty()
                && members.iter().all(|member| {
                    !matches!(member, PhpType::Void | PhpType::Never)
                        && php_type_is_destructor_free(member)
                })
        }
        _ => false,
    }
}

/// Returns true when `Op::IsTruthy` cannot emit a PHP 8.5 `NAN` coercion warning that formats into `_concat_buf`.
fn is_truthy_is_concat_inert(function: &Function, inst: &Instruction) -> bool {
    let Some(&value) = inst.operands.first() else {
        return false;
    };
    let Some(ty) = value_php_type(function, value) else {
        return false;
    };
    !matches!(ty, PhpType::Float | PhpType::Mixed | PhpType::Union(_))
}
