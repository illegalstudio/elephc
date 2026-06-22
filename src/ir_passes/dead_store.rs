//! Purpose:
//! CFG-aware dead store elimination over PHP local slots. Removes `store_local`
//! instructions whose stored value is never read on any path before the slot is
//! overwritten or the function exits.
//!
//! Called from:
//! - The fixed-point pass driver in `crate::ir_passes::driver`.
//!
//! Key details:
//! - Only non-refcounted (`!php_type_needs_lifetime_tracking`) `PhpLocal` slots
//!   that are exclusively accessed through plain `load_local`/`store_local` are
//!   eligible. Assignment lowering wraps refcounted slots with separate
//!   `acquire`/`release` instructions, so dropping a refcounted store in
//!   isolation would unbalance reference counts; scalar slots carry no such
//!   ownership ops and their scope-exit cleanup is a no-op, so removing a dead
//!   scalar store is refcount-neutral and behavior-preserving.
//! - This complements the peephole pass's per-block, value-equality store
//!   forwarding: this pass is liveness-based and crosses block boundaries, so it
//!   removes a store of a *different* value whose result is never observed.
//! - Stores are neutralized to `nop` rather than physically removed, keeping the
//!   instruction/value tables stable for the validator and later passes.

use std::collections::{HashMap, HashSet};

use crate::ir::{
    DataPool, Function, Immediate, InstId, LocalKind, LocalSlotId, Op, Ownership, Value,
};

use super::cfg::successors;
use super::driver::IrPass;
use super::rewrite::neutralize_to_nop;

/// Liveness-based dead store elimination over scalar PHP local slots.
pub struct DeadStore;

impl IrPass for DeadStore {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "dead-store"
    }

    /// Neutralizes dead scalar `store_local` instructions and reports whether any
    /// store changed. The literal pool is unused because the pass never
    /// materializes new constants.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        let eligible = eligible_slots(function);
        if eligible.is_empty() {
            return false;
        }
        let dead = collect_dead_stores(function, &eligible);
        if dead.is_empty() {
            return false;
        }
        for inst_id in dead {
            if let Some(inst) = function.instruction_mut(inst_id) {
                neutralize_to_nop(inst);
            }
        }
        true
    }
}

/// Returns the set of local slots that are safe to reason about for dead store
/// elimination.
///
/// A slot qualifies when it is an ordinary `PhpLocal`, its storage type needs no
/// lifetime tracking (so no `acquire`/`release` ownership ops surround its
/// stores), and every instruction naming the slot is a plain `load_local` or
/// `store_local`. Any other slot-naming op (ref-cell promote/alias/release,
/// `unset_local`, static-local or global access, list unpack, …) makes the slot
/// ineligible because it could read or alias the slot in a way this pass does not
/// model.
fn eligible_slots(function: &Function) -> HashSet<LocalSlotId> {
    let mut eligible: HashSet<LocalSlotId> = function
        .locals
        .iter()
        .filter(|local| local.kind == LocalKind::PhpLocal)
        .filter(|local| !Ownership::php_type_needs_lifetime_tracking(&local.php_type))
        .map(|local| local.id)
        .collect();
    if eligible.is_empty() {
        return eligible;
    }

    for inst in &function.instructions {
        if matches!(inst.op, Op::LoadLocal | Op::StoreLocal) {
            continue;
        }
        for slot in immediate_slots(inst.immediate.as_ref()) {
            eligible.remove(&slot);
        }
    }
    if eligible.is_empty() {
        return eligible;
    }

    exclude_address_escaping_slots(function, &mut eligible);
    eligible
}

