//! Purpose:
//! Hoists generation-safe `buffer<T>` handle resolution out of loops that cannot invalidate
//! a buffer, and shares one bounds-checked record address between the repeated `$b[$i]`
//! packed accesses of an iteration.
//!
//! Called from:
//! - `crate::ir_passes::driver::optimize_module`, once after the ordinary fixed point, so
//!   the follow-up function passes clean up the loads the rewrite leaves unused.
//!
//! Key details:
//! - Every `buffer_get`/`buffer_set` resolves its handle through `__rt_buffer_resolve`
//!   (registry lookup, active marker, generation) before its bounds check. Inside a loop that
//!   runs no instruction able to free, allocate or rebind a buffer, that answer cannot change
//!   between iterations, so one `buffer_resolve` in the preheader serves every access, which
//!   becomes `buffer_get_resolved`/`buffer_set_resolved`.
//! - The hoisted resolution is NON-FAULTING: it answers 0 for a stale handle and the access
//!   tests the descriptor, so the use-after-free fatal still fires at the access itself and a
//!   loop that never runs its body never reports anything, exactly as before.
//! - A loop qualifies only when every instruction in its natural body is on a default-deny
//!   allowlist: buffer element and packed field accesses, local loads, stores of scalar
//!   locals, concat resets, and instructions whose effects only read state or fatal. That
//!   includes a user function or method call whose refined effect summary is read-only. Any
//!   other call, and releases, allocations, warnings (user error handlers), output
//!   (output-buffer callbacks) and throws disqualify it, because each can run user code that
//!   frees a buffer through an alias. Blocks that leave the loop for good are outside the natural body, and coming
//!   back always passes the preheader, which resolves again.
//! - The buffer operand must be loop-invariant: an SSA value defined outside the loop, or a
//!   `load_local` of a slot nothing in the loop writes and no by-reference alias can reach.
//! - The resolver (and the slot load it reads) goes at the EARLIEST preheader position with the
//!   answer the preheader's end would give: after the handle's definition and after the last
//!   preheader instruction that could free, allocate or rebind a buffer or write the handle slot.
//!   The pass runs after LICM has filled the preheader with loop-invariant loads, so resolving at
//!   the end would keep a hoisted float such as a `float $dt` parameter live across the resolver
//!   call, and SysV x86_64 (no callee-saved XMM register) would reload it on every iteration.
//! - Inside a rewritten loop, packed `buffer_get_resolved` instructions with the same
//!   descriptor and index compute the same record address, so a dominated one is replaced by
//!   its dominator. Scalar element reads are left alone: a `buffer_set_resolved` between two of
//!   them can change the loaded value.
//! - A zero-based unit-step induction guarded by strict `< buffer_len()` of the same handle marks
//!   its exact dominated accesses with a true immediate, allowing codegen to omit their bounds
//!   comparison without removing the zero-descriptor fatal check.
//! - Skipped entirely for generators (a `yield` lets the caller run), functions with exception
//!   handlers or dynamic `eval`, and modules that enable `pcntl_async_signals()`, whose backend
//!   dispatches signal handlers after every instruction.

use std::collections::{HashMap, HashSet};

use crate::ir::{
    BlockId, CmpPredicate, Effects, Function, Immediate, InstId, Instruction, IrType, LocalSlotId,
    Module, Op, Ownership, PassOrigin, RuntimeCallTarget, RuntimeFnId, Terminator, Value, ValueDef,
    ValueId,
};
use crate::types::PhpType;

use super::cfg::has_exception_handlers;
use super::dominance::{compute_dominance, DominanceInfo};
use super::loops::{compute_loops, NaturalLoop};
use super::rewrite::{neutralize_to_nop, replace_all_uses, resolve_chains};

