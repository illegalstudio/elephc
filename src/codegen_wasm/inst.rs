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
    CmpPredicate, DataId, Effects, Immediate, InstId, Instruction, IrHeapKind, IrType, LocalSlotId,
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
        Op::StrIncDec => lower_str_inc_dec(ctx, &inst),
        Op::StrPersist => lower_str_persist(ctx, &inst),
        Op::ArrayToMixed => lower_array_to_mixed(ctx, &inst),
        Op::LooseEq | Op::LooseNotEq => lower_loose_eq(ctx, &inst),
        Op::PhpRelCmp => lower_php_rel_cmp(ctx, &inst),
        Op::ZeroLocalSlot => lower_zero_local_slot(ctx, &inst),
        Op::PropInitialized => lower_prop_initialized(ctx, &inst),
        Op::StaticPropInitialized => lower_static_prop_initialized(ctx, &inst),
        Op::PropUnset => super::objects::lower_prop_unset(ctx, &inst),
        Op::GeneratorYield => lower_generator_yield(ctx, &inst),
        Op::GeneratorYieldFrom => lower_generator_yield_from(ctx, &inst),
        Op::DynamicPropGet => super::objects::lower_dynamic_prop_get(ctx, &inst),
        Op::DynamicPropSet => super::objects::lower_dynamic_prop_set(ctx, &inst),
        Op::ReturnBoundaryMixedToInt => lower_return_boundary_int(ctx, &inst),
        Op::ReleaseUnlessAliases => lower_release_unless_aliases(ctx, &inst),
        Op::StrConcat => lower_str_concat(ctx, &inst),
        Op::Nop => lower_nop(ctx),
        Op::PushCallOperandOwner | Op::PopCallOperandOwner => {
            lower_call_operand_owner_scope(ctx, &inst)
        }
        Op::ConcatReset => lower_concat_reset(ctx),
        Op::ErrorSuppressBegin | Op::ErrorSuppressEnd => lower_error_suppress(ctx, inst.op),
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
        Op::FToStr => lower_float_to_string(ctx, &inst),
        Op::StrCharAt => lower_str_char_at(ctx, &inst),
        Op::TypePredicate => lower_type_predicate(ctx, &inst),
        Op::ConstClassName => lower_const_class_name(ctx, &inst),
        Op::LoadCalledClassId => lower_load_called_class_id(ctx, &inst),
        Op::IncludeOnceMark => super::includes::lower_include_once_mark(ctx, &inst),
        Op::IncludeOnceGuard => super::includes::lower_include_once_guard(ctx, &inst),
        Op::FunctionVariantMark => super::includes::lower_function_variant_mark(ctx, &inst),
        // The public name already dispatches through the group's own stub, so the marker that
        // introduces the group carries no code of its own — as on the native backend.
        Op::FunctionVariantDispatch => Ok(()),
        Op::Cast => lower_cast(ctx, &inst),
        Op::IsTruthy => lower_is_truthy(ctx, &inst),
        Op::IsNull => lower_is_null(ctx, &inst),
        Op::Call => lower_call(ctx, &inst),
        Op::LoadGlobal => lower_load_global(ctx, &inst),
        Op::StoreGlobal => lower_store_global(ctx, &inst),
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
        Op::ArrayGet | Op::ArrayGetSilent | Op::ArrayGetMixedKey | Op::ArrayGetMixedKeySilent => {
            lower_array_get(ctx, &inst)
        }
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
        Op::ArrayIsset => lower_array_isset(ctx, &inst),
        Op::Spaceship => lower_spaceship(ctx, &inst),
        Op::ObjectCloneShallow => super::objects::lower_object_clone_shallow(ctx, &inst),
        Op::LoadStaticLocal => lower_load_static_local(ctx, &inst),
        Op::StoreStaticLocal => lower_store_static_local(ctx, &inst),
        Op::InitStaticLocal => lower_init_static_local(ctx, &inst),
        Op::StaticLocalUninitialized => lower_static_local_uninitialized(ctx, &inst),
        Op::ScopedConstantGet => lower_scoped_constant_get(ctx, &inst),
        Op::HashAppend => super::inst_hash::lower_hash_append(ctx, &inst),
        Op::HashSpread => super::inst_hash::lower_hash_spread(ctx, &inst),
        Op::HashUnion => super::inst_hash::lower_hash_union(ctx, &inst),
        Op::ArrayUnion => super::inst_hash::lower_array_union(ctx, &inst),
        Op::ArrayHashUnion => super::inst_hash::lower_array_hash_union(ctx, &inst),
        Op::HashArrayUnion => super::inst_hash::lower_hash_array_union(ctx, &inst),
        Op::MixedBox => lower_mixed_box(ctx, &inst),
        Op::MixedClone => lower_mixed_clone(ctx, &inst),
        Op::MixedArrayAppend => lower_mixed_array_append(ctx, &inst),
        Op::PropGetForWrite => super::objects::lower_prop_get_for_write(ctx, &inst),
        Op::MixedTagOf => lower_mixed_tag_of(ctx, &inst),
        Op::IterStart => lower_iter_start(ctx, &inst),
        Op::IterNext => lower_iter_next(ctx, &inst),
        Op::IterCurrentKey => lower_iter_current_key(ctx, &inst),
        Op::IterCurrentValue => lower_iter_current_value(ctx, &inst),
        Op::IterEnd => Ok(()),
        Op::ObjectNew => super::objects::lower_object_new(ctx, &inst),
        Op::DynamicObjectNew => super::objects::lower_dynamic_object_new(ctx, &inst),
        Op::PropGet
            if super::capability::magic_get_dispatch_is_supported(ctx.module, ctx.function, &inst) =>
        {
            super::methods::lower_magic_property_get(ctx, &inst)
        }
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
        Op::LoadPropRefCell => super::refcell::lower_load_prop_ref_cell(ctx, &inst),
        Op::LoadArrayElemRefCell => super::refcell::lower_load_array_elem_ref_cell(ctx, &inst),
        Op::BindRefCellPtr => super::refcell::lower_bind_ref_cell_ptr(ctx, &inst),
        Op::StoreRefCell => super::refcell::lower_store_ref_cell(ctx, &inst),
        Op::PromoteLocalRefCell => super::refcell::lower_promote_local_ref_cell(ctx, &inst),
        Op::AliasLocalRefCell => super::refcell::lower_alias_local_ref_cell(ctx, &inst),
        Op::ReleaseLocalRefCell => super::refcell::lower_release_local_ref_cell(ctx, &inst),
        Op::ReleaseLocalSlot => lower_release_local_slot(ctx, &inst),
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
    // A GENERATOR's EIR function declares the type its `return` statement produces — what
    // `getReturn()` will answer — while the CALL answers a `Generator` object, which is what
    // the wrapper `generators::emit_wrapper` emits hands back. Reading the declared type here
    // made the call site unbox the frame pointer as a Mixed cell and fail the heap-kind check
    // before a line of the body ran.
    let (callee_return_type, callee_return_php) = if target.function.flags.is_generator {
        (
            IrType::Heap(IrHeapKind::Object),
            PhpType::Object("Generator".to_string()),
        )
    } else {
        (
            target.function.return_type,
            target.function.return_php_type.codegen_repr(),
        )
    };
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

    // A BY-REFERENCE return hands back an address the call site labels with the pointee type,
    // so the widths can differ: the body answers an i64 address and the site holds an i32
    // pointer. The call itself is unchanged; only the narrowing after it is.
    let by_ref_return_narrows = target.function.flags.by_ref_return
        && target.function.return_type == IrType::I64
        && inst
            .result
            .and_then(|result| ctx.value_repr(result).ok())
            .is_some_and(|repr| matches!(repr, WasmRepr::Ptr(_)));

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
            } else if matches!(*param_ir, IrType::Heap(IrHeapKind::Array))
                && argument_reaches_parameter_unconverted(ctx, arg, param_php)
            {
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

    if let Some(result) = inst.result {
        if by_ref_return_narrows {
            // Not a conversion: the callee's i64 and the site's i32 are the same address, and
            // the transfer layer would refuse the pair for exactly the reason it should — a
            // value of one type does not become a value of the other.
            ctx.fb
                .ins("i32.wrap_i64", "a by-reference return hands back an address");
            ctx.emit_store_value(result)?;
        } else {
            transfer::emit_store_call_result(ctx, &inst, callee_return_type, callee_return_php)?;
        }
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
    // increfs. RETURNING it is safe too, which is what changed: a borrowed HEAP-typed return
    // (array, object, boxed mixed) goes through `__rt_incref_borrowed_return`, so a callee that
    // hands its own parameter back gives the caller a reference of its own rather than moving
    // this one out. A STRING parameter returns as the bare pointer instead — `Return` only
    // increfs `IrType::Heap(_)` values — and stays safe because a string call result is never
    // treated as an owned reference: the EIR gives it no `release`, and every store of it
    // goes through `acquire`, which is `__rt_str_persist` and copies the bytes into a block
    // of the caller's own.
    release_boxed_arguments(ctx, &boxed_args);

    Ok(())
}

/// Frees the heap values synthesized for a call's arguments, after the call has returned.
///
/// Every one of them, including a temporary the callee's declared return could BE. That used to
/// be withheld, on the reading that `Terminator::Return` moves a value out without increfing — so
/// freeing it here would leave the caller holding a dead pointer. The emitted code says
/// otherwise: a callee returning a BORROWED value (which a parameter slot's contents always are)
/// calls `__rt_incref_borrowed_return` first, and the comment above that call — "the caller owns
/// the result" — is the contract. The caller therefore holds two references and gives one back.
/// (A STRING parameter is not in this class: its return is not increfed, and the caller copies
/// it with `acquire` instead of owning it — see the note in the call lowering above.)
///
/// Withholding leaked one cell per call for every `function f(mixed $x): mixed` called with
/// anything that had to be boxed, which is most of them: 3 pages became 14 over 20000 calls,
/// against a control returning `int` that stayed flat. The shape that the withholding protected —
/// a callee that hands its argument straight back — is pinned by
/// `test_cli_wasm_mixed_property_matches_php`'s probe and by a leak scan at 80000 iterations,
/// because a release too many is a use-after-free rather than a slope.
fn release_boxed_arguments(ctx: &mut FnCtx, boxed_args: &[(String, IrType)]) {
    for (temp, _) in boxed_args {
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
    // A BOXED operand is coerced PHP's way at a declared `int` parameter; a concrete one is
    // loaded unchanged, which is what this does for every non-RuntimeCall user of the helper.
    super::builtins::emit_int_operand(ctx, inst, 0)?;
    ctx.fb.ins(&format!("local.set {}", lhs), "capture integer lhs");
    super::builtins::emit_int_operand(ctx, inst, 1)?;
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
    // The token IS the catch block's id, and the landing pad feeds this word straight into
    // `$__state` — which names a SEGMENT. The two agree for every function that does not yield,
    // because `plan_segments` gives those one segment per block; inside a generator they do not,
    // so the same indirection every terminator goes through applies here too.
    let arm = ctx.block_index(crate::ir::BlockId::from_raw(token as u32));
    ctx.fb
        .ins(&format!("i32.const {arm}"), "this try's catch block, as a segment");
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
///
/// `ThrowErrorValue` is the exception: its operand is the error MESSAGE, not an object. Native
/// builds an `Error` around it, which is not reproducible here — a program that never names
/// `Error` carries no `ClassInfo` for it, so there is no class id or property layout to write.
/// The uncatchable case is instead lowered to PHP's observable for an uncaught `Error`; the
/// capability audit refuses the catchable one rather than let it escape a handler.
fn lower_throw(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    if inst.op == Op::ThrowErrorValue && throw_error_value_is_message(ctx, value)? {
        ctx.emit_load_value(value)?;
        // The Str repr arrives as (ptr i32, len i64); the helper takes the length as i32.
        ctx.fb.ins("i32.wrap_i64", "message length i64 -> i32");
        ctx.fb.ins(
            "call $__rt_fail_error_value",
            "report the uncaught Error and exit 255",
        );
        ctx.fb.ins(
            "unreachable",
            "elephc-trap:post-noreturn:throw-error-value-fatal",
        );
        return Ok(());
    }
    ctx.emit_load_value(value)?;
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

/// Whether a `ThrowErrorValue` operand is the message string rather than an exception object.
///
/// Both spellings exist in EIR: the dynamic-method-on-null path raises a composed message, while
/// a re-raise carries the object it caught.
pub(super) fn throw_error_value_is_message(ctx: &FnCtx, value: crate::ir::ValueId) -> Result<bool> {
    Ok(ctx
        .function
        .value(value)
        .is_some_and(|value| value.ir_type == IrType::Str))
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
            // A `Str` repr's length local is ALREADY an i64 — only the pointer is an i32 —
            // so widening it again is a type error the validator rejects. Unreachable until
            // now only because every program that boxed a string operand was refused earlier
            // for something else.
            ctx.fb.ins(&format!("local.get {}", len), "string length -> hi");
            ctx.fb.ins("call $__rt_mixed_from_value", "box the string operand");
        }
        // A tagged pair — `int|null`, which a nullable property read produces — already carries
        // the cell's own tag in its tag word, so boxing it is a move rather than a decision.
        // Same conversion `MixedBox` performs, and deliberately the same two lines.
        WasmRepr::Tagged { payload, tag } => {
            ctx.fb
                .ins(&format!("local.get {}", tag), "tagged scalar runtime tag");
            ctx.fb.ins("i64.extend_i32_u", "tag -> i64");
            ctx.fb
                .ins(&format!("local.get {}", payload), "tagged scalar payload");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb.ins("call $__rt_mixed_from_value", "box the tagged operand");
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

/// Lowers an integer `+`, `-` or `*` whose overflow promotes to float, naming it `operation` in
/// diagnostics. The result is a boxed `mixed` holding an int or, on overflow, a float.
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
    // A `?int` destination is an inline `{payload, tag}` PAIR, not one word, so the sentinel
    // that stands for null in a single-word slot would leave the second component unwritten and
    // the module would fail validation. Tag 8 is what every other tagged producer writes, and
    // what `is_null` and the strict comparison read back.
    if inst.result_type == IrType::TaggedScalar {
        ctx.fb.ins("i64.const 0", "tagged null carries no payload");
        ctx.fb.ins("i32.const 8", "Mixed null tag");
        return store_result(ctx, inst);
    }
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

/// Lowers `Op::StrIncDec`: PHP's `++`/`--` whose operand may hold a string.
///
/// The operand is BORROWED by the runtime helper and stays owned by its EIR value —
/// the EIR schedules its own release. A concrete `Str` operand is boxed for the call
/// and that temporary released right after, since the helper answers a fresh cell
/// either way. The `i64` immediate is the delta (`+1` or `-1`).
fn lower_str_inc_dec(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let source = operand(inst, 0)?;
    let delta = match inst.immediate {
        Some(Immediate::I64(delta)) => delta,
        _ => {
            return Err(WasmError::Unsupported(
                "str_inc_dec without a delta immediate".to_string(),
            ))
        }
    };
    let source_repr = ctx.value_repr(source)?.clone();
    let boxed_temp = if matches!(source_repr, WasmRepr::Str { .. }) {
        let cell = box_value_into_mixed_cell(ctx, source, &source_repr)?;
        ctx.fb
            .ins(&format!("local.get {}", cell), "boxed string operand");
        Some(cell)
    } else {
        ctx.emit_load_value(source)?;
        None
    };
    ctx.fb.ins(
        &format!("i64.const {}", delta),
        "the ++/-- delta the helper applies",
    );
    ctx.fb.ins(
        "call $__rt_mixed_inc_dec",
        "PHP's ++/-- across every scalar tag",
    );
    if let Some(cell) = boxed_temp {
        // The helper borrowed it and answered a fresh cell; nothing else reads it.
        ctx.fb.ins(
            &format!("(call $__rt_decref_any (local.get {}))", cell),
            "release the boxing temporary",
        );
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
    let source = operand(inst, 0)?;
    ctx.emit_load_value(source)?;
    ctx.fb.ins(
        "call $__rt_str_persist",
        "own an independent copy (ptr,len) -> (new_ptr,new_len)",
    );
    store_result(ctx, inst)?;
    // The EIR treats a concat or `str_repeat` result as a view into a reusable buffer, so it
    // never releases the source of a persist. On this target both ALLOCATE an owned heap block,
    // which the copy just made redundant: without this the original leaked on every call that
    // returned such a string (`return $a . $b;`, a handler returning `str_repeat(...)`).
    // Only a producer known to allocate, whose result nothing else reads, is freed.
    if source_is_a_sole_use_heap_string(ctx, source) {
        let repr = ctx.value_repr(source)?.clone();
        if let WasmRepr::Str { ptr, .. } = repr {
            ctx.fb.ins(&format!("local.get {ptr}"), "the persisted-from string");
            ctx.fb.ins("call $__rt_heap_free_safe", "free the original block: the copy replaced it");
        }
    }
    Ok(())
}

/// Whether `value` is a concat or `str_repeat` result that this target allocated on the heap and
/// that the persist being lowered is the only reader of.
fn source_is_a_sole_use_heap_string(ctx: &FnCtx, value: ValueId) -> bool {
    use crate::ir::{RuntimeCallTarget, RuntimeFnId};
    let Some(def) = ctx.function.value(value).and_then(|v| match v.def {
        ValueDef::Instruction { inst, .. } => ctx.function.instruction(inst),
        _ => None,
    }) else {
        return false;
    };
    let allocates = match def.op {
        Op::StrConcat => true,
        Op::RuntimeCall => matches!(
            def.immediate.as_ref(),
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::StrRepeat)))
        ),
        _ => false,
    };
    allocates
        && ctx
            .function
            .instructions
            .iter()
            .filter(|candidate| candidate.operands.contains(&value))
            .count()
            == 1
}

/// Lowers `Nop`: emits a comment; the result local (if any) keeps its default 0.
fn lower_nop(ctx: &mut FnCtx) -> Result<()> {
    ctx.fb.comment("nop");
    Ok(())
}

/// Lowers `Op::PushCallOperandOwner` / `Op::PopCallOperandOwner` to nothing.
///
/// Natively the pair maintains the unwind record a throw walks to retire a call operand's owner
/// slot. On the normal path that slot is retired by its own `release_local_slot`, so emitting
/// nothing is exact there. See `call_operand_owner_shape_issue` for the throw path.
fn lower_call_operand_owner_scope(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.fb.comment(inst.op.name());
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

/// Lowers the `@` operator's bracket instructions.
///
/// Both are a bare call to the matching depth helper; the suppression state
/// lives in the `$__diag_suppress` global that every `__rt_warn_*` helper reads
/// on entry. Nothing is pushed or popped on the value stack — `@expr` yields the
/// expression's own value, which the surrounding instructions already produce.
fn lower_error_suppress(ctx: &mut FnCtx, op: Op) -> Result<()> {
    let (callee, why) = match op {
        Op::ErrorSuppressBegin => ("$__rt_diag_push_suppression", "enter @ suppression"),
        _ => ("$__rt_diag_pop_suppression", "leave @ suppression"),
    };
    ctx.fb.ins(&format!("call {callee}"), why);
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
    transfer::emit_transfer_from_slot(ctx, slot, result)?;
    // The EIR states the ownership it expects of this load, and `own=owned` means an OWNED
    // reference — the EIR then emits a matching `release`. Copying the slot's words alone hands
    // out a BORROW, so that release eats the container's share: `$a[$k] ??= ...` consumed
    // directly read back NULL, while the same expression stored to a local was right, because
    // storing emits its own `acquire` first. A `maybe_owned` load stays a borrow and is
    // untouched, which is every array and object load that has no release.
    if owned_heap_load(ctx, result) {
        ctx.emit_load_value(result)?;
        ctx.fb.ins(
            "call $__rt_incref",
            "the EIR asked for an owned reference, and will release it",
        );
    }
    Ok(())
}

/// Whether this load's result is a HEAP value the EIR both declares `own=owned` AND releases.
///
/// Two narrower conditions than they look, both established by flipping this predicate off in the
/// same binary and re-measuring rather than by reading the EIR:
///
/// * `own=owned` alone is not enough — some owned loads carry no release, and increfing those
///   grew a probe from 3 pages to 85 over 200..20000 iterations;
/// * a release is not enough either. The EIR reads a slot's PREVIOUS value purely to release
///   it before `store_local` overwrites the slot, and that load's only use is the release.
///   Increfing there makes the release consume the new reference and strands the slot's own:
///   `$b = $a;` alone grew to 31 pages where the same loop with this predicate off is flat at 3.
///
/// So the reference is owed exactly when the value is USED for something besides being
/// released — the consumed-directly case, which is the read-back-null bug this exists to fix.
fn owned_heap_load(ctx: &FnCtx, result: crate::ir::ValueId) -> bool {
    // `MaybeOwned` counts too, and that is not a loosening — it is the case the pairing below
    // was always describing. A load that NARROWS a Mixed slot to a pointer is annotated
    // `maybe_owned`, yet the EIR still releases it, because the narrowing is defined as "an
    // unbox AND an incref" (the native backend does exactly that). This target's unbox hands
    // back a pointer BORROWED from the cell, so each such load ate one of the cell's references:
    // `catch (Throwable $e) { $e->getMessage(); $e->getCode(); }` with `$e` shared by a second
    // catch dropped the exception to refcount zero mid-block, and the next read of it walked a
    // freed block's free-list link. Two uses were survivable by luck; the third crashed.
    let declared_owned = ctx.function.value(result).is_some_and(|value| {
        matches!(
            value.ownership,
            crate::ir::Ownership::Owned | crate::ir::Ownership::MaybeOwned
        ) && matches!(value.ir_type, IrType::Heap(_))
    });
    if !declared_owned {
        return false;
    }
    let released = ctx.usage().released.contains(&result);
    // A load whose ONLY use is the release is the OVERWRITE idiom: the EIR reads the slot's
    // previous value purely to give up the slot's reference before `store_local` replaces it.
    // Increfing there makes the release consume the NEW reference and strands the slot's own —
    // which is the leak, and it has nothing to do with boxed cells: `$b = $a;` alone grew to
    // 31 pages. A load that is also USED owes a reference; one that is only released does not.
    let used_beyond_release = ctx.usage().used_beyond_release.contains(&result);
    released && used_beyond_release && !load_takes_over_the_slot(ctx, result)
}

/// Whether the slot this load reads is UNSET right afterwards, which makes the load a MOVE.
///
/// `clear_owned_hidden_temp` emits that pair — `load_local` then `unset_local` of the same slot —
/// and `lower_unset_local` states its half of the contract in its own doc: it clears the slot's
/// words "without releasing the value that was moved out by `LoadLocal`". So the slot hands its
/// reference OVER, and increfing on top of that adds one nobody ever releases.
///
/// That pair is how every hidden temp is read back at a join, which is why the leak was so
/// broad: `$a ?? $b` over a boxed left side, both nullsafe-chain joins, the ternary join. Each
/// leaked one Mixed cell — 40 bytes — per evaluation, measured as `if (($o->tag ?? 1) > 0) {}` in
/// a loop growing 3 → 14 → 51 pages over 1k → 20k → 80k iterations while the same read WITHOUT
/// the coalesce stayed flat at 3.
///
/// The test is ADJACENCY, not "is this slot ever unset": `unset($x)` on a ref-bound php-visible
/// local emits the same opcode, and the loads of `$x` before it are ordinary retaining reads that
/// still owe their reference.
fn load_takes_over_the_slot(ctx: &FnCtx, result: crate::ir::ValueId) -> bool {
    ctx.usage().moved_slot_loads.contains(&result)
}

/// Whether an argument reaches its parameter with NO conversion, so the caller still owns it.
///
/// Only then does an array parameter need a lent reference: a converted one is a fresh array
/// whose single reference is already the one the callee will release. Both call families ask
/// this same question, which is why it lives beside neither of them.
pub(super) fn argument_reaches_parameter_unconverted(
    ctx: &FnCtx,
    arg: crate::ir::ValueId,
    param_php: &PhpType,
) -> bool {
    ctx.function
        .value(arg)
        .is_some_and(|value| value.php_type.codegen_repr() == param_php.codegen_repr())
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
    transfer::emit_transfer_to_slot(ctx, value, slot)?;
    if let Some(flag) = ctx.capture_owned_flags.get(&slot.as_raw()).cloned() {
        ctx.fb.ins("i32.const 1", "the capture slot now owns its value");
        ctx.fb.ins(&format!("local.set {flag}"), "");
    }
    Ok(())
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
    // A comparison against a BOXED value is PHP's own, not a converted integer's. The cast that
    // nominally produced the integer already ran `__rt_mixed_cmp_i64` — it holds the -1/0/1 answer
    // for the pair, taken while the box was still alive — so all that is left is the original
    // predicate against zero, with the operands kept in their original order: `pred(box, other)`
    // is `pred(cmp, 0)` and `pred(other, box)` is `pred(0, cmp)`.
    if let Some((_, _, mixed_on_left)) =
        super::capability::icmp_compares_a_boxed_value(ctx.function, inst)
    {
        let three_way = operand(inst, if mixed_on_left { 0 } else { 1 })?;
        if mixed_on_left {
            ctx.emit_load_value(three_way)?;
            ctx.fb
                .ins("i64.const 0", "compare the three-way answer with zero");
        } else {
            ctx.fb
                .ins("i64.const 0", "the box sat on the right of the comparison");
            ctx.emit_load_value(three_way)?;
        }
        ctx.fb.ins(wasm_op, "the original predicate");
        ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
        return store_result(ctx, inst);
    }
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
    let source_ir = source.ir_type;
    let source_php = source.php_type.codegen_repr();
    // A TAGGED scalar carries the int-or-null an `array<int>` read answers. PHP renders null as
    // the empty string in a string context, so the null arm is a zero-length result and the other
    // arm is the ordinary decimal.
    if source_ir == IrType::TaggedScalar {
        let WasmRepr::Tagged { payload, tag } = ctx.value_repr(value)?.clone() else {
            return Err(WasmError::Unsupported(
                "IToStr over a tagged scalar with a non-tagged representation".to_string(),
            ));
        };
        ctx.fb.ins(&format!("local.get {payload}"), "the tagged payload");
        ctx.fb.ins(
            "global.get $__float_scratch",
            "integer formatting scratch buffer",
        );
        ctx.fb
            .ins("call $__rt_itoa", "format PHP integer as decimal string");
        ctx.fb.ins("i64.extend_i32_u", "widen integer string length");
        // A null tag answers the EMPTY string: keep the pointer, zero the length.
        ctx.fb.ins(&format!("local.get {tag}"), "tagged scalar tag");
        ctx.fb.ins("i32.const 8", "tagged null tag");
        ctx.fb.ins("i32.eq", "tagged scalar is null?");
        ctx.fb.ins(
            "(if (param i64) (result i64) (then (drop) (i64.const 0)))",
            "PHP renders null as the empty string",
        );
        return store_result(ctx, inst);
    }
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

/// Lowers `TypePredicate`: `is_int()`, `is_string()`, and the rest of that family.
///
/// A value whose EIR type is already concrete answers at COMPILE time — `is_int($n)` on an
/// `int` local is `true` and nothing is tested. Only a boxed value reaches a runtime test, and
/// there the cell's tag is the whole answer.
///
/// `is_iterable()` on a boxed value stays refused: PHP also accepts an object implementing
/// `Traversable`, which the tag alone cannot tell from any other object.
fn lower_type_predicate(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let Some(Immediate::TypePredicate(predicate)) = inst.immediate else {
        return Err(WasmError::Unsupported(
            "type predicate without a PhpTypePredicate immediate".to_string(),
        ));
    };
    let value = operand(inst, 0)?;
    let declared = ctx
        .function
        .value(value)
        .ok_or_else(|| WasmError::Unsupported(format!("type predicate source {:?} is missing", value)))?
        .php_type
        .clone();
    // A resource is only a resource while it is OPEN: php answers `is_resource()` false for a
    // closed one. The cell tag (and the declared type) cannot say, so the stream's own closed
    // state is consulted — through the box's stamp or the raw payload.
    if predicate == crate::ir::PhpTypePredicate::Resource {
        if matches!(declared, PhpType::Resource(_)) {
            ctx.emit_load_value(value)?;
            ctx.fb.ins("call $__rt_handle_closed", "has the stream behind this handle been closed?");
            ctx.fb.ins("i32.eqz", "open resources answer true");
            ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
            return store_result(ctx, inst);
        }
        if declared.codegen_repr() == PhpType::Mixed {
            let tag = ctx.fresh_temp(ValType::I64);
            ctx.emit_load_value(value)?;
            ctx.fb.ins("i64.load", "the cell's runtime tag");
            ctx.fb.ins(&format!("local.set {}", tag), "hold the tag");
            ctx.fb.ins(&format!("local.get {}", tag), "compare the tag");
            ctx.fb.ins("i64.const 9", "the resource tag");
            ctx.fb.ins("i64.eq", "is it a resource cell?");
            ctx.emit_load_value(value)?;
            ctx.fb.ins("call $__rt_box_closed", "the box's -1 stamp, or the stream's own record");
            ctx.fb.ins("i32.eqz", "not closed");
            ctx.fb.ins("i32.and", "a resource that is still open");
            ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
            return store_result(ctx, inst);
        }
    }
    let source = declared.codegen_repr();
    if source != PhpType::Mixed {
        let answer = static_type_predicate(ctx.module, &source, predicate).ok_or_else(|| {
            WasmError::Unsupported(format!(
                "type predicate {:?} over {:?}",
                predicate, source
            ))
        })?;
        ctx.fb.ins(
            &format!("i64.const {}", i64::from(answer)),
            "the EIR type already decides this predicate",
        );
        return store_result(ctx, inst);
    }
    let tags = boxed_predicate_tags(predicate).ok_or_else(|| {
        WasmError::Unsupported(format!("type predicate {:?} over a boxed value", predicate))
    })?;
    let tag = ctx.fresh_temp(ValType::I64);
    ctx.emit_load_value(value)?;
    ctx.fb.ins("i64.load", "the cell's runtime tag");
    ctx.fb.ins(&format!("local.set {}", tag), "hold the tag");
    let mut first = true;
    for candidate in tags {
        ctx.fb.ins(&format!("local.get {}", tag), "compare the tag");
        ctx.fb
            .ins(&format!("i64.const {}", candidate), "candidate runtime tag");
        ctx.fb.ins("i64.eq", "tag matches");
        if !first {
            ctx.fb.ins("i32.or", "any candidate tag answers true");
        }
        first = false;
    }
    ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
    store_result(ctx, inst)
}

/// The answer a predicate has for an EIR type that is already concrete, if it has one.
pub(super) fn static_type_predicate(
    module: &crate::ir::Module,
    source: &PhpType,
    predicate: crate::ir::PhpTypePredicate,
) -> Option<bool> {
    use crate::ir::PhpTypePredicate as P;
    let answer = match (predicate, source) {
        (P::Int, PhpType::Int) | (P::Bool, PhpType::Bool) | (P::Float, PhpType::Float) => true,
        (P::String, PhpType::Str) => true,
        (P::Array, PhpType::Array(_) | PhpType::AssocArray { .. }) => true,
        (P::Iterable | P::Countable, PhpType::Array(_) | PhpType::AssocArray { .. }) => true,
        // An object is countable or iterable by its CLASS: the declared class proves the answer
        // only when it implements the interface itself, since a subclass may implement it where
        // the declared class does not.
        (P::Countable | P::Iterable, PhpType::Object(class)) => {
            let interface = if predicate == P::Countable { "Countable" } else { "Traversable" };
            if super::capability::class_implements_interface(module, class, interface) {
                true
            } else {
                return None;
            }
        }
        (P::Object, PhpType::Object(_)) => true,
        (P::Resource, PhpType::Resource(_)) => true,
        (P::Scalar, PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Bool) => true,
        // Anything else this backend can name is decidably NOT the tested category.
        (
            _,
            PhpType::Int
            | PhpType::Float
            | PhpType::Str
            | PhpType::Bool
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
            | PhpType::Resource(_)
            | PhpType::Void,
        ) => false,
        _ => return None,
    };
    Some(answer)
}

/// The Mixed-cell tags a predicate accepts, when the tag alone decides it.
pub(super) fn boxed_predicate_tags(predicate: crate::ir::PhpTypePredicate) -> Option<&'static [i64]> {
    use crate::ir::PhpTypePredicate as P;
    Some(match predicate {
        P::Int => &[0],
        P::String => &[1],
        P::Float => &[2],
        P::Bool => &[3],
        P::Array => &[4, 5],
        P::Object => &[6],
        P::Resource => &[9],
        P::Scalar => &[0, 1, 2, 3],
        // `is_iterable()` also accepts a Traversable object, and `is_countable()` a Countable one,
        // which the tag cannot distinguish.
        P::Iterable | P::Countable => return None,
    })
}

/// Lowers `Op::LoadCalledClassId`: the late-static-binding class id, as an integer.
///
/// The lowering emits it where a closure is created inside a class method, so the closure can
/// answer `static::` for the class that was called. It reads the same two sources `static::class`
/// does: a static method's forwarded hidden parameter, or the class id at the front of `$this`.
fn lower_load_called_class_id(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    if let Some(slot) = ctx
        .function
        .locals
        .iter()
        .find(|local| super::objects::is_called_class_id_local(local.name.as_deref()))
        .map(|local| local.id)
    {
        ctx.emit_load_slot(slot)?;
        return store_result(ctx, inst);
    }
    if let Some(slot) = ctx
        .function
        .locals
        .iter()
        .find(|local| local.name.as_deref() == Some("this"))
        .map(|local| local.id)
    {
        ctx.emit_load_slot(slot)?;
        ctx.fb.ins("i64.load", "the receiver's class id, the object header's first word");
        return store_result(ctx, inst);
    }
    Err(WasmError::Unsupported(
        "load_called_class_id outside a class method has no called class".to_string(),
    ))
}

/// Lowers `ConstClassName`: `Foo::class` and `self::class`, a compile-time string.
///
/// The EIR already carries the resolved class name, so the answer is a data-segment address and a
/// length — nothing to compute. `static::class` is the exception: late static binding resolves it
/// from the CALLED class at runtime, through the class-name table.
fn lower_const_class_name(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return Err(WasmError::Unsupported(
            "const_class_name without a class-name Data immediate".to_string(),
        ));
    };
    let name = ctx
        .module
        .data
        .class_names
        .get(data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| {
            WasmError::Unsupported(format!("const_class_name references unknown class data {data:?}"))
        })?;
    if name == "static" {
        // The called class is what the caller forwarded (`__elephc_called_class_id`, a static
        // method's hidden first parameter) or, in an instance method, the class id stored at the
        // front of `$this` -- the same two sources the native backend reads. The id selects its
        // row in the class-name table, and an id outside it answers "" like native does.
        if let Some(slot) = ctx
            .function
            .locals
            .iter()
            .find(|local| super::objects::is_called_class_id_local(local.name.as_deref()))
            .map(|local| local.id)
        {
            ctx.emit_load_slot(slot)?;
            ctx.fb
                .ins("call $__rt_class_name_by_cid", "the forwarded called class's name");
            return store_result(ctx, inst);
        }
        if let Some(slot) = ctx
            .function
            .locals
            .iter()
            .find(|local| local.name.as_deref() == Some("this"))
            .map(|local| local.id)
        {
            ctx.emit_load_slot(slot)?;
            ctx.fb
                .ins("call $__rt_class_name_by_obj", "the receiver's runtime class name");
            return store_result(ctx, inst);
        }
        // `new static()` resolves the class from the called-class id its caller forwarded, and
        // never reads this string. With no called class to read, that is the only consumer that
        // can be served; anything else is refused rather than answering the defining class.
        if !late_static_name_is_only_consumed_by_new(ctx, inst) {
            return Err(WasmError::Unsupported(
                "static::class needs the called class, which this function does not carry"
                    .to_string(),
            ));
        }
        ctx.fb.ins("i32.const 0", "the late-bound class name is resolved at the `new` itself");
        ctx.fb.ins("i64.const 0", "so this placeholder is never read");
        return store_result(ctx, inst);
    }
    let (ptr, len) = ctx.default_str_literal(name.trim_start_matches('\\'))?;
    ctx.fb
        .ins(&format!("i32.const {ptr}"), "the class name, resolved at compile time");
    ctx.fb.ins(&format!("i64.const {len}"), "its length");
    store_result(ctx, inst)
}

/// Whether a `static::class` string feeds nothing but the late-bound `new` that resolves it.
///
/// `new static()` reads the called-class id directly, so its class operand is dead. Any OTHER
/// use — echoing it, comparing it, passing it — is a real read of a name this target cannot
/// produce, and the caller refuses instead.
fn late_static_name_is_only_consumed_by_new(ctx: &FnCtx, inst: &Instruction) -> bool {
    let Some(result) = inst.result else {
        return false;
    };
    ctx.function.instructions.iter().all(|candidate| {
        candidate
            .operands
            .iter()
            .enumerate()
            .all(|(index, operand)| {
                *operand != result || (candidate.op == Op::DynamicObjectNew && index == 0)
            })
    })
}

/// Lowers `StrCharAt`: `$s[$i]`, PHP's one-byte string offset read.
///
/// A negative index counts from the END, and anything still outside answers the EMPTY string
/// after `Warning: Uninitialized string offset N` — naming the index AS WRITTEN, so a negative
/// one is reported negative rather than resolved first. The runtime owns both, since neither
/// the bound nor the sign is known here.
fn lower_str_char_at(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(
        "call $__rt_str_char_at",
        "PHP's string offset read: one byte, or \"\" with a warning",
    );
    store_result(ctx, inst)
}

/// Lowers `FToStr`: a PHP float to its string form.
///
/// The formatting is `__rt_ftoa`'s, the same one `echo` of a float uses, so a rendered float
/// is identical whichever way it reaches output — including PHP's exponent form for large
/// magnitudes. The result is persisted before anything else can reuse the scratch buffer.
fn lower_float_to_string(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let bits = ctx.fresh_temp(ValType::I64);
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb
        .ins("i64.reinterpret_f64", "ftoa reads the raw f64 bits");
    ctx.fb
        .ins(&format!("local.set {}", bits), "hold the bits for the call");
    ctx.fb.ins(
        &format!(
            "(call $__rt_ftoa (local.get {}) (i32.add (global.get $__float_scratch) (i32.const 1024)) (i32.const 80) (i32.add (global.get $__float_scratch) (i32.const 2048)) (i32.const 792) (i32.add (global.get $__float_scratch) (i32.const 4096)))",
            bits
        ),
        "format the float the way PHP prints it",
    );
    ctx.fb
        .ins("i64.extend_i32_u", "widen the formatted length");
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
/// Pushes the function name, the parameter name and the one-based position a coercion's
/// diagnostic needs, in the order both `__rt_mixed_arg_*` helpers read them.
pub(super) fn emit_argument_coercion_names(
    ctx: &mut FnCtx,
    name: &str,
    parameter: &str,
    position: usize,
) -> Result<()> {
    let (fn_ptr, fn_len) = ctx.default_str_literal(name)?;
    let (param_ptr, param_len) = ctx.default_str_literal(parameter)?;
    ctx.fb.ins(
        &format!("i32.const {}", fn_ptr),
        "the PHP function's name, for the diagnostic text",
    );
    ctx.fb.ins(&format!("i32.const {}", fn_len), "its byte length");
    ctx.fb.ins(
        &format!("i32.const {}", param_ptr),
        "the parameter's name, without its dollar",
    );
    ctx.fb
        .ins(&format!("i32.const {}", param_len), "its byte length");
    ctx.fb.ins(
        &format!("i64.const {}", position),
        "php-src numbers arguments from 1",
    );
    Ok(())
}

/// Lowers `Op::Cast`: converts the operand to the immediate's target type, picking the runtime
/// helper from the source representation (int, float, string, bool, boxed mixed).
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
    // EVERY `(bool)` cast is php's truthiness, whatever the source holds — measured on 8.5.6,
    // `(bool)$x` and `if ($x)` agree on all of it, the NaN warning included. One rule for both
    // spellings: the storage-by-storage arms below would otherwise each need their own copy,
    // and the ones that had none refused outright (`(bool)$int`, `(bool)$float`, `(bool)$str`,
    // `(bool)$array`) while the Mixed one that existed was the SILENT cast helper, which
    // swallowed the NaN warning php emits here.
    if target == IrType::I64 && inst.result_php_type.codegen_repr() == PhpType::Bool {
        emit_truthiness(ctx, value)?;
        return store_result(ctx, inst);
    }
    // A string conversion of a statically known object is PHP's `__toString`, and the EIR
    // already carries the class, so it lowers to an ordinary direct call rather than any
    // dispatch. The callee's `(ptr i32, len i64)` result IS this backend's string storage.
    // A float reaching a string is the same rendering `FToStr` performs; routing it here keeps
    // `(string) $f` and `"$f"` byte-identical.
    if source.ir_type == IrType::F64 && target == IrType::Str {
        return lower_float_to_string(ctx, inst);
    }
    // `(string) $int` and `(string) $bool` are the rendering `IToStr` already performs for
    // `"$n"` and `echo $n`, so routing them there keeps every spelling byte-identical.
    if source.ir_type == IrType::I64
        && target == IrType::Str
        && matches!(source.php_type.codegen_repr(), PhpType::Int | PhpType::Bool)
    {
        return lower_int_like_to_string(ctx, inst);
    }
    // `(int) $string`: PHP takes the leading numeric prefix, silently, and answers 0 when
    // there is none. Shared with the boxed-string cast so the two cannot disagree.
    if source.ir_type == IrType::Str
        && target == IrType::I64
        && source.php_type.codegen_repr() == PhpType::Str
    {
        ctx.emit_load_value(value)?;
        ctx.fb.ins("i32.wrap_i64", "narrow the string length for the parser");
        ctx.fb.ins(
            "global.get $__float_scratch",
            "the parser's overflow scratch",
        );
        ctx.fb.ins(
            "call $__rt_str_to_int",
            "PHP's (int) of a string: leading numeric prefix, else 0",
        );
        return store_result(ctx, inst);
    }
    // `(float) $string`: PHP parses the LEADING numeric prefix and answers 0.0 when there is
    // none, silently — measured on 8.5.6, `"12abc"` is 12.0, `"0x1A"` and `"INF"` are 0.0, and
    // `"1e400"` is INF. That is the same parser `__rt_str_to_int` already routes float-form
    // prefixes through, so the two spellings cannot disagree about what a string holds.
    if source.ir_type == IrType::Str
        && target == IrType::F64
        && source.php_type.codegen_repr() == PhpType::Str
    {
        ctx.emit_load_value(value)?;
        ctx.fb
            .ins("i32.wrap_i64", "narrow the string length for the parser");
        ctx.fb.ins(
            "global.get $__float_scratch",
            "the parser writes its result here",
        );
        ctx.fb.ins("i32.const 10240", "clear of the strtod bignum buffers");
        ctx.fb.ins("i32.add", "result address = scratch + 10240");
        ctx.fb
            .ins("global.get $__float_scratch", "the parser's bignum region");
        ctx.fb.ins(
            "call $__rt_str_to_f64",
            "PHP's (float) of a string: leading numeric prefix, else 0.0",
        );
        ctx.fb
            .ins("global.get $__float_scratch", "reload the result address");
        ctx.fb.ins("i32.const 10240", "same offset the parser wrote to");
        ctx.fb.ins("i32.add", "result address");
        ctx.fb.ins("i64.load", "the parsed f64 bit pattern");
        ctx.fb.ins("f64.reinterpret_i64", "bits -> float");
        return store_result(ctx, inst);
    }
    if source.ir_type == IrType::Heap(IrHeapKind::Object) && target == IrType::Str {
        if let PhpType::Object(class_name) = source.php_type.clone() {
            if emit_object_to_string(ctx, value, &class_name)? {
                return store_result(ctx, inst);
            }
        }
    }
    // A cast that exists only to feed a comparison emits no conversion. It performs the
    // comparison instead: PHP compares the BOX, and here — before any release the EIR schedules
    // for an owned temporary — the box is guaranteed alive. What lands in this result is the
    // -1/0/1 answer, which the `ICmp` downstream tests against zero.
    // BOTH sides boxed: the pair is answered once, at the LEFT cast, and the right cast
    // becomes the zero the `ICmp` then compares that answer against.
    if let Some((left_cell, right_cell, is_left)) =
        super::capability::cast_pair_stands_in_for_mixed_comparison(ctx.function, inst)
    {
        if is_left {
            ctx.emit_load_value(left_cell)?;
            ctx.emit_load_value(right_cell)?;
            ctx.fb.ins(
                "call $__rt_mixed_cmp_mixed",
                "PHP's comparison of two boxed values",
            );
        } else {
            ctx.fb.ins(
                "i64.const 0",
                "the pair was answered at the other cast; compare against zero",
            );
        }
        return store_result(ctx, inst);
    }
    if let Some((cell, other, _)) =
        super::capability::cast_stands_in_for_mixed_comparison(ctx.function, inst)
    {
        ctx.emit_load_value(cell)?;
        ctx.emit_load_value(other)?;
        ctx.fb.ins(
            "call $__rt_mixed_cmp_i64",
            "PHP's comparison of a boxed value with an integer",
        );
        return store_result(ctx, inst);
    }
    // A cast that exists only to feed integer arithmetic is not a conversion either: PHP coerces
    // the operand under its own contract, which diagnoses differently from every other boundary.
    if super::capability::cast_feeds_integer_arithmetic(ctx.function, inst).is_some() {
        // php names the operator in its TypeError, so the symbol follows the consumer.
        let symbol = inst
            .result
            .and_then(|result| {
                ctx.function
                    .instructions
                    .iter()
                    .find(|candidate| candidate.operands.contains(&result))
            })
            .map(|consumer| match consumer.op {
                Op::IShl => "<<",
                Op::IShrA => ">>",
                Op::IBitAnd => "&",
                Op::IBitOr => "|",
                Op::IBitXor => "^",
                _ => "%",
            })
            .unwrap_or("%");
        let (op_ptr, op_len) = ctx.default_str_literal(symbol)?;
        let (right_ptr, right_len) = ctx.default_str_literal("int")?;
        ctx.emit_load_value(value)?;
        ctx.fb
            .ins(&format!("i32.const {}", op_ptr), "the operator PHP names");
        ctx.fb.ins(&format!("i32.const {}", op_len), "its byte length");
        ctx.fb.ins(
            &format!("i32.const {}", right_ptr),
            "the right operand's type word",
        );
        ctx.fb
            .ins(&format!("i32.const {}", right_len), "its byte length");
        ctx.fb.ins(
            "call $__rt_mixed_arith_int",
            "PHP's coercion of an arithmetic operand",
        );
        return store_result(ctx, inst);
    }
    if source.ir_type == IrType::Heap(IrHeapKind::Mixed) {
        ctx.emit_load_value(value)?;
        match target {
            IrType::I64 if inst.result_php_type.codegen_repr() == PhpType::Bool => {
                ctx.fb
                    .ins("call $__rt_mixed_cast_bool", "cast boxed Mixed to bool");
            }
            // PHP's coercion at a DECLARED return is not a `(T)` cast: it raises where the
            // cast answers a value, and deprecates where the cast is silent.
            _ if super::capability::declared_return_coercion_target(
                ctx.module,
                ctx.function,
                inst,
            )
            .is_some() =>
            {
                let (name_ptr, name_len) = ctx.default_str_literal(&ctx.function.name)?;
                ctx.fb.ins(
                    &format!("i32.const {}", name_ptr),
                    "the declaring function's name, for the TypeError text",
                );
                ctx.fb
                    .ins(&format!("i32.const {}", name_len), "its byte length");
                let (helper, note) = match (target, ctx.function.return_php_type.codegen_repr()) {
                    (IrType::I64, PhpType::Bool) => ("$__rt_mixed_return_bool", "bool"),
                    (IrType::I64, _) => ("$__rt_mixed_return_int", "int"),
                    (IrType::F64, _) => ("$__rt_mixed_return_float", "float"),
                    (IrType::Str, _) => ("$__rt_mixed_return_string", "string"),
                    (other, _) => {
                        return Err(WasmError::Unsupported(format!(
                            "declared return coercion to {:?}",
                            other
                        )));
                    }
                };
                ctx.fb.ins(
                    &format!("call {}", helper),
                    &format!("PHP's implicit coercion at a declared {} return", note),
                );
                if target == IrType::F64 {
                    ctx.fb
                        .ins("f64.reinterpret_i64", "reinterpret coerced float bits");
                } else if target == IrType::Str {
                    let len = ctx.fresh_temp(ValType::I32);
                    let ptr = ctx.fresh_temp(ValType::I32);
                    ctx.fb
                        .ins(&format!("local.set {}", len), "capture coerced string length");
                    ctx.fb
                        .ins(&format!("local.set {}", ptr), "capture coerced string pointer");
                    ctx.fb.ins(&format!("local.get {}", ptr), "coerced string pointer");
                    ctx.fb.ins(&format!("local.get {}", len), "coerced string length");
                    ctx.fb.ins("i64.extend_i32_u", "widen coerced string length");
                }
            }
            // PHP's coercion at an internal function's declared `string` PARAMETER is a third
            // operation again: it converts a scalar the way `(string)` does, deprecates a null
            // on its way to `""`, and raises where `(string)` would have answered "Array".
            IrType::I64
                if inst.result_php_type.codegen_repr() != PhpType::Bool
                    && super::capability::mixed_int_argument_coercion(ctx.function, inst)
                        .is_some() =>
            {
                let (name, parameter, position) =
                    super::capability::mixed_int_argument_coercion(ctx.function, inst)
                        .expect("the guard above proved this resolves");
                emit_argument_coercion_names(ctx, name, &parameter, position)?;
                ctx.fb.ins(
                    "call $__rt_mixed_arg_int",
                    "PHP's implicit coercion at a declared int parameter",
                );
            }
            IrType::Str
                if super::capability::mixed_string_argument_coercion(ctx.function, inst)
                    .is_some() =>
            {
                let (name, parameter, position) =
                    super::capability::mixed_string_argument_coercion(ctx.function, inst)
                        .expect("the guard above proved this resolves");
                emit_argument_coercion_names(ctx, name, &parameter, position)?;
                ctx.fb.ins(
                    "call $__rt_mixed_arg_string",
                    "PHP's implicit coercion at a declared string parameter",
                );
                let len = ctx.fresh_temp(ValType::I32);
                let ptr = ctx.fresh_temp(ValType::I32);
                ctx.fb
                    .ins(&format!("local.set {}", len), "capture coerced string length");
                ctx.fb
                    .ins(&format!("local.set {}", ptr), "capture coerced string pointer");
                ctx.fb
                    .ins(&format!("local.get {}", ptr), "coerced string pointer");
                ctx.fb
                    .ins(&format!("local.get {}", len), "coerced string length");
                ctx.fb.ins("i64.extend_i32_u", "widen coerced string length");
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

/// Pushes PHP's truthiness of the string held in `(ptr, len)` as an i64 boolean.
///
/// PHP has exactly TWO falsy strings — `""` and the single byte `"0"` — so `"00"`,
/// `"0.0"` and `" "` are all true. Shared by `Op::IsTruthy` and the explicit
/// `(bool) $string` cast: a second copy of this rule is how one spelling ends up
/// disagreeing with the other (the native backend's `empty()` did exactly that,
/// testing only the length).
fn emit_string_truthiness(ctx: &mut FnCtx, ptr: &str, len: &str) {
    ctx.fb.ins(&format!("local.get {}", len), "string length");
    ctx.fb.ins("i64.eqz", "empty string?");
    ctx.fb.ins(
        "if (result i64)",
        "empty string is false; otherwise check PHP's special string zero",
    );
    ctx.fb.ins("i64.const 0", "empty string is false");
    ctx.fb.ins("else", "non-empty string");
    ctx.fb.ins(&format!("local.get {}", len), "string length");
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

/// Lowers PHP truthiness for every scalar and heap representation used by EIR.
fn lower_is_truthy(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    emit_truthiness(ctx, operand(inst, 0)?)?;
    store_result(ctx, inst)
}

/// Pushes PHP's truthiness of `op0` as an i64 boolean, for every representation EIR uses.
///
/// Shared by `Op::IsTruthy` — `if ($x)`, `!$x`, `$a && $b` — and the explicit `(bool) $x`
/// cast, which php-src answers IDENTICALLY, diagnostics included: measured on 8.5.6,
/// `(bool)$nan` emits the same `unexpected NAN value was coerced to bool` warning `if ($nan)`
/// does. Routing both spellings here is what keeps them from drifting; a boxed value goes
/// through `__rt_mixed_truthy` (which warns) rather than the silent `__rt_mixed_cast_bool`
/// for exactly that reason.
fn emit_truthiness(ctx: &mut FnCtx, op0: ValueId) -> Result<()> {
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
            // Not `f64.ne 0.0`: that answers FALSE for a NaN, which PHP calls true, and it says
            // nothing where PHP warns.
            ctx.emit_load_value(op0)?;
            ctx.fb.ins("i64.reinterpret_f64", "read the float as bits");
            ctx.fb.ins(
                "call $__rt_float_truthy",
                "PHP's truthiness, warning on a NaN",
            );
        }
        WasmRepr::Str { ptr, len } => emit_string_truthiness(ctx, &ptr, &len),
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
                // The warning wrapper rather than the bare cast: a boxed NaN is TRUE in PHP and
                // says so first, which the silent cast cannot.
                ctx.fb.ins(&format!("local.get {}", local), "boxed value");
                ctx.fb
                    .ins("call $__rt_mixed_truthy", "PHP Mixed truthiness, warning on a NaN");
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
    Ok(())
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
        other => {
            let slot = ctx
                .static_slots
                .get(&super::statics::global_slot_key(other))
                .ok_or_else(|| WasmError::Unsupported(format!("global ${}", other)))?;
            let (address, php_type) = (slot.address, slot.php_type.codegen_repr());
            emit_global_slot_read(ctx, address, &php_type)?;
            let result = inst.result.ok_or_else(|| {
                WasmError::Unsupported("load_global without a result".to_string())
            })?;
            let ir_type = ctx.function.value(result).map(|value| value.ir_type);
            transfer::emit_store_stack_value_into_value(
                ctx,
                ir_type.unwrap_or(IrType::I64),
                php_type,
                result,
            )
        }
    }
}

/// Pushes a global's stored words, in the same shape a static property's slot uses.
fn emit_global_slot_read(ctx: &mut FnCtx, address: u32, php_type: &PhpType) -> Result<()> {
    match php_type {
        PhpType::Int | PhpType::Bool | PhpType::False => {
            ctx.fb.ins(
                &format!("(i64.load offset={address} (i32.const 0))"),
                "global value",
            );
        }
        PhpType::Float => {
            ctx.fb.ins(
                &format!("(f64.load offset={address} (i32.const 0))"),
                "global value (float)",
            );
        }
        PhpType::Str => {
            ctx.fb.ins(
                &format!("(i32.wrap_i64 (i64.load offset={address} (i32.const 0)))"),
                "global string pointer",
            );
            ctx.fb.ins(
                &format!("(i64.load offset={} (i32.const 0))", address + 8),
                "global string length",
            );
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "global of type {other:?} on wasm32-wasi"
            )))
        }
    }
    Ok(())
}

/// Lowers `Op::StoreGlobal`, writing a global's words into its slot.
///
/// A top-level `const NAME = …` lowers to this, as does an assignment through `global $x`. Only
/// non-heap values are admitted: a heap global would have to own its payload across the whole
/// program, and the slot has no place to record that it does.
fn lower_store_global(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let Some(Immediate::GlobalName(data_id)) = inst.immediate else {
        return Err(WasmError::Unsupported(
            "store_global without a name".to_string(),
        ));
    };
    let name = ctx
        .module
        .data
        .global_names
        .get(data_id.as_raw() as usize)
        .cloned()
        .ok_or_else(|| {
            WasmError::Unsupported(format!("store_global: unknown name {:?}", data_id))
        })?;
    let slot = ctx
        .static_slots
        .get(&super::statics::global_slot_key(&name))
        .ok_or_else(|| WasmError::Unsupported(format!("global ${} has no slot", name)))?;
    let (address, php_type) = (slot.address, slot.php_type.codegen_repr());
    let value = operand(inst, 0)?;
    match php_type {
        PhpType::Int | PhpType::Bool | PhpType::False => {
            ctx.fb.ins("i32.const 0", "slot base");
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins(&format!("i64.store offset={address}"), "store global");
        }
        PhpType::Float => {
            ctx.fb.ins("i32.const 0", "slot base");
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins(&format!("f64.store offset={address}"), "store global (float)");
        }
        PhpType::Str => {
            // The pointer and the length are two separate words, and the value pushes them in
            // that order, so the length is stored first while it is on top.
            let ptr = ctx.fresh_temp(super::wat::ValType::I32);
            let len = ctx.fresh_temp(super::wat::ValType::I64);
            ctx.emit_load_value(value)?;
            ctx.fb.ins(&format!("local.set {len}"), "global string length");
            ctx.fb.ins(&format!("local.set {ptr}"), "global string pointer");
            ctx.fb.ins("i32.const 0", "slot base");
            ctx.fb
                .ins(&format!("(i64.extend_i32_u (local.get {ptr}))"), "pointer");
            ctx.fb
                .ins(&format!("i64.store offset={address}"), "store global pointer");
            ctx.fb.ins("i32.const 0", "slot base");
            ctx.fb.ins(&format!("local.get {len}"), "length");
            ctx.fb.ins(
                &format!("i64.store offset={}", address + 8),
                "store global length",
            );
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "global of type {other:?} on wasm32-wasi"
            )))
        }
    }
    Ok(())
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
        "empty" => lower_empty_construct(ctx, inst),
        other => Err(WasmError::Unsupported(format!(
            "language construct {}",
            other
        ))),
    }
}