/// Drops slots whose loaded value can be reinterpreted as the slot's address.
///
/// A by-reference call argument or closure capture aliases the underlying slot:
/// codegen resolves the argument value back to its defining `load_local` and
/// passes the slot's address, so the callee can read or mutate the slot through
/// the alias. That makes every store to the slot observable, which this pass's
/// forward `load_local`-only liveness cannot see. Because the callee signature
/// (which parameters are by-reference) is not available to a single-function
/// pass, any `load_local` result consumed by an instruction that is not a proven
/// value-only consumer is treated as a potential address escape and the slot is
/// excluded. Terminator uses never alias, so they are ignored.
fn exclude_address_escaping_slots(function: &Function, eligible: &mut HashSet<LocalSlotId>) {
    let mut load_result_slot: HashMap<crate::ir::ValueId, LocalSlotId> = HashMap::new();
    for inst in &function.instructions {
        if inst.op != Op::LoadLocal {
            continue;
        }
        let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
            continue;
        };
        if !eligible.contains(&slot) {
            continue;
        }
        if let Some(result) = inst.result {
            load_result_slot.insert(result, slot);
        }
    }
    if load_result_slot.is_empty() {
        return;
    }

    for inst in &function.instructions {
        if op_is_value_only_consumer(inst.op) {
            continue;
        }
        for operand in &inst.operands {
            if let Some(slot) = load_result_slot.get(operand) {
                eligible.remove(slot);
            }
        }
    }
}

/// Returns true when an opcode consumes all its operands purely as values, so a
/// `load_local` result reaching it cannot alias the source slot.
///
/// This is an intentionally conservative allowlist (default deny): only opcodes
/// known to read operands by value are listed. Anything else — calls, object
/// construction, closure capture, ref-arg materialization, property/array/iterator
/// access, and any future opcode — is treated as a possible by-reference escape so
/// the owning slot is left out of dead store elimination.
fn op_is_value_only_consumer(op: Op) -> bool {
    use Op::*;
    matches!(
        op,
        // Integer/float arithmetic and bitwise operators.
        IAdd | ISub | IMul | IDiv | ISDiv | ISMod | IPow | INeg | IBitAnd | IBitOr | IBitXor
            | IBitNot | IShl | IShrA | FAdd | FSub | FMul | FDiv | FPow | FNeg | MixedNumericBinop
            // Comparisons.
            | ICmp | FCmp | StrEq | StrCmp | StrLooseEq | StrictEq | StrictNotEq | LooseEq
            | LooseNotEq | Spaceship
            // Scalar predicates and type queries.
            | IsNull | IsTruthy | IsEmpty | MixedTagOf
            // Numeric/string/mixed conversions.
            | IToF | FToI | IToStr | FToStr | BoolToStr | StrToI | StrToF | StrToNumber
            | ResourceToStr | Cast | MixedBox | MixedUnbox | MixedCastBool | MixedCastInt
            | MixedCastFloat | MixedCastString
            // String value operations.
            | StrConcat | StrLen | StrPersist | StrCharAt | StrInterpolate
            // Output operations consume their operand by value.
            | EchoValue | PrintValue | WriteStdout | WriteStrStdout | VarDump | PrintR | Warn
            // Stores copy the value into other storage; they never alias the source slot.
            | StoreLocal | StoreGlobal | StoreStaticLocal | StoreStaticProperty | InitStaticLocal
            | StoreRefCell | ExternGlobalStore
            // Value-level ownership/refcount bookkeeping.
            | Acquire | Release | Move | Borrow | EnsureOwned
    )
}

/// Returns the local slots named by an instruction immediate, covering both the
/// single-slot and slot-pair (ref-cell) immediate shapes.
fn immediate_slots(immediate: Option<&Immediate>) -> Vec<LocalSlotId> {
    match immediate {
        Some(Immediate::LocalSlot(slot)) => vec![*slot],
        Some(Immediate::LocalSlotPair { first, second }) => vec![*first, *second],
        _ => Vec::new(),
    }
}

