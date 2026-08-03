//! Purpose:
//! Lowers scalar EIR instructions (the `Op` enum subset) to WebAssembly for the
//! wasm32-wasi backend: integer/float arithmetic, comparisons, conversions,
//! truthiness/null predicates, constants, and local-variable access.
//!
//! Called from:
//! - `crate::codegen_wasm::function::emit_dispatch_loop` for each instruction in a
//!   block, before the block's terminator.
//!
//! Key details:
//! - Each value-producing op loads its operands onto the WASM operand stack,
//!   computes the result, then stores it into the result value's local(s).
//! - `IDiv` is PHP `/`, which always yields a float; both i64 operands are widened
//!   with `f64.convert_i64_s` before `f64.div`.
//! - Float constants are emitted bit-exactly (`i64.const <bits>; f64.reinterpret_i64`)
//!   to avoid any float-literal formatting ambiguity.
//! - Borrow rule: `value_repr`/`slot_repr` borrow `ctx`; clone the needed strings
//!   (via `local_refs()` or `.clone()`) before calling a `&mut self` method.

use super::calls::{classify_by_ref_source, resolve_direct_call, ByRefSource};
use super::context::{FnCtx, Result};
use super::transfer;
use super::values::WasmRepr;
use super::wat::ValType;
use super::WasmError;
use crate::ir::{
    CmpPredicate, DataId, Immediate, InstId, Instruction, IrHeapKind, IrType, LocalSlotId,
    MixedNumericOp, Op, Ownership, RuntimeCallTarget, RuntimeFnId, ValueDef, ValueId,
};
use crate::types::PhpType;
use std::collections::HashMap;

/// Lowers one EIR instruction by id. Loads operands, computes the result on the
/// WASM operand stack, and stores it into the result value's local(s). Unsupported
/// ops return `WasmError::Unsupported` so the pipeline can surface a clean diagnostic.
pub(super) fn lower_instruction(ctx: &mut FnCtx, inst_id: InstId) -> Result<()> {
    // Clone the instruction so we can mutate ctx.fb without holding a borrow on ctx.function.
    let inst = ctx
        .function
        .instruction(inst_id)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("missing instruction {:?}", inst_id)))?;

    match inst.op {
        Op::ConstI64 => lower_const_i64(ctx, &inst),
        Op::ConstF64 => lower_const_f64(ctx, &inst),
        Op::ConstBool => lower_const_bool(ctx, &inst),
        Op::ConstNull => lower_const_null(ctx, &inst),
        Op::ConstStr => lower_const_str(ctx, &inst),
        Op::StrLen => lower_strlen(ctx, &inst),
        Op::StrPersist => lower_str_persist(ctx, &inst),
        Op::ArrayToMixed => lower_array_to_mixed(ctx, &inst),
        Op::LooseEq | Op::LooseNotEq => lower_loose_eq(ctx, &inst),
        Op::StrConcat => lower_str_concat(ctx, &inst),
        Op::Nop => lower_nop(ctx),
        Op::ConcatReset => lower_concat_reset(ctx),
        Op::LoadLocal => lower_load_local(ctx, &inst),
        Op::StoreLocal => lower_store_local(ctx, &inst),
        Op::UnsetLocal => lower_unset_local(ctx, &inst),
        Op::IAdd => lower_int_binop(ctx, &inst, "i64.add"),
        Op::ISub => lower_int_binop(ctx, &inst, "i64.sub"),
        Op::IMul => lower_int_binop(ctx, &inst, "i64.mul"),
        Op::ICheckedAdd => lower_checked_int_binop(ctx, &inst, "add"),
        Op::ICheckedSub => lower_checked_int_binop(ctx, &inst, "sub"),
        Op::ICheckedMul => lower_checked_int_binop(ctx, &inst, "mul"),
        Op::MixedNumericBinop => lower_mixed_numeric_binop(ctx, &inst),
        Op::TryPushHandler => lower_try_push_handler(ctx, &inst),
        Op::TryPopHandler => lower_try_pop_handler(ctx, &inst),
        Op::ThrowException | Op::ThrowErrorValue => lower_throw(ctx, &inst),
        Op::CatchCurrent => lower_catch_current(ctx, &inst),
        Op::CatchBind => lower_catch_bind(ctx, &inst),
        Op::IBitAnd => lower_int_binop(ctx, &inst, "i64.and"),
        Op::IBitOr => lower_int_binop(ctx, &inst, "i64.or"),
        Op::IBitXor => lower_int_binop(ctx, &inst, "i64.xor"),
        Op::IShl => lower_int_shift(ctx, &inst, true),
        Op::IShrA => lower_int_shift(ctx, &inst, false),
        Op::ISDiv => lower_signed_int_div(ctx, &inst),
        Op::ISMod => lower_signed_int_mod(ctx, &inst),
        Op::INeg => lower_int_neg(ctx, &inst),
        Op::IBitNot => lower_int_bitnot(ctx, &inst),
        Op::IDiv => lower_int_div_to_float(ctx, &inst),
        Op::FAdd => lower_float_binop(ctx, &inst, "f64.add"),
        Op::FSub => lower_float_binop(ctx, &inst, "f64.sub"),
        Op::FMul => lower_float_binop(ctx, &inst, "f64.mul"),
        Op::FDiv => lower_float_div(ctx, &inst),
        Op::FNeg => lower_float_neg(ctx, &inst),
        Op::ICmp => lower_int_cmp(ctx, &inst),
        Op::FCmp => lower_float_cmp(ctx, &inst),
        Op::StrictEq | Op::StrictNotEq => super::strict::lower_strict_compare(ctx, &inst),
        Op::IToF => lower_itof(ctx, &inst),
        Op::IToStr => lower_int_like_to_string(ctx, &inst),
        Op::FToI => lower_ftoi(ctx, &inst),
        Op::Cast => lower_cast(ctx, &inst),
        Op::IsTruthy => lower_is_truthy(ctx, &inst),
        Op::IsNull => lower_is_null(ctx, &inst),
        Op::Call => lower_call(ctx, &inst),
        Op::LoadGlobal => lower_load_global(ctx, &inst),
        Op::RuntimeCall => lower_runtime_call(ctx, &inst),
        Op::LanguageConstructCall => lower_language_construct_call(ctx, &inst),
        Op::EchoValue | Op::PrintValue => lower_echo(ctx, &inst),
        Op::Warn => lower_array_offset_on_null_warning(ctx, &inst),
        Op::ThrowError => lower_method_call_on_null_error(ctx, &inst),
        Op::Acquire => lower_acquire(ctx, &inst),
        Op::Release => lower_release(ctx, &inst),
        Op::Move | Op::Borrow => lower_forward(ctx, &inst),
        Op::ArrayNew => lower_array_new(ctx, &inst),
        Op::ArrayLen => lower_array_len(ctx, &inst),
        Op::ArrayGet | Op::ArrayGetSilent => lower_array_get(ctx, &inst),
        Op::ArrayPush => lower_array_push(ctx, &inst),
        Op::ArraySet => lower_array_set(ctx, &inst),
        Op::ArrayToHash => super::inst_hash::lower_array_to_hash(ctx, &inst),
        Op::HashNew => super::inst_hash::lower_hash_new(ctx, &inst),
        Op::HashGet | Op::HashGetSilent => super::inst_hash::lower_hash_get(ctx, &inst),
        Op::HashSet => super::inst_hash::lower_hash_set(ctx, &inst),
        Op::HashUnset => super::inst_hash::lower_hash_unset(ctx, &inst),
        Op::HashIsset => super::inst_hash::lower_hash_isset(ctx, &inst),
        Op::GcCollect => lower_gc_collect(ctx),
        Op::LoadStaticProperty => lower_load_static_property(ctx, &inst),
        Op::StoreStaticProperty => lower_store_static_property(ctx, &inst),
        Op::ScopedConstantGet => lower_scoped_constant_get(ctx, &inst),
        Op::HashAppend => super::inst_hash::lower_hash_append(ctx, &inst),
        Op::HashUnion => super::inst_hash::lower_hash_union(ctx, &inst),
        Op::ArrayUnion => super::inst_hash::lower_array_union(ctx, &inst),
        Op::ArrayHashUnion => super::inst_hash::lower_array_hash_union(ctx, &inst),
        Op::HashArrayUnion => super::inst_hash::lower_hash_array_union(ctx, &inst),
        Op::MixedBox => lower_mixed_box(ctx, &inst),
        Op::MixedTagOf => lower_mixed_tag_of(ctx, &inst),
        Op::IterStart => lower_iter_start(ctx, &inst),
        Op::IterNext => lower_iter_next(ctx, &inst),
        Op::IterCurrentKey => lower_iter_current_key(ctx, &inst),
        Op::IterCurrentValue => lower_iter_current_value(ctx, &inst),
        Op::IterEnd => Ok(()),
        Op::ObjectNew => super::objects::lower_object_new(ctx, &inst),
        Op::PropGet => super::objects::lower_prop_get(ctx, &inst),
        Op::PropSet => super::objects::lower_prop_set(ctx, &inst),
        Op::MethodCall => super::methods::lower_method_call(ctx, &inst),
        Op::StaticMethodCall => super::methods::lower_static_method_call(ctx, &inst),
        Op::NullsafeMethodCall => super::methods::lower_nullsafe_method_call(ctx, &inst),
        Op::NullsafePropGet => super::objects::lower_nullsafe_prop_get(ctx, &inst),
        Op::InstanceOf => super::classes::lower_instanceof(ctx, &inst),
        Op::InstanceOfDynamic => super::classes::lower_instanceof_dynamic(ctx, &inst),
        Op::ClosureNew => super::closures::lower_closure_new(ctx, &inst),
        Op::ClosureCall => super::closures::lower_closure_call(ctx, &inst),
        Op::ClosureCapture => super::closures::lower_closure_capture(ctx, &inst),
        Op::FirstClassCallableNew => super::closures::lower_first_class_callable_new(ctx, &inst),
        Op::CallableDescriptorInvoke => {
            super::closures::lower_callable_descriptor_invoke(ctx, &inst)
        }
        Op::LoadRefCell => super::refcell::lower_load_ref_cell(ctx, &inst),
        Op::StoreRefCell => super::refcell::lower_store_ref_cell(ctx, &inst),
        Op::PromoteLocalRefCell => super::refcell::lower_promote_local_ref_cell(ctx, &inst),
        Op::AliasLocalRefCell => super::refcell::lower_alias_local_ref_cell(ctx, &inst),
        Op::ReleaseLocalRefCell => super::refcell::lower_release_local_ref_cell(ctx, &inst),
        Op::IterCurrentValueRef => super::refcell::lower_iter_current_value_ref(ctx, &inst),
        other => Err(WasmError::Unsupported(format!("op {:?}", other))),
    }
}

/// Stores the instruction's result into its value local(s), if it produces one.
pub(super) fn store_result(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    if let Some(r) = inst.result {
        ctx.emit_store_value(r)?;
    }
    Ok(())
}

/// Returns the i-th operand of the instruction, or an error if missing.
pub(super) fn operand(inst: &Instruction, i: usize) -> Result<ValueId> {
    inst.operands
        .get(i)
        .copied()
        .ok_or_else(|| WasmError::Unsupported(format!("missing operand {} in {:?}", i, inst.op)))
}

/// Extracts a `CmpPredicate` from the instruction's immediate, or an error.
fn cmp_immediate(inst: &Instruction) -> Result<CmpPredicate> {
    match &inst.immediate {
        Some(Immediate::CmpPredicate(pred)) => Ok(*pred),
        _ => Err(WasmError::Unsupported(format!(
            "missing CmpPredicate in {:?}",
            inst.op
        ))),
    }
}

/// Extracts an i64 from the instruction's immediate, or an error.
fn i64_immediate(inst: &Instruction) -> Result<i64> {
    match &inst.immediate {
        Some(Immediate::I64(n)) => Ok(*n),
        _ => Err(WasmError::Unsupported(format!(
            "missing i64 immediate in {:?}",
            inst.op
        ))),
    }
}

/// Extracts an f64 from the instruction's immediate, or an error.
fn f64_immediate(inst: &Instruction) -> Result<f64> {
    match &inst.immediate {
        Some(Immediate::F64(f)) => Ok(*f),
        _ => Err(WasmError::Unsupported(format!(
            "missing f64 immediate in {:?}",
            inst.op
        ))),
    }
}

/// Extracts a bool from the instruction's immediate, or an error.
fn bool_immediate(inst: &Instruction) -> Result<bool> {
    match &inst.immediate {
        Some(Immediate::Bool(b)) => Ok(*b),
        _ => Err(WasmError::Unsupported(format!(
            "missing bool immediate in {:?}",
            inst.op
        ))),
    }
}

/// Extracts a `LocalSlotId` from the instruction's immediate, or an error.
fn slot_immediate(inst: &Instruction) -> Result<LocalSlotId> {
    match &inst.immediate {
        Some(Immediate::LocalSlot(slot)) => Ok(*slot),
        _ => Err(WasmError::Unsupported(format!(
            "missing LocalSlot immediate in {:?}",
            inst.op
        ))),
    }
}

/// Extracts a `DataId` from an ordinary or source-profiled immediate.
///
/// The WASM backend currently supports profiled data only on forms whose
/// admitted semantics do not vary with strict-PHP visibility. Unsupported
/// source-sensitive forms are rejected by the capability audit.
pub(super) fn data_immediate(inst: &Instruction) -> Result<DataId> {
    match &inst.immediate {
        Some(Immediate::Data(d)) => Ok(*d),
        Some(Immediate::ProfiledData { data, .. }) => Ok(*data),
        _ => Err(WasmError::Unsupported(format!(
            "missing Data immediate in {:?}",
            inst.op
        ))),
    }
}

/// Lowers the capability-proven no-argument offset-on-null warning.
fn lower_array_offset_on_null_warning(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    if !inst.operands.is_empty()
        || inst.result.is_some()
        || inst.result_type != IrType::Void
        || inst.result_php_type.codegen_repr() != PhpType::Void
        || inst.result_ownership != Ownership::NonHeap
    {
        return Err(WasmError::Unsupported(
            "invalid array-offset-on-null Warn shape".to_string(),
        ));
    }
    let data = data_immediate(inst)?;
    let message = ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .ok_or_else(|| {
            WasmError::Unsupported(format!(
                "array-offset-on-null warning: unknown data {:?}",
                data
            ))
        })?;
    if message != crate::codegen_support::runtime::array_offset_on_null_warning() {
        return Err(WasmError::Unsupported(format!(
            "unsupported static Warn message {message:?}"
        )));
    }
    ctx.fb.ins(
        "call $__rt_warn_array_offset_on_null",
        "emit PHP array-offset-on-null warning",
    );
    Ok(())
}

const METHOD_CALL_ON_NULL_PREFIX: &str = "Call to a member function ";
const METHOD_CALL_ON_NULL_SUFFIX: &str = "() on null";

/// Returns the byte range of the method name in the exact static error emitted
/// for an ordinary method call whose nullable receiver resolved to `null`.
///
/// The WASM backend intentionally accepts only this `ThrowError` form. General
/// catchable `Error` support remains behind the capability gate until WASM
/// exception handlers preserve PHP `try`/`catch` semantics.
pub(super) fn method_call_on_null_name_range(message: &str) -> Option<(usize, usize)> {
    let method = message
        .strip_prefix(METHOD_CALL_ON_NULL_PREFIX)?
        .strip_suffix(METHOD_CALL_ON_NULL_SUFFIX)?;
    if method.is_empty() {
        return None;
    }
    Some((METHOD_CALL_ON_NULL_PREFIX.len(), method.len()))
}

/// Lowers the capability-proven method-on-null `ThrowError` through the existing
/// PHP fatal helper.
///
/// Capability validation proves the module has no admitted catch surface and
/// that the following EIR terminator is `Unreachable`. This instruction emits
/// only the non-returning call; terminator lowering emits the classified Core
/// `unreachable` immediately after it.
fn lower_method_call_on_null_error(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    if !inst.operands.is_empty()
        || inst.result.is_some()
        || inst.result_type != IrType::Void
        || inst.result_php_type.codegen_repr() != PhpType::Void
        || inst.result_ownership != Ownership::NonHeap
    {
        return Err(WasmError::Unsupported(
            "invalid method-on-null ThrowError shape".to_string(),
        ));
    }
    let data = data_immediate(inst)?;
    let message = ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .ok_or_else(|| {
            WasmError::Unsupported(format!(
                "method-on-null error: unknown data {:?}",
                data
            ))
        })?;
    let (method_offset, method_len) =
        method_call_on_null_name_range(message).ok_or_else(|| {
            WasmError::Unsupported(format!(
                "unsupported static ThrowError message {message:?}"
            ))
        })?;
    let (message_ptr, _) = ctx.str_literal(data)?;
    ctx.fb.ins(
        &format!("i32.const {}", message_ptr + method_offset as u32),
        "method-name pointer inside static error message",
    );
    ctx.fb.ins(
        &format!("i32.const {}", method_len),
        "method-name byte length",
    );
    ctx.fb.ins("i32.const 8", "Mixed null runtime tag");
    ctx.fb.ins(
        "call $__rt_fail_method_call_non_object",
        "raise PHP fatal for method call on null",
    );
    Ok(())
}

/// Lowers `Op::Call` to a direct WebAssembly call of a user function.
///
/// The callee is named by an `Immediate::Data` index into the module's function-name
/// pool. Non-by-ref arguments are pushed in source order (matching the callee's value
/// parameter locals). By-ref free-function parameters (P7c0b) are materialized
/// backend-side into a 16-byte ref cell whose pointer is passed as the callee's single
/// i32 parameter, then the cell's final value is written back into the caller's local
/// after the call. This mirrors the native `materialize_ref_arg_address` architecture
/// (backend-side temp cell + writeback) and leaves EIR and native untouched.
///
/// By-ref arg materialization (caller side), per operand:
/// - Already-ref-bound operand (`LoadRefCell(slot)`): the caller's local is already
///   cell-backed (from a prior `=&`/foreach), so its existing cell pointer is passed and
///   shared with the callee — no temp cell, no writeback, no free (the caller's owner
///   epilogue releases the cell).
/// - Fresh local (`LoadLocal(slot)`): a temp cell is heap-allocated and the slot's value
///   is retained into it (persist for strings, incref for refcounted containers and
///   callable descriptors, plain store for scalars/tagged), then the cell pointer is
///   passed. After the call the cell's final value is acquired into the slot and the
///   cell is freed. Cells are grouped by source slot so `f(&$x, &$x)` shares one cell
///   (PHP aliasing, including cross-param reads).
/// - Any other operand (literals, property reads, temporaries) is rejected with a clean
///   diagnostic (non-local by-ref deferred).
fn lower_call(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let target = resolve_direct_call(ctx.module, inst)?;
    let symbol = target.symbol;
    let name = target.name.to_string();
    let params: Vec<(IrType, PhpType, bool, bool)> = target
        .function
        .params
        .iter()
        .map(|param| {
            (
                param.ir_type,
                param.php_type.codegen_repr(),
                param.by_ref,
                param.variadic,
            )
        })
        .collect();
    let callee_return_type = target.function.return_type;
    let callee_return_php = target.function.return_php_type.codegen_repr();
    // A variadic parameter the EIR already packed is an ordinary `array<T>` argument — the
    // call site built the array — so only an UNPACKED one is outside the contract.
    let packed_variadic = params
        .iter()
        .filter(|(_, _, _, variadic)| *variadic)
        .all(|(ir, _, _, _)| matches!(ir, IrType::Heap(IrHeapKind::Array)))
        && inst.operands.len() == params.len();
    if !packed_variadic && params.iter().any(|(_, _, _, variadic)| *variadic) {
        return Err(WasmError::Unsupported(format!(
            "variadic direct call target {name:?} is outside the wasm32-wasi L1 call contract"
        )));
    }
    if params.len() != inst.operands.len() {
        return Err(WasmError::Unsupported(format!(
            "direct call target {name:?} expects {} lowered operands, got {}",
            params.len(),
            inst.operands.len()
        )));
    }

    // Pre-call pass: push each argument in the callee parameter's representation.
    // By-ref params materialize a cell pointer (a temp cell for a fresh local, the shared
    // pointer for an already-ref-bound local); all other args go through the typed
    // transfer layer so concrete values are boxed for Mixed/Union/Iterable parameters and
    // Mixed cells are unboxed for concrete parameters.
    let mut temp_cells: Vec<TempCell> = Vec::new();
    let mut slot_to_cell: HashMap<u32, usize> = HashMap::new();
    let mut boxed_args: Vec<(String, IrType)> = Vec::new();
    for (&arg, (param_ir, param_php, by_ref, _)) in inst.operands.iter().zip(&params) {
        if *by_ref {
            push_by_ref_arg(ctx, arg, &mut temp_cells, &mut slot_to_cell)?;
        } else {
            let synthesized =
                transfer::emit_push_call_argument(ctx, arg, *param_ir, param_php.clone())?;
            if let Some(cell) = synthesized {
                boxed_args.push(cell);
            } else if matches!(*param_ir, IrType::Heap(IrHeapKind::Array)) {
                // The callee OWNS its array parameter and releases it at every exit, which is
                // what gives PHP by-value semantics: a mutation inside sees two owners and copies
                // on write. So the caller lends a counted reference here and never takes it back
                // — `__rt_array_ensure_unique` consumes it when the callee does copy.
                let array = ctx.fresh_temp(ValType::I32);
                ctx.fb.ins(&format!("local.tee {}", array), "array argument");
                ctx.fb.ins(
                    &format!("(call $__rt_incref (local.get {}))", array),
                    "the callee owns its array parameter",
                );
            }
        }
    }

    ctx.fb
        .ins(&format!("call ${}", symbol), &format!("call {}", name));

    if let Some(_r) = inst.result {
        transfer::emit_store_call_result(ctx, &inst, callee_return_type, callee_return_php)?;
    } else {
        let return_arity = WasmRepr::val_types(callee_return_type).len();
        for _ in 0..return_arity {
            ctx.fb.ins("drop", "discard unused call result");
        }
    }

    // Post-call pass: write each temp cell's final value back into its source slot and
    // free the cell. Refcount-balanced for both read-only and mutated cases (see
    // `writeback_temp_cell`).
    for cell in &temp_cells {
        writeback_temp_cell(ctx, cell)?;
    }

    // Release each heap value the argument conversion allocated — a boxed Mixed cell, or an
    // array widened to `array<mixed>`. EIR asked for neither and so emits no matching release:
    // they exist only because this target specializes parameter layouts, and the callee borrows
    // them. Skipping this leaks one per call — silently, since nothing on the output path
    // observes them.
    //
    // Measured escape routes for a borrowed value: copying it into a callee local and handing it
    // to a further call are both safe (neither takes ownership), and pushing it into a container
    // increfs. RETURNING it is not — `Terminator::Return` MOVES the value out without increfing,
    // so a callee that can hand it back keeps it. The withholding is per-temporary and matched on
    // KIND, since only a return of the same kind can alias.
    release_boxed_arguments(ctx, &boxed_args, callee_return_type);

    Ok(())
}