/// Lowers `empty($x)` for a DEFINED operand: the negation of PHP truthiness.
///
/// Measured on php-src 8.5.6: `empty` IS a bool-coercion context — `empty(NAN)` carries the
/// same "unexpected NAN value was coerced to bool" warning an `if` does — so the float and
/// Mixed arms run the truthiness helpers that own that diagnostic, never a bare zero test.
/// `empty("0")` is TRUE ("" and the single byte "0" are the two falsy strings), `-0.0` is
/// empty like `+0.0`, and a bool slot's in-band NULL sentinel reads as PHP null, which is
/// empty. Operand kinds outside these arms are refused by the capability audit's matching
/// shape rule (`empty_construct_shape_issue`), so the two lists cannot drift apart silently.
fn lower_empty_construct(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    let (ir_type, php_type) = ctx
        .function
        .value(value)
        .map(|v| (v.ir_type, v.php_type.codegen_repr()))
        .ok_or_else(|| WasmError::Unsupported("empty operand is missing".to_string()))?;
    match (ir_type, &php_type) {
        (_, PhpType::Void) => {
            ctx.fb.ins("i64.const 1", "empty(null) is true");
        }
        (IrType::I64, PhpType::Int) => {
            ctx.emit_load_value(value)?;
            ctx.fb.ins("i64.eqz", "empty(int) is the zero test");
            ctx.fb.ins("i64.extend_i32_u", "PHP booleans are i64 here");
        }
        (IrType::I64, PhpType::Bool | PhpType::False) => {
            // A one-word bool slot's missed read leaves the in-band NULL sentinel, and PHP
            // null is empty; a genuine bool is only ever 0 or 1, so the extra test cannot
            // misfire on a real value (the native arm makes the same two checks).
            let word = ctx.fresh_temp(ValType::I64);
            ctx.emit_load_value(value)?;
            ctx.fb.ins(&format!("local.tee {}", word), "bool payload word");
            ctx.fb.ins("i64.eqz", "false is empty");
            ctx.fb.ins(&format!("local.get {}", word), "bool payload word");
            ctx.fb.ins(
                "i64.const 9223372036854775806",
                "null sentinel (0x7fff_ffff_ffff_fffe)",
            );
            ctx.fb.ins("i64.eq", "a sentinel-null bool is empty");
            ctx.fb.ins("i32.or", "either way empty");
            ctx.fb.ins("i64.extend_i32_u", "PHP booleans are i64 here");
        }
        (IrType::F64, PhpType::Float) => {
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins("i64.reinterpret_f64", "float bits for the truthiness helper");
            ctx.fb.ins(
                "call $__rt_float_truthy",
                "PHP float truthiness, with the NaN warning empty carries too",
            );
            ctx.fb.ins("i64.eqz", "empty is the negation of truthy");
            ctx.fb.ins("i64.extend_i32_u", "PHP booleans are i64 here");
        }
        (IrType::Str, PhpType::Str) => {
            // The two falsy strings: "" and the single byte "0".
            let ptr = ctx.fresh_temp(ValType::I32);
            let len = ctx.fresh_temp(ValType::I64);
            ctx.emit_load_value(value)?;
            ctx.fb.ins(&format!("local.set {}", len), "string length");
            ctx.fb.ins(&format!("local.set {}", ptr), "string pointer");
            ctx.fb.ins(&format!("local.get {}", len), "string length");
            ctx.fb.ins("i64.eqz", "\"\" is empty");
            ctx.fb.ins(&format!("local.get {}", len), "string length");
            ctx.fb.ins("i64.const 1", "single-byte candidate");
            ctx.fb.ins("i64.eq", "one byte long?");
            ctx.fb.ins(&format!("local.get {}", ptr), "string pointer");
            ctx.fb.ins("i32.load8_u", "first byte");
            ctx.fb.ins("i32.const 48", "the byte '0'");
            ctx.fb.ins("i32.eq", "is it '0'?");
            ctx.fb.ins("i32.and", "exactly \"0\"");
            ctx.fb.ins("i32.or", "\"\" or \"0\"");
            ctx.fb.ins("i64.extend_i32_u", "PHP booleans are i64 here");
        }
        (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed | PhpType::Union(_)) => {
            ctx.emit_load_value(value)?;
            ctx.fb.ins(
                "call $__rt_mixed_truthy",
                "PHP truthiness of the boxed value, with its NaN warning",
            );
            ctx.fb.ins("i64.eqz", "empty is the negation of truthy");
            ctx.fb.ins("i64.extend_i32_u", "PHP booleans are i64 here");
        }
        (IrType::TaggedScalar, _) => {
            // Tag 8 is null (empty); tag 2 holds float BITS in the payload; the int and bool
            // tags are the plain zero test.
            let payload = ctx.fresh_temp(ValType::I64);
            let tag = ctx.fresh_temp(ValType::I32);
            ctx.emit_load_value(value)?;
            ctx.fb.ins(&format!("local.set {}", tag), "tagged tag");
            ctx.fb.ins(&format!("local.set {}", payload), "tagged payload");
            ctx.fb.ins(&format!("local.get {}", tag), "tagged tag");
            ctx.fb.ins("i32.const 8", "null tag");
            ctx.fb.ins("i32.eq", "tagged null?");
            ctx.fb.ins("if (result i64)", "null arm");
            ctx.fb.ins("i64.const 1", "null is empty");
            ctx.fb.ins("else", "concrete scalar arm");
            ctx.fb.ins(&format!("local.get {}", tag), "tagged tag");
            ctx.fb.ins("i32.const 2", "float tag");
            ctx.fb.ins("i32.eq", "tagged float?");
            ctx.fb.ins("if (result i64)", "float arm");
            ctx.fb.ins(&format!("local.get {}", payload), "float bits payload");
            ctx.fb.ins(
                "call $__rt_float_truthy",
                "PHP float truthiness, with its NaN warning",
            );
            ctx.fb.ins("i64.eqz", "empty is the negation of truthy");
            ctx.fb.ins("i64.extend_i32_u", "PHP booleans are i64 here");
            ctx.fb.ins("else", "int/bool arm");
            ctx.fb.ins(&format!("local.get {}", payload), "scalar payload");
            ctx.fb.ins("i64.eqz", "zero is empty");
            ctx.fb.ins("i64.extend_i32_u", "PHP booleans are i64 here");
            ctx.fb.ins("end", "end float arm");
            ctx.fb.ins("end", "end null arm");
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "empty for operand {:?} on wasm32-wasi",
                other
            )));
        }
    }
    store_result(ctx, inst)
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
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionChain)) => {
            return super::objects::lower_exception_chain(ctx, inst);
        }
        // Unwind bookkeeping only; see `exception_guard_shape_issue` for why nothing is exact.
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionGuardOwned)) => {
            ctx.fb.ins("i64.const 0", "exception.guard_owned: no unwind chain on wasm");
            return store_result(ctx, inst);
        }
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::ExceptionUnguardOwned
            | RuntimeCallTarget::ExceptionUpdateArrayGuard
            | RuntimeCallTarget::ExceptionUpdateHashGuard,
        )) => {
            ctx.fb.comment("exception guard bookkeeping: no unwind chain on wasm");
            return Ok(());
        }
        Some(Immediate::RuntimeCall(target)) => {
            return Err(WasmError::Unsupported(format!(
                "runtime call target {:?}",
                target
            )));
        }
        // An untyped call is discriminated by its operands, exactly as the native backend does
        // it: three operands with a result is the generic `$mixed[$key]` read, and one operand
        // narrowing a boxed value to a declared class is the unbox below.
        // An OBJECT receiver is `ArrayAccess` indexing, not the boxed `$mixed[$key]` read —
        // both carry three operands and a result, so the receiver decides. This arm must stay
        // ahead of the one below, which would otherwise treat the object as a Mixed cell.
        None if super::capability::array_access_read_is_supported(ctx.function, inst) => {
            return super::methods::lower_array_access_get(ctx, inst);
        }
        None if super::capability::array_access_write_is_supported(ctx.function, inst) => {
            return super::methods::lower_array_access_set(ctx, inst);
        }
        None if super::capability::mixed_array_write_is_supported(ctx.module, ctx.function, inst) => {
            return lower_mixed_array_set(ctx, inst);
        }
        None if inst.operands.len() == 3 && !inst.is_void() => {
            return super::inst_hash::lower_mixed_array_get(ctx, inst);
        }
        None if super::capability::tagged_scalar_widening_is_supported(ctx.function, inst) => {
            // The transfer layer already knows how to attach the tag; it is the same conversion
            // an argument or a block parameter performs into the same slot shape.
            let source = operand(inst, 0)?;
            let result = inst
                .result
                .ok_or_else(|| WasmError::Unsupported("tagged widening has no result".into()))?;
            return super::transfer::emit_transfer_value(ctx, source, result);
        }
        None if super::capability::mixed_object_narrowing_is_supported(ctx.function, inst) => {
            return lower_mixed_object_narrowing(ctx, inst);
        }
        _ => {
            return Err(WasmError::Unsupported(format!(
                "untyped runtime call returning {:?}",
                inst.result_php_type
            )));
        }
    };
    // php checks a stream argument (open? length in range?) before the builtin runs at all.
    super::streamguard::emit_guards(ctx, inst, target)?;
    if super::builtins::is_direct_builtin(target) {
        return super::builtins::lower_direct_builtin(ctx, inst, target);
    }
    match target {
        RuntimeFnId::GetClass => super::classes::lower_get_class(ctx, inst),
        RuntimeFnId::GetObjectVars => lower_get_object_vars(ctx, inst),
        RuntimeFnId::Opendir => lower_dir_call(ctx, inst, "$__rt_opendir"),
        RuntimeFnId::Readdir => lower_dir_call(ctx, inst, "$__rt_readdir"),
        RuntimeFnId::Rewinddir => lower_dir_call(ctx, inst, "$__rt_rewinddir"),
        RuntimeFnId::Closedir => lower_dir_call(ctx, inst, "$__rt_closedir"),
        RuntimeFnId::ArrayMap => lower_array_map(ctx, inst),
        RuntimeFnId::ArrayFilter => lower_array_filter(ctx, inst),
        RuntimeFnId::Usort => lower_user_sort(ctx, inst, "usort"),
        RuntimeFnId::Uasort => lower_user_sort(ctx, inst, "uasort"),
        RuntimeFnId::Uksort => lower_user_sort(ctx, inst, "uksort"),
        RuntimeFnId::ArrayReduce => lower_array_reduce(ctx, inst),
        RuntimeFnId::ArrayWalk => lower_array_walk(ctx, inst),
        RuntimeFnId::Settype => lower_settype(ctx, inst),
        other => Err(WasmError::Unsupported(format!(
            "runtime function {:?}",
            other
        ))),
    }
}