/// Rewrites buffer accesses in every function-like body and reports whether anything changed.
pub(super) fn hoist_buffer_resolution(module: &mut Module) -> bool {
    if module.uses_pcntl_runtime(crate::ir::PcntlRuntime::AsyncSignals) {
        return false;
    }
    let Module {
        functions,
        class_methods,
        closures,
        fiber_wrappers,
        callback_wrappers,
        extern_callback_trampolines,
        runtime_callable_invokers,
        ..
    } = module;
    let mut changed = false;
    for function in functions
        .iter_mut()
        .chain(class_methods.iter_mut())
        .chain(closures.iter_mut())
        .chain(fiber_wrappers.iter_mut())
        .chain(callback_wrappers.iter_mut())
        .chain(extern_callback_trampolines.iter_mut())
        .chain(runtime_callable_invokers.iter_mut())
    {
        changed |= optimize_function(function);
    }
    changed
}

/// Hoists buffer resolution out of every qualifying loop of one function, outermost first.
fn optimize_function(function: &mut Function) -> bool {
    if !function
        .instructions
        .iter()
        .any(|inst| matches!(inst.op, Op::BufferGet | Op::BufferSet))
        || function.flags.is_generator
        || has_exception_handlers(function)
        || super::local_scope::has_dynamic_eval(function)
        || function.instructions.iter().any(|inst| inst.op.name().starts_with("eval_"))
    {
        return false;
    }
    let dominance = compute_dominance(function);
    let loops = compute_loops(function, &dominance);
    if loops.is_empty() {
        return false;
    }
    let escaping = super::by_ref_alias::address_escaping_slots(function);
    let written_anywhere = slots_named_in_function(function);
    let mut order: Vec<usize> = (0..loops.loops().len()).collect();
    order.sort_by_key(|&index| loops.loops()[index].depth);
    let mut changed = false;
    for index in order {
        let loop_ref = &loops.loops()[index];
        let Some(preheader) = loop_ref.preheader else {
            continue;
        };
        if !loop_preserves_buffers(function, loop_ref) {
            continue;
        }
        changed |= hoist_loop(
            function,
            loop_ref,
            preheader,
            &dominance,
            &escaping,
            &written_anywhere,
        );
    }
    changed
}

/// Where a loop-invariant buffer handle comes from.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum HandleSource {
    /// An SSA value defined outside the loop.
    Value(ValueId),
    /// A local slot that nothing in the loop writes.
    Slot(LocalSlotId),
}

/// A loop index proven nonnegative and strictly below one stable buffer's logical length.
#[derive(Clone, Copy)]
struct InBoundsProof {
    /// Buffer handle whose immutable logical length supplied the loop bound.
    source: HandleSource,
    /// In-loop successor of the comparison, which must dominate each marked access.
    guarded_entry: BlockId,
}

