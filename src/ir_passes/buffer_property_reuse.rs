//! Purpose:
//! Shares one read of a `buffer<T>`-typed property, one handle resolution and one packed record
//! address between the repeated `$this->slots[$i]` accesses of straight-line code, each of which
//! would otherwise reload the property, call `__rt_buffer_resolve` and recompute the address.
//!
//! Called from:
//! - `crate::ir_passes::buffer_access::optimize_function`, before and after loop descriptor
//!   hoisting, for functions that pass its generator, exception-handler and `eval` guards without
//!   `pcntl_async_signals()`.
//! - `crate::ir_passes::buffer_property_loops`, for the shared clean-loop classifier and
//!   property-read merging before descriptor hoisting.
//!
//! Key details:
//! - Two reads agree only across a CLEAN REGION: every instruction on any path from the earliest
//!   instruction either read depends on to the later read must leave buffers, buffer-typed
//!   properties and the matched local slots as they were. The classifier is default-deny: the
//!   loop pass's buffer-preserving allowlist minus every opcode that can run user code, plus the
//!   no-op `acquire`/`release` of a buffer handle and named property reads that only read the heap
//!   or throw. A call, a property store or `unset` on any object (another reference may alias the
//!   receiver), `buffer_free()`, a release, an allocation, a warning or output ends the region.
//! - Receivers and indexes match when they are one SSA value (seen through block parameters every
//!   edge passes the same value), equal integer constants, or loads of one stable slot (a
//!   by-value PHP local that only `load_local` and `store_local` name) that no instruction in the
//!   region names. With no user code in the region, nothing can write such a slot through a
//!   by-reference alias.
//! - Three rewrites run in order. A dominated buffer-typed property read with the same name,
//!   receiver and inlined class scope (`Function::inlined_scopes`) is replaced by the dominating
//!   one. A dominated packed `buffer_get` of the same
//!   property-read handle and index reuses the dominating record address. The remaining accesses
//!   of one property-read handle that a leading access dominates share one non-faulting
//!   `buffer_resolve` inserted right before that leader and become `buffer_*_resolved`, so a stale
//!   handle still dies with the use-after-free fatal at the leader, as before.
//! - Buffer `acquire`/`release` emit no code; a pair left with nothing but releases is dropped.
//! - Foreign buffer reads qualify only when a checked access consumes their handle before any
//!   observable operation or branch. A null receiver can warn and run an error handler, even with
//!   a non-nullable static type, but that read returns handle zero and the access cannot return.
//!   Relocated reads keep this first check before header re-evaluation. Other foreign property
//!   reads still end the region, apart from guarded scalar reads spliced from `$this`.
//!   Modules with `__get` or eval skip the phase entirely.
//! - Only handles read from a property are rewritten. Local buffer handles in straight-line code
//!   keep their per-access form.

use std::collections::{HashMap, HashSet};