/// Lowers `get_object_vars($obj)` to the shared per-class projection walk.
///
/// Everything that decides the answer — which properties, at what offset, under what tag — is the
/// class-id-indexed table `objvars` lays out, so the call site is just the receiver and one call.
/// The audit has already proved the receiver is statically one class, that the call sits outside
/// any class, and that every public property of that class has a `mixed` form.
fn lower_get_object_vars(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let object = operand(inst, 0)?;
    ctx.emit_load_value(object)?;
    ctx.fb.ins(
        "call $__rt_object_vars",
        "project the receiver's public properties into a fresh array<string, mixed>",
    );
    store_result(ctx, inst)
}

/// Lowers one directory-iteration call onto its runtime helper.
///
/// All four take exactly one operand and answer exactly one value, and the operand's WASM
/// representation is what decides the call shape: `opendir`'s path is a `Str` (pointer plus
/// length, two stack values), the other three take the boxed handle (one). `emit_load_value`
/// pushes whichever the operand is, so the helper's signature is the only thing that differs.
fn lower_dir_call(ctx: &mut FnCtx, inst: &Instruction, helper: &str) -> Result<()> {
    let operand = operand(inst, 0)?;
    ctx.emit_load_value(operand)?;
    ctx.fb.ins(&format!("call {helper}"), "directory iteration");
    // `closedir` leaves the box it was given marked closed (payload -1), the way `fclose` does:
    // the fd's shared mark covers the other copies, this covers the box itself even after the
    // number is handed to a new stream.
    let boxed = ctx
        .function
        .value(operand)
        .is_some_and(|value| value.ir_type == IrType::Heap(IrHeapKind::Mixed));
    if helper == "$__rt_closedir" && boxed {
        ctx.emit_load_value(operand)?;
        ctx.fb.ins("i32.const 8", "the resource payload word");
        ctx.fb.ins("i32.add", "");
        ctx.fb.ins("i64.const -1", "closed sentinel");
        ctx.fb.ins("i64.store", "mark the handle closed");
    }
    store_result(ctx, inst)
}