/// Rewrites one loop's invariant buffer accesses through preheader descriptors.
fn hoist_loop(
    function: &mut Function,
    loop_ref: &NaturalLoop,
    preheader: BlockId,
    dominance: &DominanceInfo,
    escaping: &HashSet<LocalSlotId>,
    written_anywhere: &HashSet<LocalSlotId>,
) -> bool {
    let written = slots_named_in_loop(function, loop_ref);
    let in_bounds = proven_in_bounds_indices(function, loop_ref, &written, escaping);
    let incoming = incoming_arguments(function);
    let mut groups: Vec<(HandleSource, Vec<InstId>)> = Vec::new();
    let mut replaced_loads: Vec<ValueId> = Vec::new();
    for &block in &loop_ref.blocks {
        for &inst_id in &function.blocks[block.as_raw() as usize].instructions {
            let inst = &function.instructions[inst_id.as_raw() as usize];
            if !matches!(inst.op, Op::BufferGet | Op::BufferSet) {
                continue;
            }
            let Some(&handle) = inst.operands.first() else {
                continue;
            };
            let Some(source) = handle_source(function, loop_ref, handle, &written, escaping) else {
                continue;
            };
            if matches!(source, HandleSource::Slot(_)) {
                replaced_loads.push(handle);
            }
            match groups.iter_mut().find(|(existing, _)| *existing == source) {
                Some((_, accesses)) => accesses.push(inst_id),
                None => groups.push((source, vec![inst_id])),
            }
        }
    }
    if groups.is_empty() {
        return false;
    }

    let mut rewritten = Vec::new();
    // Each later group resolves after the previous one, keeping the descriptors in group order.
    let mut floor = 0;
    for (source, accesses) in groups {
        let mut position = resolver_position(function, preheader, source, floor);
        let handle = match source {
            HandleSource::Value(value) => value,
            HandleSource::Slot(slot) => {
                let template = function.instructions[accesses[0].as_raw() as usize].operands[0];
                let load = insert_slot_load(function, preheader, position, slot, template);
                position += 1;
                load
            }
        };
        let descriptor = insert_instruction(
            function,
            preheader,
            position,
            Op::BufferResolve,
            vec![handle],
            IrType::I64,
            PhpType::Int,
        );
        floor = position + 1;
        for inst_id in accesses {
            let access_block = function
                .blocks
                .iter()
                .find(|block| block.instructions.contains(&inst_id))
                .map(|block| block.id)
                .expect("buffer access belongs to a block");
            let index = function.instructions[inst_id.as_raw() as usize].operands[1];
            let canonical_index = canonical_value(function, &incoming, index);
            let proven_in_bounds = in_bounds.get(&canonical_index).is_some_and(|proof| {
                sources_name_same_buffer(
                    function,
                    proof.source,
                    source,
                    escaping,
                    written_anywhere,
                ) && access_block != loop_ref.header
                    && dominance.dominates(proof.guarded_entry, access_block)
            });
            let inst = &mut function.instructions[inst_id.as_raw() as usize];
            inst.op = if inst.op == Op::BufferGet {
                Op::BufferGetResolved
            } else {
                Op::BufferSetResolved
            };
            inst.operands[0] = descriptor;
            inst.immediate = proven_in_bounds.then_some(Immediate::Bool(true));
            inst.effects = inst.op.default_effects();
            inst.origin = Some(PassOrigin::BufferAccess);
            rewritten.push(inst_id);
        }
    }
    neutralize_unused_loads(function, &replaced_loads);
    share_record_addresses(function, &rewritten, dominance);
    true
}

/// Returns whether a length-bound handle and an access handle identify the same stable buffer.
///
/// Exact SSA or slot sources match directly. A cached length may instead read a local slot
/// before the loop while each access reloads that slot inside the loop. That pair is equivalent
/// only when the slot cannot escape by reference and no instruction writes it anywhere in the
/// function.
fn sources_name_same_buffer(
    function: &Function,
    bound: HandleSource,
    access: HandleSource,
    escaping: &HashSet<LocalSlotId>,
    written_anywhere: &HashSet<LocalSlotId>,
) -> bool {
    if bound == access {
        return true;
    }
    let (HandleSource::Value(value), HandleSource::Slot(slot)) = (bound, access) else {
        return false;
    };
    let Some(ValueDef::Instruction { inst, .. }) = function.value(value).map(|value| value.def)
    else {
        return false;
    };
    let Some(load) = function.instruction(inst) else {
        return false;
    };
    load.op == Op::LoadLocal
        && load.immediate == Some(Immediate::LocalSlot(slot))
        && load.effects.difference(Effects::READS_LOCAL).is_empty()
        && !escaping.contains(&slot)
        && !written_anywhere.contains(&slot)
}

/// Finds unit-step zero-based loop indices guarded by `index < buffer_len(same_handle)`.
fn proven_in_bounds_indices(
    function: &Function,
    loop_ref: &NaturalLoop,
    written: &HashSet<LocalSlotId>,
    escaping: &HashSet<LocalSlotId>,
) -> HashMap<ValueId, InBoundsProof> {
    let Some((condition, continue_when_true)) = super::integer_range::loop_condition(function, loop_ref) else {
        return HashMap::new();
    };
    let Some(guarded_entry) = loop_continue_target(function, loop_ref) else {
        return HashMap::new();
    };
    let inductions = super::induction::basic_inductions(function, loop_ref, |value| {
        exact_i64_constant(function, value)
    });
    let incoming = incoming_arguments(function);
    let mut proofs = HashMap::new();
    for induction in inductions {
        if induction.step != 1 || exact_i64_constant(function, induction.initial) != Some(0) {
            continue;
        }
        let Some((CmpPredicate::Slt, bound)) = super::integer_range::induction_condition(
            function,
            condition,
            continue_when_true,
            induction.parameter,
        ) else {
            continue;
        };
        let bound = canonical_value(function, &incoming, bound);
        let Some(source) = buffer_length_source(
            function,
            loop_ref,
            &incoming,
            bound,
            written,
            escaping,
        ) else {
            continue;
        };
        proofs.insert(induction.parameter, InBoundsProof { source, guarded_entry });
    }
    proofs
}