use crate::ir::{
    BlockId, Effects, Function, Immediate, InstId, Instruction, IrType, LocalKind, LocalSlotId,
    Module, Op, PassOrigin, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::buffer_access::{
    canonical_value, incoming_arguments, insert_instruction, instruction_preserves_buffers,
    slots_named_as_storage,
};
use super::cfg::{predecessors, reverse_postorder, successors};
use super::dominance::{compute_dominance, DominanceInfo};
use super::rewrite::{defining_instruction, neutralize_to_nop, replace_all_uses, resolve_chains};

/// Most recent dominating candidates one later read is compared against, which bounds the
/// pairwise scan in very long functions.
const MAX_CANDIDATES: usize = 16;

/// An instruction's block and its index inside that block.
type Point = (BlockId, usize);

/// Read-only facts about one function that the region checks share.
struct Facts {
    /// Dominator tree of the function.
    dominance: DominanceInfo,
    /// Predecessor blocks, indexed by raw block id.
    predecessors: Vec<Vec<BlockId>>,
    /// Position of every instruction placed in a reachable block.
    positions: HashMap<InstId, Point>,
    /// Slots whose loads may be matched, see `stable_slots`.
    stable_slots: HashSet<LocalSlotId>,
    /// Reachable blocks in reverse postorder, so a dominator comes before what it dominates.
    order: Vec<BlockId>,
    /// Arguments each incoming edge passes to every block parameter.
    incoming: HashMap<(BlockId, usize), Vec<ValueId>>,
}

impl Facts {
    /// Computes the dominance, predecessor, position and slot facts of one function.
    fn of(function: &Function) -> Self {
        let dominance = compute_dominance(function);
        let mut positions = HashMap::new();
        for block in &function.blocks {
            if !dominance.is_reachable(block.id) {
                continue;
            }
            for (index, &inst_id) in block.instructions.iter().enumerate() {
                positions.insert(inst_id, (block.id, index));
            }
        }
        Self {
            predecessors: predecessors(function),
            stable_slots: stable_slots(function),
            order: reverse_postorder(function),
            incoming: incoming_arguments(function),
            dominance,
            positions,
        }
    }

    /// Returns where the instruction defining `value` sits, or `None` for a block parameter or a
    /// definition outside every reachable block.
    fn definition_point(&self, function: &Function, value: ValueId) -> Option<Point> {
        match function.value(value)?.def {
            ValueDef::Instruction { inst, .. } => self.positions.get(&inst).copied(),
            ValueDef::BlockParam { .. } => None,
        }
    }

    /// Returns whether `earlier` executes before `later` on every path that reaches `later`.
    fn dominates(&self, earlier: Point, later: Point) -> bool {
        if earlier.0 == later.0 {
            earlier.1 < later.1
        } else {
            self.dominance.strictly_dominates(earlier.0, later.0)
        }
    }

    /// Returns the reachable instructions that satisfy `keep`, dominators first.
    fn ordered(&self, function: &Function, keep: impl Fn(&Instruction) -> bool) -> Vec<InstId> {
        self.order
            .iter()
            .flat_map(|&block| function.blocks[block.as_raw() as usize].instructions.iter().copied())
            .filter(|&inst_id| keep(&function.instructions[inst_id.as_raw() as usize]))
            .collect()
    }
}

/// Returns whether no named property read in the module can dispatch to a user `__get`: no class
/// declares one and no eval bridge can declare a class later.
pub(super) fn module_allows_property_reuse(module: &Module) -> bool {
    !module.required_runtime_features.eval_bridge
        && !module.class_infos.values().any(|class| {
            class.methods.keys().any(|method| method.eq_ignore_ascii_case("__get"))
        })
}

/// Shares buffer-typed property reads, record addresses and descriptors in one function, and
/// reports whether anything changed.
pub(super) fn reuse_property_buffers(function: &mut Function) -> bool {
    if !function.instructions.iter().any(|inst| is_buffer_property_read(function, inst)) {
        return false;
    }
    let facts = Facts::of(function);
    let mut changed = merge_property_reads(function, &facts);
    let mut released_handles = Vec::new();
    changed |= share_record_addresses(function, &facts, &mut released_handles);
    changed |= share_descriptors(function, &facts, &mut released_handles);
    drop_release_only_acquires(function, &released_handles);
    changed
}

/// Merges only redundant property reads, leaving descriptor placement to loop hoisting.
pub(super) fn reuse_property_reads(function: &mut Function) -> bool {
    merge_property_reads(function, &Facts::of(function))
}

/// Shares raw packed addresses before loop resolution changes their access opcode.
pub(super) fn reuse_property_reads_and_records(function: &mut Function) -> bool {
    if !function.instructions.iter().any(|inst| is_buffer_property_read(function, inst)) {
        return false;
    }
    let mut changed = reuse_property_reads(function);
    let facts = Facts::of(function);
    let mut released_handles = Vec::new();
    changed |= share_record_addresses(function, &facts, &mut released_handles);
    drop_release_only_acquires(function, &released_handles);
    changed
}

/// Reuses buffer and guarded scalar property reads already made on the same stable receiver.
fn merge_property_reads(function: &mut Function, facts: &Facts) -> bool {
    let reads = facts.ordered(function, |inst| {
        is_buffer_property_read(function, inst) || is_inlined_scalar_property_read(function, inst)
    });
    let mut kept: Vec<InstId> = Vec::new();
    let mut replacements = HashMap::new();
    let mut redundant = Vec::new();
    for later in reads {
        let earlier = kept
            .iter()
            .rev()
            .take(MAX_CANDIDATES)
            .copied()
            .find(|&earlier| same_property_value(function, facts, earlier, later));
        match earlier {
            Some(earlier) => {
                let (Some(old), Some(new)) = (
                    function.instructions[later.as_raw() as usize].result,
                    function.instructions[earlier.as_raw() as usize].result,
                ) else {
                    continue;
                };
                replacements.insert(old, new);
                redundant.push(later);
            }
            None => kept.push(later),
        }
    }
    if replacements.is_empty() {
        return false;
    }
    replace_all_uses(function, &resolve_chains(&replacements));
    for inst_id in redundant {
        neutralize_to_nop(&mut function.instructions[inst_id.as_raw() as usize]);
    }
    true
}

/// Returns whether two property reads, `earlier` dominating `later`, load the same handle.
///
/// They must name the same property in the same inlined class scope on matching receivers.
/// Nothing between the earliest instruction either depends on and `later` may write a property,
/// run user code or write the slot a matched receiver load reads. A read that did not throw at
/// `later` either, since the property's state is unchanged.
fn same_property_value(function: &Function, facts: &Facts, earlier: InstId, later: InstId) -> bool {
    let (first, second) = (
        &function.instructions[earlier.as_raw() as usize],
        &function.instructions[later.as_raw() as usize],
    );
    let (Some(&first_point), Some(&second_point)) =
        (facts.positions.get(&earlier), facts.positions.get(&later))
    else {
        return false;
    };
    if first.immediate != second.immediate
        || first.result_php_type != second.result_php_type
        || function.inlined_scopes.get(&earlier) != function.inlined_scopes.get(&later)
        || !facts.dominates(first_point, second_point)
    {
        return false;
    }
    let Some(receivers) = match_operands(function, facts, first.operands[0], second.operands[0])
    else {
        return false;
    };
    region_is_clean_for(function, facts, first_point, second_point, &[receivers])
}

/// Replaces each packed `buffer_get` whose record address a dominating access of the same
/// property-read handle and index already computed, across a clean region.
fn share_record_addresses(
    function: &mut Function,
    facts: &Facts,
    released_handles: &mut Vec<ValueId>,
) -> bool {
    let records = facts.ordered(function, |inst| {
        inst.op == Op::BufferGet
            && matches!(inst.result_php_type.codegen_repr(), PhpType::Packed(_))
            && property_root(function, inst.operands[0]).is_some()
    });
    let mut kept: Vec<InstId> = Vec::new();
    let mut replacements = HashMap::new();
    let mut redundant = Vec::new();
    for later in records {
        let earlier = kept
            .iter()
            .rev()
            .take(MAX_CANDIDATES)
            .copied()
            .find(|&earlier| same_record(function, facts, earlier, later));
        match earlier {
            Some(earlier) => {
                let (Some(old), Some(new)) = (
                    function.instructions[later.as_raw() as usize].result,
                    function.instructions[earlier.as_raw() as usize].result,
                ) else {
                    continue;
                };
                replacements.insert(old, new);
                redundant.push(later);
            }
            None => kept.push(later),
        }
    }
    if replacements.is_empty() {
        return false;
    }
    replace_all_uses(function, &resolve_chains(&replacements));
    for inst_id in redundant {
        let inst = &mut function.instructions[inst_id.as_raw() as usize];
        released_handles.push(inst.operands[0]);
        neutralize_to_nop(inst);
    }
    true
}

/// Returns whether two packed reads, `earlier` dominating `later`, address the same record.
///
/// Both handles must come from one property read and the indexes must match, with a clean region
/// from the earliest index load (or `earlier`) to `later`: nothing in it can free the buffer or
/// write the index slot, so `earlier`'s bounds-checked address is the one `later` would compute.
fn same_record(function: &Function, facts: &Facts, earlier: InstId, later: InstId) -> bool {
    let (first, second) = (
        &function.instructions[earlier.as_raw() as usize],
        &function.instructions[later.as_raw() as usize],
    );
    let (Some(&first_point), Some(&second_point)) =
        (facts.positions.get(&earlier), facts.positions.get(&later))
    else {
        return false;
    };
    if property_root(function, first.operands[0]) != property_root(function, second.operands[0])
        || !facts.dominates(first_point, second_point)
    {
        return false;
    }
    let Some(indexes) = match_operands(function, facts, first.operands[1], second.operands[1])
    else {
        return false;
    };
    region_is_clean_for(function, facts, first_point, second_point, &[indexes])
}

/// Gives the accesses of one property-read handle that a leading access dominates, across clean
/// regions, one `buffer_resolve` right before that leader and rewrites them into resolved forms.
fn share_descriptors(
    function: &mut Function,
    facts: &Facts,
    released_handles: &mut Vec<ValueId>,
) -> bool {
    let accesses = facts.ordered(function, |inst| {
        matches!(inst.op, Op::BufferGet | Op::BufferSet)
            && property_root(function, inst.operands[0]).is_some()
    });
    // Leaders in dominance order, each with its root handle and the accesses it serves.
    let mut groups: Vec<(ValueId, InstId, Vec<InstId>)> = Vec::new();
    for access in accesses {
        let handle = function.instructions[access.as_raw() as usize].operands[0];
        let Some(root) = property_root(function, handle) else {
            continue;
        };
        let Some(&access_point) = facts.positions.get(&access) else {
            continue;
        };
        let leader = groups
            .iter_mut()
            .rev()
            .filter(|(group_root, _, _)| *group_root == root)
            .take(MAX_CANDIDATES)
            .find(|(_, leader, _)| {
                facts.positions.get(leader).is_some_and(|&leader_point| {
                    facts.dominates(leader_point, access_point)
                        && region_is_clean(function, facts, leader_point, access_point, &[])
                })
            });
        match leader {
            Some((_, _, members)) => members.push(access),
            None => groups.push((root, access, Vec::new())),
        }
    }
    let mut changed = false;
    for (root, leader, members) in groups {
        if members.is_empty() {
            continue;
        }
        let (block, position) = facts.positions[&leader];
        // Earlier insertions into the same block shifted the leader; find where it is now.
        let position = function.blocks[block.as_raw() as usize]
            .instructions
            .iter()
            .position(|&inst_id| inst_id == leader)
            .unwrap_or(position);
        let descriptor = insert_instruction(
            function,
            block,
            position,
            Op::BufferResolve,
            vec![root],
            IrType::I64,
            PhpType::Int,
        );
        for access in std::iter::once(leader).chain(members) {
            let inst = &mut function.instructions[access.as_raw() as usize];
            released_handles.push(inst.operands[0]);
            inst.op = if inst.op == Op::BufferGet {
                Op::BufferGetResolved
            } else {
                Op::BufferSetResolved
            };
            inst.operands[0] = descriptor;
            inst.immediate = None;
            inst.effects = inst.op.default_effects();
            inst.origin = Some(PassOrigin::BufferAccess);
        }
        changed = true;
    }
    changed
}

/// Neutralizes each no-op buffer `acquire` among `handles` whose result only `release`s still
/// read, together with those releases.
fn drop_release_only_acquires(function: &mut Function, handles: &[ValueId]) {
    let uses = function.value_use_counts();
    let mut seen = HashSet::new();
    for &handle in handles {
        if !seen.insert(handle) {
            continue;
        }
        let Some(ValueDef::Instruction { inst: acquire, .. }) =
            function.value(handle).map(|value| value.def)
        else {
            continue;
        };
        let acquire_inst = &function.instructions[acquire.as_raw() as usize];
        if acquire_inst.op != Op::Acquire || !is_buffer_refcount_no_op(function, acquire_inst) {
            continue;
        }
        let releases: Vec<usize> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| {
                inst.op == Op::Release && inst.operands.len() == 1 && inst.operands[0] == handle
            })
            .map(|(index, _)| index)
            .collect();
        if uses.get(&handle).copied().unwrap_or(0) != releases.len() {
            continue;
        }
        for index in releases {
            neutralize_to_nop(&mut function.instructions[index]);
        }
        neutralize_to_nop(&mut function.instructions[acquire.as_raw() as usize]);
    }
}