/// Lowers the audited `settype($var, "integer")` subset: a FLOAT local narrowed in place.
///
/// The conversion is the `(int)` cast's own — `emit_float_to_php_int`, php's modulo-2^64
/// rule with the 8.5 diagnostic — and the write-back goes through the shared transfer
/// layer, so the slot's representation (boxed for a float-then-int local, raw otherwise)
/// is the transfer's decision, not a second copy of the rule. A BOXED slot's previous
/// cell is released first: the EIR emits no release around the call, because natively
/// the helper writes the variable in place.
fn lower_settype(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    let slot = value_source_slot(ctx, value).ok_or_else(|| {
        WasmError::Unsupported("settype variable has no local slot to write back".to_string())
    })?;
    if matches!(ctx.slot_repr(slot)?, WasmRepr::Ptr(_)) {
        ctx.emit_load_slot(slot)?;
        ctx.fb.ins(
            "call $__rt_decref_any",
            "release the slot's previous cell (null-safe)",
        );
    }
    ctx.emit_load_value(value)?;
    emit_float_to_php_int(ctx);
    super::transfer::emit_store_stack_value_into_slot(ctx, IrType::I64, PhpType::Int, slot)?;
    if inst.result.is_some() {
        // This narrowing cannot fail; php's settype answers true for every float.
        ctx.fb.ins("i64.const 1", "settype answers true");
        store_result(ctx, inst)?;
    }
    Ok(())
}

/// Lowers the untyped one-operand runtime call that narrows a boxed value to a declared class.
///
/// This is what a `?Money` property looks like after `is_null()` cleared it: the EIR keeps the
/// value boxed up to the `return`, then asks for the declared `Money` slot. The native backend
/// answers it with `__rt_mixed_unbox` plus an incref, and this is the same two steps — the
/// payload word is the object pointer, and the result owns a reference to it.
///
/// A cell that is NOT an object yields the null pointer rather than its raw payload. The EIR
/// only emits this after a guard proved the value non-null, so the arm is unreachable in a
/// program the checker accepted; producing null keeps an unsound one fatal at its next use
/// instead of dereferencing an integer.
fn lower_mixed_object_narrowing(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let cell = operand(inst, 0)?;
    let tag = ctx.fresh_temp(ValType::I64);
    let lo = ctx.fresh_temp(ValType::I64);
    let object = ctx.fresh_temp(ValType::I32);
    ctx.emit_load_value(cell)?;
    ctx.fb
        .ins("call $__rt_mixed_unbox", "unbox -> (tag, lo, hi)");
    ctx.fb.ins("drop", "the high word carries nothing for an object");
    ctx.fb.ins(&format!("local.set {}", lo), "payload word");
    ctx.fb.ins(&format!("local.set {}", tag), "runtime tag");
    ctx.fb.ins(&format!("local.get {}", tag), "runtime tag");
    ctx.fb.ins("i64.const 6", "object tag");
    ctx.fb.ins("i64.eq", "is the boxed value an object?");
    ctx.fb.ins("if (result i32)", "object arm");
    ctx.fb.ins(&format!("local.get {}", lo), "payload word");
    ctx.fb.ins("i32.wrap_i64", "object pointer");
    ctx.fb.ins("else", "not an object");
    ctx.fb.ins("i32.const 0", "null pointer");
    ctx.fb.ins("end", "end object arm");
    ctx.fb
        .ins(&format!("local.tee {}", object), "keep the pointer to incref");
    ctx.fb.ins("call $__rt_incref", "the result owns a reference");
    ctx.fb
        .ins(&format!("local.get {}", object), "the narrowed object");
    store_result(ctx, inst)
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
    // A result the checker narrowed from the callback's declared return is read in THAT
    // layout, so the cells are rebuilt as raw slots.
    if inst.result_type == IrType::Heap(IrHeapKind::Array) {
        let kind = match inst.result_php_type.codegen_repr() {
            PhpType::Array(element) => match element.codegen_repr() {
                PhpType::Int => Some(0),
                PhpType::Str => Some(1),
                PhpType::Float => Some(2),
                _ => None,
            },
            _ => None,
        };
        if let Some(kind) = kind {
            ctx.fb.ins(&format!("i32.const {kind}"), "0 int, 1 string, 2 float slots");
            ctx.fb.ins("call $__rt_array_cells_to", "the layout the result is read in");
        }
    }
    // The EIR's result slot is Mixed unless the checker narrowed it, so the fresh array is
    // boxed under the container tag. `__rt_mixed_from_value` increfs a refcounted child, so
    // the map's own reference is dropped right after to leave the cell the only owner.
    if inst.result_type == IrType::Heap(IrHeapKind::Mixed) {
        let mapped = ctx.fresh_temp(ValType::I32);
        ctx.fb
            .ins(&format!("local.set {}", mapped), "the mapped array");
        ctx.fb.ins("i64.const 4", "container tag (array)");
        ctx.fb.ins(
            &format!("(i64.extend_i32_u (local.get {}))", mapped),
            "the array pointer as the cell's low word",
        );
        ctx.fb.ins("i64.const 0", "no high payload");
        ctx.fb
            .ins("call $__rt_mixed_from_value", "box the mapped array");
        let cell = ctx.fresh_temp(ValType::I32);
        ctx.fb.ins(&format!("local.set {}", cell), "the boxed result");
        ctx.fb
            .ins(&format!("local.get {}", mapped), "the map's own reference");
        ctx.fb
            .ins("call $__rt_decref_any", "the cell owns the array now");
        ctx.fb.ins(&format!("local.get {}", cell), "the boxed result");
    }
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
    write_back_container_slot(ctx, array)?;

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
/// RESULT: `array_walk` answers php's `true`, which the call site may or may not read, so
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

    // php answers `true`, and a statement-position call reads nothing — so the value is
    // materialized only when a result slot is attached AND it is a simple i64; any other
    // result repr is rejected rather than miscompiled.
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
    if super::output_buffer::module_uses_output_buffering(ctx.module)
        && ctx.module.functions.iter().any(|function| function.flags.is_main)
    {
        ctx.fb.ins(
            "call $__rt_ob_flush_all",
            "php flushes output still buffered when exit() ends the script",
        );
    }
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
/// Emits PHP's string conversion of a statically known object, or its `Error` when the class
/// has none.
///
/// Leaves this backend's string storage — `(ptr i32, len i64)` — on the stack, which is exactly
/// what a `__toString` body returns, so the conversion needs no repacking. The `Error` path
/// never returns, so the stack it leaves is the polymorphic one after `unreachable`.
fn emit_object_to_string(ctx: &mut FnCtx, object: ValueId, class_name: &str) -> Result<bool> {
    if let Some(implementation) = super::capability::object_to_string_impl(ctx.module, class_name)
    {
        let symbol = super::symbols::method_symbol(&format!("{}::__toString", implementation));
        ctx.emit_load_value(object)?;
        ctx.fb.ins(
            &format!("call ${}", symbol),
            "PHP converts an object to a string through __toString",
        );
        return Ok(true);
    }
    if super::capability::object_never_stringifies(ctx.module, class_name) {
        ctx.emit_load_value(object)?;
        ctx.fb
            .ins("i64.load", "runtime class id, which the fatal names");
        ctx.fb.ins(
            "call $__rt_fail_object_to_string",
            "PHP's Error for a class with no __toString",
        );
        ctx.fb.ins(
            "unreachable ;; elephc-trap:post-noreturn:object-to-string",
            "the fatal exits",
        );
        return Ok(true);
    }
    Ok(false)
}

/// Lowers `ReleaseLocalSlot`: the early release of a slot leaving scope.
///
/// The return epilogue already releases every owned local, so this is not new ownership work —
/// it is the SAME release, brought forward to where the scope actually ends. Bringing it
/// forward is what makes it safe to run twice: the shared helper clears the slot before
/// releasing, so the epilogue then finds a null and skips it.
fn lower_release_local_slot(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
        return Err(WasmError::Unsupported(
            "release_local_slot without a LocalSlot immediate".to_string(),
        ));
    };
    let php_type = ctx
        .function
        .locals
        .get(slot.as_raw() as usize)
        .map(|local| local.php_type.codegen_repr())
        .ok_or_else(|| {
            WasmError::Unsupported(format!("release_local_slot: unknown slot {:?}", slot))
        })?;
    if capture_slot_still_borrows_from_descriptor(ctx, slot, &php_type) {
        // A reassigned capture owns what an earlier store put there: a second reassignment
        // releases it, while the first still finds the descriptor's borrowed value.
        let Some(flag) = ctx.capture_owned_flags.get(&slot.as_raw()).cloned() else {
            return Ok(());
        };
        ctx.fb.ins(&format!("local.get {flag}"), "does the capture slot own its value yet?");
        ctx.fb.ins("if", "release only a value this frame stored");
        ctx.emit_slot_release(slot, &php_type)?;
        ctx.fb.ins("end", "end owned-capture release");
        return Ok(());
    }
    ctx.emit_slot_release(slot, &php_type)
}

/// Returns whether `slot` is a by-value closure capture this frame only BORROWS.
///
/// EIR emits `ReleaseLocalSlot` before a capture's first reassignment and leaves the decision
/// here, where the final frame type is known: the callable descriptor holds the capture's
/// reference, so releasing it would free a value the closure still owns and the next call would
/// read a freed block. The exception is a capture whose frame storage outgrew its declared
/// parameter type: the prologue boxed it into a fresh Mixed cell this frame owns.
pub(super) fn capture_slot_still_borrows_from_descriptor(
    ctx: &FnCtx,
    slot: LocalSlotId,
    local_ty: &PhpType,
) -> bool {
    let function = ctx.function;
    if !function.flags.is_closure {
        return false;
    }
    // A generator's wrapper gives the frame a reference of its own to each argument it releases
    // (an incref, or a private copy of a string) because the frame outlives the call that built
    // it. The capture is therefore NOT borrowed from the descriptor: the slot owns it from the
    // start, and the release before the first reassignment is real. Skipping it leaked the
    // wrapper's reference on every call. An indexed array is handed over without a second
    // reference, and a callable descriptor is never retained by the wrapper, so those two keep
    // the borrowed answer.
    if function.flags.is_generator && !matches!(local_ty, PhpType::Array(_) | PhpType::Callable) {
        return false;
    }
    let index = slot.as_raw() as usize;
    let capture_start = function
        .params
        .len()
        .saturating_sub(function.flags.closure_capture_count);
    let Some(param) = function.params.get(index) else {
        return false;
    };
    if index < capture_start || param.by_ref {
        return false;
    }
    let prologue_boxed =
        *local_ty == PhpType::Mixed && param.php_type.codegen_repr() != PhpType::Mixed;
    !prologue_boxed
}

/// Lowers `Op::Echo`: writes the operand to standard output through the printer for its
/// PHP type (bool, int, float, string, boxed mixed); a container prints php's "Array" and warns.
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
        // `echo $obj` is PHP's `__toString`, reached without a cast node: the echo itself does
        // the conversion. With the class known statically it is the same direct call the cast
        // arm makes, and the callee's `(ptr, len)` result is exactly what `__rt_echo_str` wants.
        PhpType::Object(ref class_name) => {
            if !emit_object_to_string(ctx, op0, class_name)? {
                return Err(WasmError::Unsupported(format!(
                    "echo of {:?} without a decidable string conversion",
                    php
                )));
            }
            ctx.fb.ins("call $__rt_echo_str", "echo the converted string");
            Ok(())
        }
        // A `void` method call still has an EXPRESSION value in PHP, and that value is null,
        // which `echo` renders as nothing at all.
        PhpType::Void => Ok(()),
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
/// Lowers a release the callee's own answer can cancel.
///
/// `$obj?->m($arg)` releases the argument after the call, but a method that RETURNS that same
/// value moved ownership into the result — releasing it here would free a value the caller is
/// still holding. The native backend compares the two payloads and skips the release when they
/// are the same pointer; this does the same with a `select` over the two runtime paths.
///
/// A value the EIR already knows it does not own, and a scalar that owns nothing, skip the
/// comparison entirely: there is no release to cancel.
fn lower_release_unless_aliases(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    let result = operand(inst, 1)?;
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
    let repr = ctx.value_repr(value)?.clone();
    let result_repr = ctx.value_repr(result)?.clone();
    // Only two pointer-shaped reprs can alias; anything else releases unconditionally, exactly
    // as `lower_release` would, because it cannot be what the callee returned.
    let aliasable = matches!(
        (&repr, &result_repr),
        (WasmRepr::Ptr(_), WasmRepr::Ptr(_)) | (WasmRepr::Str { .. }, WasmRepr::Str { .. })
    );
    if !aliasable {
        return lower_release(ctx, inst);
    }
    match (repr, result_repr) {
        (WasmRepr::Ptr(value_local), WasmRepr::Ptr(result_local)) => {
            ctx.fb.ins(
                &format!("local.get {}", value_local),
                "the released heap pointer",
            );
            ctx.fb.ins(
                &format!("local.get {}", result_local),
                "the pointer the callee answered",
            );
            ctx.fb.ins("i32.ne", "ownership moved out only when they are the same");
            ctx.fb.ins("if", "skip the release when the callee kept it");
            ctx.fb.ins(
                &format!("local.get {}", value_local),
                "release the argument the callee did not keep",
            );
            ctx.fb
                .ins("call $__rt_decref_any", "release the owned heap value by kind");
            ctx.fb.ins("end", "end of the conditional release");
            Ok(())
        }
        (
            WasmRepr::Str { ptr: value_ptr, .. },
            WasmRepr::Str {
                ptr: result_ptr, ..
            },
        ) => {
            ctx.fb
                .ins(&format!("local.get {}", value_ptr), "the released string");
            ctx.fb.ins(
                &format!("local.get {}", result_ptr),
                "the string the callee answered",
            );
            ctx.fb.ins("i32.ne", "ownership moved out only when they are the same");
            ctx.fb.ins("if", "skip the release when the callee kept it");
            ctx.fb
                .ins(&format!("local.get {}", value_ptr), "string pointer to free");
            ctx.fb.ins(
                "call $__rt_heap_free_safe",
                "free the owned string (skips non-heap)",
            );
            ctx.fb.ins("end", "end of the conditional release");
            Ok(())
        }
        _ => unreachable!("the aliasable pairs are the two matched above"),
    }
}

/// Lowers `Op::Release`: gives back the storage the operand owns.
fn lower_release(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    emit_value_release(ctx, operand(inst, 0)?)
}

/// Releases whatever storage a value owns, by its representation.
///
/// Split out of `lower_release` because a second site needs the same thing without an
/// instruction to read it from: a static local's initializer that runs on a later call has
/// built a value nobody will store, and it has no `Op::Release` of its own.
fn emit_value_release(ctx: &mut FnCtx, value: crate::ir::ValueId) -> Result<()> {
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
            clear_released_local_in_generator(ctx, &ptr);
            Ok(())
        }
        WasmRepr::Ptr(pointer) => {
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins("call $__rt_decref_any", "release the owned heap value by kind");
            clear_released_local_in_generator(ctx, &pointer);
            Ok(())
        }
        WasmRepr::I64(_) | WasmRepr::F64(_) | WasmRepr::Void => Ok(()),
        WasmRepr::Tagged { .. } => Ok(()),
    }
}

/// Zeroes a just-released pointer local, but only inside a GENERATOR body.
///
/// Ordinary code never reads a released local again, so leaving the stale pointer there costs
/// nothing. A generator's yield SPILLS every local into the frame whatever its state, and the
/// frame release then walks the spilled pointers — so a value the body had already released was
/// released a SECOND time when the generator was dropped while suspended. That corrupts the
/// allocator's free list, which showed up as `$g->next(); $g->next();` in a 1000-iteration loop
/// never terminating while one iteration of the same code was right.
///
/// Zeroing is what makes the spill honest: `__rt_decref_any` and `__rt_heap_free_safe` both
/// ignore a null, so the frame's copy becomes harmless rather than dangerous.
fn clear_released_local_in_generator(ctx: &mut FnCtx, local: &str) {
    if ctx.generator.is_none() {
        return;
    }
    ctx.fb.ins("i32.const 0", "the spill must not carry a released pointer");
    ctx.fb.ins(&format!("local.set {local}"), "");
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
    // A REF-CELL load names its slot exactly as a plain load does. Reading only `LoadLocal` left
    // a by-reference container with no slot to write back to, so a runtime that MOVED the array —
    // `$a[] = 41` growing it — dropped the new pointer on the floor and the caller kept the stale
    // one. Measured: `function m(array &$a) { $a[] = 41; } $v = [7]; m($v); count($v)` answered
    // 106808 for php-src's 2.
    if matches!(inst.op, Op::LoadLocal | Op::LoadRefCell) {
        if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
            return Some(slot);
        }
    }
    None
}

/// The static-local CELL a value was loaded from, when it was loaded from one.
///
/// A static local's storage is neither the SSA value nor a wasm local: it is a fixed address in
/// linear memory. `value_source_slot` cannot name it — there is no `LocalSlotId` whose wasm
/// locals hold it — so the moved-container write-back needs its own resolver.
fn value_source_static_cell(ctx: &FnCtx, value: ValueId) -> Option<u32> {
    let v = ctx.function.value(value)?;
    let ValueDef::Instruction { inst, .. } = v.def else {
        return None;
    };
    let inst = ctx.function.instruction(inst)?;
    if inst.op != Op::LoadStaticLocal {
        return None;
    }
    let Some(Immediate::LocalSlot(slot_id)) = inst.immediate else {
        return None;
    };
    let name = ctx
        .function
        .locals
        .get(slot_id.as_raw() as usize)?
        .name
        .clone()?;
    let key = super::statics::static_local_slot_key(&ctx.function.name, &name);
    ctx.static_slots.get(&key).map(|slot| slot.address)
}

