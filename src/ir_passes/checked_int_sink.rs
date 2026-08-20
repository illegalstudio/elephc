//! Purpose:
//! Specializes boxed checked integer add/sub/mul when every observation narrows to `int`.
//! Removes the transient Mixed allocation without changing PHP overflow semantics.
//!
//! Called from:
//! - The fixed-point pass driver after peephole and immutable-local classification.
//!
//! Key details:
//! - The accepted use graph contains only integer casts/stores plus ordinary acquire/release
//!   scaffolding. Returns, output, calls, comparisons, Mixed stores, terminators, lifetime pins,
//!   and unknown/future opcodes reject the candidate.
//! - Rewritten producers yield non-owning I64 values through `IChecked*ToInt`; removable casts
//!   and ownership instructions are neutralized only after every use has been proven safe.
//! - A second phase narrows boxed `mixed` integer locals. The per-producer phase cannot reach
//!   a loop counter: `store_local` only counts as an integer sink once the slot is `I64`, and
//!   the slot is only `mixed` because its sole writer is the boxed producer. Neither end can
//!   move first, so the slot and its producers are proven together and committed at once.

use std::collections::{HashMap, HashSet};

use crate::ir::{
    DataPool, Function, Immediate, InstId, Instruction, IrHeapKind, IrType, LocalKind,
    LocalSlotId, Op, Ownership, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::{neutralize_to_nop, replace_all_uses, resolve_chains};

/// Rewrites checked arithmetic whose complete use graph observes only a PHP integer.
pub struct CheckedIntSink;

impl IrPass for CheckedIntSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "checked_int_sink"
    }

    /// Skips functions with neither an integer sink nor a narrowable boxed integer local.
    fn is_applicable(&self, function: &Function) -> bool {
        has_potential_integer_sink(function) || has_narrowable_integer_slot(function)
    }

    /// Specializes every independently proven producer, then every provable boxed int local.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        let mut changed = sink_proven_producers(function);
        changed |= sink_integer_slots(function);
        changed
    }
}

/// Specializes every checked-arithmetic producer whose own use graph observes only an int.
fn sink_proven_producers(function: &mut Function) -> bool {
    let candidates: Vec<InstId> = function
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(raw, inst)| checked_to_int_op(inst.op).map(|_| InstId::from_raw(raw as u32)))
        .collect();
    if candidates.is_empty() {
        return false;
    }

    let uses = instruction_uses(function);
    let terminator_uses = terminator_used_values(function);
    let mut changed = false;

    for candidate in candidates {
        let Some(inst) = function.instruction(candidate) else {
            continue;
        };
        let Some(result) = inst.result else {
            continue;
        };
        let mut rewrite = SinkRewrite::default();
        if !analyze_sink_graph(
            function,
            result,
            &uses,
            &terminator_uses,
            &mut HashSet::new(),
            &mut rewrite,
        ) || !rewrite.saw_integer_sink
        {
            continue;
        }

        apply_specialization(function, candidate, result, rewrite);
        changed = true;
    }
    changed
}

/// Deferred mutations collected while proving one candidate's entire use graph.
#[derive(Default)]
struct SinkRewrite {
    replacements: HashMap<ValueId, ValueId>,
    neutralize: HashSet<InstId>,
    saw_integer_sink: bool,
}

/// Builds the instruction-use adjacency list for every SSA value.
fn instruction_uses(function: &Function) -> HashMap<ValueId, Vec<InstId>> {
    let mut uses: HashMap<ValueId, Vec<InstId>> = HashMap::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        let inst_id = InstId::from_raw(raw as u32);
        for operand in &inst.operands {
            uses.entry(*operand).or_default().push(inst_id);
        }
    }
    uses
}

/// Returns every SSA value read by a terminator, which is never an integer-only sink here.
fn terminator_used_values(function: &Function) -> HashSet<ValueId> {
    function
        .blocks
        .iter()
        .filter_map(|block| block.terminator.as_ref())
        .flat_map(super::liveness::terminator_uses)
        .collect()
}

/// Returns whether an accepted integer sink is fed by checked arithmetic through acquires.
fn has_potential_integer_sink(function: &Function) -> bool {
    function.instructions.iter().any(|user| {
        let accepts_integer = match user.op {
            Op::Cast => matches!(user.immediate, Some(Immediate::CastTarget(IrType::I64))),
            Op::StoreLocal | Op::StoreStaticLocal | Op::InitStaticLocal => {
                local_store_is_integer(function, user.immediate.as_ref())
            }
            Op::StoreRefCell => matches!(user.result_php_type.codegen_repr(), PhpType::Int),
            _ => false,
        };
        accepts_integer
            && user
                .operands
                .iter()
                .any(|operand| value_originates_from_checked(function, *operand))
    })
}