/// How two operand values were proven equal.
#[derive(Clone, Copy)]
enum OperandMatch {
    /// One SSA value, or the same integer constant.
    Value,
    /// Loads of one stable slot at the two points, equal only across a clean region that also
    /// leaves the slot unnamed.
    Slot {
        slot: LocalSlotId,
        loads: [Point; 2],
    },
}

/// Matches two operands, or returns `None` when they may differ.
fn match_operands(
    function: &Function,
    facts: &Facts,
    first: ValueId,
    second: ValueId,
) -> Option<OperandMatch> {
    // Scalar promotion leaves a fresh parameter at each join an unchanged local is live into.
    let first = canonical_value(function, &facts.incoming, first);
    let second = canonical_value(function, &facts.incoming, second);
    if first == second {
        return Some(OperandMatch::Value);
    }
    let (first_inst, second_inst) = (
        defining_instruction(function, first)?,
        defining_instruction(function, second)?,
    );
    match (first_inst.op, second_inst.op) {
        (Op::ConstI64, Op::ConstI64)
            if first_inst.immediate.is_some() && first_inst.immediate == second_inst.immediate =>
        {
            Some(OperandMatch::Value)
        }
        (Op::LoadLocal, Op::LoadLocal) => {
            let (Some(Immediate::LocalSlot(slot)), Some(Immediate::LocalSlot(other))) =
                (first_inst.immediate.as_ref(), second_inst.immediate.as_ref())
            else {
                return None;
            };
            let reads_only_local = |inst: &Instruction| {
                inst.effects.difference(Effects::READS_LOCAL).is_empty()
            };
            if slot != other
                || !facts.stable_slots.contains(slot)
                || !reads_only_local(first_inst)
                || !reads_only_local(second_inst)
            {
                return None;
            }
            Some(OperandMatch::Slot {
                slot: *slot,
                loads: [
                    facts.definition_point(function, first)?,
                    facts.definition_point(function, second)?,
                ],
            })
        }
        _ => None,
    }
}