/// Returns the in-loop successor selected when the header condition permits another iteration.
fn loop_continue_target(function: &Function, loop_ref: &NaturalLoop) -> Option<BlockId> {
    let Terminator::CondBr { then_target, else_target, .. } =
        function.block(loop_ref.header)?.terminator.as_ref()?
    else {
        return None;
    };
    match (loop_ref.contains(*then_target), loop_ref.contains(*else_target)) {
        (true, false) => Some(*then_target),
        (false, true) => Some(*else_target),
        _ => None,
    }
}

/// Returns the stable handle source read by a direct `buffer_len` bound.
fn buffer_length_source(
    function: &Function,
    loop_ref: &NaturalLoop,
    incoming: &HashMap<(BlockId, usize), Vec<ValueId>>,
    length: ValueId,
    written: &HashSet<LocalSlotId>,
    escaping: &HashSet<LocalSlotId>,
) -> Option<HandleSource> {
    let length = canonical_value(function, incoming, length);
    let ValueDef::Instruction { inst, .. } = function.value(length)?.def else {
        return None;
    };
    let length_inst = function.instruction(inst)?;
    let is_buffer_len = length_inst.op == Op::BufferLen
        || matches!(
            length_inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::BufferLen
            )))
        );
    if !is_buffer_len || length_inst.operands.len() != 1 {
        return None;
    }
    let handle = canonical_value(function, incoming, length_inst.operands[0]);
    handle_source(function, loop_ref, handle, written, escaping)
}

/// Reads an exact scalar integer constant for induction initialization and step discovery.
fn exact_i64_constant(function: &Function, value: ValueId) -> Option<i64> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    let instruction = function.instruction(inst)?;
    match (instruction.op, instruction.immediate.as_ref()) {
        (Op::ConstI64, Some(Immediate::I64(value))) => Some(*value),
        _ => None,
    }
}

/// Returns whether no instruction in the loop body can free, allocate, or rebind a buffer.
fn loop_preserves_buffers(function: &Function, loop_ref: &NaturalLoop) -> bool {
    loop_ref.blocks.iter().all(|&block| {
        function.blocks[block.as_raw() as usize]
            .instructions
            .iter()
            .all(|&inst_id| instruction_preserves_buffers(function, &function.instructions[inst_id.as_raw() as usize]))
    })
}

/// Default-deny classification of one loop instruction.
///
/// Anything able to run user code (a destructor, an error handler, an output callback, an
/// unwinding throw), allocate, or write local or heap state other than a scalar local, a
/// buffer element or a packed field disqualifies the loop. A call passes only through the
/// effects check: effect refinement gives a direct user call its callee's summary, so a callee
/// proven to only read state or fatal cannot free a buffer either.
fn instruction_preserves_buffers(function: &Function, inst: &Instruction) -> bool {
    match inst.op {
        Op::Nop
        | Op::ConcatReset
        | Op::BufferGet
        | Op::BufferSet
        | Op::BufferResolve
        | Op::BufferGetResolved
        | Op::BufferSetResolved => true,
        Op::StoreLocal => store_targets_scalar_local(function, inst),
        Op::PropGet | Op::PropSet if function.is_packed_field_access(inst) => true,
        _ => inst
            .effects
            .difference(Effects::READS_LOCAL | Effects::READS_HEAP | Effects::READS_GLOBAL | Effects::MAY_FATAL)
            .is_empty(),
    }
}