/// Frees the heap values synthesized for a call's arguments, after the call has returned.
///
/// Skips any temporary the callee's declared return could BE: a returned value moves out without
/// an incref, so freeing it here would leave the caller holding a dead pointer.
fn release_boxed_arguments(
    ctx: &mut FnCtx,
    boxed_args: &[(String, IrType)],
    callee_return_type: IrType,
) {
    for (temp, kind) in boxed_args {
        let may_be_returned = match kind {
            IrType::Heap(IrHeapKind::Mixed) => matches!(
                callee_return_type,
                IrType::Heap(IrHeapKind::Mixed) | IrType::Heap(IrHeapKind::Union)
            ),
            other => callee_return_type == *other,
        };
        if may_be_returned {
            continue;
        }
        ctx.fb.ins(
            &format!("(call $__rt_decref_any (local.get {}))", temp),
            "free the heap value synthesized for this argument",
        );
    }
}

/// A temp ref cell synthesized for a by-ref argument whose source is a fresh local.
///
/// One cell per unique source slot (grouped, so `f(&$x, &$x)` shares it). The cell holds
/// a retained copy of the slot's pre-call value; after the call `writeback_temp_cell`
/// acquires the cell's final value into the slot and releases the cell.
struct TempCell {
    /// The source slot raw id (the caller's local that the cell mirrors).
    slot_raw: u32,
    /// The i32 local holding the 16-byte cell pointer.
    ptr_local: String,
}

/// Returns the `codegen_repr` payload `PhpType` of a local slot.
///
/// Drives the retain kind (Callable special-case) and the cell's payload release in
/// `emit_ref_cell_release_seq` (`needs_payload_release`).
pub(super) fn slot_payload_type(ctx: &FnCtx, slot: LocalSlotId) -> Result<PhpType> {
    let local = ctx
        .function
        .locals
        .get(slot.as_raw() as usize)
        .ok_or_else(|| WasmError::Unsupported(format!("slot {:?} has no local metadata", slot)))?;
    Ok(local.php_type.codegen_repr())
}

/// Pushes one by-ref argument's cell pointer onto the WASM operand stack.
///
/// For an already-ref-bound local the existing cell pointer is reused (no temp cell, no
/// writeback). For a fresh local a temp cell is synthesized and recorded, grouped by
/// source slot so repeated occurrences of the same slot share one cell (PHP aliasing,
/// including cross-param reads). Non-local operands are rejected.
fn push_by_ref_arg(
    ctx: &mut FnCtx,
    arg: ValueId,
    temp_cells: &mut Vec<TempCell>,
    slot_to_cell: &mut HashMap<u32, usize>,
) -> Result<()> {
    match classify_by_ref_source(ctx.function, arg) {
        ByRefSource::AlreadyRefBound(slot_raw) => {
            let ptr = ctx.ref_cell_ptr(slot_raw)?.to_string();
            ctx.fb.ins(
                &format!("local.get {}", ptr),
                "by-ref arg: existing ref-cell pointer",
            );
        }
        ByRefSource::FreshLocal(slot) => {
            let slot_raw = slot.as_raw();
            if let Some(&idx) = slot_to_cell.get(&slot_raw) {
                ctx.fb.ins(
                    &format!("local.get {}", temp_cells[idx].ptr_local),
                    "by-ref arg: shared temp cell (slot grouping)",
                );
            } else {
                let cell = synthesize_temp_cell(ctx, slot)?;
                ctx.fb.ins(
                    &format!("local.get {}", cell.ptr_local),
                    "by-ref arg: temp cell pointer",
                );
                slot_to_cell.insert(slot_raw, temp_cells.len());
                temp_cells.push(cell);
            }
        }
        ByRefSource::NonLocal => {
            return Err(WasmError::Unsupported(
                "by-ref arg is not a local (P7c0b non-local by-ref deferred)".to_string(),
            ));
        }
    }
    Ok(())
}

/// Synthesizes a temp ref cell mirroring a fresh local's current value.
///
/// Allocates a 16-byte cell and retains the slot's value into it (persist for strings,
/// incref for refcounted containers and callable descriptors, plain store for scalars
/// and tagged values), reusing the promote path's `retain_and_store_slot_value`. The
/// slot's own locals are left untouched (read-only): the writeback later releases the
/// slot's old value, so the slot and the cell each hold an independent reference.
fn synthesize_temp_cell(ctx: &mut FnCtx, slot: LocalSlotId) -> Result<TempCell> {
    let slot_repr = ctx.slot_repr(slot)?.clone();
    let payload = slot_payload_type(ctx, slot)?;
    let ptr_local = ctx.fresh_temp(ValType::I32);
    super::refcell::emit_ref_cell_allocation(ctx, &ptr_local);
    super::refcell::retain_and_store_slot_value(ctx, &ptr_local, &slot_repr, &payload)?;
    Ok(TempCell {
        slot_raw: slot.as_raw(),
        ptr_local,
    })
}

/// Writes a temp cell's final value back into its source slot and frees the cell.
///
/// Per-slot sequence (refcount-balanced for both read-only and mutated cases):
/// 1. Load the cell's final value (the callee may have mutated it) and retain an owned
///    copy — persist for strings, incref for refcounted containers and callable
///    descriptors, no-op for scalars/tagged — so the slot owns a fresh reference.
/// 2. Release the slot's old (pre-call) value (`release_old_slot_value`), which reads the
///    slot's still-untouched locals.
/// 3. Store the retained value into the slot's locals.
/// 4. Release the cell (`emit_ref_cell_release_seq`: decref the cell's payload by kind,
///    free the 16-byte block).
///
/// Refcount trace (refcounted container, in-place mutation): synth increfs V (R+1,
/// cell+S); writeback increfs V (R+2), releases S old (R+1), stores (S owns 1), releases
/// cell (R). Net: S owns V, refcount R restored. Replacement case: the callee's
/// `store_local` releases the cell's old V and moves its new value into the cell;
/// writeback increfs the new value, releases S's old V (freed at zero), stores the new
/// value, releases the cell (S owns it). Strings use copy-on-acquire (persist to own,
/// `__rt_heap_free_safe` to release) since the runtime frees kind-1 blocks
/// unconditionally, so the slot must own its own copy rather than share the cell's
/// pointer. Scalars carry no refcount; steps 1/2 are no-ops and only the bits move.
fn writeback_temp_cell(ctx: &mut FnCtx, cell: &TempCell) -> Result<()> {
    let ptr_local = cell.ptr_local.clone();
    let slot = LocalSlotId::from_raw(cell.slot_raw);
    let payload = slot_payload_type(ctx, slot)?;
    let slot_repr = ctx.slot_repr(slot)?.clone();

    // Steps 1-3: load + retain + release-old + store, per representation. The release-old
    // runs before the store so the retained copy is not freed as the "old" value.
    match &slot_repr {
        WasmRepr::I64(slot_local) => {
            let tmp = ctx.fresh_temp(ValType::I64);
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("i64.load offset=0", "load final value @ cell+0");
            ctx.fb.ins(&format!("local.set {}", tmp), "capture final value");
            if payload == PhpType::Callable {
                ctx.fb.ins(&format!("local.get {}", tmp), "descriptor to retain");
                ctx.fb.ins("i32.wrap_i64", "narrow the descriptor pointer to i32");
                ctx.fb.ins("call $__rt_incref", "retain the descriptor for the slot");
            }
            super::refcell::release_old_slot_value(ctx, &slot_repr, &payload)?;
            ctx.fb.ins(&format!("local.get {}", tmp), "retained value");
            ctx.fb.ins(&format!("local.set {}", slot_local), "store into the slot");
        }
        WasmRepr::F64(slot_local) => {
            let tmp = ctx.fresh_temp(ValType::F64);
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("f64.load offset=0", "load final float @ cell+0");
            ctx.fb.ins(&format!("local.set {}", tmp), "capture final float");
            super::refcell::release_old_slot_value(ctx, &slot_repr, &payload)?;
            ctx.fb.ins(&format!("local.get {}", tmp), "retained float");
            ctx.fb.ins(&format!("local.set {}", slot_local), "store into the slot");
        }
        WasmRepr::Ptr(slot_local) => {
            let tmp = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("i32.load offset=0", "load final pointer @ cell+0");
            ctx.fb.ins(&format!("local.set {}", tmp), "capture final pointer");
            ctx.fb.ins(&format!("local.get {}", tmp), "container to retain");
            ctx.fb.ins("call $__rt_incref", "retain the container for the slot");
            super::refcell::release_old_slot_value(ctx, &slot_repr, &payload)?;
            ctx.fb.ins(&format!("local.get {}", tmp), "retained pointer");
            ctx.fb.ins(&format!("local.set {}", slot_local), "store into the slot");
        }
        WasmRepr::Str { ptr, len } => {
            let tmp_ptr = ctx.fresh_temp(ValType::I32);
            let tmp_len = ctx.fresh_temp(ValType::I64);
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("i32.load offset=0", "load final string ptr @ cell+0");
            ctx.fb.ins(&format!("local.set {}", tmp_ptr), "capture final string ptr");
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("i64.load offset=8", "load final length @ cell+8");
            ctx.fb.ins(&format!("local.set {}", tmp_len), "capture final length");
            // Retain via persist: an owned heap copy safe for the slot (strings use
            // copy-on-acquire; the runtime frees kind-1 blocks unconditionally, so the
            // slot must own its own copy rather than share the cell's pointer).
            ctx.fb.ins(&format!("local.get {}", tmp_ptr), "source string pointer");
            ctx.fb.ins(&format!("local.get {}", tmp_len), "source string length");
            ctx.fb.ins("call $__rt_str_persist", "persist an owned copy for the slot");
            let new_len = ctx.fresh_temp(ValType::I64);
            let new_ptr = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {}", new_len), "owned string length");
            ctx.fb.ins(&format!("local.set {}", new_ptr), "owned string pointer");
            super::refcell::release_old_slot_value(ctx, &slot_repr, &payload)?;
            ctx.fb.ins(&format!("local.get {}", new_ptr), "owned string pointer");
            ctx.fb.ins(&format!("local.set {}", ptr), "store ptr into the slot");
            ctx.fb.ins(&format!("local.get {}", new_len), "owned string length");
            ctx.fb.ins(&format!("local.set {}", len), "store len into the slot");
        }
        WasmRepr::Tagged {
            payload: pay_local,
            tag: tag_local,
        } => {
            let tmp_pay = ctx.fresh_temp(ValType::I64);
            let tmp_tag = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("i64.load offset=0", "load final payload @ cell+0");
            ctx.fb.ins(&format!("local.set {}", tmp_pay), "capture final payload");
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("i64.load offset=8", "load final tag @ cell+8");
            ctx.fb.ins("i32.wrap_i64", "narrow the tag to i32");
            ctx.fb.ins(&format!("local.set {}", tmp_tag), "capture final tag");
            super::refcell::release_old_slot_value(ctx, &slot_repr, &payload)?;
            ctx.fb.ins(&format!("local.get {}", tmp_pay), "retained payload");
            ctx.fb.ins(&format!("local.set {}", pay_local), "store payload into the slot");
            ctx.fb.ins(&format!("local.get {}", tmp_tag), "retained tag");
            ctx.fb.ins(&format!("local.set {}", tag_local), "store tag into the slot");
        }
        WasmRepr::Void => {
            return Err(WasmError::Unsupported("by-ref void slot".to_string()));
        }
    }

    // Step 4: release the cell (payload by kind + free the 16-byte block).
    super::refcell::emit_ref_cell_release_seq(ctx, &ptr_local, &payload)?;
    Ok(())
}

/// Lowers an integer binary op: load both operands, emit the wasm op, store result.
fn lower_int_binop(ctx: &mut FnCtx, inst: &Instruction, wasm_op: &str) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(wasm_op, "integer binary op");
    store_result(ctx, inst)
}

/// Lowers PHP integer shifts without inheriting WebAssembly's masked shift count.
///
/// PHP rejects a negative count, returns zero for left shifts by 64 or more, and
/// sign-fills right shifts by 64 or more. WebAssembly instead masks an i64 shift
/// count modulo 64, so both boundary cases are emitted explicitly.
fn lower_int_shift(ctx: &mut FnCtx, inst: &Instruction, left: bool) -> Result<()> {
    let lhs = ctx.fresh_temp(ValType::I64);
    let rhs = ctx.fresh_temp(ValType::I64);
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins(&format!("local.set {}", lhs), "shift lhs");
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(&format!("local.set {}", rhs), "shift count");

    ctx.fb.ins(&format!("local.get {}", rhs), "shift count");
    ctx.fb.ins("i64.const 0", "minimum valid shift count");
    ctx.fb.ins("i64.lt_s", "negative shift count?");
    ctx.fb.ins("if", "reject PHP-negative shift count");
    emit_runtime_failure(ctx, 3, "negative shift count");
    ctx.fb.ins("end", "end negative shift guard");

    ctx.fb.ins(&format!("local.get {}", rhs), "shift count");
    ctx.fb.ins("i64.const 64", "PHP integer width");
    ctx.fb.ins("i64.ge_u", "shift count is at least the integer width?");
    ctx.fb
        .ins("if (result i64)", "select PHP overshift result");
    if left {
        ctx.fb.ins("i64.const 0", "left overshift yields zero");
    } else {
        ctx.fb.ins("i64.const -1", "negative right-overshift result");
        ctx.fb.ins("i64.const 0", "non-negative right-overshift result");
        ctx.fb.ins(&format!("local.get {}", lhs), "right-shift lhs");
        ctx.fb.ins("i64.const 0", "zero sign threshold");
        ctx.fb.ins("i64.lt_s", "lhs is negative?");
        ctx.fb.ins("select", "preserve the arithmetic sign");
    }
    ctx.fb.ins("else", "ordinary in-range shift");
    ctx.fb.ins(&format!("local.get {}", lhs), "shift lhs");
    ctx.fb.ins(&format!("local.get {}", rhs), "shift count");
    ctx.fb.ins(
        if left { "i64.shl" } else { "i64.shr_s" },
        "perform in-range PHP shift",
    );
    ctx.fb.ins("end", "end overshift selection");
    store_result(ctx, inst)
}

/// Lowers signed PHP integer division with explicit zero and overflow guards.
///
/// WebAssembly traps for both `rhs == 0` and `i64::MIN / -1`. PHP surfaces
/// `DivisionByZeroError` and `ArithmeticError`, so command modules route those
/// cases through the deterministic runtime failure path before `i64.div_s`.
pub(super) fn lower_signed_int_div(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let lhs = ctx.fresh_temp(ValType::I64);
    let rhs = ctx.fresh_temp(ValType::I64);
    capture_int_operands(ctx, inst, &lhs, &rhs)?;
    emit_zero_divisor_guard(ctx, &rhs, 1, "integer division by zero");

    ctx.fb.ins(&format!("local.get {}", lhs), "integer dividend");
    ctx.fb.ins("i64.const -9223372036854775808", "PHP_INT_MIN");
    ctx.fb.ins("i64.eq", "dividend is PHP_INT_MIN?");
    ctx.fb.ins(&format!("local.get {}", rhs), "integer divisor");
    ctx.fb.ins("i64.const -1", "overflowing divisor");
    ctx.fb.ins("i64.eq", "divisor is -1?");
    ctx.fb.ins("i32.and", "integer division overflow pair?");
    ctx.fb.ins("if", "reject PHP_INT_MIN divided by -1");
    emit_runtime_failure(ctx, 4, "integer division overflow");
    ctx.fb.ins("end", "end integer division overflow guard");

    ctx.fb.ins(&format!("local.get {}", lhs), "integer dividend");
    ctx.fb.ins(&format!("local.get {}", rhs), "integer divisor");
    ctx.fb.ins("i64.div_s", "PHP signed integer division");
    store_result(ctx, inst)
}

/// Lowers signed PHP modulo with zero and `PHP_INT_MIN % -1` handling.
///
/// The latter is mathematically zero but may trap in WebAssembly engines when
/// implemented through the signed division path, so it is selected explicitly.
fn lower_signed_int_mod(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let lhs = ctx.fresh_temp(ValType::I64);
    let rhs = ctx.fresh_temp(ValType::I64);
    capture_int_operands(ctx, inst, &lhs, &rhs)?;
    emit_zero_divisor_guard(ctx, &rhs, 2, "integer modulo by zero");

    ctx.fb.ins(&format!("local.get {}", lhs), "integer dividend");
    ctx.fb.ins("i64.const -9223372036854775808", "PHP_INT_MIN");
    ctx.fb.ins("i64.eq", "dividend is PHP_INT_MIN?");
    ctx.fb.ins(&format!("local.get {}", rhs), "integer divisor");
    ctx.fb.ins("i64.const -1", "special divisor");
    ctx.fb.ins("i64.eq", "divisor is -1?");
    ctx.fb.ins("i32.and", "PHP_INT_MIN modulo -1?");
    ctx.fb.ins("if (result i64)", "select modulo special case");
    ctx.fb.ins("i64.const 0", "PHP_INT_MIN modulo -1 is zero");
    ctx.fb.ins("else", "ordinary signed modulo");
    ctx.fb.ins(&format!("local.get {}", lhs), "integer dividend");
    ctx.fb.ins(&format!("local.get {}", rhs), "integer divisor");
    ctx.fb.ins("i64.rem_s", "PHP signed modulo");
    ctx.fb.ins("end", "end modulo special-case selection");
    store_result(ctx, inst)
}

/// Captures two i64 operands in fresh locals without changing evaluation order.
fn capture_int_operands(
    ctx: &mut FnCtx,
    inst: &Instruction,
    lhs: &str,
    rhs: &str,
) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins(&format!("local.set {}", lhs), "capture integer lhs");
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(&format!("local.set {}", rhs), "capture integer rhs");
    Ok(())
}

/// Emits the shared zero-divisor check for integer operations.
fn emit_zero_divisor_guard(
    ctx: &mut FnCtx,
    rhs: &str,
    failure_code: i32,
    reason: &str,
) {
    ctx.fb.ins(&format!("local.get {}", rhs), "integer divisor");
    ctx.fb.ins("i64.eqz", "zero divisor?");
    ctx.fb.ins("if", reason);
    emit_runtime_failure(ctx, failure_code, reason);
    ctx.fb.ins("end", "end zero-divisor guard");
}

/// Emits a non-returning PHP runtime failure for command modules.
///
/// Reference PHP does not treat these as immediate fatals: a division by zero, a modulo by
/// zero, a negative shift count and `PHP_INT_MIN / -1` all raise a `Throwable` that a `catch`
/// can receive. So when the module declares the exception tag and this backend can build the
/// error class inline, the failure is RAISED and only becomes a fatal if it reaches the top of
/// `main` uncaught — which is php-src's behaviour for all five guards. Anything else keeps the
/// deterministic `__rt_fail` exit, which is still the right answer for a code with no PHP class
/// behind it. Import-free reactor modules cannot reference WASI; their exceptional path
/// therefore remains an explicit `unreachable`, while the valid path and module stay portable.
///
/// The native backend currently raises only for the two `intdiv` guards and computes a raw
/// machine result for the other three, so this target is the more faithful of the two here.
pub(super) fn emit_runtime_failure(ctx: &mut FnCtx, failure_code: i32, reason: &str) {
    let has_main = ctx.module.functions.iter().any(|f| f.flags.is_main);
    if has_main {
        if let Some((class_name, message)) = super::objects::catchable_runtime_error(failure_code)
            .filter(|_| super::function::module_uses_exceptions(ctx.module))
            .filter(|(_, message)| ctx.default_strings.contains_key(*message))
            .filter(|(class_name, _)| {
                super::objects::runtime_error_is_constructible(ctx.module, class_name)
            })
        {
            // `throw` ends this path the way `return` does, so no trap follows it — the guard
            // arm simply has no fallthrough. Every precondition is proven above; the remaining
            // error paths all report before emitting anything, so falling back to the fatal
            // below stays stack-balanced rather than stranding a half-built object.
            if super::objects::emit_runtime_error_throw(ctx, class_name, message, failure_code)
                .is_ok()
            {
                return;
            }
        }
        ctx.fb
            .ins(&format!("i32.const {}", failure_code), "runtime failure code");
        ctx.fb.ins("call $__rt_fail", reason);
    }
    let classification = if ctx
        .module
        .functions
        .iter()
        .any(|function| function.flags.is_main)
    {
        "elephc-trap:post-noreturn:arithmetic-failure exceptional arithmetic path does not return"
    } else {
        "elephc-trap:non-public:reactor-arithmetic-failure import-free reactors are outside the public command surface"
    };
    ctx.fb.ins("unreachable", classification);
}

/// Lowers PHP integer add/subtract/multiply with overflow promotion to a boxed float.
/// Returns the try token carried by a handler opcode.
fn try_token(inst: &Instruction) -> Result<i64> {
    match inst.immediate {
        Some(Immediate::I64(token)) => Ok(token),
        _ => Err(WasmError::Unsupported(format!(
            "{} without a try-token immediate",
            inst.op.name()
        ))),
    }
}

/// Lowers `TryPushHandler`: arms this `try`'s catch block for the enclosing frame.
///
/// The token is the catch block's index. Because an EIR block is a flat `br_table`
/// case rather than a lexical region, arming a handler is just recording where the
/// landing pad should resume; the enclosing handler is stowed in this token's save
/// slot so `TryPopHandler` can restore it, which is what makes nested `try` work.
fn lower_try_push_handler(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let token = try_token(inst)?;
    let save = ctx
        .handler_saves
        .get(&token)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("try token {} has no save slot", token)))?;
    ctx.fb
        .ins(&format!("local.get {}", ctx.handler_local), "enclosing handler");
    ctx.fb
        .ins(&format!("local.set {}", save), "stow it for the matching pop");
    ctx.fb
        .ins(&format!("i32.const {}", token), "this try's catch block");
    ctx.fb
        .ins(&format!("local.set {}", ctx.handler_local), "arm the handler");
    Ok(())
}

/// Lowers `TryPopHandler`: restores the handler armed before the matching push.
fn lower_try_pop_handler(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let token = try_token(inst)?;
    let save = ctx
        .handler_saves
        .get(&token)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("try token {} has no save slot", token)))?;
    ctx.fb.ins(&format!("local.get {}", save), "enclosing handler");
    ctx.fb
        .ins(&format!("local.set {}", ctx.handler_local), "disarm this try");
    Ok(())
}