/// Checks the clean region for a pair of reads whose operands matched as `operand_matches`.
///
/// The region starts at the earliest of `earlier` and every matched slot load (all of which
/// dominate `later`, so they are ordered along its dominator chain) and must not name any matched
/// slot, so both loads of each slot read the same value.
fn region_is_clean_for(
    function: &Function,
    facts: &Facts,
    earlier: Point,
    later: Point,
    operand_matches: &[OperandMatch],
) -> bool {
    let mut start = earlier;
    let mut watched = Vec::new();
    for operand_match in operand_matches {
        if let OperandMatch::Slot { slot, loads } = *operand_match {
            watched.push(slot);
            for load in loads {
                if facts.dominates(load, start) {
                    start = load;
                }
            }
        }
    }
    region_is_clean(function, facts, start, later, &watched)
}

/// Returns whether every instruction that can execute after `start` and before `end`, where
/// `start` dominates `end`, keeps reads stable and leaves the `watched` slots unnamed.
///
/// With different blocks the region is the tail of `start`'s block, every block some path from
/// it to `end`'s block crosses without re-entering it, and the head of `end`'s block, or all of
/// that block when such a path loops back into it.
fn region_is_clean(
    function: &Function,
    facts: &Facts,
    start: Point,
    end: Point,
    watched: &[LocalSlotId],
) -> bool {
    let all_clean = |instructions: &[InstId]| {
        instructions
            .iter()
            .all(|&inst_id| instruction_is_clean(function, inst_id, watched))
    };
    if start.0 == end.0 {
        return all_clean(&block_instructions(function, start.0)[start.1 + 1..end.1]);
    }
    if !all_clean(&block_instructions(function, start.0)[start.1 + 1..]) {
        return false;
    }
    let between = blocks_between(function, facts, start.0, end.0);
    let end_instructions = block_instructions(function, end.0);
    let end_head = if between.contains(&end.0) {
        end_instructions
    } else {
        &end_instructions[..end.1]
    };
    between
        .iter()
        .filter(|&&block| block != end.0)
        .all(|&block| all_clean(block_instructions(function, block)))
        && all_clean(end_head)
}

