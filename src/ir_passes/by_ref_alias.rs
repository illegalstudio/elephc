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
//!   potential address escape. Operand zero of `prop_get` and `method_call` is a
//!   value-only receiver on every lowering route; later method operands remain
//!   conservative even when they repeat that receiver. Operand one of an array/hash
//!   read, write, isset, unset, or element-address instruction is its key: PHP consumes
//!   keys by value, so only the container operand stays conservative. Terminator uses
//!   never alias.
//! - The mbstring entry points admitted by `pure_mbstring_calls` consume every string
//!   parameter by value and are listed in the runtime value-only allowlist. Their
//!   string loads may therefore become immutable and be hoisted with the call.
//! - WHY IT IS SHARED. Two passes with different jobs need the identical, safety-critical
//!   judgement; a second copy that drifted would reintroduce exactly the bug each of them
//!   fixes. `immutable_local_loads` learned this the hard way: without the exclusion it
//!   marked `$n` in `do { f($m, $n); … } while ($n > 0);` immutable, LICM hoisted the
//!   whole condition into the preheader, and the loop ran exactly once — the shape PHP's
//!   own `curl_multi_exec()` loop is written in.

use std::collections::{HashMap, HashSet};

use crate::ir::{Function, Immediate, LocalSlotId, Op, ValueId};
#[cfg(test)]
use crate::ir::{RuntimeCallTarget, RuntimeFnId};