/// Returns the eligible local slot loaded or stored by an instruction, if any.
fn instruction_slot(function: &Function, inst_id: InstId, eligible: &HashSet<LocalSlotId>) -> Option<(Op, LocalSlotId)> {
    let inst = function.instruction(inst_id)?;
    let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
        return None;
    };
    if !eligible.contains(&slot) {
        return None;
    }
    match inst.op {
        Op::LoadLocal | Op::StoreLocal => Some((inst.op, slot)),
        _ => None,
    }
}

/// Computes per-block slot live-in sets via backward dataflow to a fixed point.
///
/// A slot is live entering a block when it may be read before the next store on
/// some path. Block live-out is the union of successor live-in sets; the backward
/// walk over a block then gens a slot at each `load_local` and kills it at each
/// `store_local`. The CFG is small, so repeated full sweeps converge without an
/// explicit worklist.
fn compute_slot_live_in(
    function: &Function,
    eligible: &HashSet<LocalSlotId>,
) -> HashMap<crate::ir::BlockId, HashSet<LocalSlotId>> {
    let mut live_in: HashMap<crate::ir::BlockId, HashSet<LocalSlotId>> = function
        .blocks
        .iter()
        .map(|block| (block.id, HashSet::new()))
        .collect();

    let mut changed = true;
    while changed {
        changed = false;
        for block in function.blocks.iter().rev() {
            let mut live = block_live_out(block, &live_in);
            for &inst_id in block.instructions.iter().rev() {
                match instruction_slot(function, inst_id, eligible) {
                    Some((Op::StoreLocal, slot)) => {
                        live.remove(&slot);
                    }
                    Some((Op::LoadLocal, slot)) => {
                        live.insert(slot);
                    }
                    _ => {}
                }
            }
            if live != live_in[&block.id] {
                live_in.insert(block.id, live);
                changed = true;
            }
        }
    }
    live_in
}

/// Returns the slots live on exit from a block: the union of every successor's
/// live-in set. Terminators carry no slot uses, so only successors contribute.
fn block_live_out(
    block: &crate::ir::BasicBlock,
    live_in: &HashMap<crate::ir::BlockId, HashSet<LocalSlotId>>,
) -> HashSet<LocalSlotId> {
    let mut live = HashSet::new();
    if let Some(term) = block.terminator.as_ref() {
        for succ in successors(term) {
            if let Some(succ_live) = live_in.get(&succ) {
                live.extend(succ_live.iter().copied());
            }
        }
    }
    live
}

/// Finds every `store_local` whose stored value is dead: the slot is not live
/// immediately after the store, so the value is overwritten or the function
/// exits before any read.
fn collect_dead_stores(function: &Function, eligible: &HashSet<LocalSlotId>) -> Vec<InstId> {
    let live_in = compute_slot_live_in(function, eligible);
    let mut dead = Vec::new();
    for block in &function.blocks {
        let mut live = block_live_out(block, &live_in);
        for &inst_id in block.instructions.iter().rev() {
            match instruction_slot(function, inst_id, eligible) {
                Some((Op::StoreLocal, slot)) => {
                    if !live.contains(&slot) && store_value_is_non_heap(function, inst_id) {
                        dead.push(inst_id);
                    }
                    // The store fully overwrites the slot, so any earlier store
                    // to it (with no intervening read) is dead too.
                    live.remove(&slot);
                }
                Some((Op::LoadLocal, slot)) => {
                    live.insert(slot);
                }
                _ => {}
            }
        }
    }
    dead
}

/// Returns true when a `store_local`'s value operand has non-heap ownership.
///
/// Eligible slots are already restricted to non-lifetime-tracked storage, so the
/// stored value is expected to be a scalar; this guards against an unexpected
/// owning value reaching a scalar slot, which would make dropping the store
/// leak the value.
fn store_value_is_non_heap(function: &Function, inst_id: InstId) -> bool {
    let Some(inst) = function.instruction(inst_id) else {
        return false;
    };
    let Some(&value) = inst.operands.first() else {
        return false;
    };
    function
        .value(value)
        .map(|v: &Value| v.ownership == Ownership::NonHeap)
        .unwrap_or(false)
}
