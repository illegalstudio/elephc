//! Purpose:
//! Identifies PHP local slots whose loaded value can be reinterpreted as the slot's
//! ADDRESS — the by-reference call argument / closure capture alias — so a pass that
//! reasons about a slot's contents can fail closed for it.
//!
//! Called from:
//! - `crate::ir_passes::dead_store`, whose forward `load_local`-only liveness cannot see
//!   a store made observable through such an alias.
//! - `crate::ir_passes::immutable_local_loads`, whose "this slot never changes" proof is
//!   otherwise false for a slot a callee writes through the alias.
//!
//! Key details:
//! - HOW THE ALIAS HAPPENS. A by-reference argument is lowered as an ordinary
//!   `load_local` whose RESULT is handed to the `call`; codegen then resolves that value
//!   back to its defining `load_local` and passes the slot's ADDRESS instead of its
//!   value, so the callee reads and writes the caller's slot directly. Nothing in the IR
//!   marks the call as a store to that slot — which is exactly why every pass reasoning
//!   about slot contents has to ask this module.
//! - THE ALLOWLIST IS DEFAULT-DENY. The callee signature (which parameters are
//!   by-reference) is not available to a single-function pass, so any `load_local` result
//!   consumed by an instruction that is not a proven value-only consumer counts as a
//!   potential address escape. Terminator uses never alias and are ignored.
//! - WHY IT IS SHARED. Two passes with different jobs need the identical, safety-critical
//!   judgement; a second copy that drifted would reintroduce exactly the bug each of them
//!   fixes. `immutable_local_loads` learned this the hard way: without the exclusion it
//!   marked `$n` in `do { f($m, $n); … } while ($n > 0);` immutable, LICM hoisted the
//!   whole condition into the preheader, and the loop ran exactly once — the shape PHP's
//!   own `curl_multi_exec()` loop is written in.

use std::collections::{HashMap, HashSet};

use crate::ir::{Function, Immediate, LocalSlotId, Op, ValueId};

/// Returns every local slot whose `load_local` result escapes to an instruction that may
/// take its address, i.e. every slot a callee may be able to write through a
/// by-reference alias.
pub(super) fn address_escaping_slots(function: &Function) -> HashSet<LocalSlotId> {
    let mut load_result_slot: HashMap<ValueId, LocalSlotId> = HashMap::new();
    for inst in &function.instructions {
        if inst.op != Op::LoadLocal {
            continue;
        }
        let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
            continue;
        };
        if let Some(result) = inst.result {
            load_result_slot.insert(result, slot);
        }
    }
    let mut escaping = HashSet::new();
    if load_result_slot.is_empty() {
        return escaping;
    }
    for inst in &function.instructions {
        if op_is_value_only_consumer(inst.op) {
            continue;
        }
        for operand in &inst.operands {
            if let Some(&slot) = load_result_slot.get(operand) {
                escaping.insert(slot);
            }
        }
    }
    escaping
}

/// Returns true when an opcode consumes all its operands purely as values, so a
/// `load_local` result reaching it cannot alias the source slot.
///
/// This is an intentionally conservative allowlist (default deny): only opcodes
/// known to read operands by value are listed. Anything else — calls, object
/// construction, closure capture, ref-arg materialization, property/array/iterator
/// access, and any future opcode — is treated as a possible by-reference escape so
/// the owning slot is excluded by every caller of this module.
pub(super) fn op_is_value_only_consumer(op: Op) -> bool {
    use Op::*;
    matches!(
        op,
        // Integer/float arithmetic and bitwise operators.
        IAdd | ISub | IMul | ICheckedAdd | ICheckedSub | ICheckedMul | ICheckedAddToInt
            | ICheckedSubToInt | ICheckedMulToInt | ICheckedPow | IDiv | ISDiv | ISMod
            | IPow | INeg | IBitAnd | IBitOr | IBitXor
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