/// Lowers `ThrowException` and `ThrowErrorValue`: raises the PHP exception tag.
///
/// The payload is the exception object's pointer, so a frame that catches can inspect
/// the object to pick its matching clause. `throw` does not return, so nothing follows.
fn lower_throw(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins(
        &format!(
            "i32.const {}",
            super::function::UNCAUGHT_EXCEPTION_FAILURE_CODE
        ),
        "class-agnostic diagnostic for a user-raised exception",
    );
    ctx.fb.ins(
        &format!(
            "global.set ${}",
            super::function::EXCEPTION_FATAL_CODE_GLOBAL
        ),
        "claim the uncaught diagnostic for this exception",
    );
    ctx.fb.ins(
        &format!("throw ${}", super::function::EXCEPTION_TAG),
        "raise the PHP exception",
    );
    Ok(())
}

/// Lowers `CatchCurrent`: reads the exception the landing pad published.
///
/// Mirrors the native backend, which loads the same value from its `_exc_value` symbol.
fn lower_catch_current(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.fb.ins(
        &format!("global.get ${}", super::function::EXCEPTION_VALUE_GLOBAL),
        "the exception being handled",
    );
    store_result(ctx, inst)
}

/// Lowers `CatchBind`: takes the exception into an owned result and clears the slot.
///
/// Unlike `CatchCurrent`, which only reads, binding transfers ownership to `$e`, so the
/// runtime slot is cleared to avoid a second owner. Mirrors the native backend.
fn lower_catch_bind(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.fb.ins(
        &format!("global.get ${}", super::function::EXCEPTION_VALUE_GLOBAL),
        "take the exception being handled",
    );
    store_result(ctx, inst)?;
    ctx.fb.ins("i32.const 0", "no exception is in flight now");
    ctx.fb.ins(
        &format!("global.set ${}", super::function::EXCEPTION_VALUE_GLOBAL),
        "clear the slot; the binding owns it",
    );
    Ok(())
}

/// Lowers `MixedNumericBinop`: PHP `+`, `-`, or `*` over two boxed Mixed operands.
///
/// Both operands are already `Mixed` cells, so the whole computation — operand
/// classification, PHP's numeric-string rules, integer-overflow promotion, and boxing
/// the result — lives in `__rt_mixed_numeric_add/sub/mul`. The helper returns an owned
/// cell whose tag the caller observes at runtime.
fn lower_mixed_numeric_binop(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let helper = match inst.immediate {
        Some(Immediate::MixedNumericOp(MixedNumericOp::Add)) => "$__rt_mixed_numeric_add",
        Some(Immediate::MixedNumericOp(MixedNumericOp::Sub)) => "$__rt_mixed_numeric_sub",
        Some(Immediate::MixedNumericOp(MixedNumericOp::Mul)) => "$__rt_mixed_numeric_mul",
        _ => {
            return Err(WasmError::Unsupported(
                "mixed numeric binop without a MixedNumericOp immediate".to_string(),
            ));
        }
    };
    let lhs = ctx.fresh_temp(ValType::I32);
    let rhs = ctx.fresh_temp(ValType::I32);
    let lhs_owned = emit_operand_as_mixed(ctx, operand(inst, 0)?)?;
    ctx.fb.ins(&format!("local.set {}", lhs), "left operand cell");
    let rhs_owned = emit_operand_as_mixed(ctx, operand(inst, 1)?)?;
    ctx.fb.ins(&format!("local.set {}", rhs), "right operand cell");
    ctx.fb.ins(&format!("local.get {}", lhs), "left operand cell");
    ctx.fb.ins(&format!("local.get {}", rhs), "right operand cell");
    ctx.fb
        .ins(&format!("call {}", helper), "PHP arithmetic over boxed Mixed operands");
    store_result(ctx, inst)?;
    // Release only the cells this lowering allocated; an operand that was already a
    // Mixed value stays owned by whoever produced it.
    if lhs_owned {
        ctx.fb.ins(&format!("local.get {}", lhs), "temporary left cell");
        ctx.fb.ins("call $__rt_decref_mixed", "release the boxed left operand");
    }
    if rhs_owned {
        ctx.fb.ins(&format!("local.get {}", rhs), "temporary right cell");
        ctx.fb.ins("call $__rt_decref_mixed", "release the boxed right operand");
    }
    Ok(())
}

/// Pushes `value` as a boxed Mixed pointer, boxing a scalar operand when needed.
///
/// Returns whether a fresh cell was allocated, so the caller releases exactly what it
/// created. `MixedNumericBinop` mixes representations freely — `$n + 3` reaches the
/// backend as a Mixed cell and a raw `i64` — and the runtime helper takes two cells.
fn emit_operand_as_mixed(ctx: &mut FnCtx, value: ValueId) -> Result<bool> {
    if ctx.function.value(value).map(|v| v.ir_type) == Some(IrType::Heap(IrHeapKind::Mixed)) {
        ctx.emit_load_value(value)?;
        return Ok(false);
    }
    let php = ctx.function.value(value).map(|v| v.php_type.codegen_repr());
    match ctx.value_repr(value)?.clone() {
        WasmRepr::I64(local) => {
            let tag = match php {
                Some(PhpType::Bool) => 3,
                Some(PhpType::Void) => 8,
                _ => 0,
            };
            ctx.fb
                .ins(&format!("i64.const {}", tag), "mixed tag (int/bool/null)");
            ctx.fb.ins(&format!("local.get {}", local), "scalar -> lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb
                .ins("call $__rt_mixed_from_value", "box the scalar operand");
        }
        WasmRepr::F64(local) => {
            ctx.fb.ins("i64.const 2", "mixed tag (float)");
            ctx.fb.ins(&format!("local.get {}", local), "float value");
            ctx.fb.ins("i64.reinterpret_f64", "float bits -> lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb.ins("call $__rt_mixed_from_value", "box the float operand");
        }
        WasmRepr::Str { ptr, len } => {
            ctx.fb.ins("i64.const 1", "mixed tag (string)");
            ctx.fb
                .ins(&format!("local.get {}", ptr), "string pointer -> lo");
            ctx.fb.ins("i64.extend_i32_u", "widen the pointer");
            ctx.fb.ins(&format!("local.get {}", len), "string length -> hi");
            ctx.fb.ins("i64.extend_i32_u", "widen the length");
            ctx.fb.ins("call $__rt_mixed_from_value", "box the string operand");
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "mixed numeric operand representation {:?}",
                other
            )));
        }
    }
    Ok(true)
}

fn lower_checked_int_binop(
    ctx: &mut FnCtx,
    inst: &Instruction,
    operation: &str,
) -> Result<()> {
    if inst.result_php_type.codegen_repr() != PhpType::Mixed
        || inst.result_type != IrType::Heap(IrHeapKind::Mixed)
    {
        return Err(WasmError::Unsupported(format!(
            "checked integer {} result {:?}/{:?}",
            operation, inst.result_type, inst.result_php_type
        )));
    }

    let lhs = ctx.fresh_temp(ValType::I64);
    let rhs = ctx.fresh_temp(ValType::I64);
    let result = ctx.fresh_temp(ValType::I64);
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb
        .ins(&format!("local.set {}", lhs), "checked integer lhs");
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb
        .ins(&format!("local.set {}", rhs), "checked integer rhs");
    ctx.fb
        .ins(&format!("local.get {}", lhs), "checked integer lhs");
    ctx.fb
        .ins(&format!("local.get {}", rhs), "checked integer rhs");
    ctx.fb
        .ins(&format!("i64.{}", operation), "wrapped integer result");
    ctx.fb
        .ins(&format!("local.set {}", result), "save wrapped integer result");

    match operation {
        "add" => {
            // Signed-add overflow: ((lhs ^ result) & (rhs ^ result)) < 0.
            ctx.fb.ins(&format!("local.get {}", lhs), "overflow lhs");
            ctx.fb
                .ins(&format!("local.get {}", result), "overflow result");
            ctx.fb.ins("i64.xor", "lhs xor result");
            ctx.fb.ins(&format!("local.get {}", rhs), "overflow rhs");
            ctx.fb
                .ins(&format!("local.get {}", result), "overflow result");
            ctx.fb.ins("i64.xor", "rhs xor result");
            ctx.fb.ins("i64.and", "add overflow sign mask");
            ctx.fb.ins("i64.const 0", "zero sign threshold");
            ctx.fb.ins("i64.lt_s", "signed add overflow?");
        }
        "sub" => {
            // Signed-sub overflow: ((lhs ^ rhs) & (lhs ^ result)) < 0.
            ctx.fb.ins(&format!("local.get {}", lhs), "overflow lhs");
            ctx.fb.ins(&format!("local.get {}", rhs), "overflow rhs");
            ctx.fb.ins("i64.xor", "lhs xor rhs");
            ctx.fb.ins(&format!("local.get {}", lhs), "overflow lhs");
            ctx.fb
                .ins(&format!("local.get {}", result), "overflow result");
            ctx.fb.ins("i64.xor", "lhs xor result");
            ctx.fb.ins("i64.and", "sub overflow sign mask");
            ctx.fb.ins("i64.const 0", "zero sign threshold");
            ctx.fb.ins("i64.lt_s", "signed sub overflow?");
        }
        "mul" => emit_checked_mul_overflow_predicate(ctx, &lhs, &rhs, &result),
        other => {
            return Err(WasmError::Unsupported(format!(
                "checked integer operation {}",
                other
            )));
        }
    }

    // Both branches return a fresh kind-5 Mixed cell: tag 2 contains the
    // promoted floating-point result, while tag 0 contains the exact integer.
    ctx.fb
        .ins("if (result i32)", "overflow -> float, otherwise int");
    ctx.fb.ins("i64.const 2", "mixed tag (float)");
    ctx.fb
        .ins(&format!("local.get {}", lhs), "overflow lhs");
    ctx.fb.ins("f64.convert_i64_s", "lhs -> float");
    ctx.fb
        .ins(&format!("local.get {}", rhs), "overflow rhs");
    ctx.fb.ins("f64.convert_i64_s", "rhs -> float");
    ctx.fb
        .ins(&format!("f64.{}", operation), "promoted float result");
    ctx.fb.ins("i64.reinterpret_f64", "float bits -> mixed lo");
    ctx.fb.ins("i64.const 0", "mixed hi unused");
    ctx.fb.ins(
        "call $__rt_mixed_from_value",
        "box promoted float result",
    );
    ctx.fb.ins("else", "no overflow -> exact integer");
    ctx.fb.ins("i64.const 0", "mixed tag (int)");
    ctx.fb
        .ins(&format!("local.get {}", result), "exact integer result");
    ctx.fb.ins("i64.const 0", "mixed hi unused");
    ctx.fb
        .ins("call $__rt_mixed_from_value", "box exact integer result");
    ctx.fb.ins("end", "end checked integer result");
    store_result(ctx, inst)
}

/// Leaves an i32 predicate indicating whether the wrapped i64 product overflowed.
fn emit_checked_mul_overflow_predicate(
    ctx: &mut FnCtx,
    lhs: &str,
    rhs: &str,
    result: &str,
) {
    // Division verifies ordinary products. Guard zero and MIN/-1 first because
    // WebAssembly's signed division traps for the latter pair.
    ctx.fb.ins(&format!("local.get {}", rhs), "multiply rhs");
    ctx.fb.ins("i64.eqz", "rhs is zero?");
    ctx.fb
        .ins("if (result i32)", "zero rhs cannot overflow");
    ctx.fb.ins("i32.const 0", "no overflow");
    ctx.fb.ins("else", "non-zero rhs");
    ctx.fb.ins(&format!("local.get {}", lhs), "multiply lhs");
    ctx.fb
        .ins(&format!("i64.const {}", i64::MIN), "minimum integer");
    ctx.fb.ins("i64.eq", "lhs is minimum?");
    ctx.fb.ins(&format!("local.get {}", rhs), "multiply rhs");
    ctx.fb.ins("i64.const -1", "negative one");
    ctx.fb.ins("i64.eq", "rhs is negative one?");
    ctx.fb.ins("i32.and", "lhs MIN and rhs -1?");
    ctx.fb.ins(&format!("local.get {}", rhs), "multiply rhs");
    ctx.fb
        .ins(&format!("i64.const {}", i64::MIN), "minimum integer");
    ctx.fb.ins("i64.eq", "rhs is minimum?");
    ctx.fb.ins(&format!("local.get {}", lhs), "multiply lhs");
    ctx.fb.ins("i64.const -1", "negative one");
    ctx.fb.ins("i64.eq", "lhs is negative one?");
    ctx.fb.ins("i32.and", "rhs MIN and lhs -1?");
    ctx.fb.ins("i32.or", "division-trap overflow pair?");
    ctx.fb
        .ins("if (result i32)", "special overflow pair");
    ctx.fb.ins("i32.const 1", "overflow");
    ctx.fb.ins("else", "safe to verify with division");
    ctx.fb
        .ins(&format!("local.get {}", result), "wrapped product");
    ctx.fb.ins(&format!("local.get {}", rhs), "multiply rhs");
    ctx.fb.ins("i64.div_s", "reverse wrapped product");
    ctx.fb.ins(&format!("local.get {}", lhs), "multiply lhs");
    ctx.fb.ins("i64.ne", "reversed product differs?");
    ctx.fb.ins("end", "end special-pair guard");
    ctx.fb.ins("end", "end zero-rhs guard");
}

/// Lowers a float binary op: load both operands, emit the wasm op, store result.
fn lower_float_binop(ctx: &mut FnCtx, inst: &Instruction, wasm_op: &str) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(wasm_op, "float binary op");
    store_result(ctx, inst)
}

/// Lowers PHP floating-point division with an explicit signed-zero divisor guard.
///
/// WebAssembly follows IEEE-754 and would produce an infinity for division by
/// `+0.0` or `-0.0`; PHP's `/` operator raises `DivisionByZeroError` instead.
fn lower_float_div(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let lhs = ctx.fresh_temp(ValType::F64);
    let rhs = ctx.fresh_temp(ValType::F64);
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins(&format!("local.set {}", lhs), "float dividend");
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(&format!("local.set {}", rhs), "float divisor");
    ctx.fb.ins(&format!("local.get {}", rhs), "float divisor");
    ctx.fb.ins("f64.const 0", "positive zero");
    ctx.fb.ins("f64.eq", "positive or negative zero divisor?");
    ctx.fb.ins("if", "reject PHP float division by zero");
    emit_runtime_failure(ctx, 1, "float division by zero");
    ctx.fb.ins("end", "end float zero-divisor guard");
    ctx.fb.ins(&format!("local.get {}", lhs), "float dividend");
    ctx.fb.ins(&format!("local.get {}", rhs), "float divisor");
    ctx.fb.ins("f64.div", "PHP float division");
    store_result(ctx, inst)
}

/// Lowers `ConstI64`: pushes the immediate integer constant.
fn lower_const_i64(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let n = i64_immediate(inst)?;
    ctx.fb.ins(&format!("i64.const {}", n), "int literal");
    store_result(ctx, inst)
}

/// Lowers `ConstF64` bit-exactly: push the f64's raw bits and reinterpret them as f64.
fn lower_const_f64(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let bits = f64_immediate(inst)?.to_bits() as i64;
    ctx.fb.ins(&format!("i64.const {}", bits), "f64 literal bits");
    ctx.fb.ins("f64.reinterpret_i64", "reinterpret bits as f64");
    store_result(ctx, inst)
}

/// Lowers `ConstBool`: pushes 1 for true, 0 for false (PHP bool is an i64).
fn lower_const_bool(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let val = if bool_immediate(inst)? { 1 } else { 0 };
    ctx.fb.ins(&format!("i64.const {}", val), "bool literal");
    store_result(ctx, inst)
}

/// Lowers `ConstNull`: pushes the i64 null sentinel (0x7fff_ffff_ffff_fffe).
fn lower_const_null(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.fb.ins(
        "i64.const 9223372036854775806",
        "null sentinel (0x7fff_ffff_ffff_fffe)",
    );
    store_result(ctx, inst)
}

/// Lowers `ConstStr`: pushes the literal's linear-memory pointer (i32) and byte
/// length (i64) from the module's string-literal layout.
fn lower_const_str(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let (offset, len) = ctx.str_literal(data_immediate(inst)?)?;
    ctx.fb
        .ins(&format!("i32.const {}", offset), "string literal ptr");
    ctx.fb.ins(&format!("i64.const {}", len), "string literal len");
    store_result(ctx, inst)
}

/// Lowers `StrLen`: reads the length component of a string value.
fn lower_strlen(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let op0 = operand(inst, 0)?;
    let repr = ctx.value_repr(op0)?.clone();
    match repr {
        WasmRepr::Str { len, .. } => {
            ctx.fb.ins(&format!("local.get {}", len), "string length");
        }
        other => return Err(WasmError::Unsupported(format!("strlen of {:?}", other))),
    }
    store_result(ctx, inst)
}

/// Lowers `StrPersist`: takes an independently owned heap copy of a string.
///
/// EIR inserts this wherever a string has to outlive whatever produced it — most visibly when a
/// function RETURNS one, since the callee's frame is gone by the time the caller reads it. The
/// runtime helper it calls is the same one property stores and string defaults already use, so a
/// persisted string is owned on exactly the terms the release path expects.
fn lower_str_persist(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins(
        "call $__rt_str_persist",
        "own an independent copy (ptr,len) -> (new_ptr,new_len)",
    );
    store_result(ctx, inst)
}

/// Lowers `Nop`: emits a comment; the result local (if any) keeps its default 0.
fn lower_nop(ctx: &mut FnCtx) -> Result<()> {
    ctx.fb.comment("nop");
    Ok(())
}

/// Lowers `ConcatReset`: restores the global concat cursor to this frame's
/// baseline, freeing string temporaries built during the statement.
fn lower_concat_reset(ctx: &mut FnCtx) -> Result<()> {
    ctx.fb
        .ins(&format!("local.get {}", ctx.concat_base_local), "frame concat baseline");
    ctx.fb
        .ins("global.set $__concat_off", "reset concat cursor to baseline");
    Ok(())
}

/// Lowers `StrConcat`: appends two strings into the concat buffer via `__rt_concat`.
///
/// Pushes (a_ptr, a_len, b_ptr, b_len) — matching `__rt_concat`'s parameter order —
/// and stores the returned `(ptr, len)` into the result string value.
fn lower_str_concat(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins("call $__rt_concat", "concatenate two strings");
    store_result(ctx, inst)
}

/// Lowers `LoadLocal`: copies the slot's value into the result value's local(s).
///
/// If the slot stores a ref-cell pointer (a by-ref free-function param per P7c0b, or a
/// caller local promoted by a P7c by-ref closure capture), the value lives in the cell,
/// not the slot's own locals — so the load dereferences the cell. This retroactive
/// routing is what lets the EIR emit a plain `Op::LoadLocal` (it does not mark a
/// by-ref-captured caller local ref-bound) while still reading through the shared cell,
/// mirroring the active native backend's `local_stores_ref_cell_pointer` check in
/// `load_local_to_result`. A preceding `Op::Release(LoadLocal($x))` (emitted before a
/// store_local overwrite) therefore releases the cell's current value, not the slot's
/// stale locals.
fn lower_load_local(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let slot = slot_immediate(inst)?;
    let result = inst
        .result
        .ok_or_else(|| WasmError::Unsupported("load_local without result".to_string()))?;
    if let Ok(ptr_local) = ctx.ref_cell_ptr(slot.as_raw()) {
        let ptr_local = ptr_local.to_string();
        let result_repr = ctx.value_repr(result)?.clone();
        super::refcell::emit_cell_load(ctx, &ptr_local, &result_repr)?;
        return ctx.emit_store_value(result);
    }
    transfer::emit_transfer_from_slot(ctx, slot, result)
}

/// Lowers `StoreLocal`: stores the operand value into the slot.
///
/// If the slot stores a ref-cell pointer (by-ref param or P7c-promoted caller local),
/// the store writes through the cell. It does NOT release the cell's previous payload:
/// the EIR emits the prior-value release (`Op::Release(LoadLocal($x))`) before this op,
/// and (after the LoadLocal routing above) that release decrefs the cell's current
/// value. Releasing here too would double-free. This matches the contract documented on
/// `lower_store_ref_cell` and native `store_value_to_ref_cell_as`.
fn lower_store_local(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let slot = slot_immediate(inst)?;
    let value = operand(inst, 0)?;
    if let Ok(ptr_local) = ctx.ref_cell_ptr(slot.as_raw()) {
        let ptr_local = ptr_local.to_string();
        let value_repr = ctx.value_repr(value)?.clone();
        super::refcell::emit_cell_store(ctx, &ptr_local, &value_repr)?;
        return Ok(());
    }
    transfer::emit_transfer_to_slot(ctx, value, slot)
}

/// Lowers `UnsetLocal` for a consumed owned temporary by clearing every storage
/// component without releasing the value that was moved out by `LoadLocal`.
fn lower_unset_local(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let slot = slot_immediate(inst)?;
    let repr = ctx.slot_repr(slot)?.clone();
    match repr {
        WasmRepr::I64(local) => {
            ctx.fb.ins("i64.const 0", "clear consumed temp");
            ctx.fb.ins(&format!("local.set {}", local), "");
        }
        WasmRepr::F64(local) => {
            ctx.fb.ins("f64.const 0", "clear consumed temp");
            ctx.fb.ins(&format!("local.set {}", local), "");
        }
        WasmRepr::Ptr(local) => {
            ctx.fb.ins("i32.const 0", "clear consumed temp");
            ctx.fb.ins(&format!("local.set {}", local), "");
        }
        WasmRepr::Str { ptr, len } => {
            ctx.fb.ins("i32.const 0", "clear consumed temp pointer");
            ctx.fb.ins(&format!("local.set {}", ptr), "");
            ctx.fb.ins("i64.const 0", "clear consumed temp length");
            ctx.fb.ins(&format!("local.set {}", len), "");
        }
        WasmRepr::Tagged { payload, tag } => {
            ctx.fb.ins("i64.const 0", "clear consumed temp payload");
            ctx.fb.ins(&format!("local.set {}", payload), "");
            ctx.fb.ins("i32.const 0", "clear consumed temp tag");
            ctx.fb.ins(&format!("local.set {}", tag), "");
        }
        WasmRepr::Void => {}
    }
    Ok(())
}

/// Lowers `INeg`: computes `0 - x`.
fn lower_int_neg(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.fb.ins("i64.const 0", "0 for negation");
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins("i64.sub", "0 - x");
    store_result(ctx, inst)
}

/// Lowers `IBitNot`: computes `x ^ -1` (one's complement).
fn lower_int_bitnot(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins("i64.const -1", "all-ones mask");
    ctx.fb.ins("i64.xor", "bitwise not");
    store_result(ctx, inst)
}