/// Returns whether a `store_local` writes a scalar local, which cannot release anything.
fn store_targets_scalar_local(function: &Function, inst: &Instruction) -> bool {
    let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
        return false;
    };
    function.locals.get(slot.as_raw() as usize).is_some_and(|local| {
        matches!(
            local.php_type.codegen_repr(),
            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Pointer(_)
        )
    })
}

/// Collects every local slot an instruction in the loop names other than through a load.
fn slots_named_in_loop(function: &Function, loop_ref: &NaturalLoop) -> HashSet<LocalSlotId> {
    slots_named_as_storage(loop_ref.blocks.iter().flat_map(|&block| {
        function.blocks[block.as_raw() as usize]
            .instructions
            .iter()
            .map(|&inst_id| &function.instructions[inst_id.as_raw() as usize])
    }))
}

/// Collects every local slot named as storage anywhere in the function.
fn slots_named_in_function(function: &Function) -> HashSet<LocalSlotId> {
    slots_named_as_storage(function.instructions.iter())
}

/// Collects local slots that an instruction stream may read or write as storage rather than load.
fn slots_named_as_storage<'a>(
    instructions: impl Iterator<Item = &'a Instruction>,
) -> HashSet<LocalSlotId> {
    let mut written = HashSet::new();
    for inst in instructions {
        match &inst.immediate {
            Some(Immediate::LocalSlot(slot)) if inst.op != Op::LoadLocal => {
                written.insert(*slot);
            }
            Some(Immediate::LocalSlotPair { first, second }) => {
                written.insert(*first);
                written.insert(*second);
            }
            Some(Immediate::IterStart(metadata)) => written.extend(metadata.local_slots()),
            _ => {}
        }
    }
    written
}

/// Classifies a buffer access's handle operand as loop-invariant, or `None` when it may vary.
fn handle_source(
    function: &Function,
    loop_ref: &NaturalLoop,
    handle: ValueId,
    written: &HashSet<LocalSlotId>,
    escaping: &HashSet<LocalSlotId>,
) -> Option<HandleSource> {
    match function.value(handle)?.def {
        ValueDef::BlockParam { block, .. } => {
            (!loop_ref.contains(block)).then_some(HandleSource::Value(handle))
        }
        ValueDef::Instruction { block, inst, .. } => {
            if !loop_ref.contains(block) {
                return Some(HandleSource::Value(handle));
            }
            let load = function.instruction(inst)?;
            let Some(Immediate::LocalSlot(slot)) = load.immediate else {
                return None;
            };
            let read_only = load.effects.difference(Effects::READS_LOCAL).is_empty();
            (load.op == Op::LoadLocal
                && read_only
                && !written.contains(&slot)
                && !escaping.contains(&slot))
            .then_some(HandleSource::Slot(slot))
        }
    }
}

/// Returns the earliest preheader position at which resolving `source` yields the descriptor the
/// end of the preheader would.
///
/// The position follows the handle's own definition and every preheader instruction from `floor`
/// on that could free, allocate or rebind a buffer (the loop-body allowlist) or name the handle
/// slot as storage. The instructions after it preserve every buffer, so moving the resolution
/// ahead of them cannot change its answer, while values they define (the loads LICM hoisted into
/// the preheader) no longer live across the resolver call.
fn resolver_position(
    function: &Function,
    preheader: BlockId,
    source: HandleSource,
    floor: usize,
) -> usize {
    let block = &function.blocks[preheader.as_raw() as usize];
    let mut position = floor;
    for (index, &inst_id) in block.instructions.iter().enumerate().skip(floor) {
        let inst = &function.instructions[inst_id.as_raw() as usize];
        let pinned = match source {
            HandleSource::Value(value) => inst.result == Some(value),
            HandleSource::Slot(slot) => {
                slots_named_as_storage(std::iter::once(inst)).contains(&slot)
            }
        };
        if pinned || !instruction_preserves_buffers(function, inst) {
            position = index + 1;
        }
    }
    position
}