/// Writes a container pointer that the runtime may have MOVED back into the slot it came from.
///
/// A plain local takes the pointer directly. A by-reference slot is a CELL the caller owns, so
/// the pointer has to go through it — writing the callee's own local instead would leave the
/// caller holding the pre-move address. Callers that grow, sort, or otherwise rebind a container
/// use this rather than open-coding the local store, so the two slot kinds cannot drift apart.
pub(super) fn write_back_container_slot(ctx: &mut FnCtx, value: ValueId) -> Result<()> {
    // A STATIC local is the third slot kind, and it had the same defect the by-ref one had:
    // `static $a = []; $a[] = $x;` grew the array, `__rt_array_grow` freed the old block, and
    // the cell kept naming it. The next call read a freed block — `print_r` answered four
    // copies of a raw pointer where php prints two integers.
    //
    // No refcount work: the append MOVED the container, so the reference the cell held went
    // with it and the block it named is already gone.
    if let Some(address) = value_source_static_cell(ctx, value) {
        let value_ref = ctx.value_repr(value)?.local_refs();
        if value_ref.len() == 1 {
            let pointer = value_ref[0].clone();
            ctx.fb.ins(
                &format!(
                    "(i64.store (i32.const {address}) (i64.extend_i32_u (local.get {pointer})))"
                ),
                "the static cell takes the moved container",
            );
        }
        return Ok(());
    }
    let Some(slot) = value_source_slot(ctx, value) else {
        return Ok(());
    };
    let value_ref = ctx.value_repr(value)?.local_refs();
    if value_ref.len() != 1 {
        return Ok(());
    }
    let pointer = value_ref[0].clone();
    if let Ok(cell) = ctx.ref_cell_ptr(slot.as_raw()) {
        let cell = cell.to_string();
        ctx.fb.ins(&format!("local.get {cell}"), "by-ref cell address");
        ctx.fb
            .ins(&format!("local.get {pointer}"), "container pointer after the call");
        ctx.fb
            .ins("i32.store offset=0", "write the moved container through the cell");
        return Ok(());
    }
    let slot_ref = ctx.slot_repr(slot)?.local_refs();
    if slot_ref.len() == 1 {
        ctx.fb
            .ins(&format!("local.get {pointer}"), "container pointer after the call");
        ctx.fb
            .ins(&format!("local.set {}", slot_ref[0]), "write back to the container slot");
    }
    Ok(())
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

/// Whether this operand is a plain integer word rather than something boxed.
///
/// The ordering helpers take one of those unboxed, so the answer picks which of them runs.
fn spaceship_operand_is_plain_int(ctx: &FnCtx, value: crate::ir::ValueId) -> bool {
    ctx.function.value(value).is_some_and(|v| {
        v.ir_type == IrType::I64
            && matches!(
                v.php_type.codegen_repr(),
                PhpType::Int | PhpType::Bool | PhpType::False
            )
    })
}

/// Whether this operand is a plain float word.
fn spaceship_operand_is_plain_float(ctx: &FnCtx, value: crate::ir::ValueId) -> bool {
    ctx.function
        .value(value)
        .is_some_and(|v| v.ir_type == IrType::F64 && v.php_type.codegen_repr() == PhpType::Float)
}

/// Pushes an operand as an f64, converting a plain integer on the way.
fn emit_spaceship_operand_as_float(ctx: &mut FnCtx, value: crate::ir::ValueId) -> Result<()> {
    ctx.emit_load_value(value)?;
    if spaceship_operand_is_plain_int(ctx, value) {
        ctx.fb
            .ins("f64.convert_i64_s", "the integer side of a float comparison");
    }
    Ok(())
}

/// Lowers `Op::Spaceship` — php's `<=>`, answering -1, 0 or 1.
///
/// This is the ORDERING the relational operators are predicates over, so it runs on the same
/// three-way helpers `Op::PhpRelCmp` already uses rather than on a second copy of php's
/// comparison table. Which one depends only on how the operands are stored:
///
/// * two plain integers compare as longs, exactly — `PHP_INT_MAX <=> PHP_INT_MAX - 1` is 1
///   where converting to double first would answer 0;
/// * a float on either side makes it a double comparison, where php answers 1 for a NaN in
///   BOTH directions rather than the 0 a `gt - lt` would give;
/// * a boxed operand carries its own tag, and php's rules for one are not "cast then compare"
///   — a non-numeric string against a long renders the LONG as a string and compares bytes.
///   `__rt_mixed_cmp_i64` and `__rt_mixed_cmp_mixed` are that table.
///
/// `cmp(int, cell)` is `-cmp(cell, int)`, which is how a plain integer on the LEFT reaches the
/// cell-first helper.
fn lower_spaceship(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let left = operand(inst, 0)?;
    let right = operand(inst, 1)?;
    let left_int = spaceship_operand_is_plain_int(ctx, left);
    let right_int = spaceship_operand_is_plain_int(ctx, right);
    let has_float =
        spaceship_operand_is_plain_float(ctx, left) || spaceship_operand_is_plain_float(ctx, right);

    if left_int && right_int {
        ctx.emit_load_value(left)?;
        ctx.emit_load_value(right)?;
        ctx.fb
            .ins("call $__rt_i64_threeway", "two longs compare as longs");
        return store_result(ctx, inst);
    }
    if has_float && (left_int || spaceship_operand_is_plain_float(ctx, left)) && (right_int || spaceship_operand_is_plain_float(ctx, right))
    {
        emit_spaceship_operand_as_float(ctx, left)?;
        emit_spaceship_operand_as_float(ctx, right)?;
        ctx.fb.ins(
            "call $__rt_f64_threeway",
            "a float on either side, with php's NaN-answers-1 rule",
        );
        return store_result(ctx, inst);
    }
    if left_int || right_int {
        // The helper takes the cell first, so a plain integer on the LEFT is served by the
        // mirrored call and one negation.
        let (cell_operand, plain, mirrored) = if right_int {
            (left, right, false)
        } else {
            (right, left, true)
        };
        let cell = ctx.fresh_temp(ValType::I32);
        let cell_owned = emit_operand_as_mixed(ctx, cell_operand)?;
        ctx.fb.ins(&format!("local.set {cell}"), "boxed operand cell");
        ctx.fb.ins(&format!("local.get {cell}"), "boxed operand cell");
        ctx.emit_load_value(plain)?;
        ctx.fb.ins(
            "call $__rt_mixed_cmp_i64",
            "php's ordering of a boxed value against a long",
        );
        if mirrored {
            let answer = ctx.fresh_temp(ValType::I64);
            ctx.fb.ins(&format!("local.set {answer}"), "cmp(cell, int)");
            ctx.fb.ins("i64.const 0", "negate: cmp(int, cell) = -cmp(cell, int)");
            ctx.fb.ins(&format!("local.get {answer}"), "cmp(cell, int)");
            ctx.fb.ins("i64.sub", "the operands were the other way round");
        }
        store_result(ctx, inst)?;
        if cell_owned {
            ctx.fb.ins(&format!("local.get {cell}"), "temporary cell");
            ctx.fb
                .ins("call $__rt_decref_mixed", "release the boxed operand");
        }
        return Ok(());
    }
    let lhs = ctx.fresh_temp(ValType::I32);
    let rhs = ctx.fresh_temp(ValType::I32);
    let lhs_owned = emit_operand_as_mixed(ctx, left)?;
    ctx.fb.ins(&format!("local.set {lhs}"), "left operand cell");
    let rhs_owned = emit_operand_as_mixed(ctx, right)?;
    ctx.fb.ins(&format!("local.set {rhs}"), "right operand cell");
    ctx.fb.ins(&format!("local.get {lhs}"), "left operand cell");
    ctx.fb.ins(&format!("local.get {rhs}"), "right operand cell");
    ctx.fb.ins(
        "call $__rt_mixed_cmp_mixed",
        "php's ordering of two boxed values",
    );
    store_result(ctx, inst)?;
    if lhs_owned {
        ctx.fb.ins(&format!("local.get {lhs}"), "temporary left cell");
        ctx.fb
            .ins("call $__rt_decref_mixed", "release the boxed left operand");
    }
    if rhs_owned {
        ctx.fb.ins(&format!("local.get {rhs}"), "temporary right cell");
        ctx.fb
            .ins("call $__rt_decref_mixed", "release the boxed right operand");
    }
    Ok(())
}

/// Lowers `Op::ArrayIsset` — `isset($a[$i])` over an indexed array.
///
/// php folds two questions into one answer, and so does the helper: an offset that is not
/// there and an offset holding null are both false. The index arrives as an integer word,
/// because the checker requires an integer-like offset here — a boxed or string key reaches
/// the associative path instead.
fn lower_array_isset(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let array = operand(inst, 0)?;
    let index = operand(inst, 1)?;
    ctx.emit_load_value(array)?;
    ctx.emit_load_value(index)?;
    ctx.fb.ins(
        "call $__rt_array_isset",
        "isset over an indexed array: present and non-null",
    );
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
    // A BOXED key goes through php's key coercion — canonical-decimal strings only,
    // floats truncating under a deprecation, null as the "" string key — which is a
    // different contract from every scalar conversion this file lowers. The helper
    // answers an owned cell (the element by the array's stamp, or null on a miss).
    if matches!(
        ctx.function.value(index).map(|v| v.ir_type),
        Some(IrType::Heap(IrHeapKind::Mixed) | IrType::Str)
    ) {
        let warn = if matches!(inst.op, Op::ArrayGet | Op::ArrayGetMixedKey) { 1 } else { 0 };
        ctx.emit_load_value(array)?;
        // A concrete string key is boxed for the call; the helper borrows it and the
        // temporary is released right after.
        let key_temp = if matches!(ctx.function.value(index).map(|v| v.ir_type), Some(IrType::Str)) {
            let repr = ctx.value_repr(index)?.clone();
            let cell = box_value_into_mixed_cell(ctx, index, &repr)?;
            ctx.fb.ins(&format!("local.get {cell}"), "boxed string key");
            Some(cell)
        } else {
            ctx.emit_load_value(index)?;
            None
        };
        ctx.fb
            .ins(&format!("i32.const {warn}"), "warn on a miss? (silent reads do not)");
        ctx.fb.ins(
            "call $__rt_array_get_mixed_index",
            "indexed read through php's boxed-key coercion",
        );
        if let Some(cell) = key_temp {
            ctx.fb.ins(
                &format!("(call $__rt_decref_any (local.get {cell}))"),
                "release the boxing temporary",
            );
        }
        // An `array<int>` read answers the int|null TAGGED pair: unbox the helper's cell
        // into (payload, tag) and give the cell back — the pair carries no reference.
        if matches!(ctx.value_repr(result)?, WasmRepr::Tagged { .. }) {
            let cell = ctx.fresh_temp(ValType::I32);
            ctx.fb
                .ins(&format!("local.set {cell}"), "the helper's owned cell");
            ctx.fb.ins(
                &format!("(call $__rt_mixed_unbox (local.get {cell}))"),
                "read (tag, lo, hi) out of the cell",
            );
            ctx.fb.ins("drop", "hi is unused for int|null");
            let lo = ctx.fresh_temp(ValType::I64);
            let tag = ctx.fresh_temp(ValType::I64);
            ctx.fb.ins(&format!("local.set {lo}"), "payload");
            ctx.fb.ins(&format!("local.set {tag}"), "tag");
            ctx.fb.ins(
                &format!("(call $__rt_decref_any (local.get {cell}))"),
                "the pair carries no reference",
            );
            ctx.fb.ins(&format!("local.get {lo}"), "tagged payload");
            ctx.fb.ins(
                &format!("(i32.wrap_i64 (local.get {tag}))"),
                "tagged tag (0 int, 8 null)",
            );
        }
        return ctx.emit_store_value(result);
    }
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
        // A read the audit proved in bounds keeps the element's own width: the getter's tag can
        // only be the integer one, so it is dropped rather than tested. The audit is what makes
        // this safe — a raw i64 has no null to answer a miss with — so the two must not drift.
        (PhpType::Int, WasmRepr::I64(_)) => {
            ctx.emit_load_value(array)?;
            ctx.emit_load_value(index)?;
            ctx.fb.ins(
                "call $__rt_array_get_tagged_int",
                "indexed array get, proven in bounds",
            );
            ctx.fb.ins("drop", "the tag can only be the integer one here");
        }
        // A float element read the audit PROVED in bounds keeps its own width. There is no
        // tagged form to fall back on and no need for one: `__rt_array_get_float` answers NaN
        // for a miss because every bit pattern is a valid float, so a read that might miss is
        // refused rather than given a sentinel that a real value could collide with.
        (PhpType::Float, WasmRepr::F64(_)) => {
            ctx.emit_load_value(array)?;
            ctx.emit_load_value(index)?;
            ctx.fb.ins(
                "call $__rt_array_get_float",
                "indexed array get, proven in bounds",
            );
        }
        (PhpType::Bool, WasmRepr::Ptr(_)) => {
            ctx.emit_load_value(array)?;
            ctx.emit_load_value(index)?;
            ctx.fb
                .ins("call $__rt_array_get_mixed_bool", "indexed array get (boxed bool|null)");
        }
        (PhpType::Mixed, WasmRepr::Ptr(_)) => {
            ctx.emit_load_value(array)?;
            ctx.emit_load_value(index)?;
            ctx.fb.ins(
                "call $__rt_array_get_mixed_cell",
                "indexed array get (stored cell, or boxed null)",
            );
        }
        (PhpType::Str, WasmRepr::Ptr(_)) => {
            ctx.emit_load_value(array)?;
            ctx.emit_load_value(index)?;
            ctx.fb
                .ins("call $__rt_array_get_mixed_str", "indexed array get (boxed string|null)");
        }
        // An element that is itself an ARRAY, or an OBJECT, is one pointer in an 8-byte slot.
        // Both read the same way, and both use pointer 0 for the miss — the same null the
        // native backend hands back, and the value every getter now treats as "read through a
        // missed element" rather than loading from address 0.
        //
        // `__rt_array_get_object` hands back the stored pointer BORROWED, but the EIR types this
        // result `own=owned` and emits a matching `release`. Without a reference of its own that
        // release drops the PARENT's, freeing a child the parent still points at — which is why
        // `$g[1][0][1]` read freed memory while `$x = $g[1]; count($x)` did not: the chained form
        // has no `acquire` to balance it. `__rt_incref` is null-safe, so a miss stays a miss.
        (PhpType::Array(_) | PhpType::Object(_), WasmRepr::Ptr(pointer)) => {
            let pointer = pointer.clone();
            ctx.emit_load_value(array)?;
            ctx.emit_load_value(index)?;
            ctx.fb.ins(
                "call $__rt_array_get_object",
                "indexed array get (pointer element, 0 when missing)",
            );
            if inst.result_ownership == Ownership::Owned {
                ctx.fb.ins(&format!("local.tee {pointer}"), "keep the pointer on the stack");
                ctx.fb
                    .ins("call $__rt_incref", "the caller owns what it reads (null-safe)");
                ctx.fb.ins(&format!("local.get {pointer}"), "restore the read result");
            }
        }
        (element, repr) => {
            return Err(WasmError::Unsupported(format!(
                "array_get element {:?} into {:?}",
                element, repr
            )));
        }
    }
    store_result(ctx, inst)?;
    // A proven-in-bounds read has no miss to diagnose, and no tag left to test for one.
    let proven_in_bounds = (element_type == PhpType::Int
        && matches!(result_repr, WasmRepr::I64(_)))
        || (element_type == PhpType::Float && matches!(result_repr, WasmRepr::F64(_)));
    if inst.op == Op::ArrayGet && !proven_in_bounds {
        emit_undefined_array_index_warning_if_null(
            ctx,
            array,
            index,
            &element_type,
            &result_repr,
        )?;
    }
    Ok(())
}

/// Emits the warning-only branch for a normal indexed read whose result is null.
///
/// Tagged integers carry the null tag in their i32 tag local. Boxed bool/string reads carry it
/// at offset zero of the fresh Mixed cell. A POINTER element — a nested array or an object —
/// is null when the pointer itself is 0; reading a tag at its offset zero would read an array's
/// LENGTH instead, which is why the element type rather than the representation decides. The
/// index SSA value is reloaded only after the getter has completed, so it is evaluated exactly
/// once.
///
/// PHP has two different diagnostics here and picks between them by looking at the SOURCE, not
/// the result. Reading a missing key off a real array is `Undefined array key N`; reading any
/// offset off a null — which is what a chained read gets after an earlier miss — is `Trying to
/// access array offset on null`. Both produce a null result, so the source pointer is what
/// separates them:
///
/// ```text
///   $a = [[1, 2]];  $b = $a[5];  echo $b[1];
///   php-src: Warning: Undefined array key 5
///            Warning: Trying to access array offset on null
/// ```
fn emit_undefined_array_index_warning_if_null(
    ctx: &mut FnCtx,
    array: ValueId,
    index: ValueId,
    element_type: &PhpType,
    result_repr: &WasmRepr,
) -> Result<()> {
    match (element_type, result_repr) {
        (PhpType::Array(_) | PhpType::Object(_), WasmRepr::Ptr(pointer)) => {
            ctx.fb
                .ins(&format!("local.get {pointer}"), "read the pointer element");
            ctx.fb.ins("i32.eqz", "a null pointer IS the missing element");
        }
        (_, WasmRepr::Tagged { tag, .. }) => {
            ctx.fb.ins(&format!("local.get {tag}"), "read nullable result tag");
            ctx.fb.ins("i32.const 8", "Mixed null tag");
            ctx.fb.ins("i32.eq", "missing indexed element");
        }
        (_, WasmRepr::Ptr(cell)) => {
            ctx.fb.ins(&format!("local.get {cell}"), "load boxed nullable result");
            ctx.fb.ins("i64.load", "Mixed tag @ +0");
            ctx.fb.ins("i64.const 8", "Mixed null tag");
            ctx.fb.ins("i64.eq", "missing indexed element");
        }
        (_, other) => {
            return Err(WasmError::Unsupported(format!(
                "array_get warning for result representation {:?}",
                other
            )));
        }
    }
    ctx.fb.ins("if", "warn only when the indexed read produced null");
    // A null SOURCE means this read was chained onto an earlier miss, which is PHP's other
    // diagnostic. The source is loaded here rather than reused from before the getter so the
    // non-null path pays nothing for it.
    ctx.emit_load_value(array)?;
    ctx.fb.ins("i32.eqz", "was the array itself null?");
    ctx.fb.ins("if", "select PHP's diagnostic from the SOURCE");
    ctx.fb.ins(
        "call $__rt_warn_array_offset_on_null",
        "offset read through a null",
    );
    ctx.fb.ins("else", "a real array with no such key");
    ctx.emit_load_value(index)?;
    ctx.fb.ins(
        "call $__rt_warn_undefined_array_key_int",
        "emit PHP undefined-array-key warning",
    );
    ctx.fb.ins("end", "end diagnostic selection");
    ctx.fb.ins("end", "continue with the stored null result");
    Ok(())
}

/// Resolves the `Class::$prop` label an `Op::LoadStaticProperty` / `Op::StoreStaticProperty`
/// carries into a LOCAL holding its slot's absolute address, plus the declared type.
///
/// A label naming a class — including `self::` and `parent::`, which are compile-time scopes —
/// has one address, and the local just holds the constant. `static::` does not: late static
/// binding picks the slot from the CALLED class, so the local is set by an if-ladder over the
/// class id every static call already forwards as hidden parameter 0. Producing an ADDRESS
/// rather than a constant offset is what lets one load/store body serve both.
fn static_property_address(ctx: &mut FnCtx, inst: &Instruction) -> Result<(String, PhpType)> {
    let label = ctx
        .module
        .data
        .strings
        .get(data_immediate(inst)?.as_raw() as usize)
        .cloned()
        .ok_or_else(|| {
            WasmError::Unsupported("static property without an interned label".to_string())
        })?;
    let scope = super::statics::enclosing_class(&ctx.function.name).map(str::to_string);
    let address = ctx.fresh_temp(ValType::I32);
    if let Some(slot) =
        super::statics::resolve_label(ctx.module, ctx.static_slots, &label, scope.as_deref())
    {
        let (slot_address, php_type) = (slot.address, slot.php_type.codegen_repr());
        ctx.fb.ins(
            &format!("(local.set {address} (i32.const {slot_address}))"),
            "the one slot this label names",
        );
        return Ok((address, php_type));
    }
    let (_, property) = label
        .split_once("::")
        .ok_or_else(|| WasmError::Unsupported(format!("malformed static label {label}")))?;
    let property = property.trim_start_matches('$').to_string();
    let scope = scope.ok_or_else(|| {
        WasmError::Unsupported(format!("static property {label} has no lowered slot"))
    })?;
    let arms: Vec<(u64, u32, PhpType)> =
        super::statics::late_static_slots(ctx.module, ctx.static_slots, &property, &scope)
            .into_iter()
            .map(|(class_id, slot)| (class_id, slot.address, slot.php_type.codegen_repr()))
            .collect();
    let Some((_, _, php_type)) = arms.first().cloned() else {
        return Err(WasmError::Unsupported(format!(
            "static property {label} has no lowered slot"
        )));
    };
    if let Some((_, _, other)) = arms.iter().find(|(_, _, candidate)| *candidate != php_type) {
        return Err(WasmError::Unsupported(format!(
            "static property {label} is {php_type:?} on one class and {other:?} on another, \
             which a late-bound access cannot read at one width"
        )));
    }
    let called = ctx
        .function
        .locals
        .iter()
        .find(|local| super::objects::is_called_class_id_local(local.name.as_deref()))
        .map(|local| local.id)
        .ok_or_else(|| {
            WasmError::Unsupported(format!(
                "{label} outside a function carrying the called-class id"
            ))
        })?;
    let class_id = ctx.fresh_temp(ValType::I64);
    ctx.emit_load_slot(called)?;
    ctx.fb.ins(
        &format!("local.set {class_id}"),
        "the called class, forwarded by every static call",
    );
    ctx.fb.ins(
        &format!("(local.set {address} (i32.const 0))"),
        "no arm has matched yet",
    );
    for (candidate, slot_address, _) in &arms {
        ctx.fb.ins(
            &format!(
                "(if (i64.eq (local.get {class_id}) (i64.const {})) (then (local.set {address} (i32.const {slot_address}))))",
                *candidate as i64
            ),
            "this class selects its own slot",
        );
    }
    ctx.fb.ins(
        &format!(
            "(if (i32.eqz (local.get {address})) (then (call $__rt_fail (i32.const 16)) unreachable))"
        ),
        "elephc-trap:post-noreturn:late-static-property-unresolved",
    );
    Ok((address, php_type))
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
    let cached =
        emit_enum_case_singleton(ctx, &enum_name, &case_name, case_value.as_ref(), address)?;
    ctx.fb.ins(
        &format!("(call $__rt_incref (local.get {}))", cached),
        "a case read hands out an OWNED reference",
    );
    ctx.fb.ins(&format!("local.get {}", cached), "the case singleton");
    store_result(ctx, inst)
}

/// Materializes one enum case singleton, once, and answers the local holding its pointer.
///
/// A case is PHP's own class constant holding a SINGLETON object, so the slot starts at zero and
/// the object is built on first use — the same lazy shape the native backend uses. The reference
/// is left BORROWED: a plain `Suit::Hearts` read increfs it for its consumer, and `cases()`
/// increfs it for the array, so folding the incref in here would double-count one of them.
pub(super) fn emit_enum_case_singleton(
    ctx: &mut FnCtx,
    enum_name: &str,
    case_name: &str,
    case_value: Option<&crate::types::EnumCaseValue>,
    address: u32,
) -> Result<String> {
    let class_info = ctx
        .module
        .class_infos
        .get(enum_name)
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
    // An enum case is not a Throwable, so it carries no creation line.
    let object = super::objects::emit_object_allocation(ctx, enum_name, &class_info, 0)?;
    // `name` is every case's property; a BACKED enum adds `value`.
    let (_, name_offset, _) = super::objects::resolve_property_slot(&class_info, "name")?;
    super::objects::emit_scalar_default(
        ctx,
        &object,
        name_offset,
        &crate::codegen::LiteralDefaultValue::Str(case_name.to_string()),
        "name",
    )?;
    if let Some(value) = case_value {
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
    Ok(cached)
}

/// Drops the reference a static store CONSUMED, when the EIR handed one over.
///
/// A widened arithmetic result arrives as a freshly built Mixed cell with `own=owned` and
/// NO matching release — the EIR expects the store to take it. Narrowing reads the cell's
/// payload out and leaves the cell itself with no owner, so it is dropped here.
pub(super) fn release_consumed_static_source(
    ctx: &mut FnCtx,
    value: ValueId,
    narrows: bool,
) -> Result<()> {
    if !narrows {
        return Ok(());
    }
    if !matches!(
        ctx.function.value(value).map(|v| v.ownership),
        Some(Ownership::Owned)
    ) {
        return Ok(());
    }
    ctx.emit_load_value(value)?;
    ctx.fb
        .ins("call $__rt_decref_any", "the narrowed cell had no other owner");
    Ok(())
}

/// Pushes an operand in the SLOT's storage shape, converting it if it arrives in another.
///
/// A static slot's type is fixed by the declaration and an assignment's type is whatever the
/// expression produced, so the two differ routinely: `C::$maybe = 5;` on a `?int` hands over a
/// bare long, and `C::$maybe = null;` a Void. The transfer table already states every such
/// conversion, and asking it here is what keeps the slot from growing a private copy of it.
///
/// Returns whether the conversion ALLOCATED — a boxed scalar, a widened array — which is the
/// question `emit_slot_retain` asks, because a fresh value has no other owner.
fn emit_static_slot_source(
    ctx: &mut FnCtx,
    value: crate::ir::ValueId,
    slot_ir: IrType,
    slot_php: PhpType,
) -> Result<bool> {
    let (value_ir, value_php) = ctx
        .function
        .value(value)
        .map(|v| (v.ir_type, v.php_type.codegen_repr()))
        .ok_or_else(|| {
            WasmError::Unsupported(format!("static store operand {value:?} missing"))
        })?;
    ctx.emit_load_value(value)?;
    let allocated =
        super::transfer::emit_convert_stack_value(ctx, value_ir, value_php, slot_ir, slot_php)?;
    Ok(allocated.is_some())
}

/// Takes the slot's own reference to a refcounted value, when the slot needs one.
///
/// `allocated` says the materialization just MADE this value — a boxed scalar, a narrowed
/// string copy. Nothing else owns it and no EIR release names it, so the slot takes it as it
/// stands; increffing there would strand the block for the life of the program.
fn emit_slot_retain(
    ctx: &mut FnCtx,
    ownership: SlotOwnership,
    allocated: bool,
    pointer: &str,
    what: &str,
) -> Result<()> {
    if ownership == SlotOwnership::Steal || allocated {
        return Ok(());
    }
    // Before the release of the previous occupant, not after: `static $n; $n = $n;` names the
    // same block twice, and releasing first would free what is about to be stored.
    ctx.fb.ins(
        &format!("(call $__rt_incref (local.get {pointer}))"),
        &format!("the slot takes its own reference to the {what}; the EIR releases the assigned value"),
    );
    Ok(())
}

/// What a READER of a static slot owes the value it reads.
///
/// The two slot kinds differ here for the same reason they differ on the write: the EIR
/// annotates them differently, and the annotation is the contract.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SlotRead {
    /// Always take a reference. A static PROPERTY read is typed `own=maybe_owned` and the EIR
    /// emits the matching release, so the reader owes one unconditionally — without it the
    /// release comes out of the SLOT's reference and the next read finds freed storage.
    Owned,
    /// Take one only where the EIR will give it back, which `owned_heap_load` decides. A static
    /// LOCAL read is not annotated owned, and an unconditional retain leaked one block per
    /// call: `return $n;` hands the cell to the declared-int return boundary, which consumes
    /// it and releases nothing. A read that ESCAPES is paid for at the return instead, the
    /// same way a returned parameter slot is.
    AsAnnotated,
}

/// Who owns a refcounted value once a static slot has taken it.
///
/// The two static writes differ here and nowhere else, so this is the whole difference between
/// them rather than two copies of one routine.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SlotOwnership {
    /// The slot takes OVER the caller's reference. A static property store works this way, and
    /// so does a static local's one-time initializer: the EIR emits no release for either, so
    /// taking a second reference would leak the first.
    Steal,
    /// The slot takes a reference OF ITS OWN. An ordinary `static $x = ...;` assignment works
    /// this way: the EIR still releases the assigned value once the statement ends, so a slot
    /// that merely recorded the pointer would be left holding freed storage on the next call.
    /// That is what `static $n = 0; $n = $n + 1;` did before this existed — the counter read
    /// back as null on its second call, because the cell it named had already been freed.
    Retain,
}

/// Resolves a function static local to `(value address, type, initialized-flag address)`.
///
/// All three static-local ops name the variable the same way — a `LocalSlot` immediate into
/// the function's own local table — and the planner keyed the slot by the same pair, so the
/// three share one resolver.
fn static_local_address(ctx: &mut FnCtx, inst: &Instruction) -> Result<(String, PhpType, u32)> {
    let Some(Immediate::LocalSlot(slot_id)) = inst.immediate else {
        return Err(WasmError::Unsupported(format!(
            "{} without a local-slot immediate",
            inst.op.name()
        )));
    };
    let name = ctx
        .function
        .locals
        .get(slot_id.as_raw() as usize)
        .and_then(|local| local.name.clone())
        .ok_or_else(|| {
            WasmError::Unsupported(format!("{} names no source variable", inst.op.name()))
        })?;
    let key = super::statics::static_local_slot_key(&ctx.function.name, &name);
    let slot = ctx.static_slots.get(&key).ok_or_else(|| {
        WasmError::Unsupported(format!("static local {key} has no lowered slot"))
    })?;
    let (slot_address, php_type) = (slot.address, slot.php_type.codegen_repr());
    let address = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(
        &format!("(local.set {address} (i32.const {slot_address}))"),
        "the one cell this static local names",
    );
    Ok((
        address,
        php_type,
        slot_address + super::statics::STATIC_LOCAL_INIT_OFFSET,
    ))
}