/// Lowers `IDiv` (PHP `/`): guards zero, widens both i64 operands, and divides.
fn lower_int_div_to_float(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let lhs = ctx.fresh_temp(ValType::I64);
    let rhs = ctx.fresh_temp(ValType::I64);
    capture_int_operands(ctx, inst, &lhs, &rhs)?;
    emit_zero_divisor_guard(ctx, &rhs, 1, "PHP division by zero");
    ctx.fb.ins(&format!("local.get {}", lhs), "integer dividend");
    ctx.fb.ins("f64.convert_i64_s", "lhs to float");
    ctx.fb.ins(&format!("local.get {}", rhs), "integer divisor");
    ctx.fb.ins("f64.convert_i64_s", "rhs to float");
    ctx.fb.ins("f64.div", "php / is float division");
    store_result(ctx, inst)
}

/// Lowers `FNeg`: negates a float.
fn lower_float_neg(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins("f64.neg", "negate float");
    store_result(ctx, inst)
}

/// Maps an integer comparison predicate to its signed wasm comparison op.
fn int_cmp_op(pred: CmpPredicate) -> Result<&'static str> {
    Ok(match pred {
        CmpPredicate::Eq => "i64.eq",
        CmpPredicate::Ne => "i64.ne",
        CmpPredicate::Slt => "i64.lt_s",
        CmpPredicate::Sle => "i64.le_s",
        CmpPredicate::Sgt => "i64.gt_s",
        CmpPredicate::Sge => "i64.ge_s",
        other => {
            return Err(WasmError::Unsupported(format!(
                "integer compare predicate {:?}",
                other
            )))
        }
    })
}

/// Maps a float comparison predicate to its (ordered) wasm comparison op.
fn float_cmp_op(pred: CmpPredicate) -> &'static str {
    match pred {
        CmpPredicate::Eq => "f64.eq",
        CmpPredicate::Ne => "f64.ne",
        CmpPredicate::Slt | CmpPredicate::Olt => "f64.lt",
        CmpPredicate::Sle | CmpPredicate::Ole => "f64.le",
        CmpPredicate::Sgt | CmpPredicate::Ogt => "f64.gt",
        CmpPredicate::Sge | CmpPredicate::Oge => "f64.ge",
    }
}

/// Lowers `ICmp`: signed integer comparison yielding an i64 boolean (0/1).
fn lower_int_cmp(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let wasm_op = int_cmp_op(cmp_immediate(inst)?)?;
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(wasm_op, "integer comparison");
    ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
    store_result(ctx, inst)
}

/// Lowers `FCmp`: ordered float comparison yielding an i64 boolean (0/1).
fn lower_float_cmp(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let wasm_op = float_cmp_op(cmp_immediate(inst)?);
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(wasm_op, "float comparison");
    ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
    store_result(ctx, inst)
}

/// Lowers `IToF`: signed integer to float.
fn lower_itof(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins("f64.convert_i64_s", "int to float");
    store_result(ctx, inst)
}

/// Lowers integer-backed PHP integers and booleans to stable owned strings.
///
/// PHP renders integers as decimal text, `true` as `"1"`, and `false` as the
/// empty string. Formatting first uses the shared integer scratch buffer, then
/// persists the bytes before another conversion can overwrite them. The EIR
/// keeps its conservative `MaybeOwned` string contract; the WASM value itself
/// is an owned heap copy and is safe to release through the generic path.
fn lower_int_like_to_string(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    let source = ctx
        .function
        .value(value)
        .ok_or_else(|| WasmError::Unsupported(format!("IToStr source {:?} is missing", value)))?;
    let source_php = source.php_type.codegen_repr();
    match source_php {
        PhpType::Int => {
            ctx.emit_load_value(value)?;
            ctx.fb.ins(
                "global.get $__float_scratch",
                "integer formatting scratch buffer",
            );
            ctx.fb
                .ins("call $__rt_itoa", "format PHP integer as decimal string");
            ctx.fb
                .ins("i64.extend_i32_u", "widen integer string length");
        }
        PhpType::Bool => {
            emit_normalized_bool(ctx, value)?;
            ctx.fb.ins(
                "global.get $__float_scratch",
                "boolean formatting scratch buffer",
            );
            ctx.fb
                .ins("call $__rt_itoa", "format normalized PHP boolean");
            ctx.fb
                .ins("drop", "discard integer formatter length for PHP boolean");
            emit_normalized_bool(ctx, value)?;
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "IToStr for PHP type {:?}",
                other
            )));
        }
    }
    ctx.fb.ins(
        "call $__rt_str_persist",
        "persist formatted PHP string before scratch reuse",
    );
    store_result(ctx, inst)
}

/// Pushes one canonical i64 boolean for an integer-backed EIR value.
fn emit_normalized_bool(ctx: &mut FnCtx, value: ValueId) -> Result<()> {
    ctx.emit_load_value(value)?;
    ctx.fb.ins("i64.eqz", "boolean source is zero?");
    ctx.fb.ins("i32.eqz", "normalize non-zero boolean to one");
    ctx.fb.ins("i64.extend_i32_u", "widen normalized boolean");
    Ok(())
}

/// Lowers `FToI`: float to signed integer (truncate toward zero; NaN -> 0).
fn lower_ftoi(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    emit_float_to_php_int(ctx);
    store_result(ctx, inst)
}

/// Converts the f64 on the operand stack to a PHP integer.
///
/// Routes raw IEEE-754 bits through the centralized runtime helper so finite
/// out-of-range values use PHP's modulo-2^64 result instead of WebAssembly's
/// saturating conversion. The helper also maps NaN and infinities to zero.
/// Under the PHP 8.5 profile the diagnosing variant runs instead: it emits the
/// unrepresentable-float warning first and then returns the identical value.
fn emit_float_to_php_int(ctx: &mut FnCtx) {
    ctx.fb.ins("i64.reinterpret_f64", "float bits for PHP int cast");
    if matches!(
        crate::codegen_support::compile_php_version(),
        crate::web_prelude::PhpVersion::Php85
    ) {
        ctx.fb.ins(
            "call $__rt_float_to_int_warn",
            "apply PHP 8.5 float-to-int semantics with its diagnostic",
        );
        return;
    }
    ctx.fb
        .ins("call $__rt_float_to_int", "apply PHP 64-bit float-to-int semantics");
}

/// Lowers an internal or explicit EIR cast, including boxed Mixed-to-scalar conversions.
///
/// Checked integer arithmetic returns an owned Mixed cell so an overflow can
/// promote to float. Typed PHP contexts immediately cast that cell back to the
/// declared scalar type; these paths call the same WASM runtime helpers used by
/// hashes and mixed-value output.
fn lower_cast(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    let target = match inst.immediate {
        Some(Immediate::CastTarget(target) | Immediate::ExplicitCastTarget(target)) => target,
        _ => {
            return Err(WasmError::Unsupported(
                "cast without a CastTarget immediate".to_string(),
            ));
        }
    };
    if target != inst.result_type {
        return Err(WasmError::Unsupported(format!(
            "cast target {:?} differs from result type {:?}",
            target, inst.result_type
        )));
    }

    let source = ctx
        .function
        .value(value)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("cast source {:?} is missing", value)))?;
    if source.ir_type == IrType::Heap(IrHeapKind::Mixed) {
        ctx.emit_load_value(value)?;
        match target {
            IrType::I64 if inst.result_php_type.codegen_repr() == PhpType::Bool => {
                ctx.fb
                    .ins("call $__rt_mixed_cast_bool", "cast boxed Mixed to bool");
            }
            IrType::I64 => {
                ctx.fb
                    .ins("call $__rt_mixed_cast_int", "cast boxed Mixed to int");
            }
            IrType::F64 => {
                ctx.fb.ins(
                    "call $__rt_mixed_cast_float",
                    "cast boxed Mixed to float bits",
                );
                ctx.fb
                    .ins("f64.reinterpret_i64", "reinterpret Mixed float bits");
            }
            IrType::Str => {
                ctx.fb.ins(
                    "call $__rt_mixed_cast_string",
                    "cast boxed Mixed to owned string",
                );
                let len = ctx.fresh_temp(ValType::I32);
                let ptr = ctx.fresh_temp(ValType::I32);
                ctx.fb
                    .ins(&format!("local.set {}", len), "capture cast string length");
                ctx.fb
                    .ins(&format!("local.set {}", ptr), "capture cast string pointer");
                ctx.fb
                    .ins(&format!("local.get {}", ptr), "cast string pointer");
                ctx.fb
                    .ins(&format!("local.get {}", len), "cast string length");
                ctx.fb
                    .ins("i64.extend_i32_u", "widen cast string length");
            }
            other => {
                return Err(WasmError::Unsupported(format!(
                    "boxed Mixed cast to {:?}",
                    other
                )));
            }
        }
        return store_result(ctx, inst);
    }
    if source.ir_type == IrType::TaggedScalar {
        let WasmRepr::Tagged { payload, tag } = ctx.value_repr(value)?.clone() else {
            return Err(WasmError::Unsupported(
                "tagged-scalar cast source has a non-tagged WASM representation".to_string(),
            ));
        };
        match target {
            IrType::I64 => {
                ctx.fb
                    .ins(&format!("local.get {}", tag), "tagged scalar tag");
                ctx.fb.ins("i32.const 8", "tagged null tag");
                ctx.fb.ins("i32.eq", "tagged scalar is null?");
                ctx.fb.ins(
                    "if (result i64)",
                    "PHP null casts to zero; otherwise cast the integer payload",
                );
                ctx.fb.ins("i64.const 0", "null scalar cast result");
                ctx.fb.ins("else", "non-null tagged integer");
                ctx.fb
                    .ins(&format!("local.get {}", payload), "tagged integer payload");
                if inst.result_php_type.codegen_repr() == PhpType::Bool {
                    ctx.fb.ins("i64.const 0", "zero");
                    ctx.fb.ins("i64.ne", "integer payload truthiness");
                    ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
                }
                ctx.fb.ins("end", "end tagged scalar cast");
            }
            IrType::F64 => {
                ctx.fb
                    .ins(&format!("local.get {}", tag), "tagged scalar tag");
                ctx.fb.ins("i32.const 8", "tagged null tag");
                ctx.fb.ins("i32.eq", "tagged scalar is null?");
                ctx.fb.ins(
                    "if (result f64)",
                    "PHP null casts to 0.0; otherwise widen the integer payload",
                );
                ctx.fb.ins("f64.const 0", "null scalar float cast");
                ctx.fb.ins("else", "non-null tagged integer");
                ctx.fb
                    .ins(&format!("local.get {}", payload), "tagged integer payload");
                ctx.fb.ins("f64.convert_i64_s", "integer payload to float");
                ctx.fb.ins("end", "end tagged scalar float cast");
            }
            other => {
                return Err(WasmError::Unsupported(format!(
                    "tagged scalar cast to {:?}",
                    other
                )));
            }
        }
        return store_result(ctx, inst);
    }

    match (source.ir_type, target) {
        (IrType::I64, IrType::I64)
        | (IrType::F64, IrType::F64)
        | (IrType::Str, IrType::Str) => ctx.emit_load_value(value)?,
        (IrType::I64, IrType::F64) => {
            ctx.emit_load_value(value)?;
            ctx.fb.ins("f64.convert_i64_s", "cast int to float");
        }
        (IrType::F64, IrType::I64) => {
            ctx.emit_load_value(value)?;
            emit_float_to_php_int(ctx);
        }
        (source, target) => {
            return Err(WasmError::Unsupported(format!(
                "cast from {:?} to {:?}",
                source, target
            )));
        }
    }
    store_result(ctx, inst)
}

/// Lowers PHP truthiness for every scalar and heap representation used by EIR.
fn lower_is_truthy(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let op0 = operand(inst, 0)?;
    let repr = ctx.value_repr(op0)?.clone();
    let value = ctx
        .function
        .value(op0)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("is_truthy source {:?} is missing", op0)))?;
    match repr {
        WasmRepr::I64(_) => {
            if matches!(value.php_type, PhpType::Void | PhpType::Never) {
                ctx.fb.ins("i64.const 0", "null/never is false");
            } else {
                ctx.emit_load_value(op0)?;
                ctx.fb.ins("i64.const 0", "zero");
                ctx.fb.ins("i64.ne", "truthy = x != 0");
                ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
            }
        }
        WasmRepr::F64(_) => {
            ctx.emit_load_value(op0)?;
            ctx.fb.ins("f64.const 0.0", "zero");
            ctx.fb.ins("f64.ne", "truthy = x != 0.0");
            ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
        }
        WasmRepr::Str { ptr, len } => {
            ctx.fb
                .ins(&format!("local.get {}", len), "string length");
            ctx.fb.ins("i64.eqz", "empty string?");
            ctx.fb.ins(
                "if (result i64)",
                "empty string is false; otherwise check PHP's special string zero",
            );
            ctx.fb.ins("i64.const 0", "empty string is false");
            ctx.fb.ins("else", "non-empty string");
            ctx.fb
                .ins(&format!("local.get {}", len), "string length");
            ctx.fb.ins("i64.const 1", "single-byte string length");
            ctx.fb.ins("i64.eq", "single-byte string?");
            ctx.fb
                .ins("if (result i64)", "only the exact string \"0\" is false");
            ctx.fb
                .ins(&format!("local.get {}", ptr), "single-byte string pointer");
            ctx.fb.ins("i32.load8_u", "load the only byte");
            ctx.fb.ins("i32.const 48", "ASCII zero");
            ctx.fb.ins("i32.ne", "single-byte string is not \"0\"");
            ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
            ctx.fb.ins("else", "multi-byte non-empty string");
            ctx.fb.ins("i64.const 1", "non-empty non-\"0\" string is true");
            ctx.fb.ins("end", "end single-byte string check");
            ctx.fb.ins("end", "end string truthiness");
        }
        WasmRepr::Ptr(local) => match value.ir_type {
            IrType::Heap(IrHeapKind::Array)
            | IrType::Heap(IrHeapKind::Hash)
            | IrType::Heap(IrHeapKind::Iterable) => {
                ctx.fb
                    .ins(&format!("local.get {}", local), "container pointer");
                ctx.fb.ins("i64.load offset=0", "container length");
                ctx.fb.ins("i64.const 0", "empty length");
                ctx.fb.ins("i64.ne", "non-empty container");
                ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
            }
            IrType::Heap(IrHeapKind::Mixed) | IrType::Heap(IrHeapKind::Union) => {
                ctx.fb.ins(&format!("local.get {}", local), "boxed value");
                ctx.fb
                    .ins("call $__rt_mixed_cast_bool", "PHP Mixed truthiness");
            }
            IrType::Heap(IrHeapKind::Object) => {
                ctx.fb.ins("i64.const 1", "objects are always truthy");
            }
            other => {
                return Err(WasmError::Unsupported(format!(
                    "is_truthy of heap type {:?}",
                    other
                )));
            }
        },
        WasmRepr::Tagged { payload, tag } => {
            ctx.fb.ins(&format!("local.get {}", tag), "tagged scalar tag");
            ctx.fb.ins("i32.const 8", "tagged null tag");
            ctx.fb.ins("i32.eq", "tagged scalar is null?");
            ctx.fb
                .ins("if (result i64)", "null is false; integer payload otherwise");
            ctx.fb.ins("i64.const 0", "tagged null is false");
            ctx.fb.ins("else", "non-null tagged scalar");
            ctx.fb
                .ins(&format!("local.get {}", payload), "tagged scalar payload");
            ctx.fb.ins("i64.const 0", "zero");
            ctx.fb.ins("i64.ne", "payload is nonzero");
            ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
            ctx.fb.ins("end", "end tagged-scalar truthiness");
        }
        WasmRepr::Void => {
            ctx.fb.ins("i64.const 0", "void is false");
        }
    }
    store_result(ctx, inst)
}

/// Lowers `Op::LoadGlobal` for supported superglobals.
///
/// `$argc` is read via `__rt_argc` (WASI `args_sizes_get`); `$argv` is built as an
/// indexed string array via `__rt_argv` (WASI `args_get`). Other globals are not
/// yet supported.
fn lower_load_global(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let data_id = match &inst.immediate {
        Some(Immediate::GlobalName(d)) => *d,
        _ => return Err(WasmError::Unsupported("load_global without a name".to_string())),
    };
    let name = ctx
        .module
        .data
        .global_names
        .get(data_id.as_raw() as usize)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("load_global: unknown name {:?}", data_id)))?;
    match name.as_str() {
        "argc" => {
            ctx.fb.ins("call $__rt_argc", "load $argc");
            transfer::emit_store_stack_value_into_value(
                ctx,
                IrType::I64,
                PhpType::Int,
                inst.result.ok_or_else(|| WasmError::Unsupported("load_global argc without result".to_string()))?,
            )
        }
        "argv" => {
            ctx.fb
                .ins("call $__rt_argv", "build $argv (indexed string array)");
            transfer::emit_store_stack_value_into_value(
                ctx,
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Str)),
                inst.result.ok_or_else(|| WasmError::Unsupported("load_global argv without result".to_string()))?,
            )
        }
        other => Err(WasmError::Unsupported(format!("global ${}", other))),
    }
}

/// Lowers a compiler-resident language construct by dispatching on its name.
///
/// Only `exit`/`die` are handled so far; ordinary builtins use typed
/// `RuntimeCall` targets and other language constructs return `Unsupported`.
fn lower_language_construct_call(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let data_id = data_immediate(inst)?;
    let name = ctx
        .module
        .data
        .function_names
        .get(data_id.as_raw() as usize)
        .cloned()
        .ok_or_else(|| {
            WasmError::Unsupported(format!(
                "language construct: unknown name data {:?}",
                data_id
            ))
        })?;
    match name.as_str() {
        "exit" | "die" => lower_exit(ctx, inst),
        // `isset($x)` is exactly "not null" for a variable the checker proved defined — an
        // undefined one never reaches here, the front-end rejects it earlier.
        "isset" => {
            emit_is_null_test(ctx, operand(inst, 0)?)?;
            ctx.fb.ins("i64.eqz", "isset is the negation of is-null");
            ctx.fb.ins("i64.extend_i32_u", "PHP booleans are i64 here");
            store_result(ctx, inst)
        }
        other => Err(WasmError::Unsupported(format!(
            "language construct {}",
            other
        ))),
    }
}

/// Lowers the typed runtime-call subset currently implemented by the WASM backend.
fn lower_runtime_call(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let target = match inst.immediate {
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(target)
            | RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => target,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::UnaryString(target))) => {
            return super::builtins::lower_unary_string(ctx, inst, target);
        }
        Some(Immediate::RuntimeCall(target)) => {
            return Err(WasmError::Unsupported(format!(
                "runtime call target {:?}",
                target
            )));
        }
        _ => {
            return Err(WasmError::Unsupported(format!(
                "untyped runtime call returning {:?}",
                inst.result_php_type
            )));
        }
    };
    if super::builtins::is_direct_builtin(target) {
        return super::builtins::lower_direct_builtin(ctx, inst, target);
    }
    match target {
        RuntimeFnId::GetClass => super::classes::lower_get_class(ctx, inst),
        RuntimeFnId::ArrayMap => lower_array_map(ctx, inst),
        RuntimeFnId::ArrayFilter => lower_array_filter(ctx, inst),
        RuntimeFnId::Usort => lower_user_sort(ctx, inst, "usort"),
        RuntimeFnId::Uasort => lower_user_sort(ctx, inst, "uasort"),
        RuntimeFnId::Uksort => lower_user_sort(ctx, inst, "uksort"),
        RuntimeFnId::ArrayReduce => lower_array_reduce(ctx, inst),
        RuntimeFnId::ArrayWalk => lower_array_walk(ctx, inst),
        other => Err(WasmError::Unsupported(format!(
            "runtime function {:?}",
            other
        ))),
    }
}

/// Lowers `array_map($f, $arr)` where operand 0 `$f` is a `Callable` descriptor
/// (a closure or a free-function first-class callable) and operand 1 `$arr` is an
/// INDEXED `array<int|str|mixed>`, into a `__rt_array_map_callable` runtime call
/// returning a fresh `array<mixed>` of the mapped results. The WASM analogue of the
/// native `lower_array_map_descriptor_callback`.
///
/// Both operands are materialized BORROWED: neither is released here. The EIR owns
/// operands 0 and 1 and releases them at the call site (the source array is borrowed
/// by `array_map`, the descriptor is released after). The runtime returns an Owned
/// array pointer, stored via `store_result`.
///
/// Deferred (returns `Unsupported`, never miscompiled — mirroring the deferral
/// pattern in `closures::lower_callable_descriptor_invoke`): a string/array/object
/// callback (operand 0 not `Callable`); a hash/assoc or otherwise non-indexed source
/// (operand 1 not `Heap(Array)`); and the multi-array zip / 3-arg `array_map(null, …)`
/// shapes (operand count != 2), which need a different runtime contract.
fn lower_array_map(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    // Single callback + single source only. A 3+-operand array_map (multi-array zip
    // or array_map(null, ...)) needs a different runtime contract and is deferred.
    if inst.operands.len() != 2 {
        return Err(WasmError::Unsupported(format!(
            "array_map with {} operands on wasm32-wasi (only single callback + single \
             indexed array supported; multi-array/null-callback deferred)",
            inst.operands.len()
        )));
    }
    let callable = operand(inst, 0)?;
    let array = operand(inst, 1)?;

    // GUARD: operand 0 must be a Callable descriptor (closure or free-fn FCC). A
    // string/array/object callback would not be an i64 descriptor and needs its own
    // runtime callback selection (deferred slice).
    let callable_php = ctx.value_php_type(callable)?.codegen_repr();
    if !matches!(callable_php, PhpType::Callable) {
        return Err(WasmError::Unsupported(format!(
            "array_map with a {:?} callback on wasm32-wasi (only Callable descriptors \
             supported; string/array/object callbacks deferred)",
            callable_php
        )));
    }
    // GUARD: operand 1 must be an INDEXED array (value_type 0/1/7 = int/string/mixed-cell).
    // A HashNew assoc source has a different layout the runtime helper cannot read.
    let array_ir = ctx.function.value(array).map(|v| v.ir_type);
    if !matches!(array_ir, Some(IrType::Heap(IrHeapKind::Array))) {
        return Err(WasmError::Unsupported(format!(
            "array_map over a {:?} source on wasm32-wasi (only indexed array<int|str|mixed> \
             supported; hash/assoc sources deferred)",
            array_ir
        )));
    }

    // operand 0: callable descriptor (i64) -> i32 for __rt_array_map_callable.
    let desc = ctx.fresh_temp(ValType::I32);
    ctx.emit_load_value(callable)?;
    ctx.fb.ins("i32.wrap_i64", "callable descriptor i64 -> i32");
    ctx.fb.ins(&format!("local.set {}", desc), "save descriptor pointer");

    // operand 1: the indexed source array (a single i32 pointer).
    let src = ctx.fresh_temp(ValType::I32);
    ctx.emit_load_value(array)?;
    ctx.fb.ins(&format!("local.set {}", src), "save source array pointer");

    // __rt_array_map_callable(desc, src) -> i32 result array pointer (Owned). Neither
    // operand is released here: the EIR owns/releases operands 0 and 1 at the call site.
    ctx.fb.ins(
        &format!(
            "(call $__rt_array_map_callable (local.get {}) (local.get {}))",
            desc, src
        ),
        "map each element through the callback into a fresh array<mixed>",
    );
    store_result(ctx, inst)
}