/// Inserts a copy of an in-loop handle load into the preheader at `position` and returns its
/// value.
fn insert_slot_load(
    function: &mut Function,
    preheader: BlockId,
    position: usize,
    slot: LocalSlotId,
    template: ValueId,
) -> ValueId {
    let (ir_type, php_type, ownership, effects) = {
        let value = &function.values[template.as_raw() as usize];
        let ValueDef::Instruction { inst, .. } = value.def else {
            unreachable!("slot handles are defined by load_local");
        };
        let load = &function.instructions[inst.as_raw() as usize];
        (value.ir_type, value.php_type.clone(), value.ownership, load.effects)
    };
    let result = insert_instruction(
        function,
        preheader,
        position,
        Op::LoadLocal,
        Vec::new(),
        ir_type,
        php_type,
    );
    function.values[result.as_raw() as usize].ownership = ownership;
    let ValueDef::Instruction { inst, .. } = function.values[result.as_raw() as usize].def else {
        unreachable!("inserted instructions define their result");
    };
    let load = &mut function.instructions[inst.as_raw() as usize];
    load.immediate = Some(Immediate::LocalSlot(slot));
    load.result_ownership = ownership;
    load.effects = effects;
    result
}

/// Inserts a result-producing instruction into a block at `position` and returns its value.
///
/// The instructions from `position` on move one slot down, so the recorded definition index of
/// each of their results is refreshed to keep the value table in step with the block layout.
fn insert_instruction(
    function: &mut Function,
    block: BlockId,
    position: usize,
    op: Op,
    operands: Vec<ValueId>,
    ir_type: IrType,
    php_type: PhpType,
) -> ValueId {
    let inst_id = InstId::from_raw(function.instructions.len() as u32);
    let result = ValueId::from_raw(function.values.len() as u32);
    function.values.push(Value {
        ir_type,
        php_type: php_type.clone(),
        def: ValueDef::Instruction { block, index: position as u32, inst: inst_id },
        ownership: Ownership::NonHeap,
    });
    let mut inst = Instruction::new(
        op,
        operands,
        None,
        Some(result),
        ir_type,
        php_type,
        Ownership::NonHeap,
        op.default_effects(),
        None,
    );
    inst.origin = Some(PassOrigin::BufferAccess);
    function.instructions.push(inst);
    function.blocks[block.as_raw() as usize].instructions.insert(position, inst_id);
    let shifted: Vec<InstId> =
        function.blocks[block.as_raw() as usize].instructions[position + 1..].to_vec();
    for (offset, shifted_id) in shifted.into_iter().enumerate() {
        if let Some(value) = function.instructions[shifted_id.as_raw() as usize].result {
            function.values[value.as_raw() as usize].def = ValueDef::Instruction {
                block,
                index: (position + 1 + offset) as u32,
                inst: shifted_id,
            };
        }
    }
    result
}

/// Neutralizes in-loop handle loads that no instruction or terminator reads any more.
fn neutralize_unused_loads(function: &mut Function, loads: &[ValueId]) {
    let uses = function.value_use_counts();
    for &value in loads {
        if uses.get(&value).copied().unwrap_or(0) > 0 {
            continue;
        }
        if let ValueDef::Instruction { inst, .. } = function.values[value.as_raw() as usize].def {
            neutralize_to_nop(&mut function.instructions[inst.as_raw() as usize]);
        }
    }
}