/// Follows transparent acquire definitions back to a boxed checked-arithmetic producer.
fn value_originates_from_checked(function: &Function, mut value: ValueId) -> bool {
    for _ in 0..function.values.len() {
        let Some(value_ref) = function.values.get(value.as_raw() as usize) else {
            return false;
        };
        let crate::ir::ValueDef::Instruction { inst, .. } = value_ref.def else {
            return false;
        };
        let Some(definition) = function.instruction(inst) else {
            return false;
        };
        if checked_to_int_op(definition.op).is_some() {
            return true;
        }
        if definition.op != Op::Acquire || definition.immediate.is_some() {
            return false;
        }
        let Some(operand) = definition.operands.first() else {
            return false;
        };
        value = *operand;
    }
    false
}

/// Proves that every use of `value` is an accepted integer sink or removable scaffold.
fn analyze_sink_graph(
    function: &Function,
    value: ValueId,
    uses: &HashMap<ValueId, Vec<InstId>>,
    terminator_uses: &HashSet<ValueId>,
    visited: &mut HashSet<ValueId>,
    rewrite: &mut SinkRewrite,
) -> bool {
    if !visited.insert(value) {
        return true;
    }
    if terminator_uses.contains(&value) {
        return false;
    }
    let Some(value_uses) = uses.get(&value) else {
        return false;
    };
    for use_id in value_uses {
        let Some(user) = function.instruction(*use_id) else {
            return false;
        };
        match user.op {
            Op::Cast if matches!(user.immediate, Some(Immediate::CastTarget(IrType::I64))) => {
                let Some(cast_result) = user.result else {
                    return false;
                };
                rewrite.replacements.insert(cast_result, value);
                rewrite.neutralize.insert(*use_id);
                rewrite.saw_integer_sink = true;
            }
            Op::StoreLocal | Op::StoreStaticLocal | Op::InitStaticLocal
                if local_store_is_integer(function, user.immediate.as_ref()) =>
            {
                rewrite.saw_integer_sink = true;
            }
            Op::StoreRefCell
                if matches!(user.result_php_type.codegen_repr(), PhpType::Int) =>
            {
                rewrite.saw_integer_sink = true;
            }
            Op::Acquire if user.immediate.is_none() => {
                let Some(acquired) = user.result else {
                    return false;
                };
                if !analyze_sink_graph(
                    function,
                    acquired,
                    uses,
                    terminator_uses,
                    visited,
                    rewrite,
                ) {
                    return false;
                }
                rewrite.replacements.insert(acquired, value);
                rewrite.neutralize.insert(*use_id);
            }
            Op::Release => {
                rewrite.neutralize.insert(*use_id);
            }
            _ => return false,
        }
    }
    true
}

/// Returns true when a local-slot immediate names concrete integer storage.
fn local_store_is_integer(function: &Function, immediate: Option<&Immediate>) -> bool {
    let Some(Immediate::LocalSlot(slot)) = immediate else {
        return false;
    };
    integer_local(function, *slot)
}

/// Checks that a local slot uses the concrete I64/PHP-int frame representation.
fn integer_local(function: &Function, slot: LocalSlotId) -> bool {
    function
        .locals
        .get(slot.as_raw() as usize)
        .is_some_and(|local| {
            local.ir_type == IrType::I64
                && matches!(local.php_type.codegen_repr(), PhpType::Int)
        })
}

/// Maps one boxed checked opcode to its allocation-free int-observing counterpart.
fn checked_to_int_op(op: Op) -> Option<Op> {
    match op {
        Op::ICheckedAdd => Some(Op::ICheckedAddToInt),
        Op::ICheckedSub => Some(Op::ICheckedSubToInt),
        Op::ICheckedMul => Some(Op::ICheckedMulToInt),
        _ => None,
    }
}

/// Commits a proven specialization, redirects scalar uses, and erases old scaffolding.
fn apply_specialization(
    function: &mut Function,
    candidate: InstId,
    result: ValueId,
    rewrite: SinkRewrite,
) {
    let replacements = resolve_chains(&rewrite.replacements);
    replace_all_uses(function, &replacements);
    for inst_id in rewrite.neutralize {
        if let Some(inst) = function.instruction_mut(inst_id) {
            neutralize_to_nop(inst);
        }
    }

    specialize_checked_producer(function, candidate)
        .expect("checked-int sink candidate retained its opcode until commit");
    if let Some(value) = function.value_mut(result) {
        value.ir_type = IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }
}