/// Lowers `array_filter($arr, $f)` where — note the REVERSED operand order vs
/// `array_map` — operand 0 `$arr` is the INDEXED `array<int|str|mixed>` source and
/// operand 1 `$f` is a `Callable` descriptor (a closure or a free-function first-class
/// callable). It lowers to a `__rt_array_filter_callable(desc, src)` runtime call that
/// returns a fresh array of the kept elements. The result is RE-INDEXED to keys
/// `0..kept-1` (it does NOT preserve PHP keys), exactly mirroring the native
/// `__rt_array_filter` divergence — key preservation is deliberately out of scope here.
///
/// Both operands are materialized BORROWED: neither is released here. The EIR owns
/// operands 0 and 1 and releases them at the call site (the source array is borrowed
/// by `array_filter`, the descriptor is released after). The runtime returns an Owned
/// array pointer, stored via `store_result`.
///
/// Deferred (returns `Unsupported`, never miscompiled): the 3-operand
/// `array_filter($arr, $cb, $mode)` and 1-operand falsy-filter `array_filter($arr)`
/// shapes (operand count != 2); a string/array/object callback (operand 1 not
/// `Callable`); and a hash/assoc or otherwise non-indexed source (operand 0 not
/// `Heap(Array)`).
fn lower_array_filter(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    // Single indexed array + single Callable callback only. The 3-arg `mode` form
    // (ARRAY_FILTER_USE_KEY/BOTH) and the 1-arg no-callback falsy filter each need a
    // different runtime contract and are deferred.
    if inst.operands.len() != 2 {
        return Err(WasmError::Unsupported(format!(
            "array_filter with {} operands on wasm32-wasi (only a single indexed array + \
             single Callable callback supported; 3-arg mode / no-callback deferred)",
            inst.operands.len()
        )));
    }
    // REVERSED vs array_map: operand 0 is the ARRAY, operand 1 is the CALLBACK.
    let array = operand(inst, 0)?;
    let callable = operand(inst, 1)?;

    // GUARD: operand 1 must be a Callable descriptor (closure or free-fn FCC). A
    // string/array/object callback would not be an i64 descriptor and needs its own
    // runtime callback selection (deferred slice).
    let callable_php = ctx.value_php_type(callable)?.codegen_repr();
    if !matches!(callable_php, PhpType::Callable) {
        return Err(WasmError::Unsupported(format!(
            "array_filter with a {:?} callback on wasm32-wasi (only Callable descriptors \
             supported; string/array/object callbacks deferred)",
            callable_php
        )));
    }
    // GUARD: operand 0 must be an INDEXED array (value_type 0/1/7 = int/string/mixed-cell).
    // A HashNew assoc source has a different layout the runtime helper cannot read.
    let array_ir = ctx.function.value(array).map(|v| v.ir_type);
    if !matches!(array_ir, Some(IrType::Heap(IrHeapKind::Array))) {
        return Err(WasmError::Unsupported(format!(
            "array_filter over a {:?} source on wasm32-wasi (only indexed array<int|str|mixed> \
             supported; hash/assoc sources deferred)",
            array_ir
        )));
    }

    // operand 1: callable descriptor (i64) -> i32 for __rt_array_filter_callable.
    let desc = ctx.fresh_temp(ValType::I32);
    ctx.emit_load_value(callable)?;
    ctx.fb.ins("i32.wrap_i64", "callable descriptor i64 -> i32");
    ctx.fb.ins(&format!("local.set {}", desc), "save descriptor pointer");

    // operand 0: the indexed source array (a single i32 pointer).
    let src = ctx.fresh_temp(ValType::I32);
    ctx.emit_load_value(array)?;
    ctx.fb.ins(&format!("local.set {}", src), "save source array pointer");

    // __rt_array_filter_callable(desc, src) -> i32 result array pointer (Owned). Neither
    // operand is released here: the EIR owns/releases operands 0 and 1 at the call site.
    ctx.fb.ins(
        &format!(
            "(call $__rt_array_filter_callable (local.get {}) (local.get {}))",
            desc, src
        ),
        "keep each element whose callback result is truthy, re-indexed into a fresh array",
    );
    store_result(ctx, inst)
}

/// Lowers `usort` / `uasort` / `uksort` (`$array, $cmp`) where operand 0 `$array` is a
/// BY-REFERENCE indexed `array<int>` (mutated IN PLACE and re-indexed) and operand 1
/// `$cmp` is a `Callable` descriptor (a closure or a free-function first-class callable)
/// taking two ints and returning an int (`<0 / 0 / >0`). All three lower to the SAME
/// `__rt_usort_callable(desc, arr)` runtime call that copy-on-write-uniques the array,
/// STABLE bubble-sorts its int slots in place via 2-arg `__rt_closure_call` comparisons
/// on the element VALUES, and returns the (possibly cloned) array pointer. The `name`
/// argument only feeds the `Unsupported` diagnostics; it does not change the lowering.
///
/// MIRROR-NATIVE: native lowers all three identically (it sorts the indexed slots by
/// value and re-indexes), so uasort's PHP key-preservation and uksort's PHP key-
/// comparison are pre-existing native divergences that are intentionally NOT modeled
/// here — there is no key tracking or assoc/hash machinery on this path.
///
/// BY-REF WRITEBACK: usort mutates its first argument, so the returned pointer is stored
/// back into the array operand value's local AND mirrored to the operand's source slot
/// via `value_source_slot` — the verbatim `lower_array_set`/`lower_array_push` by-ref
/// template — so a later `LoadLocal` of `$array` sees the sorted (possibly cloned) array.
///
/// RESULT: usort returns bool `true`. The bool (`WasmRepr::I64` 1) is materialized only
/// when the call site uses the result (`inst.result.is_some()`); the runtime's array
/// pointer is the writeback target, never the bool result.
///
/// Both operands are materialized BORROWED: neither is released here. The EIR owns and
/// releases the descriptor and the array local; the runtime only adjusts refcounts on a
/// COW clone (`__rt_array_ensure_unique`).
///
/// Deferred (returns `Unsupported`, never miscompiled): a non-2-operand form (the
/// optional sort-flags / multi-array shapes are not `usort`); a string/array/object
/// comparator (operand 1 not `Callable`); and any non-`array<int>` source (operand 0 not
/// `Heap(Array)` with an `Int` element type). The runtime reads 8-byte int slots, so a
/// 16-byte string/mixed source would be misread — the int-element guard is load-bearing
/// for correctness.
fn lower_user_sort(ctx: &mut FnCtx, inst: &Instruction, name: &str) -> Result<()> {
    // Single ($array, $callable) form only. usort has no sort-flags / multi-array
    // overloads; any other arity is a different builtin shape and is deferred.
    if inst.operands.len() != 2 {
        return Err(WasmError::Unsupported(format!(
            "{name} with {} operands on wasm32-wasi (only ($array, $callable) supported; \
             the optional sort-flags / multi-array forms are not usort)",
            inst.operands.len()
        )));
    }
    let array = operand(inst, 0)?;
    let callable = operand(inst, 1)?;

    // GUARD: operand 1 must be a Callable descriptor (closure or free-fn FCC). A
    // string/array/object comparator would not be an i64 descriptor and needs its own
    // runtime callback selection (deferred slice).
    let callable_php = ctx.value_php_type(callable)?.codegen_repr();
    if !matches!(callable_php, PhpType::Callable) {
        return Err(WasmError::Unsupported(format!(
            "{name} with a {:?} comparator on wasm32-wasi (only Callable descriptors \
             supported; string/array/object callbacks deferred)",
            callable_php
        )));
    }
    // GUARD: operand 0 must be an indexed `array<int>`. The runtime stable-bubble-sorts
    // 8-byte int slots in place; a 16-byte string/mixed source would be misread, so reject
    // any non-`Heap(Array)` source or any non-`Int` element type (string/mixed/object
    // element sorts are deferred). This guard is load-bearing for correctness.
    let array_ir = ctx.function.value(array).map(|v| v.ir_type);
    let array_php = ctx.value_php_type(array)?.codegen_repr();
    let is_int_array = matches!(array_ir, Some(IrType::Heap(IrHeapKind::Array)))
        && matches!(&array_php, PhpType::Array(elem) if elem.codegen_repr() == PhpType::Int);
    if !is_int_array {
        return Err(WasmError::Unsupported(format!(
            "{name} over a {:?} source on wasm32-wasi (only indexed array<int> supported; \
             string/mixed/object element sorts deferred)",
            array_php
        )));
    }

    // operand 1: callable descriptor (i64) -> i32 for __rt_usort_callable.
    let desc = ctx.fresh_temp(ValType::I32);
    ctx.emit_load_value(callable)?;
    ctx.fb.ins("i32.wrap_i64", "callable descriptor i64 -> i32");
    ctx.fb.ins(&format!("local.set {}", desc), "save descriptor pointer");

    // operand 0 (by-ref): push the descriptor then the array pointer; the runtime
    // COW-uniques, stable-bubble-sorts the int slots in place, and returns the (possibly
    // cloned) pointer, left on the stack for the writeback.
    ctx.fb.ins(&format!("local.get {}", desc), "descriptor pointer");
    ctx.emit_load_value(array)?; // array pointer (i32)
    ctx.fb.ins(
        "call $__rt_usort_callable",
        "stable bubble-sort the int array in place (COW), returns the array pointer",
    );

    // The runtime returned the (possibly cloned) pointer: store it back into the array
    // operand value's local.
    ctx.emit_store_value(array)?;
    // And mirror it to the source slot so a later LoadLocal sees the sorted pointer —
    // the verbatim lower_array_set / lower_array_push by-ref writeback.
    if let Some(slot) = value_source_slot(ctx, array) {
        let array_ref = ctx.value_repr(array)?.local_refs();
        let slot_ref = ctx.slot_repr(slot)?.local_refs();
        if array_ref.len() == 1 && slot_ref.len() == 1 {
            ctx.fb
                .ins(&format!("local.get {}", array_ref[0]), "sorted array pointer");
            ctx.fb
                .ins(&format!("local.set {}", slot_ref[0]), "write back to the array slot");
        }
    }

    // The user-comparator sort returns bool true; materialize it only when the call
    // site uses the result.
    if inst.result.is_some() {
        ctx.fb.ins("i64.const 1", "user-comparator sort returns bool true");
        store_result(ctx, inst)?;
    }
    Ok(())
}

/// Lowers `array_reduce($array, $callback, $initial)` where operand 0 `$array` is an
/// INDEXED `array<int>`, operand 1 `$callback` is a `Callable` descriptor (a closure or
/// a free-function first-class callable) of shape `(int $carry, int $item) -> int`, and
/// operand 2 `$initial` is the initial int carry. It lowers to a
/// `__rt_array_reduce_callable(desc, arr, carry)` runtime call that folds the array
/// left-to-right (`carry = callback($carry, $item)`) and returns the final i64 carry.
/// The fourth higher-order consumer after `array_map`/`array_filter`/`usort`, and the
/// simplest: a scalar i64 carry threaded through the loop, NO array output, NO by-ref,
/// NO key handling, NO sort. The WASM analogue of the native `lower_array_reduce`.
///
/// RAW-i64 CARRY: both the element and the initial are 8-byte int scalars, so the carry,
/// the item, and the callback result are all raw i64 — there is NO Mixed cell to thread
/// across iterations. The runtime re-boxes the current i64 carry into a fresh arg cell
/// each iteration (and reads the callback's int result back from the result cell), so the
/// carry never accumulates heap state.
///
/// RESULT-REPR BRANCH (mirrors native `box_int_result_for_mixed_builtin`): the runtime
/// returns the carry as an i64 on the stack. When the instruction's result repr is
/// `Ptr` (a `Mixed`/`Union` result), the carry is boxed into a tag-0 int Mixed cell via
/// `__rt_mixed_from_value`; when it is `I64` (an `Int` result), the raw i64 is stored
/// directly. A void call site (`inst.result` is `None`) drops the carry to keep the stack
/// balanced. Any other result repr is rejected rather than miscompiled.
///
/// Both `desc` and `arr` are materialized BORROWED: neither is released here. The EIR
/// owns operands 0/1/2 (the array is borrowed by `array_reduce`, the descriptor and the
/// initial are released after) and releases them at the call site.
///
/// Deferred (returns `Unsupported`, never miscompiled): a non-3-operand form; a
/// string/array/object callback (operand 1 not `Callable`); a non-`array<int>` source
/// (operand 0 not `Heap(Array)` with an `Int` element — the runtime reads 8-byte int
/// slots, so a 16-byte string/mixed source would be misread); and a non-int initial
/// carry (operand 2 not `WasmRepr::I64` — a string/mixed/null-default carry has no
/// 8-byte runtime contract).
fn lower_array_reduce(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    // Single ($array, $callable, $initial) form only. The EIR planner always supplies a
    // 3-operand call (defaulting a missing initial to null); any other arity is a
    // different builtin shape and is deferred.
    if inst.operands.len() != 3 {
        return Err(WasmError::Unsupported(format!(
            "array_reduce with {} operands on wasm32-wasi (only ($array, $callable, \
             $initial) supported; other arities deferred)",
            inst.operands.len()
        )));
    }
    let array = operand(inst, 0)?;
    let callable = operand(inst, 1)?;
    let initial = operand(inst, 2)?;

    // GUARD: operand 1 must be a Callable descriptor (closure or free-fn FCC). A
    // string/array/object callback would not be an i64 descriptor and needs its own
    // runtime callback selection (deferred slice).
    let callable_php = ctx.value_php_type(callable)?.codegen_repr();
    if !matches!(callable_php, PhpType::Callable) {
        return Err(WasmError::Unsupported(format!(
            "array_reduce with a {:?} callback on wasm32-wasi (only Callable descriptors \
             supported; string/array/object callbacks deferred)",
            callable_php
        )));
    }
    // GUARD: operand 0 must be an indexed `array<int>`. The runtime reads 8-byte int slots
    // (`arr+24+i*8`); a 16-byte string/mixed source would be misread, so reject any
    // non-`Heap(Array)` source or any non-`Int` element type (string/mixed/object element
    // folds are deferred). This guard is load-bearing for correctness.
    let array_ir = ctx.function.value(array).map(|v| v.ir_type);
    let array_php = ctx.value_php_type(array)?.codegen_repr();
    let is_int_array = matches!(array_ir, Some(IrType::Heap(IrHeapKind::Array)))
        && matches!(&array_php, PhpType::Array(elem) if elem.codegen_repr() == PhpType::Int);
    if !is_int_array {
        return Err(WasmError::Unsupported(format!(
            "array_reduce over a {:?} source on wasm32-wasi (only indexed array<int> \
             supported; string/mixed/object element folds deferred)",
            array_php
        )));
    }
    // GUARD: operand 2 (initial carry) must be an i64. Only an int/bool initial has the
    // 8-byte runtime contract the carry threading relies on; a string/mixed/null-default
    // initial is deferred.
    let initial_repr = ctx.value_repr(initial)?.clone();
    if !matches!(initial_repr, WasmRepr::I64(_)) {
        return Err(WasmError::Unsupported(format!(
            "array_reduce with a {:?} initial carry on wasm32-wasi (only an int/bool \
             initial supported; string/mixed/null-default carry deferred)",
            initial_repr
        )));
    }

    // operand 1: callable descriptor (i64) -> i32 for __rt_array_reduce_callable.
    let desc = ctx.fresh_temp(ValType::I32);
    ctx.emit_load_value(callable)?;
    ctx.fb.ins("i32.wrap_i64", "callable descriptor i64 -> i32");
    ctx.fb.ins(&format!("local.set {}", desc), "save descriptor pointer");

    // Push the descriptor, then the array pointer (i32), then the initial carry (i64);
    // the runtime folds carry = cb(carry, item) left-to-right and leaves the final i64
    // carry on the stack. Neither operand is released here: the EIR owns/releases
    // operands 0/1/2 at the call site.
    ctx.fb.ins(&format!("local.get {}", desc), "descriptor pointer");
    ctx.emit_load_value(array)?; // array pointer (i32)
    ctx.emit_load_value(initial)?; // initial carry (i64)
    ctx.fb.ins(
        "call $__rt_array_reduce_callable",
        "left-to-right fold carry = cb(carry, item), returns the final i64 carry",
    );

    // The final i64 carry is on the stack. Box it (Mixed/Union result), store it raw
    // (Int result), or drop it (void call) — mirroring native box_int_result_for_mixed_builtin.
    match inst.result {
        Some(result) => match ctx.value_repr(result)?.clone() {
            WasmRepr::Ptr(_) => {
                let carry = ctx.fresh_temp(ValType::I64);
                ctx.fb.ins(&format!("local.set {}", carry), "save the folded i64 carry");
                ctx.fb.ins(
                    &format!(
                        "(call $__rt_mixed_from_value (i64.const 0) (local.get {}) (i64.const 0))",
                        carry
                    ),
                    "box the int carry into a Mixed cell (tag 0) for a Mixed/Union result",
                );
                store_result(ctx, inst)
            }
            WasmRepr::I64(_) => store_result(ctx, inst),
            other => Err(WasmError::Unsupported(format!(
                "array_reduce into {:?} on wasm32-wasi (only an Int or Mixed/Union result \
                 supported)",
                other
            ))),
        },
        None => {
            ctx.fb.ins("drop", "discard the unused folded carry (no result)");
            Ok(())
        }
    }
}

/// Lowers `array_walk($array, $callback)` where operand 0 `$array` is an INDEXED
/// `array<int>` and operand 1 `$callback` is a `Callable` descriptor (a closure or a
/// free-function first-class callable) of shape `(int $value) -> void`. It lowers to a
/// `__rt_array_walk_callable(desc, arr)` runtime call that invokes the callback once per
/// element, value-only and left-to-right, FOR SIDE EFFECTS — NO key argument, NO by-ref
/// `$value` writeback, NO array mutation, and NO result. The fifth higher-order consumer
/// after `array_map`/`array_filter`/`usort`/`array_reduce`, and the simplest: a 1-arg
/// per-element call, no carry, no writeback, no result materialization, no COW. The WASM
/// analogue of the native `lower_array_walk`.
///
/// READ-ONLY / VALUE-ONLY (mirrors native exactly): native `lower_array_walk` passes the
/// callback only the element value (1 visible param, NO key), models the callback return
/// as `Void`, and does NO by-ref writeback; the WASM closure wrappers cannot express a
/// by-ref visible param anyway, so this is both the native contract and the only thing
/// WASM can express here. The 3-arg `$extra` form is rejected by the checker and never
/// reaches this lowering.
///
/// RESULT: `array_walk` lowers to `Void` in EIR (native `store_void_builtin_result`), so
/// `inst.result` is normally `None` and the runtime returns nothing — the stack is
/// balanced after the call with no value to store or drop. DEFENSIVE (mirror `lower_user_sort`
/// bool handling): should a call site attach a result slot, the PHP `true`
/// (`WasmRepr::I64` 1) is materialized only when that result repr is a simple i64 slot;
/// any other result repr is rejected rather than miscompiled.
///
/// Both `desc` and `arr` are materialized BORROWED: neither is released here, and the
/// runtime does NOT incref/decref/ensure-unique or mutate the array (read-only). The EIR
/// owns operands 0/1 (the array is borrowed by `array_walk`, the descriptor released
/// after) and releases them at the call site.
///
/// Deferred (returns `Unsupported`, never miscompiled): a non-2-operand form (the 3-arg
/// `$extra` shape); a string/array/object callback (operand 1 not `Callable`); and a
/// non-`array<int>` source (operand 0 not `Heap(Array)` with an `Int` element — the
/// runtime reads 8-byte int slots, so a 16-byte string/mixed source would be misread).
fn lower_array_walk(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    // Single ($array, $callable) form only. The 3-arg `array_walk($a, $cb, $extra)` form
    // is rejected by the checker and never reaches here; any other arity is deferred.
    if inst.operands.len() != 2 {
        return Err(WasmError::Unsupported(format!(
            "array_walk with {} operands on wasm32-wasi (only ($array, $callable) supported; \
             the 3-arg $extra form is deferred)",
            inst.operands.len()
        )));
    }
    let array = operand(inst, 0)?;
    let callable = operand(inst, 1)?;

    // GUARD: operand 1 must be a Callable descriptor (closure or free-fn FCC). A
    // string/array/object callback would not be an i64 descriptor and needs its own
    // runtime callback selection (deferred slice).
    let callable_php = ctx.value_php_type(callable)?.codegen_repr();
    if !matches!(callable_php, PhpType::Callable) {
        return Err(WasmError::Unsupported(format!(
            "array_walk with a {:?} callback on wasm32-wasi (only Callable descriptors \
             supported; string/array/object callbacks deferred)",
            callable_php
        )));
    }
    // GUARD: operand 0 must be an indexed `array<int>`. The runtime reads 8-byte int slots
    // (`arr+24+i*8`); a 16-byte string/mixed source would be misread, so reject any
    // non-`Heap(Array)` source or any non-`Int` element type (string/mixed/object element
    // walks are deferred). This guard is load-bearing for correctness.
    let array_ir = ctx.function.value(array).map(|v| v.ir_type);
    let array_php = ctx.value_php_type(array)?.codegen_repr();
    let is_int_array = matches!(array_ir, Some(IrType::Heap(IrHeapKind::Array)))
        && matches!(&array_php, PhpType::Array(elem) if elem.codegen_repr() == PhpType::Int);
    if !is_int_array {
        return Err(WasmError::Unsupported(format!(
            "array_walk over a {:?} source on wasm32-wasi (only indexed array<int> supported; \
             string/mixed/object element walks deferred)",
            array_php
        )));
    }

    // operand 1: callable descriptor (i64) -> i32 for __rt_array_walk_callable.
    let desc = ctx.fresh_temp(ValType::I32);
    ctx.emit_load_value(callable)?;
    ctx.fb.ins("i32.wrap_i64", "callable descriptor i64 -> i32");
    ctx.fb.ins(&format!("local.set {}", desc), "save descriptor pointer");

    // Push the descriptor then the array pointer (i32); the runtime visits each int
    // element value-only left-to-right (read-only) and returns NOTHING — the stack stays
    // balanced after the call. Neither operand is released here: the EIR owns/releases
    // operands 0/1 at the call site, and the array stays borrowed (no COW/mutation).
    ctx.fb.ins(&format!("local.get {}", desc), "descriptor pointer");
    ctx.emit_load_value(array)?; // array pointer (i32)
    ctx.fb.ins(
        "call $__rt_array_walk_callable",
        "visit each int element value-only left-to-right (read-only), no result",
    );

    // array_walk lowers to Void in EIR (store_void_builtin_result), so inst.result is
    // normally None and there is nothing to store. DEFENSIVE (mirror lower_user_sort's bool
    // handling): materialize PHP `true` only when a result slot is attached AND it is a
    // simple i64; any other result repr is rejected rather than miscompiled.
    if let Some(result) = inst.result {
        match ctx.value_repr(result)?.clone() {
            WasmRepr::I64(_) => {
                ctx.fb.ins("i64.const 1", "array_walk returns bool true");
                store_result(ctx, inst)?;
            }
            other => {
                return Err(WasmError::Unsupported(format!(
                    "array_walk into {:?} on wasm32-wasi (array_walk returns bool true; only an \
                     Int/Bool result slot supported)",
                    other
                )));
            }
        }
    }
    Ok(())
}