/// Replaces each packed record address that a dominating access with the same descriptor and
/// index already computed.
fn share_record_addresses(function: &mut Function, rewritten: &[InstId], dominance: &DominanceInfo) {
    let mut positions: HashMap<InstId, (BlockId, usize)> = HashMap::new();
    for block in &function.blocks {
        for (index, &inst_id) in block.instructions.iter().enumerate() {
            positions.insert(inst_id, (block.id, index));
        }
    }
    let incoming = incoming_arguments(function);
    let mut groups: HashMap<(ValueId, ValueId), Vec<InstId>> = HashMap::new();
    for &inst_id in rewritten {
        let inst = &function.instructions[inst_id.as_raw() as usize];
        if inst.op != Op::BufferGetResolved
            || !matches!(inst.result_php_type.codegen_repr(), PhpType::Packed(_))
        {
            continue;
        }
        let index = canonical_value(function, &incoming, inst.operands[1]);
        groups.entry((inst.operands[0], index)).or_default().push(inst_id);
    }
    let dominates = |earlier: InstId, later: InstId| {
        let (earlier_block, earlier_index) = positions[&earlier];
        let (later_block, later_index) = positions[&later];
        if earlier_block == later_block {
            earlier_index < later_index
        } else {
            dominance.strictly_dominates(earlier_block, later_block)
        }
    };
    let mut replacements = HashMap::new();
    let mut redundant = Vec::new();
    for accesses in groups.values() {
        for &later in accesses {
            let Some(&earlier) = accesses.iter().find(|&&earlier| earlier != later && dominates(earlier, later)) else {
                continue;
            };
            let (Some(old), Some(new)) = (
                function.instructions[later.as_raw() as usize].result,
                function.instructions[earlier.as_raw() as usize].result,
            ) else {
                continue;
            };
            replacements.insert(old, new);
            redundant.push(later);
        }
    }
    if replacements.is_empty() {
        return;
    }
    replace_all_uses(function, &resolve_chains(&replacements));
    for inst_id in redundant {
        neutralize_to_nop(&mut function.instructions[inst_id.as_raw() as usize]);
    }
}

/// Collects, for every block parameter, the arguments each incoming edge passes to it.
fn incoming_arguments(function: &Function) -> HashMap<(BlockId, usize), Vec<ValueId>> {
    let mut incoming: HashMap<(BlockId, usize), Vec<ValueId>> = HashMap::new();
    for block in &function.blocks {
        let Some(terminator) = block.terminator.as_ref() else {
            continue;
        };
        for (target, args) in edge_arguments(terminator) {
            for (index, &arg) in args.iter().enumerate() {
                incoming.entry((target, index)).or_default().push(arg);
            }
        }
    }
    incoming
}

/// Returns each explicit successor of a terminator with the arguments it passes.
fn edge_arguments(terminator: &crate::ir::Terminator) -> Vec<(BlockId, &[ValueId])> {
    use crate::ir::Terminator;
    match terminator {
        Terminator::Br { target, args } => vec![(*target, args.as_slice())],
        Terminator::CondBr { then_target, then_args, else_target, else_args, .. } => {
            vec![(*then_target, then_args.as_slice()), (*else_target, else_args.as_slice())]
        }
        Terminator::Switch { cases, default, default_args, .. } => {
            let mut edges: Vec<_> = cases
                .iter()
                .map(|case| (case.target, case.args.as_slice()))
                .collect();
            edges.push((*default, default_args.as_slice()));
            edges
        }
        Terminator::GeneratorSuspend { resume, resume_args, .. } => {
            vec![(*resume, resume_args.as_slice())]
        }
        Terminator::Return { .. }
        | Terminator::Throw { .. }
        | Terminator::Fatal { .. }
        | Terminator::Unreachable => Vec::new(),
    }
}

/// Follows block parameters whose every incoming edge passes one and the same value.
///
/// Scalar promotion places a parameter at each join a promoted local is live into, even when
/// no path changes it, so the loop index reaches the second half of an iteration as a fresh
/// parameter equal to the header's. Seeing through such trivial parameters lets two accesses
/// of `$b[$i]` on either side of an `if` share one record address.
fn canonical_value(
    function: &Function,
    incoming: &HashMap<(BlockId, usize), Vec<ValueId>>,
    value: ValueId,
) -> ValueId {
    let mut current = value;
    for _ in 0..function.values.len() {
        let Some(ValueDef::BlockParam { block, index }) = function.value(current).map(|value| value.def) else {
            break;
        };
        let Some(args) = incoming.get(&(block, index as usize)) else {
            break;
        };
        let mut distinct = args.iter().copied().filter(|&arg| arg != current);
        let Some(first) = distinct.next() else {
            break;
        };
        if distinct.any(|arg| arg != first) {
            break;
        }
        current = first;
    }
    current
}