/// Returns whether one region instruction keeps reads stable and names none of the `watched`
/// slots.
fn instruction_is_clean(function: &Function, inst_id: InstId, watched: &[LocalSlotId]) -> bool {
    let inst = &function.instructions[inst_id.as_raw() as usize];
    keeps_reads_stable(function, inst)
        && (watched.is_empty()
            || !slots_named_as_storage(std::iter::once(inst))
                .iter()
                .any(|slot| watched.contains(slot)))
}

/// Returns one block's instruction list.
fn block_instructions(function: &Function, block: BlockId) -> &[InstId] {
    &function.blocks[block.as_raw() as usize].instructions
}

/// Returns the blocks that some path from the end of `from` to the start of `to` passes through
/// without re-entering `from`, including `to` when such a path enters it before the target visit.
fn blocks_between(
    function: &Function,
    facts: &Facts,
    from: BlockId,
    to: BlockId,
) -> HashSet<BlockId> {
    let block_successors = |block: BlockId| {
        function.blocks[block.as_raw() as usize]
            .terminator
            .as_ref()
            .map(successors)
            .unwrap_or_default()
    };
    let forward = blocks_reached_avoiding(from, block_successors(from), block_successors);
    let backward = blocks_reached_avoiding(
        from,
        facts.predecessors[to.as_raw() as usize].clone(),
        |block| facts.predecessors[block.as_raw() as usize].clone(),
    );
    forward.intersection(&backward).copied().collect()
}