/// Lowers `exit`/`die` with PHP's integer-versus-message behavior.
///
/// Integers and booleans become the process status. A string is written in full
/// to stdout before exiting with status zero, and no argument exits with zero.
/// Dynamically typed or float arguments are rejected until their PHP coercion
/// diagnostics can be preserved instead of silently choosing the wrong branch.
fn lower_exit(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let status = ctx.fresh_temp(ValType::I32);
    match inst.operands.first().copied() {
        None => ctx.fb.ins("i32.const 0", "exit status 0"),
        Some(argument) => {
            let php_type = ctx.value_php_type(argument)?.codegen_repr();
            match php_type {
                PhpType::Int | PhpType::Bool => {
                    ctx.emit_load_value(argument)?;
                    ctx.fb.ins("i32.wrap_i64", "exit code to i32");
                }
                PhpType::Str => {
                    ctx.emit_load_value(argument)?;
                    ctx.fb
                        .ins("call $__rt_echo_str", "print the PHP exit message");
                    ctx.fb.ins("i32.const 0", "string exit status 0");
                }
                other => {
                    return Err(WasmError::Unsupported(format!(
                        "exit/die argument of type {:?} requiring runtime PHP coercion",
                        other
                    )));
                }
            }
        }
    }
    ctx.fb
        .ins(&format!("local.set {}", status), "save exit status across cleanup");
    ctx.emit_ref_cell_owner_epilogue()?;
    ctx.emit_reassigned_capture_epilogue(None)?;
    ctx.emit_local_epilogue_cleanup(None)?;
    ctx.fb
        .ins(&format!("local.get {}", status), "restore exit status after cleanup");
    ctx.fb.ins("call $wasi_proc_exit", "WASI proc_exit(code)");
    ctx.fb.ins(
        "unreachable",
        "elephc-trap:post-noreturn:explicit-proc-exit WASI proc_exit is non-returning",
    );
    Ok(())
}

/// Lowers `EchoValue`/`PrintValue` by dispatching on the operand's PHP type.
///
/// Integers and booleans share the i64 representation, so the PHP type is used to
/// pick the right runtime helper (booleans print "1"/"" rather than "0"/"1").
/// Floats render as `%.14G` text via `__rt_echo_f64`; mixed values defer to the
/// tag-dispatching `__rt_mixed_write_stdout`. Array and object output still need
/// more runtime support and are not handled yet.
fn lower_echo(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let op0 = operand(inst, 0)?;
    let php = ctx
        .function
        .value(op0)
        .map(|v| v.php_type.codegen_repr())
        .ok_or_else(|| WasmError::Unsupported(format!("echo: unknown operand {:?}", op0)))?;
    match php {
        PhpType::Bool => {
            ctx.emit_load_value(op0)?;
            ctx.fb.ins("call $__rt_echo_bool", "echo boolean to stdout");
            Ok(())
        }
        PhpType::Int => {
            ctx.emit_load_value(op0)?;
            ctx.fb.ins("call $__rt_echo_i64", "echo integer to stdout");
            Ok(())
        }
        PhpType::Float => {
            // Pushes the f64 value; __rt_echo_f64 reinterprets it to bits for __rt_ftoa.
            ctx.emit_load_value(op0)?;
            ctx.fb.ins("call $__rt_echo_f64", "echo float to stdout");
            Ok(())
        }
        PhpType::Str => {
            // Pushes ptr (i32) then len (i64), matching __rt_echo_str's params.
            ctx.emit_load_value(op0)?;
            ctx.fb.ins("call $__rt_echo_str", "echo string to stdout");
            Ok(())
        }
        PhpType::Mixed => {
            // The Mixed pointer; the runtime dispatches on the cell's tag.
            ctx.emit_load_value(op0)?;
            ctx.fb
                .ins("call $__rt_mixed_write_stdout", "echo mixed value (tag-dispatched)");
            Ok(())
        }
        // PHP prints the literal text "Array" for a container and warns. A CONCRETE array
        // reaches this since nested arrays bind at their own type; the Mixed arm above hits
        // the same rule through the cell's tag.
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            ctx.fb
                .ins("call $__rt_warn_array_to_string", "PHP warns before printing");
            let _ = ctx.value_repr(op0)?;
            ctx.fb.ins("i32.const 0", "\"Array\" is written into the float scratch");
            ctx.fb.ins("drop", "the container itself is not read");
            ctx.fb.ins(
                "(call $__rt_echo_array_word)",
                "echo the literal text PHP prints for a container",
            );
            Ok(())
        }
        PhpType::TaggedScalar => {
            let WasmRepr::Tagged { payload, tag } = ctx.value_repr(op0)?.clone() else {
                return Err(WasmError::Unsupported(
                    "tagged-scalar echo operand has a non-tagged WASM representation".to_string(),
                ));
            };
            ctx.fb
                .ins(&format!("local.get {}", tag), "tagged scalar tag");
            ctx.fb.ins("i32.const 8", "tagged null tag");
            ctx.fb.ins("i32.ne", "tagged scalar is non-null");
            ctx.fb.ins("if", "PHP echo emits nothing for null");
            ctx.fb
                .ins(&format!("local.get {}", payload), "tagged integer payload");
            ctx.fb
                .ins("call $__rt_echo_i64", "echo non-null tagged integer");
            ctx.fb.ins("end", "end tagged scalar echo");
            Ok(())
        }
        other => Err(WasmError::Unsupported(format!("echo of {:?}", other))),
    }
}

/// Lowers `Op::Acquire`: makes the operand value safe to store as a new owner.
///
/// A PHP string is copied into an owned heap block (`__rt_str_persist`), matching
/// PHP string value semantics; a heap pointer is increfed (`__rt_incref`); scalars
/// forward unchanged. The result value receives the acquired value. A `Mixed`
/// (tagged) value is not handled yet (its ownership lands with the boxing phase).
///
/// A callable is a heap descriptor carried as `WasmRepr::I64` (a zero-extended i32
/// pointer), so the generic `I64` arm below would forward it without incref'ing the
/// descriptor and leak it. Callables are therefore routed explicitly to `__rt_incref`
/// on the wrapped i32 pointer before forwarding (P7a0).
fn lower_acquire(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    if ctx.value_php_type(value)? == PhpType::Callable {
        ctx.emit_load_value(value)?;
        ctx.fb
            .ins("i32.wrap_i64", "narrow the callable descriptor pointer to i32");
        ctx.fb.ins("call $__rt_incref", "incref the callable descriptor");
        return forward_value(ctx, value, inst);
    }
    let repr = ctx.value_repr(value)?.clone();
    match repr {
        WasmRepr::Str { .. } => {
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins("call $__rt_str_persist", "persist string to an owned heap copy");
            store_result(ctx, inst)
        }
        WasmRepr::Ptr(_) => {
            ctx.emit_load_value(value)?;
            ctx.fb.ins("call $__rt_incref", "incref the owned heap value");
            forward_value(ctx, value, inst)
        }
        WasmRepr::I64(_) | WasmRepr::F64(_) | WasmRepr::Void => forward_value(ctx, value, inst),
        WasmRepr::Tagged { .. } => forward_value(ctx, value, inst),
    }
}

/// Lowers `Op::Release`: releases storage the value may own.
///
/// No-op for ownership states that cannot own heap storage (non-heap, borrowed,
/// persistent, moved). A string is freed through the bounds/refcount-guarded
/// `__rt_heap_free_safe` (so transient concat/literal pointers are skipped there);
/// a heap pointer is released through the `__rt_decref_any` kind dispatcher. A
/// `Mixed` (tagged) value is not handled yet.
fn lower_release(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    let ownership = ctx
        .function
        .value(value)
        .map(|v| v.ownership)
        .unwrap_or(Ownership::NonHeap);
    if matches!(
        ownership,
        Ownership::NonHeap | Ownership::Borrowed | Ownership::Persistent | Ownership::Moved
    ) {
        return Ok(());
    }
    // A callable is a heap descriptor carried as `WasmRepr::I64`, so the generic
    // `I64` arm below is a no-op and an owned callable would leak. Route callables
    // to `__rt_decref_any` on the wrapped i32 pointer; the kind dispatcher resolves
    // heap-header kind 6 to `__rt_callable_descriptor_release` (P7a0).
    if ctx.value_php_type(value)? == PhpType::Callable {
        ctx.emit_load_value(value)?;
        ctx.fb
            .ins("i32.wrap_i64", "narrow the callable descriptor pointer to i32");
        ctx.fb
            .ins("call $__rt_decref_any", "release the callable descriptor (kind 6)");
        return Ok(());
    }
    let repr = ctx.value_repr(value)?.clone();
    match repr {
        WasmRepr::Str { ptr, .. } => {
            ctx.fb
                .ins(&format!("local.get {}", ptr), "string pointer to free");
            ctx.fb
                .ins("call $__rt_heap_free_safe", "free the owned string (skips non-heap)");
            Ok(())
        }
        WasmRepr::Ptr(_) => {
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins("call $__rt_decref_any", "release the owned heap value by kind");
            Ok(())
        }
        WasmRepr::I64(_) | WasmRepr::F64(_) | WasmRepr::Void => Ok(()),
        WasmRepr::Tagged { .. } => Ok(()),
    }
}

/// Lowers `Op::Move` / `Op::Borrow`: pure value forwarding, copying the operand's
/// local(s) into the result's local(s) with no refcount change.
fn lower_forward(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    forward_value(ctx, value, inst)
}

/// Copies `value`'s local(s) into the instruction result's local(s), if the
/// instruction produces a result. Errors if the two reprs differ in local arity.
fn forward_value(ctx: &mut FnCtx, value: ValueId, inst: &Instruction) -> Result<()> {
    let Some(result) = inst.result else {
        return Ok(());
    };
    transfer::emit_transfer_value(ctx, value, result)
}

/// Returns the local slot a value was loaded from, if its defining instruction is
/// a `LoadLocal`. Used by `ArrayPush` to write a reallocated array pointer back to
/// the variable's slot (mirroring the native `source_load_local_slot`).
pub(super) fn value_source_slot(ctx: &FnCtx, value: ValueId) -> Option<LocalSlotId> {
    let v = ctx.function.value(value)?;
    let ValueDef::Instruction { inst, .. } = v.def else {
        return None;
    };
    let inst = ctx.function.instruction(inst)?;
    if inst.op == Op::LoadLocal {
        if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
            return Some(slot);
        }
    }
    None
}

/// Lowers `Op::ArrayNew`: allocates an empty indexed array with the immediate
/// capacity. The element size defaults to 16 bytes; `__rt_array_push_int` shrinks
/// it to 8 on the first scalar push, matching the native backend.
fn lower_array_new(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let capacity = match &inst.immediate {
        Some(Immediate::Capacity(c)) => *c as i64,
        _ => return Err(WasmError::Unsupported("array_new without a capacity".to_string())),
    };
    ctx.fb
        .ins(&format!("i64.const {}", capacity), "initial capacity");
    ctx.fb
        .ins("i64.const 16", "default elem_size (specialized on first push)");
    ctx.fb.ins("call $__rt_array_new", "allocate indexed array");
    store_result(ctx, inst)
}

/// Lowers `Op::ArrayLen`: reads the i64 length stored at the array header (A+0).
fn lower_array_len(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins("i64.load", "array length @ +0");
    store_result(ctx, inst)
}

/// Lowers indexed array reads with an explicit null-capable result representation.
///
/// Integers return a `(payload, tag)` scalar pair; booleans and strings return
/// fresh Mixed cells. A missing `ArrayGet` emits PHP's undefined-key warning
/// after storing the null result, while `ArrayGetSilent` preserves the same
/// value path without observable diagnostics.
fn lower_array_get(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let array = operand(inst, 0)?;
    let index = operand(inst, 1)?;
    let result = inst
        .result
        .ok_or_else(|| WasmError::Unsupported("array_get without a result".to_string()))?;
    let source_php_type = ctx.value_php_type(array)?;
    let element_type = match &source_php_type {
        PhpType::Array(element) => element.codegen_repr(),
        PhpType::Union(members) if members.len() == 2 => {
            let Some(PhpType::Array(element)) = members
                .iter()
                .find(|member| matches!(member, PhpType::Array(_)))
            else {
                return Err(WasmError::Unsupported(format!(
                    "array_get nullable source is not an indexed array: {:?}",
                    source_php_type
                )));
            };
            if !members
                .iter()
                .any(|member| matches!(member, PhpType::Void | PhpType::Never))
            {
                return Err(WasmError::Unsupported(format!(
                    "array_get union source is not nullable: {:?}",
                    source_php_type
                )));
            }
            element.codegen_repr()
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "array_get source is not an indexed array: {:?}",
                other
            )));
        }
    };
    let result_repr = ctx.value_repr(result)?.clone();
    match (&element_type, &result_repr) {
        (PhpType::Int, WasmRepr::Tagged { .. }) => {
            ctx.emit_load_value(array)?;
            ctx.emit_load_value(index)?;
            ctx.fb
                .ins("call $__rt_array_get_tagged_int", "indexed array get (tagged int)");
        }
        (PhpType::Bool, WasmRepr::Ptr(_)) => {
            ctx.emit_load_value(array)?;
            ctx.emit_load_value(index)?;
            ctx.fb
                .ins("call $__rt_array_get_mixed_bool", "indexed array get (boxed bool|null)");
        }
        (PhpType::Str, WasmRepr::Ptr(_)) => {
            ctx.emit_load_value(array)?;
            ctx.emit_load_value(index)?;
            ctx.fb
                .ins("call $__rt_array_get_mixed_str", "indexed array get (boxed string|null)");
        }
        (element, repr) => {
            return Err(WasmError::Unsupported(format!(
                "array_get element {:?} into {:?}",
                element, repr
            )));
        }
    }
    store_result(ctx, inst)?;
    if inst.op == Op::ArrayGet {
        emit_undefined_array_index_warning_if_null(ctx, index, &result_repr)?;
    }
    Ok(())
}

/// Emits the warning-only branch for a normal indexed read whose result is null.
///
/// Tagged integers carry the null tag in their i32 tag local. Boxed bool/string
/// reads carry it at offset zero of the fresh Mixed cell. The index SSA value is
/// reloaded only after the getter has completed, so it is evaluated exactly once.
fn emit_undefined_array_index_warning_if_null(
    ctx: &mut FnCtx,
    index: ValueId,
    result_repr: &WasmRepr,
) -> Result<()> {
    match result_repr {
        WasmRepr::Tagged { tag, .. } => {
            ctx.fb.ins(&format!("local.get {tag}"), "read nullable result tag");
            ctx.fb.ins("i32.const 8", "Mixed null tag");
            ctx.fb.ins("i32.eq", "missing indexed element");
        }
        WasmRepr::Ptr(cell) => {
            ctx.fb.ins(&format!("local.get {cell}"), "load boxed nullable result");
            ctx.fb.ins("i64.load", "Mixed tag @ +0");
            ctx.fb.ins("i64.const 8", "Mixed null tag");
            ctx.fb.ins("i64.eq", "missing indexed element");
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "array_get warning for result representation {:?}",
                other
            )));
        }
    }
    ctx.fb.ins("if", "warn only when the indexed read produced null");
    ctx.emit_load_value(index)?;
    ctx.fb.ins(
        "call $__rt_warn_undefined_array_key_int",
        "emit PHP undefined-array-key warning",
    );
    ctx.fb.ins("end", "continue with the stored null result");
    Ok(())
}

/// Lowers `Op::LooseEq` / `Op::LooseNotEq` — PHP's `==` and `!=` — over the CONCRETE pairs.
///
/// Same-type pairs are the machine comparison: two ints or two bools are `i64.eq`, and two floats
/// `f64.eq`, which already answers false for NaN the way PHP does. Two strings go through
/// `__rt_str_loose_eq`, which compares numerically only when BOTH are fully numeric.
///
/// An int against a float WIDENS the int, precision loss and all: `PHP_INT_MAX == 9.22e18` is
/// true in PHP even though the values differ, because both round to 2^63. Numeric STRINGS do not
/// follow that rule — see `__rt_str_loose_eq`, where php-src's `oflow` flag settles the same pair
/// as unequal.
///
/// Every other pair — anything involving a Mixed cell, an array, an object, or a bool against a
/// number — needs the rest of PHP 8's comparison table and is refused by the capability gate.
/// Resolves the `Class::$prop` label an `Op::LoadStaticProperty` / `Op::StoreStaticProperty`
/// carries, and answers its slot address and declared type.
fn static_property_slot<'a>(
    ctx: &'a FnCtx,
    inst: &Instruction,
) -> Result<&'a super::statics::StaticSlot> {
    let label = ctx
        .module
        .data
        .strings
        .get(data_immediate(inst)?.as_raw() as usize)
        .ok_or_else(|| {
            WasmError::Unsupported("static property without an interned label".to_string())
        })?;
    super::statics::resolve_label(ctx.module, ctx.static_slots, label).ok_or_else(|| {
        WasmError::Unsupported(format!("static property {label} has no lowered slot"))
    })
}

/// Lowers `Op::ScopedConstantGet` for an ENUM CASE (`Suit::Hearts`).
///
/// A case is PHP's own class constant holding a SINGLETON object, so this reads a pointer
/// slot that starts at zero and materializes the object on first use — the same lazy shape
/// the native backend uses, which is what keeps a program that never touches an enum from
/// paying for one.
///
/// The read hands out an OWNED reference: the consumer acquires into its destination and
/// releases the temporary, so without the incref the singleton's count would drift down by
/// one per read and the case would be freed while its slot still points at it.
fn lower_scoped_constant_get(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let label = ctx
        .module
        .data
        .strings
        .get(data_immediate(inst)?.as_raw() as usize)
        .cloned()
        .ok_or_else(|| {
            WasmError::Unsupported("scoped constant without an interned label".to_string())
        })?;
    let (address, enum_name, case_name, case_value) = {
        let (slot, enum_name, case) =
            super::statics::resolve_enum_case(ctx.module, ctx.static_slots, &label).ok_or_else(
                || WasmError::Unsupported(format!("scoped constant {label} is not an enum case")),
            )?;
        (
            slot.address,
            enum_name.to_string(),
            case.name.clone(),
            case.value.clone(),
        )
    };
    let class_info = ctx
        .module
        .class_infos
        .get(&enum_name)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("enum {enum_name} has no class shape")))?;

    let cached = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(
        &format!("(i32.load offset={address} (i32.const 0))"),
        "the case singleton, or zero on first use",
    );
    ctx.fb.ins(&format!("local.set {}", cached), "cached singleton");
    ctx.fb
        .ins(&format!("(if (i32.eqz (local.get {}))", cached), "materialize once");
    ctx.fb.ins("(then", "first read of this case");
    let object = super::objects::emit_object_allocation(ctx, &enum_name, &class_info)?;
    // `name` is every case's property; a BACKED enum adds `value`.
    let (_, name_offset, _) = super::objects::resolve_property_slot(&class_info, "name")?;
    super::objects::emit_scalar_default(
        ctx,
        &object,
        name_offset,
        &crate::codegen::LiteralDefaultValue::Str(case_name.clone()),
        "name",
    )?;
    if let Some(value) = &case_value {
        let (_, value_offset, _) = super::objects::resolve_property_slot(&class_info, "value")?;
        let literal = match value {
            crate::types::EnumCaseValue::Int(number) => {
                crate::codegen::LiteralDefaultValue::Int(*number)
            }
            crate::types::EnumCaseValue::Str(text) => {
                crate::codegen::LiteralDefaultValue::Str(text.clone())
            }
        };
        super::objects::emit_scalar_default(ctx, &object, value_offset, &literal, "value")?;
    }
    ctx.fb.ins("i32.const 0", "static region base");
    ctx.fb.ins(&format!("local.get {}", object), "the fresh singleton");
    ctx.fb
        .ins(&format!("i32.store offset={address}"), "publish the singleton");
    ctx.fb.ins(
        &format!("(local.set {} (local.get {}))", cached, object),
        "use it for this read too",
    );
    ctx.fb.ins("))", "close the materialization guard");

    ctx.fb.ins(
        &format!("(call $__rt_incref (local.get {}))", cached),
        "a case read hands out an OWNED reference",
    );
    ctx.fb.ins(&format!("local.get {}", cached), "the case singleton");
    store_result(ctx, inst)
}

/// Drops the reference a static store CONSUMED, when the EIR handed one over.
///
/// A widened arithmetic result arrives as a freshly built Mixed cell with `own=owned` and
/// NO matching release — the EIR expects the store to take it. Narrowing reads the cell's
/// payload out and leaves the cell itself with no owner, so it is dropped here.
fn release_consumed_static_source(ctx: &mut FnCtx, value: ValueId, narrows: bool) -> Result<()> {
    if !narrows {
        return Ok(());
    }
    if !matches!(
        ctx.function.value(value).map(|v| v.ownership),
        Some(crate::ir::Ownership::Owned)
    ) {
        return Ok(());
    }
    ctx.emit_load_value(value)?;
    ctx.fb
        .ins("call $__rt_decref_any", "the narrowed cell had no other owner");
    Ok(())
}