/// Returns every local slot whose `load_local` result escapes to an instruction that may
/// take its address, i.e. every slot a callee may be able to write through a
/// by-reference alias.
pub(crate) fn address_escaping_slots(function: &Function) -> HashSet<LocalSlotId> {
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
        // A packed field store copies a scalar into the record; unlike an object property store
        // it can never bind a promoted by-reference parameter, so the operand stays a value.
        // `buffer_len` and `buffer_ptr` likewise observe a buffer handle by value through their
        // typed runtime targets; keep every other runtime call on the default-deny path.
        if op_is_value_only_consumer(inst.op)
            || function.is_packed_field_access(inst)
            || runtime_call_observes_scalar_by_value(inst)
        {
            continue;
        }
        for (index, operand) in inst.operands.iter().enumerate() {
            // Property reads and method dispatch load the receiver pointer by value, including
            // magic getters and nullable/boxed receivers. Only the receiver is exempt: a later
            // method argument can still pass this same loaded value by reference.
            if index == 0 && matches!(inst.op, Op::PropGet | Op::MethodCall) {
                continue;
            }
            // An array/hash key is always consumed by value: the container may retain or
            // normalize it, but it never binds the source local by reference. The container
            // operand (position 0) stays default-deny.
            if index == 1
                && matches!(
                    inst.op,
                    Op::ArrayGet
                        | Op::ArrayGetSilent
                        | Op::ArrayGetForWrite
                        | Op::HashGet
                        | Op::HashGetSilent
                        | Op::HashGetForWrite
                        | Op::ArrayIsset
                        | Op::HashIsset
                        | Op::ArrayElemAddr
                        | Op::ArraySet
                        | Op::HashSet
                        | Op::ArraySetMixedKey
                        | Op::ArrayGetMixedKey
                        | Op::ArrayGetMixedKeySilent
                        | Op::ArrayKeyExists
                        | Op::HashUnset
                )
            {
                continue;
            }
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
/// access, and any future opcode is treated as a possible by-reference escape so
/// the owning slot is excluded by every caller of this module. The receiver-only
/// exemptions for property reads and method calls are handled by operand position.
pub(super) fn op_is_value_only_consumer(op: Op) -> bool {
    use Op::*;
    matches!(
        op,
        // Integer/float arithmetic and bitwise operators.
        IAdd | ISub | IMul | ICheckedAdd | ICheckedSub | ICheckedMul | ICheckedAddToInt
            | ICheckedSubToInt | ICheckedMulToInt | ICheckedNumericChainToInt | ICheckedPow
            | IDiv | ISDiv | ISMod
            | IPow | INeg | IBitAnd | IBitOr | IBitXor
            | IBitNot | IShl | IShrA | FAdd | FSub | FMul | FDiv | FPow | FNeg | MixedNumericBinop
            // Comparisons.
            | ICmp | FCmp | StrEq | StrCmp | StrLooseEq | StrictEq | StrictNotEq | LooseEq
            | LooseNotEq | PhpRelCmp | Spaceship
            // Scalar predicates and type queries.
            | IsNull | IsTruthy | IsEmpty | MixedTagOf | TypePredicate
            // Numeric/string/mixed conversions.
            | IToF | FToI | IToStr | FToStr | BoolToStr | StrToI | StrToF | StrToNumber
            | ResourceToStr | Cast | MixedBox | MixedUnbox | MixedCastBool | MixedCastInt
            | MixedCastFloat | MixedCastString | ReturnBoundaryMixedToInt
            // String value operations.
            | StrConcat | StrAppend | StrLen | StrPersist | StrCharAt | StrInterpolate | StrIncDec
            // Output operations consume their operand by value.
            | EchoValue | PrintValue | WriteStdout | WriteStrStdout | VarDump | PrintR | Warn
            // Stores copy the value into other storage; they never alias the source slot.
            | StoreLocal | StoreGlobal | StoreStaticLocal | StoreStaticProperty | InitStaticLocal
            | StoreRefCell | ExternGlobalStore
            // Value-level ownership/refcount bookkeeping.
            | Acquire | Release | Move | Borrow | EnsureOwned
            // Buffer element access reads its handle and index operands by value; the stored
            // element is a scalar copied into the payload, never an alias of a local slot.
            | BufferLen | BufferGet | BufferSet | BufferResolve | BufferGetResolved
            | BufferSetResolved
    )
}

/// Returns whether a typed runtime call consumes every operand by value.
///
/// These arithmetic operations read a boxed or raw scalar and return a fresh result; none of them
/// can bind an operand by reference or retain a pointer into a local slot. Keeping them on the
/// value-only path lets `mem2reg` promote a proven integer loop counter that feeds the call.
pub(crate) fn runtime_call_observes_scalar_by_value(inst: &crate::ir::Instruction) -> bool {
    let Some(crate::ir::Immediate::RuntimeCall(
        crate::ir::RuntimeCallTarget::Function(target)
        | crate::ir::RuntimeCallTarget::ProfiledFunction { target, .. },
    )) = &inst.immediate
    else {
        return false;
    };
    matches!(
        target,
        crate::ir::RuntimeFnId::BufferLen
            | crate::ir::RuntimeFnId::BufferPtr
            | crate::ir::RuntimeFnId::Intdiv
            | crate::ir::RuntimeFnId::Abs
            | crate::ir::RuntimeFnId::Sqrt
            | crate::ir::RuntimeFnId::Round
            | crate::ir::RuntimeFnId::Sprintf
            | crate::ir::RuntimeFnId::ArraySumInt
            | crate::ir::RuntimeFnId::ArraySumProbeSmall
            | crate::ir::RuntimeFnId::ArraySumIntCached
            | crate::ir::RuntimeFnId::ArrayFilterCountMod
            | crate::ir::RuntimeFnId::ArrayMapSumAdd
            | crate::ir::RuntimeFnId::ArrayMapSumAddInt
            | crate::ir::RuntimeFnId::ArrayMapSumAddIntCached
            | crate::ir::RuntimeFnId::ArrayMapSumProbeInt
            | crate::ir::RuntimeFnId::ArrayMapSumAddSafe
            // These mbstring entry points take every string parameter by value and
            // return a fresh scalar. They are also the reads `licm` may hoist once
            // their explicit-UTF-8 arguments make them pure.
            | crate::ir::RuntimeFnId::MbStrlen
            | crate::ir::RuntimeFnId::MbStrwidth
            | crate::ir::RuntimeFnId::MbCheckEncoding
            | crate::ir::RuntimeFnId::MbSubstrCount
            | crate::ir::RuntimeFnId::MbOrdInt
            | crate::ir::RuntimeFnId::MbStrposInt
            | crate::ir::RuntimeFnId::MbStriposInt
            | crate::ir::RuntimeFnId::MbStrrposInt
            | crate::ir::RuntimeFnId::MbStrriposInt
            | crate::ir::RuntimeFnId::MbStrstrLen
            | crate::ir::RuntimeFnId::MbStristrLen
            | crate::ir::RuntimeFnId::MbStrrchrLen
            | crate::ir::RuntimeFnId::MbStrrichrLen
            | crate::ir::RuntimeFnId::MbStrimwidthLen
            | crate::ir::RuntimeFnId::MbChrLen
            | crate::ir::RuntimeFnId::MbConvertEncodingLen
            | crate::ir::RuntimeFnId::MbSplitCount
            | crate::ir::RuntimeFnId::MbEregReplaceLen
            | crate::ir::RuntimeFnId::MbDetectEncodingLen
            | crate::ir::RuntimeFnId::MbSubstr
            | crate::ir::RuntimeFnId::MbStrcut
            | crate::ir::RuntimeFnId::MbScrub
            | crate::ir::RuntimeFnId::MbConvertKana
            | crate::ir::RuntimeFnId::MbEncodeNumericentityLen
            | crate::ir::RuntimeFnId::MbDecodeNumericentityLen
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Builder, IrHeapKind, IrType, LocalKind, Ownership, Terminator};
    use crate::types::PhpType;

    /// Both buffer-length forms and `buffer_ptr` consume their loaded handles by value while an
    /// unclassified runtime call remains conservatively address-escaping.
    #[test]
    fn buffer_len_runtime_call_does_not_escape_its_slot() {
        let mut function = Function::new("runtime_calls".into(), IrType::Void, PhpType::Void);
        let buffer_type = PhpType::Buffer(Box::new(PhpType::Int));
        let len_slot = function.add_local(
            Some("len_buffer".into()),
            IrType::Heap(IrHeapKind::Buffer),
            buffer_type.clone(),
            LocalKind::PhpLocal,
        );
        let other_slot = function.add_local(
            Some("other_buffer".into()),
            IrType::Heap(IrHeapKind::Buffer),
            buffer_type.clone(),
            LocalKind::PhpLocal,
        );
        let eir_len_slot = function.add_local(
            Some("eir_len_buffer".into()),
            IrType::Heap(IrHeapKind::Buffer),
            buffer_type.clone(),
            LocalKind::PhpLocal,
        );
        let ptr_slot = function.add_local(
            Some("ptr_buffer".into()),
            IrType::Heap(IrHeapKind::Buffer),
            buffer_type.clone(),
            LocalKind::PhpLocal,
        );
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", vec![]);
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let len_handle = builder.emit_load_local(
                len_slot,
                IrType::Heap(IrHeapKind::Buffer),
                buffer_type.clone(),
            );
            builder.emit(
                Op::RuntimeCall,
                vec![len_handle],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                    RuntimeFnId::BufferLen,
                ))),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            );
            let eir_len_handle = builder.emit_load_local(
                eir_len_slot,
                IrType::Heap(IrHeapKind::Buffer),
                buffer_type.clone(),
            );
            builder.emit(
                Op::BufferLen,
                vec![eir_len_handle],
                None,
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            );
            let ptr_handle = builder.emit_load_local(
                ptr_slot,
                IrType::Heap(IrHeapKind::Buffer),
                buffer_type.clone(),
            );
            builder.emit(
                Op::RuntimeCall,
                vec![ptr_handle],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                    RuntimeFnId::BufferPtr,
                ))),
                IrType::I64,
                PhpType::Pointer(None),
                Ownership::NonHeap,
            );
            let other_handle = builder.emit_load_local(
                other_slot,
                IrType::Heap(IrHeapKind::Buffer),
                buffer_type,
            );
            builder.emit(
                Op::RuntimeCall,
                vec![other_handle],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                    RuntimeFnId::BufferFree,
                ))),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }

        let escaping = address_escaping_slots(&function);
        assert!(!escaping.contains(&len_slot));
        assert!(!escaping.contains(&eir_len_slot));
        assert!(!escaping.contains(&ptr_slot));
        assert!(escaping.contains(&other_slot));
    }
}