/// Collects every block reachable from `seeds` through `next` without entering `avoid`.
fn blocks_reached_avoiding(
    avoid: BlockId,
    seeds: Vec<BlockId>,
    next: impl Fn(BlockId) -> Vec<BlockId>,
) -> HashSet<BlockId> {
    let mut seen = HashSet::new();
    let mut work = seeds;
    while let Some(block) = work.pop() {
        if block == avoid || !seen.insert(block) {
            continue;
        }
        work.extend(next(block));
    }
    seen
}

/// Default-deny classification of an instruction between two matched reads.
///
/// It must not free, allocate or rebind a buffer, write a property or a non-scalar local, warn,
/// print or run user code. A user call is rejected even with a read-only effect summary:
/// summaries drop the callee's local effects, and matched slots are not proven free of address
/// escapes, so the region never relies on how a callee could reach the caller's frame.
pub(super) fn keeps_reads_stable(function: &Function, inst: &Instruction) -> bool {
    match inst.op {
        Op::Acquire | Op::Release => is_buffer_refcount_no_op(function, inst),
        Op::PropGet if !function.is_packed_field_access(inst) => {
            is_plain_property_read(inst)
                && (reads_the_method_receiver(function, inst) || is_buffer_property_read(function, inst)
                    || is_inlined_scalar_property_read(function, inst))
        }
        op if may_run_user_code(op) => false,
        _ => instruction_preserves_buffers(function, inst),
    }
}

/// Returns whether an opcode can call into user PHP code or code outside the compiled program.
fn may_run_user_code(op: Op) -> bool {
    matches!(
        op,
        Op::Call
            | Op::FunctionVariantCall
            | Op::MethodCall
            | Op::NullsafeMethodCall
            | Op::StaticMethodCall
            | Op::ClosureCall
            | Op::ClosureBind
            | Op::ExprCall
            | Op::CallableDescriptorInvoke
            | Op::PipeCall
            | Op::IteratorMethodCall
            | Op::LanguageConstructCall
            | Op::ExternCall
            | Op::SplRuntimeCall
            | Op::FiberRuntimeCall
            | Op::GcControl
            | Op::CoreBuiltin
            | Op::DynamicPropGet
            | Op::NullsafePropGet
            | Op::EvalLiteralCall
            | Op::EvalFunctionCall
            | Op::EvalFunctionCallArray
            | Op::EvalObjectNew
            | Op::EvalStaticMethodCall
    )
}