/// Lowers `Op::LoadStaticProperty` (`Class::$prop`).
///
/// The slot is a compile-time address in static memory holding `value_lo` at +0 and
/// `value_hi` at +8 — the same shape an instance property slot has — so the read is a
/// direct load with no lookup. A string answers the stored `(ptr, len)` BORROWED: the
/// EIR's `acquire` persists the caller's own copy, exactly as the instance path does.
fn lower_load_static_property(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let (address, php_type) = {
        let slot = static_property_slot(ctx, inst)?;
        (slot.address, slot.php_type.codegen_repr())
    };
    match php_type {
        PhpType::Int | PhpType::Bool | PhpType::False => {
            ctx.fb
                .ins(&format!("(i64.load offset={address} (i32.const 0))"), "static property value");
        }
        PhpType::Float => {
            ctx.fb.ins(
                &format!("(f64.load offset={address} (i32.const 0))"),
                "static property value (float)",
            );
        }
        PhpType::Str => {
            ctx.fb.ins(
                &format!("(i32.wrap_i64 (i64.load offset={address} (i32.const 0)))"),
                "static property string pointer",
            );
            ctx.fb.ins(
                &format!("(i64.load offset={} (i32.const 0))", address + 8),
                "static property string length",
            );
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "load of a static property typed {other:?}"
            )))
        }
    }
    store_result(ctx, inst)
}

/// Lowers `Op::StoreStaticProperty` (`Class::$prop = v`).
///
/// A string slot owns its bytes, so the incoming value is persisted and the outgoing one
/// released first — and because the refcount helpers no-op below the heap, releasing an
/// initial LITERAL default is a no-op rather than a fault.
fn lower_store_static_property(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    let (address, php_type) = {
        let slot = static_property_slot(ctx, inst)?;
        (slot.address, slot.php_type.codegen_repr())
    };
    // A Mixed source narrows into the concrete slot through the same coercion an instance
    // property store uses; anything else is already in the slot's representation.
    let narrows = matches!(
        ctx.function.value(value).map(|v| v.ir_type),
        Some(IrType::Heap(IrHeapKind::Mixed))
    ) && php_type != PhpType::Mixed;
    match php_type {
        PhpType::Int | PhpType::Bool | PhpType::False => {
            // Materialize BEFORE the base address: narrowing runs its own loads and stores,
            // so leaving the base underneath it would have it consumed as an operand.
            let word = ctx.fresh_temp(ValType::I64);
            if narrows {
                super::transfer::emit_narrow_mixed_into(ctx, value, &php_type)?;
            } else {
                ctx.emit_load_value(value)?;
            }
            release_consumed_static_source(ctx, value, narrows)?;
            ctx.fb.ins(&format!("local.set {}", word), "the value to store");
            ctx.fb.ins("i32.const 0", "static region base");
            ctx.fb.ins(&format!("local.get {}", word), "the value to store");
            ctx.fb
                .ins(&format!("i64.store offset={address}"), "store the static property");
        }
        PhpType::Float => {
            let word = ctx.fresh_temp(ValType::F64);
            if narrows {
                super::transfer::emit_narrow_mixed_into(ctx, value, &php_type)?;
            } else {
                ctx.emit_load_value(value)?;
            }
            release_consumed_static_source(ctx, value, narrows)?;
            ctx.fb.ins(&format!("local.set {}", word), "the value to store");
            ctx.fb.ins("i32.const 0", "static region base");
            ctx.fb.ins(&format!("local.get {}", word), "the value to store");
            ctx.fb.ins(
                &format!("f64.store offset={address}"),
                "store the static property (float)",
            );
        }
        PhpType::Str => {
            let pointer = ctx.fresh_temp(ValType::I32);
            let length = ctx.fresh_temp(ValType::I64);
            if narrows {
                super::transfer::emit_narrow_mixed_into(ctx, value, &php_type)?;
            } else {
                ctx.emit_load_value(value)?;
            }
            ctx.fb.ins(&format!("local.set {}", length), "incoming string length");
            ctx.fb.ins(&format!("local.set {}", pointer), "incoming string pointer");
            // The slot TAKES the incoming reference rather than persisting a second copy:
            // the EIR hands one over (its `acquire` already made the owned copy) and emits no
            // release, so persisting again would leak exactly that copy. A bare literal has
            // no reference to take, and storing its static-data address is equally safe —
            // the refcount helpers no-op below the heap.
            // Release what the slot held; a literal default sits below the heap and no-ops.
            ctx.fb.ins(
                &format!("(i32.wrap_i64 (i64.load offset={address} (i32.const 0)))"),
                "the string the slot held",
            );
            ctx.fb
                .ins("call $__rt_decref_any", "release the previous value (null-safe)");
            ctx.fb.ins("i32.const 0", "static region base");
            ctx.fb.ins(
                &format!("(i64.extend_i32_u (local.get {}))", pointer),
                "the taken pointer as the slot's low word",
            );
            ctx.fb
                .ins(&format!("i64.store offset={address}"), "store the string pointer");
            ctx.fb.ins("i32.const 0", "static region base");
            ctx.fb.ins(&format!("local.get {}", length), "the taken length");
            ctx.fb.ins(
                &format!("i64.store offset={}", address + 8),
                "store the string length",
            );
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "store into a static property typed {other:?}"
            )))
        }
    }
    Ok(())
}

/// Lowers `Op::GcCollect`, the cycle-collection safe point `unset(...)` emits.
///
/// Refcounting cannot reclaim a reference cycle, so this is the only path on this target
/// that frees one. The pass structure lives in `super::gc`; the safe point is just its
/// call.
fn lower_gc_collect(ctx: &mut FnCtx) -> Result<()> {
    ctx.fb.ins(
        "call $__rt_gc_collect_cycles",
        "reclaim graphs only their own members still reference",
    );
    Ok(())
}

fn lower_loose_eq(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let left = operand(inst, 0)?;
    let right = operand(inst, 1)?;
    let lhs = ctx.value_repr(left)?.clone();
    let rhs = ctx.value_repr(right)?.clone();
    match (&lhs, &rhs) {
        (WasmRepr::I64(_), WasmRepr::I64(_)) => {
            ctx.emit_load_value(left)?;
            ctx.emit_load_value(right)?;
            ctx.fb.ins("i64.eq", "same-width scalar comparison");
            ctx.fb.ins("i64.extend_i32_u", "PHP booleans are i64 here");
        }
        (WasmRepr::F64(_), WasmRepr::F64(_)) => {
            ctx.emit_load_value(left)?;
            ctx.emit_load_value(right)?;
            ctx.fb.ins("f64.eq", "float comparison; NaN equals nothing");
            ctx.fb.ins("i64.extend_i32_u", "PHP booleans are i64 here");
        }
        (WasmRepr::Str { .. }, WasmRepr::Str { .. }) => {
            ctx.emit_load_value(left)?;
            ctx.emit_load_value(right)?;
            ctx.fb.ins(
                "call $__rt_str_loose_eq",
                "numeric only when BOTH sides are fully numeric",
            );
        }
        (WasmRepr::I64(_), WasmRepr::F64(_)) => {
            ctx.emit_load_value(left)?;
            ctx.fb.ins("f64.convert_i64_s", "PHP widens the integer, losing precision as it goes");
            ctx.emit_load_value(right)?;
            ctx.fb.ins("f64.eq", "compare as doubles");
            ctx.fb.ins("i64.extend_i32_u", "PHP booleans are i64 here");
        }
        (WasmRepr::F64(_), WasmRepr::I64(_)) => {
            ctx.emit_load_value(left)?;
            ctx.emit_load_value(right)?;
            ctx.fb.ins("f64.convert_i64_s", "PHP widens the integer, losing precision as it goes");
            ctx.fb.ins("f64.eq", "compare as doubles");
            ctx.fb.ins("i64.extend_i32_u", "PHP booleans are i64 here");
        }
        (other_left, other_right) => {
            return Err(WasmError::Unsupported(format!(
                "loose comparison of {other_left:?} against {other_right:?}"
            )))
        }
    }
    if inst.op == Op::LooseNotEq {
        ctx.fb.ins("i64.eqz", "!= is the negation of ==");
        ctx.fb.ins("i64.extend_i32_u", "PHP booleans are i64 here");
    }
    store_result(ctx, inst)
}

/// Lowers `Op::ArrayToMixed`: EIR's own `array<T>` -> `array<mixed>` widening.
///
/// EIR emits this where a concrete array is stored somewhere typed `array<mixed>`, and marks the
/// result `own=owned` — so unlike the call-argument conversion, which synthesizes a temporary the
/// call site has to free, this result is an EIR value the EIR releases itself.
fn lower_array_to_mixed(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let source = operand(inst, 0)?;
    let element = match ctx.function.value(source).map(|v| v.php_type.codegen_repr()) {
        Some(PhpType::Array(element)) => *element,
        other => {
            return Err(WasmError::Unsupported(format!(
                "array_to_mixed takes an indexed array, got {other:?}"
            )))
        }
    };
    let (tag, elem_size) = super::transfer::array_widen_shape(&element).ok_or_else(|| {
        WasmError::Unsupported(format!("array_to_mixed has no element copy for {element:?}"))
    })?;
    ctx.emit_load_value(source)?;
    ctx.fb.ins(&format!("i64.const {tag}"), "cell tag for every element");
    ctx.fb.ins(&format!("i64.const {elem_size}"), "source slot stride");
    ctx.fb.ins(
        "call $__rt_array_widen_to_mixed",
        "copy into a fresh owned mixed-cell array",
    );
    store_result(ctx, inst)
}

/// Lowers `Op::ArrayPush`. Appends via the runtime (which may reallocate) and
/// writes the returned pointer back into the operand value's local and its source
/// slot, so `$arr[] = v` keeps the variable pointing at the live array — exactly
/// what the native backend does.
fn lower_array_push(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let array = operand(inst, 0)?;
    let value = operand(inst, 1)?;
    let value_repr = ctx.value_repr(value)?.clone();
    // EIR pushes RAW scalars into an `array<mixed>` — that is what a heterogeneous
    // literal like `[1, "a", 2.5]` emits — and leaves the boxing to the backend, the
    // way the native one does. Dispatching on the source repr alone would send those
    // to the int/string helpers and write the wrong slot layout.
    // A CONTAINER destined for a Mixed-cell array is boxed too, for the same reason: the slot
    // holds a CELL pointer, and storing the container's own pointer there read back as the
    // container's length reinterpreted as a tag — a nested array printed as a denormal float.
    if array_stores_mixed_cells(ctx, array)
        && (!matches!(value_repr, WasmRepr::Ptr(_))
            || matches!(
                ctx.function.value(value).map(|v| v.ir_type),
                Some(IrType::Heap(
                    IrHeapKind::Object | IrHeapKind::Array | IrHeapKind::Hash
                ))
            ))
    {
        return push_boxed_scalar(ctx, inst, array, value, &value_repr);
    }
    match value_repr {
        WasmRepr::I64(_) => {
            ctx.emit_load_value(array)?;
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins("call $__rt_array_push_int", "append int (may reallocate)");
        }
        WasmRepr::F64(_) => {
            ctx.emit_load_value(array)?;
            ctx.emit_load_value(value)?;
            ctx.fb.ins(
                "call $__rt_array_push_float",
                "append float into a value_type-2 array (may reallocate)",
            );
        }
        WasmRepr::Str { .. } => {
            ctx.emit_load_value(array)?;
            ctx.emit_load_value(value)?; // string pointer (i32) + length (i64)
            ctx.fb
                .ins("call $__rt_array_push_str", "append string (persists + may reallocate)");
        }
        WasmRepr::Ptr(_) => {
            // A Mixed/Union value is a kind-5 Mixed-cell pointer pushed into a
            // value_type-7 mixed-cell array (16-byte slots, cell at slot+0) — the
            // shape the closure/FCC arg buffer uses. The array shares ownership of
            // the cell (incref) and the EIR releases the operand after the push
            // (`release_indexed_array_write_operand`), mirroring the native
            // `__rt_array_push_refcounted` contract (`__rt_array_push_mixed` stores
            // the cell BORROWED). Other heap kinds (array/hash/object containers)
            // have no WASM append helper yet and stay unsupported.
            let value_ir = ctx.function.value(value).map(|v| v.ir_type);
            if !matches!(
                value_ir,
                Some(IrType::Heap(
                    IrHeapKind::Mixed
                        | IrHeapKind::Union
                        | IrHeapKind::Object
                        | IrHeapKind::Array
                        | IrHeapKind::Hash
                ))
            ) {
                return Err(WasmError::Unsupported(format!(
                    "array_push of {:?} on wasm32-wasi",
                    value_repr
                )));
            }
            // An object or nested-array element is the same contract with a different helper: the
            // array takes a SHARE (incref here), and the EIR releases the operand right after the
            // push. The two differ only in the `value_type` the empty array is shaped to.
            //
            // This is the CONCRETE-element form only. A container pushed into an `array<mixed>`
            // must be BOXED into a cell instead — routing it here stored a raw pointer in a
            // cell-strided array, which read back as a denormal float and lost the container.
            let element_is_mixed = matches!(
                ctx.function
                    .value(array)
                    .map(|v| v.php_type.codegen_repr()),
                Some(PhpType::Array(element)) if matches!(element.codegen_repr(), PhpType::Mixed)
            );
            if let Some(container_tag) = match value_ir {
                _ if element_is_mixed => None,
                Some(IrType::Heap(IrHeapKind::Object)) => Some(4),
                Some(IrType::Heap(IrHeapKind::Array)) => Some(5),
                Some(IrType::Heap(IrHeapKind::Hash)) => Some(6),
                _ => None,
            } {
                let obj = ctx.fresh_temp(ValType::I32);
                ctx.emit_load_value(value)?;
                ctx.fb.ins(&format!("local.set {}", obj), "container to append");
                ctx.fb.ins(
                    &format!("(call $__rt_incref (local.get {}))", obj),
                    "the array shares the child (the EIR releases the operand after the push)",
                );
                ctx.emit_load_value(array)?;
                ctx.fb.ins(&format!("local.get {}", obj), "child pointer for the append");
                ctx.fb
                    .ins(&format!("i64.const {container_tag}"), "value_type for the slot");
                ctx.fb.ins(
                    "call $__rt_array_push_ptr",
                    "append into a pointer-slot array (may reallocate)",
                );
                stamp_bool_array_result_type(ctx, array, value)?;
                ctx.emit_store_value(array)?;
                if let Some(slot) = value_source_slot(ctx, array) {
                    let array_ref = ctx.value_repr(array)?.local_refs();
                    let slot_ref = ctx.slot_repr(slot)?.local_refs();
                    if array_ref.len() == 1 && slot_ref.len() == 1 {
                        ctx.fb.ins(
                            &format!("local.get {}", array_ref[0]),
                            "reallocated array pointer",
                        );
                        ctx.fb
                            .ins(&format!("local.set {}", slot_ref[0]), "write back to the array slot");
                    }
                }
                return Ok(());
            }
            let cell = ctx.fresh_temp(ValType::I32);
            ctx.emit_load_value(value)?; // kind-5 Mixed-cell pointer (i32)
            ctx.fb.ins(&format!("local.set {}", cell), "mixed cell to append");
            ctx.fb.ins(
                &format!("(call $__rt_incref (local.get {}))", cell),
                "array shares the mixed cell (the EIR releases the operand after the push)",
            );
            ctx.emit_load_value(array)?;
            ctx.fb
                .ins(&format!("local.get {}", cell), "mixed cell pointer for the append");
            ctx.fb.ins(
                "call $__rt_array_push_mixed",
                "append mixed cell into a value_type-7 array (may reallocate)",
            );
        }
        other => return Err(WasmError::Unsupported(format!("array_push of {:?}", other))),
    }
    stamp_bool_array_result_type(ctx, array, value)?;
    // The runtime returned the (possibly reallocated) pointer: store it back into
    // the array operand value's local.
    ctx.emit_store_value(array)?;
    // And mirror it to the source slot so a later LoadLocal sees the live pointer.
    if let Some(slot) = value_source_slot(ctx, array) {
        let array_ref = ctx.value_repr(array)?.local_refs();
        let slot_ref = ctx.slot_repr(slot)?.local_refs();
        if array_ref.len() == 1 && slot_ref.len() == 1 {
            ctx.fb
                .ins(&format!("local.get {}", array_ref[0]), "reallocated array pointer");
            ctx.fb
                .ins(&format!("local.set {}", slot_ref[0]), "write back to the array slot");
        }
    }
    Ok(())
}

/// Returns whether this array operand stores boxed Mixed cells (`value_type` 7).
fn array_stores_mixed_cells(ctx: &FnCtx, array: ValueId) -> bool {
    matches!(
        ctx.function.value(array).map(|v| v.php_type.codegen_repr()),
        Some(PhpType::Array(element)) if *element == PhpType::Mixed
    )
}

/// Appends a raw scalar to a Mixed-cell array by boxing it at the push site.
///
/// The tag comes from the operand's PHP type — `Void` is EIR's `const_null` — and the
/// payload is the cell's `(lo, hi)` pair: a string puts its pointer in `lo` and its length
/// in `hi`, every other scalar leaves `hi` zero.
///
/// `__rt_mixed_from_value` hands back a cell with one reference and PERSISTS string bytes;
/// `__rt_array_push_mixed` stores the cell BORROWED. So this transfers its single reference
/// to the array and must NOT incref — unlike the already-boxed path, whose operand the EIR
/// releases afterwards. The array's `__rt_array_free_deep` releases every cell.
fn push_boxed_scalar(
    ctx: &mut FnCtx,
    inst: &Instruction,
    array: ValueId,
    value: ValueId,
    value_repr: &WasmRepr,
) -> Result<()> {
    let php = ctx
        .function
        .value(value)
        .map(|v| v.php_type.codegen_repr())
        .unwrap_or(PhpType::Mixed);
    let cell = ctx.fresh_temp(ValType::I32);
    match value_repr {
        WasmRepr::I64(_) => {
            let tag = match php {
                PhpType::Int => 0,
                PhpType::Bool | PhpType::False => 3,
                PhpType::Void => 8,
                other => {
                    return Err(WasmError::Unsupported(format!(
                        "array_push boxes no tag for {other:?}"
                    )))
                }
            };
            ctx.fb.ins(&format!("i64.const {tag}"), "scalar tag");
            ctx.emit_load_value(value)?;
            ctx.fb.ins("i64.const 0", "no high payload");
        }
        WasmRepr::F64(_) => {
            ctx.fb.ins("i64.const 2", "float tag");
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins("i64.reinterpret_f64", "the cell carries the float's bits");
            ctx.fb.ins("i64.const 0", "no high payload");
        }
        WasmRepr::Str { .. } => {
            // The pointer and length arrive in that order, but the cell wants the tag
            // first — so park them before composing the call.
            let ptr = ctx.fresh_temp(ValType::I32);
            let len = ctx.fresh_temp(ValType::I64);
            ctx.emit_load_value(value)?;
            ctx.fb.ins(&format!("local.set {}", len), "string length");
            ctx.fb.ins(&format!("local.set {}", ptr), "string pointer");
            ctx.fb.ins("i64.const 1", "string tag");
            ctx.fb.ins(
                &format!("(i64.extend_i32_u (local.get {}))", ptr),
                "pointer payload",
            );
            ctx.fb.ins(&format!("local.get {}", len), "length payload");
        }
        WasmRepr::Ptr(_) => {
            // A container into a Mixed-cell array. The EIR emits NO release after this push,
            // unlike the concrete `array<Object>` one — so the operand's single reference is
            // transferred here. `__rt_mixed_from_value` increfs a refcounted child, so the
            // operand is released right after to leave the cell as the only owner.
            let tag = match ctx.function.value(value).map(|v| v.ir_type) {
                Some(IrType::Heap(IrHeapKind::Array)) => 4,
                Some(IrType::Heap(IrHeapKind::Hash)) => 5,
                _ => 6,
            };
            ctx.fb
                .ins(&format!("i64.const {tag}"), "container tag (4 array, 5 hash, 6 object)");
            ctx.emit_load_value(value)?;
            ctx.fb.ins("i64.extend_i32_u", "ptr -> lo");
            ctx.fb.ins("i64.const 0", "no high payload");
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "array_push boxes no {other:?} into a mixed cell"
            )))
        }
    }
    ctx.fb.ins(
        "call $__rt_mixed_from_value",
        "box the scalar (one reference, handed to the array)",
    );
    if matches!(value_repr, WasmRepr::Ptr(_)) {
        // The boxing increfed; drop the operand's own reference so the cell owns it alone.
        let boxed = ctx.fresh_temp(ValType::I32);
        ctx.fb.ins(&format!("local.set {}", boxed), "boxed object cell");
        ctx.emit_load_value(value)?;
        ctx.fb
            .ins("call $__rt_decref_any", "the cell owns the object now");
        ctx.fb.ins(&format!("local.get {}", boxed), "boxed object cell");
    }
    ctx.fb.ins(&format!("local.set {}", cell), "boxed element");
    ctx.emit_load_value(array)?;
    ctx.fb
        .ins(&format!("local.get {}", cell), "mixed cell pointer");
    ctx.fb.ins(
        "call $__rt_array_push_mixed",
        "append into a value_type-7 array (may reallocate)",
    );
    // Deliberately NOT stamp_bool_array_result_type: pushing a bool would restamp the
    // array's value_type to 3 (scalar), and `__rt_array_free_deep` would then skip the
    // child loop and leak every cell. The bool lives INSIDE its cell here; the array's
    // value_type 7 that `__rt_array_push_mixed` writes is the correct one.
    ctx.emit_store_value(array)?;
    if let Some(slot) = value_source_slot(ctx, array) {
        let array_ref = ctx.value_repr(array)?.local_refs();
        let slot_ref = ctx.slot_repr(slot)?.local_refs();
        if array_ref.len() == 1 && slot_ref.len() == 1 {
            ctx.fb
                .ins(&format!("local.get {}", array_ref[0]), "reallocated array pointer");
            ctx.fb
                .ins(&format!("local.set {}", slot_ref[0]), "write back to the array slot");
        }
    }
    let _ = inst;
    Ok(())
}

