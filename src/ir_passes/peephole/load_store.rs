//! Purpose:
//! Redundant load/store peephole over scalar local slots: forward a load to the
//! value the slot already holds, and drop a store that writes back the resident
//! value.
//!
//! Called from:
//! - `crate::ir_passes::peephole::Peephole::run` via `collect`.
//!
//! Key details:
//! - A per-block value-numbering tracks the SSA value resident in each tracked
//!   slot. `LoadLocal` of a slot with a known resident value folds to that value
//!   (load neutralized); `StoreLocal` of the resident value is a dead store
//!   (neutralized). State resets at block boundaries and is invalidated by any
//!   other instruction that names the slot.
//! - Forwarding is restricted to `NonHeap` values on non-escaping
//!   `PhpLocal`/`HiddenTemp`/`NamedArgTemp` slots. A slot *escapes* when its
//!   address can be taken: a load of it feeds an op that is not a pure by-value
//!   consumer (any call may pass it by reference), or it is referenced by a
//!   ref-cell promote/alias/release or invoker-ref op. Escaping slots are skipped
//!   entirely. This matters because by-reference call arguments require the value
//!   to remain a real `load_local` (the backend takes the slot's address), and a
//!   callee may mutate the slot, so a later load must not be forwarded to a
//!   pre-call value.
//! - Non-escaping plain locals are never aliased and cannot be mutated by a call,
//!   so forwarding is dominance-safe: the resident value was defined earlier in
//!   the same block and dominates the folded load and its uses.

use std::collections::{HashMap, HashSet};

use crate::ir::{Function, Immediate, Instruction, LocalKind, LocalSlotId, Op, Ownership, ValueId};

use super::Rewrites;

/// Collects scalar load-forwarding and dead-store rewrites across every block,
/// skipping any slot whose address can escape.
pub(super) fn collect(function: &Function, rewrites: &mut Rewrites) {
    let escaping = escaping_slots(function);
    for block in &function.blocks {
        // The value currently resident in each tracked scalar slot, reset per
        // block (no cross-block value flow is assumed here).
        let mut resident: HashMap<LocalSlotId, ValueId> = HashMap::new();
        for &inst_id in &block.instructions {
            let Some(inst) = function.instruction(inst_id) else {
                continue;
            };
            match inst.op {
                Op::LoadLocal => {
                    let Some(slot) = slot_of(inst) else { continue };
                    let Some(result) = inst.result else { continue };
                    if !is_tracked_slot(function, slot) || escaping.contains(&slot) {
                        continue;
                    }
                    match resident.get(&slot).copied() {
                        Some(value) if forwardable(function, value, result) => {
                            rewrites.rauw.insert(result, value);
                            rewrites.nops.push(inst_id);
                            // `value` stays resident.
                        }
                        _ => {
                            // The load result is now the slot's resident value.
                            resident.insert(slot, result);
                        }
                    }
                }
                Op::StoreLocal => {
                    let Some(slot) = slot_of(inst) else { continue };
                    let Some(&stored) = inst.operands.first() else { continue };
                    if !is_tracked_slot(function, slot) || escaping.contains(&slot) {
                        continue;
                    }
                    if resident.get(&slot) == Some(&stored) && is_non_heap(function, stored) {
                        // Storing back the value already resident: dead store.
                        rewrites.nops.push(inst_id);
                    } else {
                        resident.insert(slot, stored);
                    }
                }
                _ => {
                    // Any other instruction that names a slot invalidates it.
                    for slot in slots_of(inst) {
                        resident.remove(&slot);
                    }
                }
            }
        }
    }
}

/// Computes the set of tracked slots whose address can be taken, so they must be
/// excluded from forwarding. A slot escapes when a load of it is consumed by an
/// op that is not a pure by-value consumer (a call may pass it by reference), or
/// when it is named by a ref-cell promote/alias/release or invoker-ref op.
fn escaping_slots(function: &Function) -> HashSet<LocalSlotId> {
    let mut escaping = HashSet::new();
    for inst in &function.instructions {
        if matches!(
            inst.op,
            Op::PromoteLocalRefCell
                | Op::AliasLocalRefCell
                | Op::ReleaseLocalRefCell
                | Op::InvokerRefArg
        ) {
            for slot in slots_of(inst) {
                escaping.insert(slot);
            }
        }
        if !consumes_operands_by_value(inst.op) {
            for &operand in &inst.operands {
                if let Some(slot) = load_local_slot(function, operand) {
                    escaping.insert(slot);
                }
            }
        }
    }
    escaping
}