/// Returns whether an `acquire` or `release` works on a buffer handle, which codegen leaves as a
/// plain value move: buffers are freed explicitly, never by reference counting.
fn is_buffer_refcount_no_op(function: &Function, inst: &Instruction) -> bool {
    inst.operands
        .first()
        .and_then(|&operand| function.value(operand))
        .is_some_and(|value| matches!(value.php_type.codegen_repr(), PhpType::Buffer(_)))
}

/// Returns whether a named property read was refined to only read the heap or throw: no warning
/// (a user error handler), no deoptimization and no accessor that writes anything.
fn is_plain_property_read(inst: &Instruction) -> bool {
    matches!(inst.immediate, Some(Immediate::Data(_)))
        && inst.effects.difference(Effects::READS_HEAP | Effects::MAY_THROW).is_empty()
}

/// An inlined object-property read retains its receiver guard; packed fields remain mutable.
pub(super) fn is_inlined_scalar_property_read(function: &Function, inst: &Instruction) -> bool {
    inst.op == Op::PropGet && is_plain_property_read(inst)
        && !function.is_packed_field_access(inst)
        && inst.result_ownership == crate::ir::Ownership::NonHeap
        && matches!(inst.result_php_type, PhpType::Int | PhpType::Float | PhpType::Bool)
        && inst.result.and_then(|value| function.value(value))
            .is_some_and(|value| matches!(value.def,
                ValueDef::Instruction { inst, .. } if function.inlined_scopes.contains_key(&inst)))
}

/// Returns whether a property read's receiver is the method's own `$this`, which PHP never lets
/// be null, reassigned or bound by reference inside the method, so the read cannot warn.
pub(super) fn reads_the_method_receiver(function: &Function, inst: &Instruction) -> bool {
    function.flags.is_method
        && function.params.first().is_some_and(|param| param.name == "this")
        && inst
            .operands
            .first()
            .and_then(|&receiver| defining_instruction(function, receiver))
            .is_some_and(|load| {
                load.op == Op::LoadLocal
                    && load.immediate == Some(Immediate::LocalSlot(LocalSlotId::from_raw(0)))
            })
}

/// Accepts `$this` reads and foreign buffer reads whose null path cannot resume execution.
pub(super) fn is_buffer_property_read(function: &Function, inst: &Instruction) -> bool {
    inst.op == Op::PropGet
        && inst.operands.len() == 1
        && !function.is_packed_field_access(inst)
        && is_plain_property_read(inst)
        && inst
            .result
            .and_then(|result| function.value(result))
            .is_some_and(|value| matches!(value.php_type.codegen_repr(), PhpType::Buffer(_)))
        && (reads_the_method_receiver(function, inst)
            // A relocated foreign read retained its checked entry access, possibly in a new block.
            || inst.origin == Some(PassOrigin::BufferAccess)
            || super::buffer_property_receivers::traps_on_null(function, inst))
}

/// Returns the buffer-typed property read a handle comes from, looking through buffer `acquire`s.
fn property_root(function: &Function, handle: ValueId) -> Option<ValueId> {
    let mut current = handle;
    for _ in 0..function.values.len() {
        let inst = defining_instruction(function, current)?;
        if inst.op == Op::Acquire && is_buffer_refcount_no_op(function, inst) {
            current = inst.operands[0];
            continue;
        }
        return is_buffer_property_read(function, inst).then_some(current);
    }
    None
}

/// Returns the by-value PHP locals whose loads may be matched.
///
/// Reference binding, iteration, unset and cleanup opcodes name a slot to alias, rebind or release
/// it, and an incoming by-reference parameter reads storage its caller shares, so loads of any
/// such slot are never matched.
fn stable_slots(function: &Function) -> HashSet<LocalSlotId> {
    let mut stable: HashSet<LocalSlotId> = function
        .locals
        .iter()
        .filter(|local| local.kind == LocalKind::PhpLocal)
        .filter(|local| {
            !function
                .params
                .get(local.id.as_raw() as usize)
                .is_some_and(|param| param.by_ref)
        })
        .map(|local| local.id)
        .collect();
    for inst in &function.instructions {
        if matches!(inst.op, Op::LoadLocal | Op::StoreLocal) {
            continue;
        }
        for slot in slots_named_as_storage(std::iter::once(inst)) {
            stable.remove(&slot);
        }
    }
    stable
}