/// Lowers `Op::ArraySet` (`$a[i] = v`). Calls the copy-on-write-aware runtime
/// setter (`__rt_array_set_int`/`__rt_array_set_str`), which may clone or
/// reallocate the array, then writes the returned pointer back into the array
/// operand's value local and its source slot — mirroring `lower_array_push` and
/// the native backend. `ArraySet` produces no result value; the array operand IS
/// the in/out storage.
fn lower_array_set(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let array = operand(inst, 0)?;
    let index = operand(inst, 1)?;
    let value = operand(inst, 2)?;
    // The index must be a single i64 (EIR coerces indexed-array indices to int).
    match ctx.value_repr(index)? {
        WasmRepr::I64(_) => {}
        other => {
            return Err(WasmError::Unsupported(format!(
                "array_set index of {:?}",
                other
            )))
        }
    }
    let value_repr = ctx.value_repr(value)?.clone();
    match value_repr {
        WasmRepr::I64(_) => {
            ctx.emit_load_value(array)?; // array pointer
            ctx.emit_load_value(index)?; // index (i64)
            ctx.emit_load_value(value)?; // scalar value (i64)
            ctx.fb
                .ins("call $__rt_array_set_int", "set scalar element (COW, may reallocate)");
        }
        WasmRepr::Str { .. } => {
            ctx.emit_load_value(array)?; // array pointer
            ctx.emit_load_value(index)?; // index (i64)
            ctx.emit_load_value(value)?; // string pointer (i32) + length (i64)
            ctx.fb
                .ins("call $__rt_array_set_str", "set string element (COW, persists, may reallocate)");
        }
        other => return Err(WasmError::Unsupported(format!("array_set of {:?}", other))),
    }
    stamp_bool_array_result_type(ctx, array, value)?;
    // The runtime returned the (possibly cloned/reallocated) pointer: store it
    // back into the array operand value's local.
    ctx.emit_store_value(array)?;
    // And mirror it to the source slot so a later LoadLocal sees the live pointer.
    if let Some(slot) = value_source_slot(ctx, array) {
        let array_ref = ctx.value_repr(array)?.local_refs();
        let slot_ref = ctx.slot_repr(slot)?.local_refs();
        if array_ref.len() == 1 && slot_ref.len() == 1 {
            ctx.fb
                .ins(&format!("local.get {}", array_ref[0]), "reallocated array pointer");
            ctx.fb
                .ins(&format!("local.set {}", slot_ref[0]), "write back to the array slot");
        }
    }
    Ok(())
}

/// Preserves the boolean runtime tag after the shared integer array mutators.
///
/// The scalar helpers deliberately normalize an empty array to 8-byte slots and
/// clear its value-type bits. PHP booleans share that payload width with ints,
/// so the statically typed WASM path must restore tag 3 before promotion to a
/// hash or any other runtime operation that observes element tags.
fn stamp_bool_array_result_type(
    ctx: &mut FnCtx,
    array: ValueId,
    value: ValueId,
) -> Result<()> {
    let array_is_bool = ctx
        .function
        .value(array)
        .map(|value| value.php_type.codegen_repr())
        .and_then(|php_type| match php_type {
            PhpType::Array(element) => Some(matches!(
                element.codegen_repr(),
                PhpType::Bool | PhpType::False
            )),
            _ => None,
        })
        .unwrap_or(false);
    let value_is_bool = ctx
        .function
        .value(value)
        .map(|value| {
            matches!(
                value.php_type.codegen_repr(),
                PhpType::Bool | PhpType::False
            )
        })
        .unwrap_or(false);
    if !array_is_bool && !value_is_bool {
        return Ok(());
    }
    let result = ctx.fresh_temp(ValType::I32);
    ctx.fb
        .ins(&format!("local.set {}", result), "save mutated boolean array pointer");
    ctx.fb
        .ins(&format!("local.get {}", result), "boolean array pointer");
    ctx.fb.ins("i32.const 8", "kind-word offset");
    ctx.fb.ins("i32.sub", "address of array kind word");
    ctx.fb
        .ins(&format!("local.get {}", result), "boolean array pointer");
    ctx.fb.ins("i32.const 8", "kind-word offset");
    ctx.fb.ins("i32.sub", "address of array kind word");
    ctx.fb.ins("i64.load", "load indexed-array kind word");
    ctx.fb.ins("i64.const -32513", "clear value-type bits 8..14");
    ctx.fb.ins("i64.and", "kind word without value type");
    ctx.fb.ins("i64.const 768", "boolean value type 3 shifted by 8");
    ctx.fb.ins("i64.or", "stamp boolean value type");
    ctx.fb.ins("i64.store", "store indexed-array kind word");
    ctx.fb
        .ins(&format!("local.get {}", result), "reload mutated boolean array pointer");
    Ok(())
}

/// Lowers `Op::MixedBox`: boxes a scalar/string/heap/tagged value into a Mixed
/// cell via `__rt_mixed_from_value`, picking the static or runtime tag from the
/// operand representation. A value already stored as a Mixed cell is forwarded.
fn lower_mixed_box(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    let repr = ctx.value_repr(value)?.clone();
    let php = ctx.function.value(value).map(|v| v.php_type.codegen_repr());
    let ir = ctx.function.value(value).map(|v| v.ir_type);
    match repr {
        WasmRepr::I64(local) => {
            // Int -> tag 0, Bool -> tag 3, null (ConstNull, PhpType::Void) -> tag 8
            // (all three are i64-represented; `lower_boxed_null` reaches this arm).
            let tag = match php {
                Some(PhpType::Bool) => 3,
                Some(PhpType::Void) => 8,
                _ => 0,
            };
            ctx.fb.ins(&format!("i64.const {}", tag), "mixed tag (int/bool/null)");
            ctx.fb.ins(&format!("local.get {}", local), "scalar -> lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb
                .ins("call $__rt_mixed_from_value", "box scalar into a mixed cell");
            store_result(ctx, inst)
        }
        WasmRepr::F64(local) => {
            ctx.fb.ins("i64.const 2", "mixed tag (float)");
            ctx.fb.ins(&format!("local.get {}", local), "float value");
            ctx.fb.ins("i64.reinterpret_f64", "float bits -> lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb.ins("call $__rt_mixed_from_value", "box float");
            store_result(ctx, inst)
        }
        WasmRepr::Str { ptr, len } => {
            ctx.fb.ins("i64.const 1", "mixed tag (string)");
            ctx.fb.ins(&format!("local.get {}", ptr), "string pointer");
            ctx.fb.ins("i64.extend_i32_u", "ptr -> lo");
            ctx.fb.ins(&format!("local.get {}", len), "string length -> hi");
            ctx.fb
                .ins("call $__rt_mixed_from_value", "box string (persists a copy)");
            store_result(ctx, inst)
        }
        WasmRepr::Ptr(local) => match ir {
            // A value that is already a Mixed cell: forward it unchanged.
            Some(IrType::Heap(IrHeapKind::Mixed)) => forward_value(ctx, value, inst),
            Some(IrType::Heap(kind)) => {
                let tag = match kind {
                    IrHeapKind::Array => 4,
                    IrHeapKind::Hash => 5,
                    IrHeapKind::Object => 6,
                    other => {
                        return Err(WasmError::Unsupported(format!(
                            "mixed_box of heap kind {:?}",
                            other
                        )))
                    }
                };
                ctx.fb.ins(&format!("i64.const {}", tag), "mixed tag (heap kind)");
                ctx.fb.ins(&format!("local.get {}", local), "heap pointer");
                ctx.fb.ins("i64.extend_i32_u", "ptr -> lo");
                ctx.fb.ins("i64.const 0", "hi unused");
                ctx.fb
                    .ins("call $__rt_mixed_from_value", "box heap value (increfs the child)");
                store_result(ctx, inst)
            }
            _ => Err(WasmError::Unsupported("mixed_box of a non-heap pointer".to_string())),
        },
        WasmRepr::Tagged { payload, tag }
            if ir == Some(IrType::TaggedScalar) && php == Some(PhpType::TaggedScalar) =>
        {
            ctx.fb
                .ins(&format!("local.get {}", tag), "tagged scalar runtime tag");
            ctx.fb.ins("i64.extend_i32_u", "tag -> i64");
            ctx.fb
                .ins(&format!("local.get {}", payload), "tagged scalar payload");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb
                .ins("call $__rt_mixed_from_value", "box tagged scalar");
            store_result(ctx, inst)
        }
        WasmRepr::Tagged { .. } => Err(WasmError::Unsupported(format!(
            "mixed_box of invalid tagged storage {:?}/{:?}",
            ir, php
        ))),
        WasmRepr::Void => Err(WasmError::Unsupported("mixed_box of void".to_string())),
    }
}

/// Lowers `Op::MixedTagOf`: returns the runtime tag integer of a Mixed value by
/// unboxing it and keeping only the tag result.
fn lower_mixed_tag_of(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb
        .ins("call $__rt_mixed_unbox", "unbox -> (tag, lo, hi)");
    ctx.fb.ins("drop", "discard hi");
    ctx.fb.ins("drop", "discard lo");
    store_result(ctx, inst)
}

/// Extracts the iterator result, source, element type, and container kind from
/// an `IterStart` instruction.
fn iter_start_metadata(
    ctx: &FnCtx,
    inst: &Instruction,
) -> Result<(ValueId, ValueId, PhpType, bool)> {
    let iter = inst
        .result
        .ok_or_else(|| WasmError::Unsupported("iter_start without a result".to_string()))?;
    let source = operand(inst, 0)?;
    let src_php = ctx
        .function
        .value(source)
        .map(|v| v.php_type.codegen_repr());
    let (elem, is_hash) = match src_php {
        Some(PhpType::Array(inner)) => (inner.codegen_repr(), false),
        Some(PhpType::AssocArray { value, .. }) => (value.codegen_repr(), true),
        Some(other) => {
            return Err(WasmError::Unsupported(format!("foreach over {:?}", other)))
        }
        None => return Err(WasmError::Unsupported("iter_start source has no type".to_string())),
    };
    Ok((iter, source, elem, is_hash))
}

/// Reserves locals for every iterator before block bodies are lowered.
///
/// Block ids reflect construction order, and inlined control flow can place a
/// loop header before the block containing its `IterStart`. This prepass removes
/// the previous compile-time dependency on block storage order.
pub(super) fn reserve_iterators(ctx: &mut FnCtx) -> Result<()> {
    let starts: Vec<Instruction> = ctx
        .function
        .blocks
        .iter()
        .flat_map(|block| block.instructions.iter())
        .filter_map(|inst_id| ctx.function.instruction(*inst_id))
        .filter(|inst| inst.op == Op::IterStart)
        .cloned()
        .collect();
    for inst in starts {
        let (iter, _, elem, is_hash) = iter_start_metadata(ctx, &inst)?;
        ctx.iter_reserve(iter, elem, is_hash);
    }
    Ok(())
}

/// Lowers `Op::IterStart` by initializing the iterator locals reserved in the
/// function prepass.
fn lower_iter_start(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let (iter, source, _, _) = iter_start_metadata(ctx, inst)?;
    ctx.iter_initialize(iter, source)
}

/// Lowers `Op::IterNext` and pushes the i64 loop-continue boolean the header's `CondBr`
/// consumes. For an indexed array it pre-increments the cursor and tests `cursor <
/// length`. For a hash it calls `__rt_hash_iter_next(source, cursor)`, which advances the
/// slot cursor in insertion order and returns `(new_cursor, has_more)`; the new cursor is
/// stored back and `has_more` becomes the loop condition.
fn lower_iter_next(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let iter = operand(inst, 0)?;
    let slots = ctx.iter_slots(iter)?;
    let (src, cur, is_hash) = (slots.source.clone(), slots.cursor.clone(), slots.is_hash);
    if is_hash {
        ctx.fb.ins(&format!("local.get {}", src), "hash source");
        ctx.fb.ins(&format!("local.get {}", cur), "current slot cursor");
        ctx.fb
            .ins("call $__rt_hash_iter_next", "advance to the next entry in insertion order");
        // Returns (new_cursor, has_more) with has_more on top.
        let has_more = ctx.fresh_temp(ValType::I64);
        ctx.fb.ins(&format!("local.set {}", has_more), "captured has_more");
        ctx.fb.ins(&format!("local.set {}", cur), "store advanced slot cursor");
        ctx.fb.ins(&format!("local.get {}", has_more), "has_more for the loop CondBr");
        return store_result(ctx, inst);
    }
    ctx.fb.ins(&format!("local.get {}", cur), "current cursor");
    ctx.fb.ins("i64.const 1", "advance by one");
    ctx.fb.ins("i64.add", "cursor + 1");
    ctx.fb.ins(&format!("local.set {}", cur), "store advanced cursor");
    ctx.fb.ins(&format!("local.get {}", cur), "cursor");
    ctx.fb.ins(&format!("local.get {}", src), "source array");
    ctx.fb.ins("i64.load", "array length @ +0");
    ctx.fb.ins("i64.lt_s", "cursor < length");
    ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
    store_result(ctx, inst)
}

/// Lowers `Op::IterCurrentKey`. For an indexed array the key is the cursor (boxed into a
/// Mixed int when the result is Mixed, else the raw i64). For a hash it delegates to
/// `inst_hash::lower_hash_iter_key`, which reads the key fields from the current entry.
fn lower_iter_current_key(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let iter = operand(inst, 0)?;
    let slots = ctx.iter_slots(iter)?;
    if slots.is_hash {
        let (src, cur) = (slots.source.clone(), slots.cursor.clone());
        return super::inst_hash::lower_hash_iter_key(ctx, inst, &src, &cur);
    }
    let cur = slots.cursor.clone();
    let result = inst
        .result
        .ok_or_else(|| WasmError::Unsupported("iter_current_key without a result".to_string()))?;
    let result_repr = ctx.value_repr(result)?.clone();
    match result_repr {
        WasmRepr::I64(_) => {
            ctx.fb.ins(&format!("local.get {}", cur), "key = cursor");
            store_result(ctx, inst)
        }
        WasmRepr::Ptr(_) | WasmRepr::Tagged { .. } => {
            ctx.fb.ins("i64.const 0", "mixed tag (int key)");
            ctx.fb.ins(&format!("local.get {}", cur), "cursor -> lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb
                .ins("call $__rt_mixed_from_value", "box the integer key");
            store_result(ctx, inst)
        }
        other => Err(WasmError::Unsupported(format!("iter key into {:?}", other))),
    }
}

/// Lowers `Op::IterCurrentValue`. For an indexed array it reads `source[cursor]` with the
/// element getter picked from the element type, boxing into a Mixed cell when the value
/// variable is Mixed (the usual case). For a hash it delegates to
/// `inst_hash::lower_hash_iter_value`, which reads the value fields from the current
/// entry and reconstructs an owned result.
fn lower_iter_current_value(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let iter = operand(inst, 0)?;
    let slots = ctx.iter_slots(iter)?;
    if slots.is_hash {
        let (src, cur) = (slots.source.clone(), slots.cursor.clone());
        return super::inst_hash::lower_hash_iter_value(ctx, inst, &src, &cur);
    }
    let (src, cur, elem) = (slots.source.clone(), slots.cursor.clone(), slots.elem.clone());
    let result = inst
        .result
        .ok_or_else(|| WasmError::Unsupported("iter_current_value without a result".to_string()))?;
    let result_repr = ctx.value_repr(result)?.clone();
    // "Boxed" means the binding is a Mixed CELL, not merely a heap pointer: an object element is
    // a `Ptr` too, and boxing it into a cell when the slot wants the object itself binds the wrong
    // thing — the property read then finds an empty slot rather than the object's.
    let boxed = matches!(result_repr, WasmRepr::Ptr(_) | WasmRepr::Tagged { .. })
        && matches!(
            ctx.function.value(result).map(|v| v.php_type.codegen_repr()),
            Some(PhpType::Mixed) | None
        );
    match &elem {
        // `Void` is an empty array's element type — the loop body never runs, so the element is
        // never actually read and the integer contract stands in for it.
        PhpType::Int | PhpType::Bool | PhpType::Void => {
            let tag = if matches!(elem, PhpType::Bool) { 3 } else { 0 };
            if boxed {
                ctx.fb.ins(&format!("i64.const {}", tag), "mixed tag");
            }
            ctx.fb.ins(&format!("local.get {}", src), "source array");
            ctx.fb.ins(&format!("local.get {}", cur), "cursor index");
            ctx.fb
                .ins("call $__rt_array_get_int", "foreach element (int)");
            if boxed {
                ctx.fb.ins("i64.const 0", "hi unused");
                ctx.fb
                    .ins("call $__rt_mixed_from_value", "box the element");
            }
            store_result(ctx, inst)
        }
        PhpType::Mixed => {
            // A Mixed-cell array has 16-byte slots with the cell pointer at slot+0. The binding is
            // OWNED (`iter_current_value` is `own=owned` and the EIR releases it), and the array
            // keeps its own share, so the read increfs.
            let cell = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(
                &format!(
                    "(i32.wrap_i64 (i64.load (i32.add (i32.add (local.get {}) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get {}) (i64.const 16))))))",
                    src, cur
                ),
                "foreach element (mixed cell, borrowed)",
            );
            ctx.fb.ins(&format!("local.set {}", cell), "borrowed cell");
            ctx.fb.ins(
                &format!("(call $__rt_incref (local.get {}))", cell),
                "foreach binds an OWNED value; the EIR releases it",
            );
            ctx.fb.ins(&format!("local.get {}", cell), "owned cell");
            store_result(ctx, inst)
        }
        PhpType::Object(_) | PhpType::Array(_) | PhpType::AssocArray { .. } => {
            // The accessor answers a BORROWED pointer; who increfs depends on the destination.
            // Object (value_type 4) and nested-array (5) slots are both a pointer at slot+0, so
            // the READ is shared — but the boxing tag is NOT: a cell tagged 6 is an object and
            // tagged 4 an array, and every later reader dispatches on that tag.
            let cell_tag = match elem {
                PhpType::Array(_) => 4,
                PhpType::AssocArray { .. } => 5,
                _ => 6,
            };
            let object = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.get {}", src), "source array");
            ctx.fb.ins(&format!("local.get {}", cur), "cursor index");
            ctx.fb
                .ins("call $__rt_array_get_object", "foreach element (container, borrowed)");
            ctx.fb.ins(&format!("local.set {}", object), "borrowed element");
            if boxed {
                ctx.fb.ins(
                    &format!("i64.const {cell_tag}"),
                    "mixed tag (4 = array, 6 = object)",
                );
                ctx.fb
                    .ins(&format!("(i64.extend_i32_u (local.get {}))", object), "ptr -> lo");
                ctx.fb.ins("i64.const 0", "hi unused");
                // `__rt_mixed_from_value` increfs a refcounted child itself.
                ctx.fb
                    .ins("call $__rt_mixed_from_value", "box the container element");
            } else {
                ctx.fb.ins(
                    &format!("(call $__rt_incref (local.get {}))", object),
                    "foreach binds an OWNED value; the EIR releases it",
                );
                ctx.fb.ins(&format!("local.get {}", object), "owned element");
            }
            store_result(ctx, inst)
        }
        PhpType::Float => {
            ctx.fb.ins(&format!("local.get {}", src), "source array");
            ctx.fb.ins(&format!("local.get {}", cur), "cursor index");
            ctx.fb
                .ins("call $__rt_array_get_float", "foreach element (float)");
            if boxed {
                ctx.fb.ins("i64.reinterpret_f64", "the cell carries the float's bits");
                ctx.fb.ins("i64.const 0", "hi unused");
                ctx.fb
                    .ins("call $__rt_mixed_from_value", "box the element");
            }
            store_result(ctx, inst)
        }
        PhpType::Str => {
            if boxed {
                let tmp_len = ctx.fresh_temp(ValType::I64);
                let tmp_ptr = ctx.fresh_temp(ValType::I32);
                ctx.fb.ins(&format!("local.get {}", src), "source array");
                ctx.fb.ins(&format!("local.get {}", cur), "cursor index");
                ctx.fb
                    .ins("call $__rt_array_get_str", "foreach element (string)");
                ctx.fb.ins(&format!("local.set {}", tmp_len), "element length");
                ctx.fb.ins(&format!("local.set {}", tmp_ptr), "element pointer");
                ctx.fb.ins("i64.const 1", "mixed tag (string)");
                ctx.fb.ins(&format!("local.get {}", tmp_ptr), "ptr");
                ctx.fb.ins("i64.extend_i32_u", "ptr -> lo");
                ctx.fb.ins(&format!("local.get {}", tmp_len), "len -> hi");
                ctx.fb
                    .ins("call $__rt_mixed_from_value", "box the string element");
                store_result(ctx, inst)
            } else {
                ctx.fb.ins(&format!("local.get {}", src), "source array");
                ctx.fb.ins(&format!("local.get {}", cur), "cursor index");
                ctx.fb
                    .ins("call $__rt_array_get_str", "foreach element (string)");
                store_result(ctx, inst)
            }
        }
        other => Err(WasmError::Unsupported(format!(
            "foreach value of element type {:?}",
            other
        ))),
    }
}

/// Lowers `IsNull` from PHP type metadata and boxed/tagged runtime tags.
fn lower_is_null(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    emit_is_null_test(ctx, operand(inst, 0)?)?;
    store_result(ctx, inst)
}

/// Pushes 1 when the value is PHP `null`, 0 otherwise.
///
/// Shared by `Op::IsNull` and the `isset` language construct, which is its negation — one set of
/// per-representation tag rules rather than two that can drift.
fn emit_is_null_test(ctx: &mut FnCtx, op0: crate::ir::ValueId) -> Result<()> {
    let repr = ctx.value_repr(op0)?.clone();
    let php_type = ctx.value_php_type(op0)?.clone();
    let ir_type = ctx
        .function
        .value(op0)
        .map(|value| value.ir_type)
        .ok_or_else(|| WasmError::Unsupported("is_null operand is missing".to_string()))?;
    match (repr, php_type, ir_type) {
        (_, PhpType::Void | PhpType::Never, _) => {
            ctx.fb.ins("i64.const 1", "statically null value");
        }
        (WasmRepr::Ptr(local), PhpType::Union(_), IrType::Heap(kind))
            if kind != IrHeapKind::Mixed =>
        {
            ctx.fb
                .ins(&format!("local.get {}", local), "nullable container pointer");
            ctx.fb.ins("i32.eqz", "nullable container is null");
            ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
        }
        (
            WasmRepr::Ptr(local),
            PhpType::Mixed | PhpType::Union(_),
            IrType::Heap(IrHeapKind::Mixed),
        ) => {
            ctx.fb.ins(&format!("local.get {}", local), "boxed value");
            ctx.fb
                .ins("call $__rt_mixed_unbox", "read boxed Mixed tag");
            ctx.fb.ins("drop", "discard high payload");
            ctx.fb.ins("drop", "discard low payload");
            ctx.fb.ins("i64.const 8", "Mixed null tag");
            ctx.fb.ins("i64.eq", "boxed value is null");
            ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
        }
        (WasmRepr::Tagged { tag, .. }, PhpType::TaggedScalar, IrType::TaggedScalar) => {
            ctx.fb.ins(&format!("local.get {}", tag), "tagged scalar tag");
            ctx.fb.ins("i32.const 8", "tagged null tag");
            ctx.fb.ins("i32.eq", "tagged scalar is null");
            ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
        }
        _ => {
            ctx.fb.ins("i64.const 0", "statically non-null value");
        }
    }
    Ok(())
}