/// Returns the slot loaded by `value` when it is defined by a `LoadLocal` with a
/// local-slot immediate, used to detect operands that alias a local.
fn load_local_slot(function: &Function, value: ValueId) -> Option<LocalSlotId> {
    let crate::ir::ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    let inst = function.instruction(inst)?;
    if inst.op != Op::LoadLocal {
        return None;
    }
    slot_of(inst)
}

/// Returns true when an opcode consumes all its operands purely by value, so a
/// loaded local flowing into it cannot have its address taken. Conservatively
/// false for everything not certain — calls, closures/captures, constructors,
/// ref-cell ops, and container/pointer mutators — which forces the slot to be
/// treated as escaping (correctness over coverage).
fn consumes_operands_by_value(op: Op) -> bool {
    use Op::*;
    matches!(
        op,
        IAdd | ISub
            | IMul
            | IDiv
            | ISDiv
            | ISMod
            | IPow
            | INeg
            | IBitAnd
            | IBitOr
            | IBitXor
            | IBitNot
            | IShl
            | IShrA
            | FAdd
            | FSub
            | FMul
            | FDiv
            | FPow
            | FNeg
            | MixedNumericBinop
            | ICmp
            | FCmp
            | StrEq
            | StrCmp
            | StrLooseEq
            | StrictEq
            | StrictNotEq
            | LooseEq
            | LooseNotEq
            | Spaceship
            | IsNull
            | IsTruthy
            | IsEmpty
            | InstanceOf
            | InstanceOfDynamic
            | IToF
            | FToI
            | IToStr
            | FToStr
            | BoolToStr
            | StrToI
            | StrToF
            | StrToNumber
            | ResourceToStr
            | Cast
            | MixedBox
            | MixedUnbox
            | MixedTagOf
            | ArrayToMixed
            | HashToMixed
            | MixedCastBool
            | MixedCastInt
            | MixedCastFloat
            | MixedCastString
            | StrConcat
            | StrLen
            | StrCharAt
            | StrInterpolate
            | StrPersist
            | EchoValue
            | PrintValue
            | WriteStdout
            | WriteStrStdout
            | VarDump
            | PrintR
            | Warn
            | Acquire
            | Release
            | Move
            | Borrow
            | EnsureOwned
            | StoreLocal
            | StoreGlobal
            | StoreStaticLocal
            | StoreStaticProperty
            | InitStaticLocal
            | ThrowException
            | GeneratorYield
            | GeneratorYieldFrom
            | GeneratorReturn
    )
}

/// Returns the single local slot named by an instruction's immediate, if any.
fn slot_of(inst: &Instruction) -> Option<LocalSlotId> {
    match inst.immediate {
        Some(Immediate::LocalSlot(slot)) => Some(slot),
        _ => None,
    }
}

/// Returns every local slot named by an instruction's immediate (one for a
/// `LocalSlot`, two for a `LocalSlotPair`), used for barrier invalidation.
fn slots_of(inst: &Instruction) -> Vec<LocalSlotId> {
    match inst.immediate {
        Some(Immediate::LocalSlot(slot)) => vec![slot],
        Some(Immediate::LocalSlotPair { first, second }) => vec![first, second],
        _ => Vec::new(),
    }
}

/// Returns true when a slot's plain scalar storage is safe to value-number: a
/// non-aliased local kind whose by-ref forms always go through ref cells.
fn is_tracked_slot(function: &Function, slot: LocalSlotId) -> bool {
    let Some(local) = function.locals.get(slot.as_raw() as usize) else {
        return false;
    };
    matches!(
        local.kind,
        LocalKind::PhpLocal | LocalKind::HiddenTemp | LocalKind::NamedArgTemp
    )
}

/// Returns true when redirecting a load `result` to the resident `value` is a
/// pure scalar forward: both are `NonHeap` and share ir/php type.
fn forwardable(function: &Function, value: ValueId, result: ValueId) -> bool {
    let (Some(resident), Some(loaded)) = (function.value(value), function.value(result)) else {
        return false;
    };
    resident.ownership == Ownership::NonHeap
        && loaded.ownership == Ownership::NonHeap
        && resident.ir_type == loaded.ir_type
        && resident.php_type == loaded.php_type
}

/// Returns true when a value carries no heap ownership (safe to drop a store of).
fn is_non_heap(function: &Function, value: ValueId) -> bool {
    function
        .value(value)
        .map(|v| v.ownership == Ownership::NonHeap)
        .unwrap_or(false)
}