/// Rewrites one boxed checked producer into its allocation-free int-observing counterpart.
///
/// Returns `None` when the instruction is no longer a boxed checked operation, which lets
/// callers distinguish an already-specialized producer from a commit-time inconsistency.
fn specialize_checked_producer(function: &mut Function, candidate: InstId) -> Option<()> {
    let replacement_op = checked_to_int_op(function.instruction(candidate)?.op)?;
    let result = function.instruction(candidate)?.result;
    if let Some(inst) = function.instruction_mut(candidate) {
        inst.op = replacement_op;
        inst.result_type = IrType::I64;
        inst.result_php_type = PhpType::Int;
        inst.result_ownership = Ownership::NonHeap;
        inst.effects = replacement_op.default_effects();
    }
    if let Some(value) = result.and_then(|result| function.value_mut(result)) {
        value.ir_type = IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }
    Some(())
}

/// Deferred mutations collected while proving one boxed integer local can hold a raw int.
#[derive(Default)]
struct SlotRewrite {
    /// Boxed checked producers feeding the slot, rewritten to their `IChecked*ToInt` form.
    producers: Vec<InstId>,
    /// Acquire, release, and old-value load scaffolding the scalar slot no longer needs.
    neutralize: HashSet<InstId>,
    /// Acquire results redirected to the now-scalar result of their own producer.
    replacements: HashMap<ValueId, ValueId>,
}

/// Narrows every boxed `mixed` integer local whose complete access graph observes an int.
fn sink_integer_slots(function: &mut Function) -> bool {
    let candidates = narrowable_integer_slots(function);
    if candidates.is_empty() {
        return false;
    }
    let mut changed = false;
    for slot in candidates {
        // Recomputed per slot: committing one retype rewrites operands the next proof reads.
        let uses = instruction_uses(function);
        let terminator_uses = terminator_used_values(function);
        let Some(plan) = plan_slot_specialization(function, slot, &uses, &terminator_uses) else {
            continue;
        };
        apply_slot_specialization(function, slot, plan);
        changed = true;
    }
    changed
}

/// Returns true when the function holds a boxed integer local a retype could narrow.
fn has_narrowable_integer_slot(function: &Function) -> bool {
    !narrowable_integer_slots(function).is_empty()
        && function
            .instructions
            .iter()
            .any(|inst| checked_to_int_op(inst.op).is_some())
}

/// Returns the ordinary PHP locals currently stored as a boxed `mixed` cell.
///
/// Parameter slots are excluded because their representation is the calling convention, not
/// a local storage choice; every other local kind carries aliasing or lifetime rules
/// (ref cells, captures, statics, generator state) that a representation change would break.
fn narrowable_integer_slots(function: &Function) -> Vec<LocalSlotId> {
    let param_count = function.params.len();
    function
        .locals
        .iter()
        .enumerate()
        .filter(|(raw, local)| {
            *raw >= param_count
                && local.kind == LocalKind::PhpLocal
                && local.ir_type == IrType::Heap(IrHeapKind::Mixed)
        })
        .map(|(raw, _)| LocalSlotId::from_raw(raw as u32))
        .collect()
}

/// Proves every access to `slot` observes a PHP int, collecting the rewrite that follows.
///
/// Returns `None` at the first unprovable access, so a rejected slot leaves the function
/// untouched. At least one boxed producer must be removed for the retype to be worth it.
fn plan_slot_specialization(
    function: &Function,
    slot: LocalSlotId,
    uses: &HashMap<ValueId, Vec<InstId>>,
    terminator_uses: &HashSet<ValueId>,
) -> Option<SlotRewrite> {
    let mut plan = SlotRewrite::default();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !instruction_mentions_slot(inst, slot) {
            continue;
        }
        let inst_id = InstId::from_raw(raw as u32);
        match inst.op {
            Op::StoreLocal => {
                accept_slot_writer(function, inst, slot, uses, terminator_uses, &mut plan)?
            }
            Op::LoadLocal => {
                accept_slot_reader(function, inst, inst_id, uses, terminator_uses, &mut plan)?
            }
            // `unset`, ref-cell promotion, by-ref exposure, and every future slot-naming
            // opcode can observe the boxed representation, so the slot keeps it.
            _ => return None,
        }
    }
    (!plan.producers.is_empty()).then_some(plan)
}

/// Returns true when an instruction names `slot` through either slot-carrying immediate.
fn instruction_mentions_slot(inst: &Instruction, slot: LocalSlotId) -> bool {
    match inst.immediate {
        Some(Immediate::LocalSlot(named)) => named == slot,
        Some(Immediate::LocalSlotPair { first, second }) => first == slot || second == slot,
        _ => false,
    }
}