/// Lowers `Op::LoadStaticLocal` — reading a function's `static $x`.
fn lower_load_static_local(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let (address, php_type, _) = static_local_address(ctx, inst)?;
    emit_static_slot_load(ctx, inst, &address, php_type, SlotRead::AsAnnotated)
}

/// Lowers `Op::StoreStaticLocal` — assigning to a function's `static $x`.
fn lower_store_static_local(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    let (address, php_type, _) = static_local_address(ctx, inst)?;
    emit_static_slot_store(ctx, value, &address, php_type, SlotOwnership::Retain)
}

/// Lowers `Op::StaticLocalUninitialized`: 1 until the initializer of `static $x = expr;` ran.
///
/// EIR branches on it so the initializer expression is only BUILT on the first call. It reads
/// the same run-once flag `lower_init_static_local` sets, which that store still tests too.
fn lower_static_local_uninitialized(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let (_, _, flag) = static_local_address(ctx, inst)?;
    ctx.fb.ins(
        &format!("(i32.load (i32.const {flag}))"),
        "has this initializer already run?",
    );
    ctx.fb.ins("i32.eqz", "1 while it has not");
    ctx.fb.ins("i64.extend_i32_u", "the flag as an i64");
    store_result(ctx, inst)
}

/// Lowers `Op::InitStaticLocal` — the `= expr` half of `static $x = expr;`.
///
/// php runs that initializer ONCE, on the first call, and every later call finds what the
/// last one left. So the store is guarded by a flag rather than by the value: `static $x = 0;`
/// and `static $x = null;` both leave a cell indistinguishable from an untouched one, and
/// re-running the initializer would reset the variable on every call — which is the whole
/// thing a static local exists not to do.
///
/// The EXPRESSION, though, is not inside the guard — EIR evaluates it before this instruction
/// reaches the backend, so every call builds it and only the first call stores it. Nothing
/// observable follows from building it twice, because php restricts a static initializer to a
/// constant expression, but the block has to go back: the `else` branch releases it.
fn lower_init_static_local(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    let (address, php_type, flag) = static_local_address(ctx, inst)?;
    ctx.fb.ins(
        &format!("(i32.load (i32.const {flag}))"),
        "has this initializer already run?",
    );
    ctx.fb.ins("i32.eqz", "only the first call initializes");
    ctx.fb.ins("if", "static local initializer");
    ctx.fb.ins(
        &format!("(i32.store (i32.const {flag}) (i32.const 1))"),
        "mark it run before the store, so a re-entrant call sees it",
    );
    emit_static_slot_store(ctx, value, &address, php_type, SlotOwnership::Steal)?;
    ctx.fb.ins("else", "the initializer has already run");
    // The EXPRESSION is not inside the guard — EIR evaluates it before this instruction, on
    // every call — so a later call has just built a value the slot will not take. php builds
    // it once; nothing observable follows from building it again, because a static
    // initializer is a constant expression, but the block does have to go back. `static
    // $names = ["zero", "one", "two"];` read in a loop grew from 5 pages to 539 over 200k
    // calls without this.
    emit_value_release(ctx, value)?;
    ctx.fb.ins("end", "end static local initializer");
    Ok(())
}

/// Lowers `Op::LoadStaticProperty` (`Class::$prop`).
///
/// Reads the 16-byte slot at its declared width: one word for a scalar, `(pointer, length)` for
/// a string, and a single pointer for a CONTAINER — which the reader takes a reference to,
/// because the slot keeps its own and the EIR's release balances the one handed out here.
fn lower_load_static_property(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let (address, php_type) = static_property_address(ctx, inst)?;
    emit_static_slot_load(ctx, inst, &address, php_type, SlotRead::Owned)
}

/// Reads one 16-byte static slot at its declared width, whatever named it.
///
/// A static PROPERTY and a function's `static $x` are the same storage with different keys, so
/// they read it the same way — which is why this is a function and not a copy.
fn emit_static_slot_load(
    ctx: &mut FnCtx,
    inst: &Instruction,
    address: &str,
    php_type: PhpType,
    read: SlotRead,
) -> Result<()> {
    // The slot's width and the RESULT's width are two different things, and the second is the
    // one the consumer will read. `static $n = 0; $n = $n + 1;` types the slot Mixed — checked
    // arithmetic widens, because an overflow promotes the sum to a float — while the addition
    // that reads it wants a concrete i64. Reading at the slot's width and stopping there hands
    // the adder a cell POINTER, which is how this first showed up: a module that fails
    // validation with "expected i64, found i32".
    let result_php = match inst.result {
        Some(result) => ctx.value_php_type(result)?.codegen_repr(),
        None => php_type.clone(),
    };
    let result_ir = match inst.result {
        Some(result) => ctx
            .function
            .value(result)
            .map(|value| value.ir_type)
            .ok_or_else(|| {
                WasmError::Unsupported(format!("{} result has no EIR type", inst.op.name()))
            })?,
        None => IrType::from_php(&php_type),
    };
    let slot_ir = IrType::from_php(&php_type);
    // And the same question every other load on this target asks: does the reader OWE a
    // reference? `owned_heap_load` is the one answer — the EIR must both declare the result
    // owned and release it somewhere the release is not the only use. A concrete result is not
    // a heap value at all, so a slot read for its integer answers no here without a second
    // test.
    let takes_reference = read == SlotRead::Owned
        || inst.result.is_some_and(|result| owned_heap_load(ctx, result));
    // A BOXED slot is the exception to `SlotRead::Owned`, and it is new rather than a change:
    // a Mixed static property became reachable only once nullable statics were admitted, so
    // nothing depends on the unconditional retain the container arm keeps. Giving it one
    // leaked a cell per read — `C::$list = [$i]; C::$list === null;` in a loop grew from 4
    // pages to 295 over 200k iterations, because `=== null` consumes the cell and releases
    // nothing. A read that ESCAPES by return is paid for at the return instead.
    let boxed_takes_reference = inst
        .result
        .is_some_and(|result| owned_heap_load(ctx, result));
    match php_type.clone() {
        PhpType::Int | PhpType::Bool | PhpType::False => {
            ctx.fb.ins(
                &format!("(i64.load (local.get {address}))"),
                "static property value",
            );
        }
        PhpType::Float => {
            ctx.fb.ins(
                &format!("(f64.load (local.get {address}))"),
                "static property value (float)",
            );
        }
        PhpType::Str => {
            ctx.fb.ins(
                &format!("(i32.wrap_i64 (i64.load (local.get {address})))"),
                "static property string pointer",
            );
            ctx.fb.ins(
                &format!("(i64.load offset=8 (local.get {address}))"),
                "static property string length",
            );
        }
        // The `int|null` pair, flat: payload in the first word, Mixed tag in the second. It
        // holds no heap pointer, so there is nothing to retain and nothing to release — which
        // is what makes a nullable INT slot cheaper than a boxed one.
        PhpType::TaggedScalar => {
            ctx.fb.ins(
                &format!("(i64.load (local.get {address}))"),
                "the tagged payload",
            );
            ctx.fb.ins(
                &format!("(i32.load offset=8 (local.get {address}))"),
                "the tag (0 int, 2 float, 3 bool, 8 null)",
            );
        }
        // A container slot holds one pointer. The EIR types the read `own=maybe_owned` and emits
        // a matching release, so the reader takes a reference of its own; without it that release
        // would come out of the SLOT's, and the next read would find freed storage.
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            let pointer = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(
                &format!("(local.set {pointer} (i32.wrap_i64 (i64.load (local.get {address}))))"),
                "static property container pointer",
            );
            if takes_reference {
                ctx.fb.ins(
                    &format!("(call $__rt_incref (local.get {pointer}))"),
                    "the reader owns what it reads (null-safe)",
                );
            }
            ctx.fb
                .ins(&format!("local.get {pointer}"), "the container pointer");
        }
        // A boxed slot reads like a container: one pointer, and the reader takes a reference
        // because the EIR types the read owned and emits a matching release.
        //
        // Only when the EIR will give it back, though. `return $n;` on a static local reads the
        // cell, hands it to the declared-int return boundary and never releases it — an incref
        // there grew a 200k-iteration counter from 3 pages to 124.
        PhpType::Mixed | PhpType::Union(_) => {
            let cell = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(
                &format!("(local.set {cell} (i32.wrap_i64 (i64.load (local.get {address}))))"),
                "the slot's cell pointer",
            );
            if boxed_takes_reference {
                ctx.fb.ins(
                    &format!("(call $__rt_incref (local.get {cell}))"),
                    "the reader owns what it reads (null-safe)",
                );
            }
            ctx.fb.ins(&format!("local.get {cell}"), "the cell pointer");
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "load of a static property typed {other:?}"
            )))
        }
    }
    // Now the slot's shape becomes the result's. Both directions occur: a Mixed slot read as an
    // int unboxes, and a concrete slot read as Mixed boxes. The fresh cell a boxing conversion
    // allocates is handed straight to the result, which owns it, so there is nothing to free.
    let _ =
        super::transfer::emit_convert_stack_value(ctx, slot_ir, php_type, result_ir, result_php)?;
    store_result(ctx, inst)
}

/// Lowers `Op::StoreStaticProperty` (`Class::$prop = v`).
///
/// A string or container slot owns what it holds, so the outgoing value is released first — and
/// because the refcount helpers no-op below the heap, releasing an initial LITERAL default is a
/// no-op rather than a fault.
fn lower_store_static_property(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    let (address, php_type) = static_property_address(ctx, inst)?;
    emit_static_slot_store(ctx, value, &address, php_type, SlotOwnership::Steal)
}

/// Writes one 16-byte static slot at its declared width, whatever named it.
///
/// Shared with the static-local path for the same reason the load is: the slot's shape decides
/// the code, and its key decides nothing.
fn emit_static_slot_store(
    ctx: &mut FnCtx,
    value: crate::ir::ValueId,
    address: &str,
    php_type: PhpType,
    ownership: SlotOwnership,
) -> Result<()> {
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
            ctx.fb.ins(
                &format!("(i64.store (local.get {address}) (local.get {word}))"),
                "store the static property",
            );
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
            ctx.fb.ins(
                &format!("(f64.store (local.get {address}) (local.get {word}))"),
                "store the static property (float)",
            );
        }
        PhpType::TaggedScalar => {
            let tag = ctx.fresh_temp(ValType::I32);
            let payload = ctx.fresh_temp(ValType::I64);
            // `C::$maybe = 5;` hands over a bare long and `C::$maybe = null;` a Void, because
            // the value's type is what the expression produced, not what the slot holds. The
            // transfer table already knows how to make a tagged pair out of either.
            let unboxed = matches!(
                ctx.function.value(value).map(|v| v.ir_type),
                Some(IrType::Heap(IrHeapKind::Mixed))
            );
            emit_static_slot_source(ctx, value, IrType::TaggedScalar, PhpType::TaggedScalar)?;
            // The pair carries the cell's two facts and not the cell, so whoever unboxed it
            // gives it back. The EIR does not: it reads a nullable slot as one that TAKES its
            // value, which is true of the boxed shape and not of this one.
            release_consumed_static_source(ctx, value, unboxed)?;
            ctx.fb.ins(&format!("local.set {}", tag), "incoming tag");
            ctx.fb.ins(&format!("local.set {}", payload), "incoming payload");
            ctx.fb.ins(
                &format!("(i64.store (local.get {address}) (local.get {payload}))"),
                "store the tagged payload",
            );
            ctx.fb.ins(
                &format!("(i32.store offset=8 (local.get {address}) (local.get {tag}))"),
                "store the tag beside it",
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
            emit_slot_retain(ctx, ownership, narrows, &pointer, "string")?;
            // The slot TAKES the incoming reference rather than persisting a second copy:
            // the EIR hands one over (its `acquire` already made the owned copy) and emits no
            // release, so persisting again would leak exactly that copy. A bare literal has
            // no reference to take, and storing its static-data address is equally safe —
            // the refcount helpers no-op below the heap.
            ctx.fb.ins(
                &format!("(call $__rt_decref_any (i32.wrap_i64 (i64.load (local.get {address}))))"),
                "release the string the slot held (null-safe)",
            );
            ctx.fb.ins(
                &format!("(i64.store (local.get {address}) (i64.extend_i32_u (local.get {pointer})))"),
                "store the string pointer",
            );
            ctx.fb.ins(
                &format!("(i64.store offset=8 (local.get {address}) (local.get {length}))"),
                "store the string length",
            );
        }
        // Same hand-over as a string: the EIR's `acquire` made the owned copy and emits no
        // release, so the slot takes that reference and gives up whatever it held.
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            let pointer = ctx.fresh_temp(ValType::I32);
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins(&format!("local.set {}", pointer), "incoming container pointer");
            emit_slot_retain(ctx, ownership, false, &pointer, "container")?;
            ctx.fb.ins(
                &format!("(call $__rt_decref_any (i32.wrap_i64 (i64.load (local.get {address}))))"),
                "release the container the slot held (null-safe)",
            );
            ctx.fb.ins(
                &format!("(i64.store (local.get {address}) (i64.extend_i32_u (local.get {pointer})))"),
                "store the container pointer",
            );
            ctx.fb.ins(
                &format!("(i64.store offset=8 (local.get {address}) (i64.const 0))"),
                "a container slot has no second word",
            );
        }
        // A Mixed slot holds one CELL pointer, so it is the container case with a different
        // name, and whatever it held goes back.
        //
        // This is the shape `static $n = 0; $n = $n + 1;` produces. The slot is Mixed not
        // because the program stores mixed things in it but because CHECKED arithmetic widens
        // — an overflow would promote the sum to a float — so the commonest static local
        // there is lands here rather than in the int arm.
        PhpType::Mixed | PhpType::Union(_) => {
            let cell = ctx.fresh_temp(ValType::I32);
            // The flag says whether this boxed a scalar into a FRESH cell. A fresh one has no
            // other owner and no EIR release, so the slot takes it outright whatever the
            // ownership rule says; only a cell the EIR is still holding needs a second
            // reference.
            //
            // A CONTAINER reaches a boxed slot too — `C::$list = [1, 2, 3];` on a
            // `?array` — and `emit_operand_as_mixed` has no arm for a raw pointer, so the
            // conversion goes through the transfer table, which does.
            let allocated = if matches!(ctx.value_repr(value)?, WasmRepr::Ptr(_))
                && !matches!(
                    ctx.function.value(value).map(|v| v.ir_type),
                    Some(IrType::Heap(IrHeapKind::Mixed))
                ) {
                emit_static_slot_source(
                    ctx,
                    value,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                )?
            } else {
                emit_operand_as_mixed(ctx, value)?
            };
            ctx.fb
                .ins(&format!("local.set {}", cell), "incoming boxed value");
            emit_slot_retain(ctx, ownership, allocated, &cell, "boxed value")?;
            ctx.fb.ins(
                &format!("(call $__rt_decref_any (i32.wrap_i64 (i64.load (local.get {address}))))"),
                "release the cell the slot held (null-safe)",
            );
            ctx.fb.ins(
                &format!("(i64.store (local.get {address}) (i64.extend_i32_u (local.get {cell})))"),
                "store the cell pointer",
            );
            ctx.fb.ins(
                &format!("(i64.store offset=8 (local.get {address}) (i64.const 0))"),
                "a boxed slot has no second word",
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

/// Lowers PHP's verification of a value reaching a DECLARED `int` return.
///
/// `__rt_mixed_return_int` is the whole rule and this target already had it — the operand used
/// to arrive as a `Cast` the capability audit recognized by its consumer. Upstream gave the
/// boundary its own opcode, which says the same thing without the pattern match: the message
/// prefix rides as data, and the function name the `TypeError` needs is the enclosing EIR
/// function's, which spells a method `C::m` exactly as php prints it.
///
/// An operand already narrowed to an integer word carries nothing to verify and is forwarded.
fn lower_return_boundary_int(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let source = operand(inst, 0)?;
    if ctx.function.value(source).map(|value| value.ir_type) == Some(IrType::I64) {
        ctx.emit_load_value(source)?;
        return store_result(ctx, inst);
    }
    ctx.emit_load_value(source)?;
    let (name_ptr, name_len) = ctx.default_str_literal(&ctx.function.name)?;
    ctx.fb.ins(
        &format!("i32.const {}", name_ptr),
        "the declaring function's name, for the TypeError text",
    );
    ctx.fb
        .ins(&format!("i32.const {}", name_len), "its byte length");
    ctx.fb.ins(
        "call $__rt_mixed_return_int",
        "PHP's verification at a declared int return",
    );
    store_result(ctx, inst)
}

/// Lowers `Op::PropInitialized`: the probe `isset($obj->typed)` and every `?->` hop lower to.
///
/// The WRITE is the capability gate's question, and it is what makes the answer a constant for a
/// concrete receiver: the instruction is admitted only for a property whose write this target can
/// PROVE — a default, a promoted constructor parameter, an enum materializer, every concrete
/// descendant, or a constructor store. A property without such a proof is one this target refuses
/// to read at all, so no admitted probe can be looking at an unwritten slot.
///
/// NULL is the receiver's question, and for a NULLABLE receiver it is answered at run time from
/// the cell's own tag. Both constructs that reach here with one need that:
///
/// * `$a?->next?->n` probes `$a->next`, typed `Node|null`, on a path the chain's own branch has
///   already cleared — so the tag is 6 and the test is redundant but free;
/// * `isset($maybeNull->n)` probes the same shape with NOTHING having tested it, and the EIR reads
///   the probe itself as the null test: its false arm is the `false` answer. A constant `true`
///   there answers `y` where php answers `n`.
///
/// One answer for both is the trade taken deliberately over a dominance analysis in the gate that
/// would let the first case keep the constant. The test costs one unbox on a path that is about to
/// unbox the same cell for the read, and it cannot be wrong for a shape nobody anticipated.
/// `emit_is_null_test` is ASKED rather than a tag comparison written here, because it already
/// knows a null POINTER reads as php null — which a fresh `tag == 8` would have missed.
fn lower_prop_initialized(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let receiver = operand(inst, 0)?;
    if matches!(ctx.value_php_type(receiver)?, PhpType::Union(_)) {
        emit_is_null_test(ctx, receiver)?;
        ctx.fb.ins("i64.eqz", "php reads a null receiver as not set");
        ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
        return store_result(ctx, inst);
    }
    // A slot some `unset` can reach is the third case, and the one that makes `unset($o->n)`
    // observable at all: the write is proved by the declaration, then taken away again at run
    // time, so the probe reads the marker the unset left. Asked through the same function the
    // unset and the read ask.
    if let Some(answer) = super::objects::emit_prop_initialized_from_marker(ctx, inst, receiver)? {
        let _ = answer;
        return store_result(ctx, inst);
    }
    ctx.fb.ins(
        "i64.const 1",
        "the gate admitted this probe only where the write is proved",
    );
    store_result(ctx, inst)
}

/// Lowers `Op::StaticPropInitialized`: the probe `isset(C::$p)` lowers to.
///
/// The constant `true`, for the reason `static_prop_initialized_shape_issue` states: a static slot
/// carries no uninitialized sentinel, so the gate admits the probe only where the DECLARATION
/// proves the write — a default, or an untyped declaration php gives an implicit `null`. The
/// null-VALUE question is not this instruction's: the EIR reads the slot on the true arm and hands
/// the value to `isset`, which is why `public static ?int $n = null;` still answers false.
fn lower_static_prop_initialized(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.fb.ins(
        "i64.const 1",
        "the gate admitted this probe only where the declaration proves the write",
    );
    store_result(ctx, inst)
}

/// Lowers `Op::GeneratorYield`: `yield $v`, which SUSPENDS the body.
///
/// This target cannot switch stacks, so suspending is returning — and what makes that
/// survivable is that every function here is already a `br_table` state machine over
/// `$__state`. The yield writes the block to resume at, spills every local into the
/// frame, records the yielded value where `current()` reads it, and returns
/// `(1, value)`. The next `__rt_gen_resume` reloads all of it and branches back.
///
/// The RESULT of a yield is what `send()` passes in, which is a later slice: it answers
/// the frame's `sent_value`, which the wrapper zeroed and nothing yet writes, so a plain
/// `yield` reads null — exactly what php gives when nobody sends.
fn lower_generator_yield(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let Some((frame, frame_local)) = ctx.generator.clone() else {
        return Err(WasmError::Unsupported(
            "generator_yield outside a generator body".to_string(),
        ));
    };
    // `yield $v` is one operand; `yield $k => $v` is two, KEY first. The key is the only
    // difference between them, and php's numbering is the reason it cannot just be ignored: a
    // plain yield answers `largest_used_integer_key + 1`, so an explicit integer key MOVES what
    // the next plain one answers.
    let (key, value) = match inst.operands.len() {
        2 => (Some(operand(inst, 0)?), operand(inst, 1)?),
        _ => (None, operand(inst, 0)?),
    };
    // The value crosses a suspension, so it has to be a heap cell rather than a local's
    // words: box it first, and the frame owns that cell until the next yield replaces it.
    ctx.emit_load_value(value)?;
    let (value_ir, value_php) = ctx
        .function
        .value(value)
        .map(|value| (value.ir_type, value.php_type.codegen_repr()))
        .ok_or_else(|| WasmError::Unsupported("yielded value is missing".to_string()))?;
    let _ = super::transfer::emit_convert_stack_value(
        ctx,
        value_ir,
        value_php,
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
    )?;
    let boxed = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(&format!("local.set {boxed}"), "the yielded value, boxed");
    // A value that is ALREADY a Mixed cell is forwarded, not copied, so the frame would install a
    // cell somebody else owns as its `last_value` — and the next yield releases that. A borrowed
    // parameter or slot load must therefore be retained first; only a temporary this yield alone
    // consumes hands its reference over.
    if value_ir == IrType::Heap(IrHeapKind::Mixed) && !value_is_an_unshared_temporary(ctx, value) {
        ctx.fb.ins(
            &format!("(call $__rt_incref (local.get {boxed}))"),
            "the frame takes its own reference on a forwarded cell",
        );
    }
    // A yield CONSUMES its operand — on the native backend the suspend helper takes the value
    // and owns it, which is why the EIR emits no release for `yield $a . $b`. Boxing here does
    // not take anything over: `__rt_mixed_from_value` persists a COPY of a string and increfs a
    // container, so the operand's own block is left with no owner. Measured as one 24-byte cell
    // plus its bytes per yield: `yield $p . "x"` grew 3 → 26 pages over 20000 iterations while
    // `$q = $p . "x"; yield $q;` — where the slot owns the string and the epilogue frees it —
    // stayed flat.
    //
    // Only what THIS yield allocated and nothing else can reach. Two facts decide it, and the
    // EIR carries both.
    //
    // The first is where the bytes came from: an instruction that ALLOCATED them — a concat, a
    // call, the copy an `acquire` of a string makes — hands over the only pointer there is, while
    // a `load_local` hands over one the SLOT still owns, and a `const_str` one that lives in
    // static data and was never allocated at all. Deciding this on ownership got it wrong in both
    // directions, because `str_concat` and `load_local` are both `maybe_owned`: `$a = "aa" . "bb";
    // yield $a;` freed the slot's storage, the release walk freed it again, and the allocator's
    // free list came apart — which surfaces as a WRONG ANSWER in whatever allocates next, three
    // generators later, rather than as a fault here.
    //
    // The second is whether anything else uses the value, which is what makes the rule safe to
    // get wrong. The yield is one use — its own — so exactly one means nothing else can observe
    // the block, and any other (the `release` the EIR emits, a `store_local` that parks it in a
    // slot, a second read) leaves it alone. That direction leaks at worst; the other corrupts.
    if value_is_an_unshared_temporary(ctx, value) {
        match ctx.value_repr(value)?.clone() {
            WasmRepr::Str { ptr, .. } => {
                ctx.fb.ins(
                    &format!("(call $__rt_heap_free_safe (local.get {ptr}))"),
                    "the string the box copied, which nothing else owns",
                );
            }
            WasmRepr::Ptr(pointer) if value_ir == IrType::Heap(IrHeapKind::Mixed) => {
                // A cell the box FORWARDED rather than copied is the same cell now in the
                // frame, so there is nothing to give back.
                let _ = pointer;
            }
            WasmRepr::Ptr(pointer) => {
                ctx.fb.ins(
                    &format!("(call $__rt_decref_any (local.get {pointer}))"),
                    "the container the box increfed, whose own reference ends here",
                );
            }
            _ => {}
        }
    }
    ctx.fb.ins(
        &format!(
            "(call $__rt_decref_any (i32.wrap_i64 (i64.load offset={} (local.get {frame_local}))))",
            crate::codegen_support::runtime::generators::frame::OFF_LAST_VALUE
        ),
        "release whatever the previous yield left there (null-safe)",
    );
    ctx.fb.ins(
        &format!(
            "(i64.store offset={} (local.get {frame_local}) (i64.extend_i32_u (local.get {boxed})))",
            crate::codegen_support::runtime::generators::frame::OFF_LAST_VALUE
        ),
        "current() reads this",
    );
    // The KEY, after the value so a failure to box it cannot leave a half-written pair.
    match key {
        Some(key) => {
            ctx.emit_load_value(key)?;
            let (key_ir, key_php) = ctx
                .function
                .value(key)
                .map(|value| (value.ir_type, value.php_type.codegen_repr()))
                .ok_or_else(|| WasmError::Unsupported("yielded key is missing".to_string()))?;
            let _ = super::transfer::emit_convert_stack_value(
                ctx,
                key_ir,
                key_php,
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
            )?;
            // Park the boxed key: the helper takes `(frame, key)`, and the conversion left the
            // key on the stack, so pushing the frame after it would hand the two over swapped —
            // which read the frame as the key and stored nothing.
            let boxed_key = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {boxed_key}"), "the yielded key, boxed");
            if key_ir == IrType::Heap(IrHeapKind::Mixed) && !value_is_an_unshared_temporary(ctx, key) {
                ctx.fb.ins(
                    &format!("(call $__rt_incref (local.get {boxed_key}))"),
                    "the frame takes its own reference on a forwarded key cell",
                );
            }
            // `Immediate::Bool(true)` marks a DELEGATED yield — the ones `yield from <array>`
            // desugars into — whose key belongs to the source being delegated to, not to this
            // generator's numbering. php forwards it verbatim and leaves the counter where it
            // was, so `yield from [10, 20]; yield 30;` answers keys 0, 1 and then 0 again.
            let delegated = matches!(inst.immediate, Some(Immediate::Bool(true)));
            let helper = if delegated {
                "$__rt_gen_forward_key"
            } else {
                "$__rt_gen_note_key"
            };
            ctx.fb.ins(
                &format!("(call {helper} (local.get {frame_local}) (local.get {boxed_key}))"),
                if delegated {
                    "a delegated key is forwarded, and the counter is not this generator's to move"
                } else {
                    "store the key and move the counter if it is an integer"
                },
            );
        }
        None => {
            ctx.fb.ins(
                &format!("(call $__rt_gen_auto_key (local.get {frame_local}))"),
                "php numbers a plain yield for you",
            );
        }
    }
    // What this yield EVALUATES TO is whatever `send()` parks in the frame — and that has not
    // happened yet. The read belongs to the RESUME, so `emit_dispatch_loop` emits it at the top
    // of the segment this yield resumes into; reading it here answered the PREVIOUS send, one
    // resume late, which printed `()` where php printed `(a)` and `(a)` on the line after.
    // Where to come back to. `plan_segments` ended a segment AT this yield, so the rest of
    // the block is the next one — which is the whole reason segments exist: an EIR block
    // index cannot name a point in the middle of a block, and every yield is in the middle
    // of one.
    let resume_at = ctx.segment + 1;
    ctx.fb.ins(
        &format!(
            "(i32.store offset={} (local.get {frame_local}) (i32.const {resume_at}))",
            crate::codegen_support::runtime::generators::frame::OFF_STATE_IDX
        ),
        "resume in the segment past this yield",
    );
    super::generators::emit_spill(ctx, &frame, &frame_local);
    // Unwind the concat buffer the way LEAVING a frame does, because suspending IS leaving one.
    // The resume captured this baseline on entry and `concat_reset` inside the body rewinds to
    // it, but a yield returns without ever reaching a statement boundary — so a `yield $a . $b`
    // left its operand in the buffer and the cursor never came back, growing it once per
    // suspension. The yielded value is already a PERSISTED copy inside the Mixed cell, so
    // nothing here depends on the buffer's contents surviving.
    ctx.fb.ins(
        &format!("local.get {}", ctx.concat_base_local),
        "this resume's concat baseline",
    );
    ctx.fb
        .ins("global.set $__concat_off", "give the buffer back before suspending");
    ctx.fb.ins("i32.const 1", "a value was yielded");
    ctx.fb.ins(&format!("local.get {boxed}"), "and this is it");
    ctx.fb.ins("return", "suspend: leave the resume body");
    Ok(())
}

/// Lowers `Op::GeneratorYieldFrom`: `yield from <generator>`, a suspension that repeats.
///
/// Delegation is the one construct here whose resume point is ITSELF. While the inner generator
/// has values the outer re-enters this instruction, forwards the inner's key and value, and
/// suspends again at the same segment; when the inner runs out the outer does NOT suspend — it
/// falls through into the rest of its body inside that same resume, and the instruction's value is
/// the inner's return. `plan_segments` gives the delegation a segment of its own precisely so that
/// re-entry re-reads nothing but the frame: the `inner()` call that built the generator sits in
/// the segment before, and running it again would build a new one on every resume.
///
/// Falling through is why this cannot simply be a second kind of yield. A segment that does not
/// end its block runs into the next one, which is ordinary control flow the dispatch loop already
/// emits — nothing here has to name where to continue.
///
/// php's key rule is the other half: the inner's keys come through as the INNER's own numbering
/// (`yield from` a two-value generator forwards 0 and 1 even from an outer that has already
/// yielded), and the outer's `auto_key_counter` is untouched, so its own next plain `yield`
/// continues from where IT left off. Forwarding `last_key` verbatim is exactly that.
fn lower_generator_yield_from(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let Some((frame, frame_local)) = ctx.generator.clone() else {
        return Err(WasmError::Unsupported(
            "generator_yield_from outside a generator body".to_string(),
        ));
    };
    let delegated = crate::codegen_support::runtime::generators::frame::OFF_DELEGATED_ITER;
    let inner = operand(inst, 0)?;
    // The operand is a value the spill carries, so on re-entry this reads back the SAME inner
    // generator. Installing it is therefore a one-time act, and the frame's own word is the record
    // of whether it has happened — a second incref would be a reference nothing gives back.
    ctx.emit_load_value(inner)?;
    let inner_ptr = ctx.fresh_temp(ValType::I32);
    ctx.fb
        .ins(&format!("local.set {inner_ptr}"), "the generator being delegated to");
    ctx.fb.ins(
        &format!("(i32.eqz (i32.wrap_i64 (i64.load offset={delegated} (local.get {frame_local}))))"),
        "is this the first entry into the delegation?",
    );
    ctx.fb.ins("if", "install the inner generator once");
    ctx.fb.ins(
        &format!("(call $__rt_incref (local.get {inner_ptr}))"),
        "the frame takes a reference of its own",
    );
    ctx.fb.ins(
        &format!(
            "(i64.store offset={delegated} (local.get {frame_local})              (i64.extend_i32_u (local.get {inner_ptr})))"
        ),
        "delegated_iter is what every later resume advances",
    );
    ctx.fb.ins("end", "end first-entry install");
    ctx.fb.ins(
        &format!("(call $__rt_gen_delegate_step (local.get {frame_local}))"),
        "advance the inner one step, forwarding its key and value",
    );
    let payload = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(
        &format!("local.set {payload}"),
        "the value yielded, or the inner's return once it is done",
    );
    ctx.fb.ins("if", "the inner yielded: suspend the outer too");
    let resume_at = ctx.segment;
    ctx.fb.ins(
        &format!(
            "(i32.store offset={} (local.get {frame_local}) (i32.const {resume_at}))",
            crate::codegen_support::runtime::generators::frame::OFF_STATE_IDX
        ),
        "come back to this same delegation",
    );
    super::generators::emit_spill(ctx, &frame, &frame_local);
    ctx.fb.ins(
        &format!("local.get {}", ctx.concat_base_local),
        "this resume's concat baseline",
    );
    ctx.fb
        .ins("global.set $__concat_off", "give the buffer back before suspending");
    ctx.fb.ins("i32.const 1", "a value was yielded");
    ctx.fb.ins(&format!("local.get {payload}"), "the inner's value, forwarded");
    ctx.fb.ins("return", "suspend: leave the resume body");
    ctx.fb.ins("end", "end delegation suspend");
    // Exhausted, and still inside the same resume: the expression's value is the inner's return.
    ctx.fb.ins(
        &format!("local.get {payload}"),
        "`yield from` evaluates to the inner's return value",
    );
    store_result(ctx, inst)
}

/// Whether a value's storage has no owner other than the use being lowered.
///
/// The question every site that BOXES a value into a Mixed cell has to answer, because
/// `__rt_mixed_from_value` persists a COPY of a string rather than taking the block over: the
/// original is then unreferenced, and whether that is a leak or a double free depends on whether
/// anything else owns it.
///
/// Two facts decide it, and the EIR carries both. The first is where the bytes came from: an
/// instruction that ALLOCATED them — a concat, a call, the copy an `acquire` of a string makes —
/// hands over the only pointer there is, while a `load_local` hands over one the SLOT still owns
/// and a `const_str` one that lives in static data and was never allocated at all. The second is
/// whether anything else uses the value: the site being lowered is one use, so exactly one means
/// nothing else can observe the block, and any other (the `release` the EIR emits, a
/// `store_local` that parks it in a slot, a second read) leaves it alone.
///
/// That direction leaks at worst; the other corrupts. Deciding it on OWNERSHIP instead got it
/// wrong in both directions, because `str_concat` and `load_local` are both `maybe_owned`.
pub(super) fn value_is_an_unshared_temporary(ctx: &FnCtx, value: ValueId) -> bool {
    let allocated_here = ctx
        .function
        .value(value)
        .and_then(|value| match value.def {
            ValueDef::Instruction { inst, .. } => ctx.function.instruction(inst),
            _ => None,
        })
        .is_some_and(|def| {
            def.effects.intersects(Effects::ALLOC_HEAP | Effects::ALLOC_CONCAT)
                // `acquire` of a string is `__rt_str_persist`, which always copies into a fresh
                // block — an allocation its effects do not name.
                || def.op == Op::Acquire
        });
    // Counted per OPERAND occurrence: an instruction taking the value twice is two uses, and
    // calling that "only use" would box the same block twice and free it twice.
    let only_use = ctx
        .usage()
        .user_results
        .get(&value)
        .is_some_and(|users| users.len() == 1);
    allocated_here && only_use
}

/// Lowers `Op::ZeroLocalSlot`: writes the zero of each of a slot's locals over it.
///
/// The instruction marks a binding ABANDONED — `$q = "a" . $n; $q = 1;` — where the slot keeps
/// its old storage type and the epilogue keeps walking it. Leaving the old pointer there would
/// have the epilogue free a block another local still owns, so the slot is cleared instead. A
/// WASM slot is a fixed set of typed locals rather than a stack offset, so "zero" is the zero
/// of each local's own type; the release of the previous occupant is a separate instruction the
/// EIR already emitted ahead of this one.
fn lower_zero_local_slot(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
        return Err(WasmError::Unsupported(
            "zero_local_slot without a LocalSlot immediate".to_string(),
        ));
    };
    let repr = ctx.slot_repr(slot)?.clone();
    for (local, val_type) in repr.local_refs().into_iter().zip(repr.component_val_types()) {
        let zero = match val_type {
            ValType::I32 => "i32.const 0",
            ValType::I64 => "i64.const 0",
            ValType::F64 => "f64.const 0",
        };
        ctx.fb.ins(zero, "the abandoned binding's slot reads as zero");
        ctx.fb.ins(
            &format!("local.set {}", local),
            "clear it so the epilogue walks past rather than freeing",
        );
    }
    Ok(())
}

/// Lowers `<`, `<=`, `>` and `>=` where at least one operand is a boxed cell.
///
/// The ordering itself is php-src's `zend_compare`, which this target already carries as
/// `__rt_mixed_cmp_mixed`/`__rt_mixed_cmp_i64`. What relational comparison adds on top is the
/// unordered rule, and it is the reason this cannot simply test the three-way answer against
/// zero: php answers 1 for `NAN <=> $x` in BOTH directions, but every relational operator on
/// that pair is false. `__rt_mixed_rel_*` applies the predicate behind that guard.
///
/// A genuine int on the LEFT is handed to the cell-first helper with a mirrored predicate,
/// since `cmp(int, cell)` is `-cmp(cell, int)` and mirroring costs nothing.
fn lower_php_rel_cmp(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let left = operand(inst, 0)?;
    let right = operand(inst, 1)?;
    let predicate = super::capability::php_rel_cmp_predicate(inst).ok_or_else(|| {
        WasmError::Unsupported(format!("php_rel_cmp predicate {:?}", inst.immediate))
    })?;
    // A plain long is the one operand shape the helpers take unboxed. Everything else reaches
    // them as a Mixed cell: an already-boxed value is loaded as-is, and a `TaggedScalar` — the
    // inline `{payload, tag}` pair `$ints[$i]` reads as when the index may be missing — is boxed
    // here and released after, exactly as `MixedNumericBinop` boxes the same shape.
    let plain_int = |ctx: &FnCtx, value: crate::ir::ValueId| -> bool {
        ctx.function
            .value(value)
            .is_some_and(|v| v.ir_type == IrType::I64 && v.php_type.codegen_repr() == PhpType::Int)
    };
    if !plain_int(ctx, left) && !plain_int(ctx, right) {
        let lhs = ctx.fresh_temp(ValType::I32);
        let rhs = ctx.fresh_temp(ValType::I32);
        let lhs_owned = emit_operand_as_mixed(ctx, left)?;
        ctx.fb.ins(&format!("local.set {}", lhs), "left operand cell");
        let rhs_owned = emit_operand_as_mixed(ctx, right)?;
        ctx.fb.ins(&format!("local.set {}", rhs), "right operand cell");
        ctx.fb.ins(&format!("local.get {}", lhs), "left operand cell");
        ctx.fb.ins(&format!("local.get {}", rhs), "right operand cell");
        ctx.fb.ins(
            &format!("i32.const {predicate}"),
            "0 = <, 1 = <=, 2 = >, 3 = >=",
        );
        ctx.fb.ins(
            "call $__rt_mixed_rel_mixed",
            "php's ordering of two boxed values, with the NaN rule relational comparison needs",
        );
        store_result(ctx, inst)?;
        // Release only what this lowering allocated; an operand that arrived boxed stays owned
        // by whoever produced it.
        if lhs_owned {
            ctx.fb.ins(&format!("local.get {}", lhs), "temporary left cell");
            ctx.fb
                .ins("call $__rt_decref_mixed", "release the boxed left operand");
        }
        if rhs_owned {
            ctx.fb.ins(&format!("local.get {}", rhs), "temporary right cell");
            ctx.fb
                .ins("call $__rt_decref_mixed", "release the boxed right operand");
        }
        return Ok(());
    }
    let (cell_operand, other, predicate) = if plain_int(ctx, right) {
        (left, right, predicate)
    } else {
        // The cell takes the helper's first position; the predicate mirrors to compensate.
        (right, left, super::capability::mirrored_rel_predicate(predicate))
    };
    let cell = ctx.fresh_temp(ValType::I32);
    let cell_owned = emit_operand_as_mixed(ctx, cell_operand)?;
    ctx.fb.ins(&format!("local.set {}", cell), "boxed operand cell");
    ctx.fb.ins(&format!("local.get {}", cell), "boxed operand cell");
    ctx.emit_load_value(other)?;
    ctx.fb.ins(
        &format!("i32.const {predicate}"),
        "0 = <, 1 = <=, 2 = >, 3 = >=",
    );
    ctx.fb.ins(
        "call $__rt_mixed_rel_i64",
        "php's ordering of a boxed value against an integer",
    );
    store_result(ctx, inst)?;
    if cell_owned {
        ctx.fb
            .ins(&format!("local.get {}", cell), "temporary operand cell");
        ctx.fb
            .ins("call $__rt_decref_mixed", "release the boxed operand");
    }
    Ok(())
}

/// Lowers `==`: a boxed operand compares through the php-src `zend_compare` rules, a pair of
/// scalars through numeric or string comparison by the operands' types.
fn lower_loose_eq(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let left = operand(inst, 0)?;
    let right = operand(inst, 1)?;
    // A BOXED operand compares through php-src's `zend_compare`, never through a conversion:
    // `10 == "abc"` is FALSE because PHP renders the long and compares the bytes. Gated on the
    // IR type rather than the representation, since an array and an object are pointers too.
    let cell = |value: crate::ir::ValueId| -> bool {
        ctx.function.value(value).is_some_and(|v| {
            v.ir_type == IrType::Heap(IrHeapKind::Mixed) && v.php_type.codegen_repr() == PhpType::Mixed
        })
    };
    // Only a genuine INT: against a PHP bool, PHP converts both sides to bool first, which is a
    // different rule and stays refused.
    let plain_int = |value: crate::ir::ValueId| -> bool {
        ctx.function
            .value(value)
            .is_some_and(|v| v.ir_type == IrType::I64 && v.php_type.codegen_repr() == PhpType::Int)
    };
    let boxed_comparison = if cell(left) && cell(right) {
        ctx.emit_load_value(left)?;
        ctx.emit_load_value(right)?;
        Some("call $__rt_mixed_cmp_mixed")
    } else if cell(left) && plain_int(right) {
        ctx.emit_load_value(left)?;
        ctx.emit_load_value(right)?;
        Some("call $__rt_mixed_cmp_i64")
    } else if plain_int(left) && cell(right) {
        // Equality is symmetric, so the box may take the helper's first position.
        ctx.emit_load_value(right)?;
        ctx.emit_load_value(left)?;
        Some("call $__rt_mixed_cmp_i64")
    } else {
        None
    };
    if let Some(call) = boxed_comparison {
        ctx.fb.ins(call, "php-src's comparison of a boxed value");
        ctx.fb.ins("i64.eqz", "equal is a three-way answer of zero");
        ctx.fb.ins("i64.extend_i32_u", "PHP booleans are i64 here");
        if inst.op == Op::LooseNotEq {
            ctx.fb.ins("i64.eqz", "!= is the negation of ==");
            ctx.fb.ins("i64.extend_i32_u", "PHP booleans are i64 here");
        }
        return store_result(ctx, inst);
    }
    let lhs = ctx.value_repr(left)?.clone();
    let rhs = ctx.value_repr(right)?.clone();
    // The int|null TAGGED pair against an int: payload equality when the tag is int,
    // and null equals only 0 — php's bool rule folded to one comparison each way.
    let tagged_int = match (&lhs, &rhs) {
        (WasmRepr::Tagged { payload, tag }, WasmRepr::I64(_)) => {
            Some((payload.clone(), tag.clone(), right))
        }
        (WasmRepr::I64(_), WasmRepr::Tagged { payload, tag }) => {
            Some((payload.clone(), tag.clone(), left))
        }
        _ => None,
    };
    if let Some((payload, tag, int_side)) = tagged_int {
        ctx.fb.ins(&format!("local.get {tag}"), "int|null tag");
        ctx.fb.ins("i32.const 8", "the null tag");
        ctx.fb.ins("i32.eq", "is the pair null?");
        ctx.fb.ins("if (result i64)", "null: equal only to 0");
        ctx.emit_load_value(int_side)?;
        ctx.fb.ins("i64.eqz", "null == int is the bool rule");
        ctx.fb.ins("i64.extend_i32_u", "bool to i64");
        ctx.fb.ins("else", "int payload: compare the longs");
        ctx.fb.ins(&format!("local.get {payload}"), "tagged payload");
        ctx.emit_load_value(int_side)?;
        ctx.fb.ins("i64.eq", "long equality");
        ctx.fb.ins("i64.extend_i32_u", "bool to i64");
        ctx.fb.ins("end", "either way an i64 bool");
        if inst.op == Op::LooseNotEq {
            ctx.fb.ins("i64.eqz", "!= is the negation of ==");
            ctx.fb.ins("i64.extend_i32_u", "PHP booleans are i64 here");
        }
        return store_result(ctx, inst);
    }
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
                write_back_container_slot(ctx, array)?;
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
    write_back_container_slot(ctx, array)?;
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
    let cell = box_value_into_mixed_cell(ctx, value, value_repr)?;
    ctx.emit_load_value(array)?;
    ctx.fb
        .ins(&format!("local.get {}", cell), "mixed cell pointer");
    ctx.fb.ins(
        "call $__rt_array_push_mixed",
        "append into a value_type-7 array (may reallocate)",
    );
    // Deliberately NOT stamp_bool_array_result_type: pushing a bool would restamp the array's
    // value_type to 3 (scalar), and `__rt_array_free_deep` would then skip the child loop and
    // leak every cell. The bool lives INSIDE its cell here.
    ctx.emit_store_value(array)?;
    write_back_container_slot(ctx, array)?;
    let _ = inst;
    Ok(())
}

/// Boxes one raw value into a fresh Mixed cell and answers the local holding it.
///
/// Shared by the `array_push` and `array_set` paths into a `value_type`-7 array, which need the
/// identical composition and differ only in the runtime call that follows. The cell carries ONE
/// reference, handed to the array without increfing — for a container operand the boxing increfed
/// the child, so the operand's own reference is dropped here to leave the cell as sole owner.
pub(super) fn box_value_into_mixed_cell(
    ctx: &mut FnCtx,
    value: ValueId,
    value_repr: &WasmRepr,
) -> Result<String> {
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
        // A tagged pair already carries the cell's own tag in its tag word, so boxing it moves
        // two words rather than deciding anything — the same conversion `MixedBox` performs.
        // `int|null` is the only shape codegen builds this repr from, so the tag is 0 or 8 and
        // the high word is unused.
        WasmRepr::Tagged { payload, tag } => {
            let (payload, tag) = (payload.clone(), tag.clone());
            ctx.fb
                .ins(&format!("local.get {}", tag), "tagged scalar runtime tag");
            ctx.fb.ins("i64.extend_i32_u", "tag -> i64");
            ctx.fb
                .ins(&format!("local.get {}", payload), "tagged scalar payload");
            ctx.fb.ins("i64.const 0", "no high payload");
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
    // A STRING was copied into the cell, not taken over — so a block nothing else owns is left
    // behind. `[1, "a" . $i]` leaked one per element built that way, with no property or call in
    // sight; the same rule a yield applies decides it, because it is the same box.
    if let WasmRepr::Str { ptr, .. } = value_repr {
        if value_is_an_unshared_temporary(ctx, value) {
            let boxed = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {}", boxed), "the cell holding the copy");
            ctx.fb.ins(
                &format!("(call $__rt_heap_free_safe (local.get {}))", ptr),
                "the string the cell copied, which nothing else owns",
            );
            ctx.fb.ins(&format!("local.get {}", boxed), "the cell");
        }
    }
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
    Ok(cell)
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

    // An `array<mixed>` stores 16-byte slots holding a boxed cell, so a RAW scalar reaching one
    // is boxed at the write site and its single reference handed over — the same contract
    // `array_push` uses for the same value shape.
    //
    // An ALREADY-boxed cell is NOT routed here — but NOT for the reason previously recorded.
    //
    // The old note blamed `__rt_array_set_mixed` ("corrupts a slot when an earlier write went
    // through this same setter — bisected and not yet explained"). RE-MEASURED: the setter is
    // SOUND. Admitting this path and increfing (the contract `array_push` uses for the same
    // shape) answers php-src exactly on a plain overwrite, a double overwrite of one slot, a
    // COW split, both references to a shared array, and nested arrays.
    //
    // The real blocker is an ownership ASYMMETRY one level down. `lower_load_local` emits a
    // plain transfer: loading an owned Mixed local BORROWS, taking no reference. The EIR
    // nevertheless emits `release` on that handle when the value is consumed directly rather
    // than stored, e.g. `echo "" . ($a[$k] ??= $v)`:
    //
    //     v64 = load_local slot[6]   ;; owned Mixed, no incref emitted
    //     v65 = cast v64 Str
    //     release v64                ;; no matching acquire — eats the ARRAY's share
    //
    // Assigning the same result to a local instead emits `acquire` before the release, which
    // balances, which is why only the concatenated form is wrong. So admitting the write would
    // hand the array a share that the consumer then frees, and `$a[0]` reads back as null —
    // a WRONG ANSWER where the refusal is merely missing coverage. Minimal repro and the R/S
    // pair are in the campaign notes; the fix belongs in load_local/release, not here.
    let value_is_boxed_cell = matches!(
        ctx.function.value(value).map(|v| v.ir_type),
        Some(IrType::Heap(IrHeapKind::Mixed))
    );
    if array_stores_mixed_cells(ctx, array) && !value_is_boxed_cell {
        // A RAW scalar is boxed at the write site and its single reference handed over — the
        // same contract `array_push` uses for the same value shape.
        let cell = box_value_into_mixed_cell(ctx, value, &value_repr)?;
        ctx.emit_load_value(array)?;
        ctx.emit_load_value(index)?;
        ctx.fb
            .ins(&format!("local.get {}", cell), "mixed cell pointer");
        ctx.fb.ins(
            "call $__rt_array_set_mixed",
            "set boxed element (COW, releases the replaced cell)",
        );
        // Deliberately NOT stamp_bool_array_result_type: a bool lives INSIDE its cell here, and
        // restamping the array's value_type to 3 would make the deep free skip every child.
        ctx.emit_store_value(array)?;
        write_back_container_slot(ctx, array)?;
        return Ok(());
    }

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
        // A container element — a nested array or an object — is one pointer in an 8-byte slot,
        // the same shape `array_push` writes. The array takes a SHARE of the child (incref here)
        // and the EIR releases the operand right after the set, exactly as it does for a push.
        // An `array<mixed>` is NOT this shape: its slots hold Mixed cells, and storing a raw
        // pointer into a cell-strided array reads back as a denormal float, so it is left to the
        // arm below rather than routed here.
        WasmRepr::Ptr(_)
            if matches!(
                ctx.function.value(value).map(|v| v.ir_type),
                Some(IrType::Heap(IrHeapKind::Array | IrHeapKind::Object | IrHeapKind::Hash))
            ) && !array_stores_mixed_cells(ctx, array) =>
        {
            let container_tag = match ctx.function.value(value).map(|v| v.ir_type) {
                Some(IrType::Heap(IrHeapKind::Object)) => 4,
                Some(IrType::Heap(IrHeapKind::Array)) => 5,
                _ => 6,
            };
            let child = ctx.fresh_temp(ValType::I32);
            ctx.emit_load_value(value)?;
            ctx.fb.ins(&format!("local.set {}", child), "container to store");
            ctx.fb.ins(
                &format!("(call $__rt_incref (local.get {}))", child),
                "the array shares the child (the EIR releases the operand after the set)",
            );
            ctx.emit_load_value(array)?; // array pointer
            ctx.emit_load_value(index)?; // index (i64)
            ctx.fb
                .ins(&format!("local.get {}", child), "child pointer for the store");
            ctx.fb
                .ins(&format!("i64.const {container_tag}"), "value_type for the slot");
            ctx.fb.ins(
                "call $__rt_array_set_ptr",
                "set container element (COW, releases the replaced child)",
            );
        }
        // An ALREADY-boxed cell written into an `array<mixed>`. The array takes its own share
        // and the EIR releases the operand right after the set, the same contract the container
        // arm above uses — and the same one `array_push` uses for this value shape.
        WasmRepr::Ptr(_)
            if matches!(
                ctx.function.value(value).map(|v| v.ir_type),
                Some(IrType::Heap(IrHeapKind::Mixed))
            ) && array_stores_mixed_cells(ctx, array) =>
        {
            let cell = ctx.fresh_temp(ValType::I32);
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins(&format!("local.set {}", cell), "the cell being stored");
            ctx.fb.ins(
                &format!("(call $__rt_incref (local.get {}))", cell),
                "the array takes its own share of the cell",
            );
            ctx.emit_load_value(array)?;
            ctx.emit_load_value(index)?;
            ctx.fb
                .ins(&format!("local.get {}", cell), "mixed cell pointer");
            ctx.fb.ins(
                "call $__rt_array_set_mixed",
                "set boxed element (COW, releases the replaced cell)",
            );
            // As in the raw-scalar path: a bool lives INSIDE its cell here, so restamping the
            // array's value type would make the deep free skip every child.
            ctx.emit_store_value(array)?;
            write_back_container_slot(ctx, array)?;
            return Ok(());
        }
        other => return Err(WasmError::Unsupported(format!("array_set of {:?}", other))),
    }
    stamp_bool_array_result_type(ctx, array, value)?;
    // The runtime returned the (possibly cloned/reallocated) pointer: store it
    // back into the array operand value's local.
    ctx.emit_store_value(array)?;
    // And mirror it to the source slot so a later LoadLocal sees the live pointer.
    write_back_container_slot(ctx, array)?;
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

/// Boxes `value` into a cell the container write owns alone (`value_boxes_into_a_container_write`).
pub(super) fn emit_owned_cell_for_container_write(ctx: &mut FnCtx, value: ValueId) -> Result<String> {
    if ctx.function.value(value).map(|value| value.ir_type) == Some(IrType::Heap(IrHeapKind::Mixed)) {
        let cell = ctx.fresh_temp(ValType::I32);
        ctx.emit_load_value(value)?;
        ctx.fb.ins("call $__rt_mixed_unbox", "the boxed value's (tag, lo, hi)");
        ctx.fb.ins("call $__rt_mixed_from_value", "a copy the write owns");
        ctx.fb.ins(&format!("local.set {cell}"), "the element's own cell");
        return Ok(cell);
    }
    let repr = ctx.value_repr(value)?.clone();
    box_value_into_mixed_cell(ctx, value, &repr)
}

/// Lowers `Op::MixedArrayAppend`: `$a[] = $v` through a declared `array`.
///
/// The value is boxed into a cell of its own, which the write takes over; the container is
/// republished inside the receiver's cell, so there is no pointer to write back here.
fn lower_mixed_array_append(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let receiver = operand(inst, 0)?;
    let value = operand(inst, 1)?;
    let cell = emit_owned_cell_for_container_write(ctx, value)?;
    ctx.emit_load_value(receiver)?;
    ctx.fb.ins(&format!("local.get {cell}"), "the element, owned by the write");
    ctx.fb.ins("call $__rt_mixed_cell_append", "append inside the array's cell");
    emit_container_write_status(ctx, "mixed-cell-append");
    Ok(())
}

/// Lowers the untyped `$a[$k] = $v` through a declared `array` (`mixed_array_write_is_supported`).
fn lower_mixed_array_set(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let receiver = operand(inst, 0)?;
    let key = operand(inst, 1)?;
    let value = operand(inst, 2)?;
    let cell = emit_owned_cell_for_container_write(ctx, value)?;
    ctx.emit_load_value(receiver)?;
    match ctx.value_repr(key)?.clone() {
        WasmRepr::Str { .. } => {
            // `$a["1"]` is `$a[1]`: integer-like strings collapse the way the hash does it.
            ctx.emit_load_value(key)?;
            ctx.fb.ins("call $__rt_hash_normalize_key", "the key as (lo, hi)");
        }
        _ => {
            ctx.emit_load_value(key)?;
            ctx.fb.ins("i64.const -1", "an integer key");
        }
    }
    ctx.fb.ins(&format!("local.get {cell}"), "the value, owned by the write");
    ctx.fb.ins("call $__rt_mixed_cell_set", "write inside the array's cell");
    emit_container_write_status(ctx, "mixed-cell-set");
    Ok(())
}

/// Raises php's fatal for a container write the runtime answered with a failure status.
///
/// 0 is an append whose next key is taken; 2 is a cell that holds no array, which a declared
/// `array` cannot reach — it would be a corrupt cell, reported as a heap-type mismatch.
fn emit_container_write_status(ctx: &mut FnCtx, site: &str) {
    let status = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(&format!("local.set {status}"), "the write's status");
    ctx.fb.ins(&format!("(i32.eqz (local.get {status}))"), "next element occupied?");
    ctx.fb.ins("if", "php append failure (non-returning)");
    ctx.fb.ins("i32.const 7", "occupied next array element failure code");
    ctx.fb.ins("call $__rt_fail", "report Cannot add element to the array as occupied");
    ctx.fb.ins(
        "unreachable",
        &format!("elephc-trap:post-noreturn:{site}-occupied occupied next array element does not return"),
    );
    ctx.fb.ins("end", "end append failure guard");
    ctx.fb.ins(&format!("(i32.eq (local.get {status}) (i32.const 2))"), "no array in the cell?");
    ctx.fb.ins("if", "corrupt array cell (non-returning)");
    ctx.fb.ins("i32.const 9", "heap type mismatch failure code");
    ctx.fb.ins("call $__rt_fail", "report the heap type mismatch");
    ctx.fb.ins(
        "unreachable",
        &format!("elephc-trap:post-noreturn:{site}-not-an-array a cell without an array does not return"),
    );
    ctx.fb.ins("end", "end corrupt cell guard");
}

/// Lowers `Op::MixedClone`: a new cell holding the same value as a boxed source.
///
/// `__rt_mixed_unbox` yields the source's `(tag, lo, hi)` — following a nested cell, and
/// answering null for a null pointer — and `__rt_mixed_from_value` builds the copy from them:
/// it persists its own copy of a string and increfs an array, hash or object, so the clone
/// owns what it holds and the source keeps what it had. The native `__rt_mixed_clone` gives
/// the same answer: arrays stay copy-on-write shared, objects keep their identity.
fn lower_mixed_clone(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let source = operand(inst, 0)?;
    ctx.emit_load_value(source)?;
    ctx.fb.ins("call $__rt_mixed_unbox", "the source cell's (tag, lo, hi)");
    ctx.fb.ins(
        "call $__rt_mixed_from_value",
        "a fresh cell owning its own copy of that value",
    );
    store_result(ctx, inst)
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
            if mixed_box_takes_over_its_string(ctx, inst, value) {
                let cell = ctx.fresh_temp(ValType::I32);
                ctx.fb.ins(&format!("local.set {cell}"), "hold the fresh cell");
                ctx.fb.ins(
                    &format!("(call $__rt_heap_free_safe (local.get {ptr}))"),
                    "the copy this box replaced, which nothing else owns",
                );
                ctx.fb.ins(&format!("local.get {cell}"), "the cell is the result");
            }
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

/// Whether this `mixed_box` has TAKEN OVER its string operand, leaving the operand's own copy
/// with no owner.
///
/// On a refcounted target `mixed_box` of a string absorbs the reference its operand holds, which
/// is why the EIR emits no release for that operand. `__rt_mixed_from_value` cannot absorb
/// anything — strings are copy-on-acquire here and it persists a copy of its own — so the
/// operand's block is orphaned. Measured as `$o->label ?? "z"` in a loop: 40 bytes an iteration,
/// while the same read without the `??` stayed flat, because only the coalesce's boxed temp
/// reaches this path.
///
/// Three conditions, and each rules out a different way of being wrong:
///
/// * the operand must come from `Op::Acquire`, the one instruction that MAKES a copy here
///   (`__rt_str_persist`), so the free cannot touch a slot's or an object's own bytes — a
///   borrowed property read hands out the object's pointer unchanged;
/// * nothing may release it, or this would be the second free;
/// * this box must be its only other use, or this would be a use-after-free.
fn mixed_box_takes_over_its_string(
    ctx: &FnCtx,
    inst: &Instruction,
    value: crate::ir::ValueId,
) -> bool {
    let produced_by_a_copy = ctx.usage().acquired.contains(&value);
    if !produced_by_a_copy {
        return false;
    }
    // `inst` is the dispatcher's own clone, so this box is identified by its RESULT rather than
    // by address — `ptr::eq` against the function's instruction list is false for every
    // candidate, which silently made the whole predicate answer "something else uses it".
    !ctx.usage()
        .user_results
        .get(&value)
        .is_some_and(|users| users.iter().any(|result| *result != inst.result))
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
) -> Result<(ValueId, ValueId, PhpType, bool, bool, bool, bool)> {
    let iter = inst
        .result
        .ok_or_else(|| WasmError::Unsupported("iter_start without a result".to_string()))?;
    let source = operand(inst, 0)?;
    let src_php = ctx
        .function
        .value(source)
        .map(|v| v.php_type.codegen_repr());
    let (elem, is_hash, dynamic, from_pointer, is_generator) = match src_php {
        Some(PhpType::Array(inner)) => (inner.codegen_repr(), false, false, false, false),
        Some(PhpType::AssocArray { value, .. }) => (value.codegen_repr(), true, false, false, false),
        // A GENERATOR is not a container: there is nothing to index and nothing to snapshot.
        // Walking it means resuming its body, which answers whether a value came out, and the
        // value itself is whatever the yield recorded in the frame.
        Some(PhpType::Object(name)) if name.trim_start_matches('\\') == "Generator" => {
            (PhpType::Mixed, false, false, false, true)
        }
        // A BOXED source: only the cell's tag says whether this is an indexed array, a hash, or
        // something PHP will not iterate at all, so the storage question is answered at runtime.
        Some(PhpType::Mixed) => (PhpType::Mixed, false, true, false, false),
        // An `iterable` PARAMETER is the same question asked of a raw container pointer: the
        // heap header's kind byte separates an indexed array from a hash. The element type is
        // Mixed because the advance and read helpers box whatever the container's own
        // `value_type` holds, so a raw `array<int>` walks without being widened first.
        Some(PhpType::Iterable) => (PhpType::Mixed, false, true, true, false),
        Some(other) => {
            return Err(WasmError::Unsupported(format!("foreach over {:?}", other)))
        }
        None => return Err(WasmError::Unsupported("iter_start source has no type".to_string())),
    };
    Ok((iter, source, elem, is_hash, dynamic, from_pointer, is_generator))
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
        let (iter, _, elem, is_hash, dynamic, from_pointer, is_generator) =
            iter_start_metadata(ctx, &inst)?;
        ctx.iter_reserve(iter, elem, is_hash, dynamic, from_pointer, is_generator);
    }
    Ok(())
}

/// Lowers `Op::IterStart` by initializing the iterator locals reserved in the
/// function prepass.
fn lower_iter_start(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let (iter, source, _, _, _, _, is_generator) = iter_start_metadata(ctx, inst)?;
    if is_generator {
        // Starting a `foreach` over a generator is php's REWIND: it refuses a generator that has
        // finished (`Cannot traverse an already closed generator`) or has moved past its first
        // yield (`Cannot rewind a generator that was already run`), and otherwise runs an
        // untouched one to its first yield. A generator already SITTING at its first yield
        // (`current()` or `rewind()` got it there) is left alone, which is what lets the loop
        // show that value rather than skip it.
        let slots = ctx.iter_slots(iter)?;
        let source_local = slots.source.clone();
        let cursor_local = slots.cursor.clone();
        ctx.emit_load_value(source)?;
        ctx.fb
            .ins(&format!("local.set {source_local}"), "the generator's frame");
        ctx.fb.ins("i64.const 0", "no step taken yet");
        ctx.fb
            .ins(&format!("local.set {cursor_local}"), "the first `iter_next` only reports liveness");
        let flags = crate::codegen_support::runtime::generators::frame::OFF_FLAGS;
        let terminated = crate::codegen_support::runtime::generators::frame::FLAG_TERMINATED;
        let advanced = super::generators::FLAG_ADVANCED;
        ctx.fb.ins(
            &format!("(i32.and (i32.load offset={flags} (local.get {source_local})) (i32.const {terminated}))"),
            "has the generator already finished?",
        );
        ctx.fb.ins("if", "php refuses to traverse a closed generator");
        emit_runtime_failure(
            ctx,
            super::function::GENERATOR_CLOSED_FAILURE_CODE,
            "report php's Cannot traverse an already closed generator",
        );
        ctx.fb.ins("end", "end closed-generator check");
        ctx.fb.ins(
            &format!("(i32.and (i32.load offset={flags} (local.get {source_local})) (i32.const {advanced}))"),
            "has a consumer already advanced it past the first yield?",
        );
        ctx.fb.ins("if", "php refuses to rewind one that has run");
        emit_runtime_failure(
            ctx,
            super::function::REWIND_ADVANCED_FAILURE_CODE,
            "report php's Cannot rewind a generator that was already run",
        );
        ctx.fb.ins("end", "end already-advanced check");
        ctx.fb.ins(
            &format!("(call $__rt_gen_prime (local.get {source_local}))"),
            "an untouched generator runs to its first yield",
        );
        return Ok(());
    }
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
    if slots.is_generator {
        // The FIRST call is the rewind `lower_iter_start` already did, so it only reports whether
        // the generator is alive (a body that yields nothing ends the loop at once). Every later
        // call advances it: the resume answers `(has_value, value)`, and the value it yielded is
        // already in the frame where `iter_current_value` reads it. A terminated generator
        // answers 0, which ends the loop — the same boolean an array walk's `cursor < length`
        // produces. The cursor local, unused for a generator, is the first-call flag.
        let flags = crate::codegen_support::runtime::generators::frame::OFF_FLAGS;
        let terminated = crate::codegen_support::runtime::generators::frame::FLAG_TERMINATED;
        ctx.fb.ins(
            &format!(
                "(if (result i32) (i64.eqz (local.get {cur})) \
                 (then (local.set {cur} (i64.const 1)) \
                       (i32.eqz (i32.and (i32.load offset={flags} (local.get {src})) (i32.const {terminated})))) \
                 (else (call $__rt_gen_foreach_step (local.get {src}))))"
            ),
            "first step reports liveness; later steps resume the body",
        );
        ctx.fb
            .ins("i64.extend_i32_u", "the loop CondBr consumes an i64 boolean");
        return store_result(ctx, inst);
    }
    if let Some(kind) = slots.dynamic_kind.clone() {
        ctx.fb.ins(&format!("local.get {}", src), "container pointer");
        ctx.fb.ins(&format!("local.get {}", cur), "current cursor");
        ctx.fb
            .ins(&format!("local.get {}", kind), "runtime container kind");
        ctx.fb.ins(
            "call $__rt_mixed_iter_next",
            "advance whichever storage the tag chose",
        );
        let has_more = ctx.fresh_temp(ValType::I64);
        ctx.fb
            .ins(&format!("local.set {}", has_more), "captured has_more");
        ctx.fb
            .ins(&format!("local.set {}", cur), "store advanced cursor");
        ctx.fb
            .ins(&format!("local.get {}", has_more), "has_more for the loop CondBr");
        return store_result(ctx, inst);
    }
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
    if let Some(kind) = slots.dynamic_kind.clone() {
        let (src, cur) = (slots.source.clone(), slots.cursor.clone());
        ctx.fb.ins(&format!("local.get {}", src), "container pointer");
        ctx.fb.ins(&format!("local.get {}", cur), "current cursor");
        ctx.fb
            .ins(&format!("local.get {}", kind), "runtime container kind");
        ctx.fb.ins("call $__rt_mixed_iter_key", "boxed foreach key");
        return store_result(ctx, inst);
    }
    if slots.is_hash {
        let (src, cur) = (slots.source.clone(), slots.cursor.clone());
        return super::inst_hash::lower_hash_iter_key(ctx, inst, &src, &cur);
    }
    if slots.is_generator {
        // The key the yield recorded, owned by the frame until the next yield replaces it — so
        // the read takes a reference of its own, exactly as the value read does.
        let src = slots.source.clone();
        let cell = ctx.fresh_temp(ValType::I32);
        ctx.fb.ins(
            &format!(
                "(i32.wrap_i64 (i64.load offset={} (local.get {src})))",
                crate::codegen_support::runtime::generators::frame::OFF_LAST_KEY
            ),
            "the key this generator yielded",
        );
        ctx.fb.ins(&format!("local.set {cell}"), "");
        ctx.fb.ins(&format!("local.get {cell}"), "");
        ctx.fb
            .ins("call $__rt_incref", "the frame keeps its own reference");
        ctx.fb.ins(&format!("local.get {cell}"), "the foreach key");
        return store_result(ctx, inst);
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
    if slots.is_generator {
        // The value is the cell the yield put in the frame, and the frame keeps owning it
        // until the next yield replaces it — so the read takes a reference of its own, the
        // way every other borrowed-container read here does.
        let src = slots.source.clone();
        let cell = ctx.fresh_temp(ValType::I32);
        ctx.fb.ins(
            &format!(
                "(i32.wrap_i64 (i64.load offset={} (local.get {src})))",
                crate::codegen_support::runtime::generators::frame::OFF_LAST_VALUE
            ),
            "the cell this generator yielded",
        );
        ctx.fb.ins(&format!("local.set {cell}"), "");
        ctx.fb.ins(&format!("local.get {cell}"), "");
        ctx.fb
            .ins("call $__rt_incref", "the frame keeps its own reference");
        ctx.fb.ins(&format!("local.get {cell}"), "the foreach value");
        return store_result(ctx, inst);
    }
    if let Some(kind) = slots.dynamic_kind.clone() {
        let (src, cur) = (slots.source.clone(), slots.cursor.clone());
        ctx.fb.ins(&format!("local.get {}", src), "container pointer");
        ctx.fb.ins(&format!("local.get {}", cur), "current cursor");
        ctx.fb
            .ins(&format!("local.get {}", kind), "runtime container kind");
        ctx.fb
            .ins("call $__rt_mixed_iter_value", "boxed foreach value, owned");
        return store_result(ctx, inst);
    }
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
        // A raw ARRAY or OBJECT pointer is null exactly when it is 0, whatever the static type
        // says. The type is not always right — reading a missing element of `array<array<int>>`
        // is typed `array<int>` because the null gets dropped at that boundary — and the
        // fallback below answers from the type, so without this a pointer that IS 0 reports
        // itself non-null. Testing the pointer cannot be wrong: a live array or object is
        // allocated, so 0 is never one of them.
        (WasmRepr::Ptr(local), PhpType::Array(_) | PhpType::Object(_), IrType::Heap(kind))
            if matches!(kind, IrHeapKind::Array | IrHeapKind::Object) =>
        {
            ctx.fb
                .ins(&format!("local.get {}", local), "container pointer");
            ctx.fb.ins("i32.eqz", "a null pointer IS null");
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
        // The TAG is the truth for any two-word scalar, whatever the static type calls it. A
        // `?int` PARAMETER carries its nullability in the declaration rather than in a
        // `TaggedScalar` php type, so requiring that name here sent it to the `statically
        // non-null` fallback below — and `describe_maybe(null)` then took the non-null branch
        // over a value whose tag said 8. Same failure shape as the pointer arm above.
        (WasmRepr::Tagged { tag, .. }, _, IrType::TaggedScalar) => {
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