/// Accepts one `store_local` into the candidate slot.
///
/// A store is provable when it writes a value that is already a raw `I64`, or the `acquire`
/// of a boxed checked producer whose remaining readers are release scaffolding — the exact
/// shape `$i++` lowers to.
fn accept_slot_writer(
    function: &Function,
    store: &Instruction,
    slot: LocalSlotId,
    uses: &HashMap<ValueId, Vec<InstId>>,
    terminator_uses: &HashSet<ValueId>,
    plan: &mut SlotRewrite,
) -> Option<()> {
    let stored = *store.operands.first()?;
    if value_is_scalar_int(function, stored) {
        return Some(());
    }
    let acquire_id = defining_instruction_id(function, stored)?;
    let acquire = function.instruction(acquire_id)?;
    if acquire.op != Op::Acquire || acquire.immediate.is_some() {
        return None;
    }
    let produced = *acquire.operands.first()?;
    let producer_id = defining_instruction_id(function, produced)?;
    checked_to_int_op(function.instruction(producer_id)?.op)?;

    let context = ScaffoldContext { slot, acquire_id };
    accept_ownership_scaffold(function, stored, &context, uses, terminator_uses, plan)?;
    accept_ownership_scaffold(function, produced, &context, uses, terminator_uses, plan)?;

    plan.replacements.insert(stored, produced);
    plan.neutralize.insert(acquire_id);
    plan.producers.push(producer_id);
    Some(())
}

/// The one store and one acquire an ownership scaffold is allowed to feed.
struct ScaffoldContext {
    slot: LocalSlotId,
    acquire_id: InstId,
}

/// Accepts the acquire/release scaffolding around a boxed value the retype makes scalar.
///
/// Every reader must be the store into the candidate slot, the acquire being folded away, or
/// a `release` that the scalar slot renders dead. Anything else — a second store, a call, a
/// terminator — can still observe the boxed cell, so the slot keeps its representation.
fn accept_ownership_scaffold(
    function: &Function,
    value: ValueId,
    context: &ScaffoldContext,
    uses: &HashMap<ValueId, Vec<InstId>>,
    terminator_uses: &HashSet<ValueId>,
    plan: &mut SlotRewrite,
) -> Option<()> {
    if terminator_uses.contains(&value) {
        return None;
    }
    for use_id in uses.get(&value).map(Vec::as_slice).unwrap_or_default() {
        if *use_id == context.acquire_id {
            continue;
        }
        let user = function.instruction(*use_id)?;
        match user.op {
            Op::Release => {
                plan.neutralize.insert(*use_id);
            }
            Op::StoreLocal if instruction_mentions_slot(user, context.slot) => {}
            _ => return None,
        }
    }
    Some(())
}

/// Accepts one `load_local` from the candidate slot.
///
/// A read that already narrows to `I64` needs nothing: the retype only deletes the unbox its
/// coercion would have emitted. A boxed read is provable only when it exists solely to
/// release the previous value — ownership traffic a scalar slot no longer generates.
fn accept_slot_reader(
    function: &Function,
    load: &Instruction,
    load_id: InstId,
    uses: &HashMap<ValueId, Vec<InstId>>,
    terminator_uses: &HashSet<ValueId>,
    plan: &mut SlotRewrite,
) -> Option<()> {
    let result = load.result?;
    if value_is_scalar_int(function, result) {
        return Some(());
    }
    if terminator_uses.contains(&result) {
        return None;
    }
    for use_id in uses.get(&result).map(Vec::as_slice).unwrap_or_default() {
        if function.instruction(*use_id)?.op != Op::Release {
            return None;
        }
        plan.neutralize.insert(*use_id);
    }
    plan.neutralize.insert(load_id);
    Some(())
}

/// Returns true when an SSA value already carries the unboxed PHP integer representation.
fn value_is_scalar_int(function: &Function, value: ValueId) -> bool {
    function.value(value).is_some_and(|value| {
        value.ir_type == IrType::I64 && matches!(value.php_type.codegen_repr(), PhpType::Int)
    })
}

/// Returns the id of the instruction defining `value`, when it has an instruction definition.
fn defining_instruction_id(function: &Function, value: ValueId) -> Option<InstId> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    Some(inst)
}

/// Commits a proven slot retype: scalar storage, sunk producers, and no ownership traffic.
fn apply_slot_specialization(function: &mut Function, slot: LocalSlotId, plan: SlotRewrite) {
    let replacements = resolve_chains(&plan.replacements);
    replace_all_uses(function, &replacements);
    for inst_id in &plan.neutralize {
        if let Some(inst) = function.instruction_mut(*inst_id) {
            neutralize_to_nop(inst);
        }
    }
    for producer in &plan.producers {
        specialize_checked_producer(function, *producer);
    }
    if let Some(local) = function.locals.get_mut(slot.as_raw() as usize) {
        local.ir_type = IrType::I64;
        local.php_type = PhpType::Int;
    }
}
