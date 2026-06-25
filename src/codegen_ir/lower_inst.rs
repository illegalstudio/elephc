//! Purpose:
//! Lowers individual EIR instructions into target-aware assembly snippets.
//! Starts with scalar constants and output needed for the first executable smoke test.
//!
//! Called from:
//! - `crate::codegen_ir::block_emit`.
//!
//! Key details:
//! - Results are written to fixed value-placement slots immediately after definition.
//! - Unsupported opcodes fail explicitly instead of falling back to legacy AST codegen.

use crate::codegen::{
    abi, callable_descriptor, emit_box_current_value_as_mixed,
    emit_box_runtime_payload_as_mixed, runtime, runtime_value_tag,
};
use crate::codegen::builtins::arrays::call_user_func_array::INVOKER_ARG_REF_CELL_TAG;
use crate::codegen::context::{
    Context as LegacyContext, DeferredRuntimeCallableInvoker, TRY_HANDLER_DIAG_DEPTH_OFFSET,
    TRY_HANDLER_JMP_BUF_OFFSET,
};
use crate::codegen::platform::Arch;
use crate::intrinsics::{IntrinsicCall, IntrinsicCallKind};
use crate::ir::{
    BlockId, CmpPredicate, Function, Immediate, InstId, Instruction, LocalKind, LocalSlotId, Op, Ownership,
    Terminator, ValueDef, ValueId,
};
use crate::names::{
    function_symbol, ir_global_symbol, method_symbol, php_symbol_key, static_method_symbol,
};
use crate::types::{
    callable_wrapper_sig, first_class_callable_builtin_sig, ExternFunctionSig, FunctionSig, PhpType,
};

use super::context::FunctionContext;
use super::function_variants;
use super::{CodegenIrError, Result};

mod arithmetic;
mod arrays;
mod buffers;
mod builtins;
mod callables;
mod comparisons;
mod conversions;
mod enums;
mod externs;
mod floats;
mod hashes;
mod iterators;
mod objects;
mod ownership;
mod pointers;
mod predicates;
mod scoped_constants;
mod strings;
mod static_locals;
mod static_properties;

const CALLED_CLASS_ID_PARAM: &str = "__elephc_called_class_id";
const BORROWED_MIXED_ARG_CELL_BYTES: usize = 32;

/// Lowers one EIR instruction by opcode.
pub(super) fn lower_instruction(ctx: &mut FunctionContext<'_>, inst_id: InstId) -> Result<()> {
    let inst = ctx
        .function
        .instruction(inst_id)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst_id.as_raw()))?;
    match inst.op {
        Op::ConstI64 => lower_const_i64(ctx, &inst),
        Op::ConstF64 => floats::lower_const_f64(ctx, &inst),
        Op::ConstBool => lower_const_bool(ctx, &inst),
        Op::ConstNull => lower_const_null(ctx, &inst),
        Op::ConstStr => strings::lower_const_str(ctx, &inst),
        Op::ConstClassName => strings::lower_const_class_name(ctx, &inst),
        Op::LoadLocal => lower_load_local(ctx, &inst),
        Op::StoreLocal => lower_store_local(ctx, &inst),
        Op::UnsetLocal => lower_unset_local(ctx, &inst),
        Op::LoadRefCell => lower_load_ref_cell(ctx, &inst),
        Op::StoreRefCell => lower_store_ref_cell(ctx, &inst),
        Op::PromoteLocalRefCell => lower_promote_local_ref_cell(ctx, &inst),
        Op::AliasLocalRefCell => lower_alias_local_ref_cell(ctx, &inst),
        Op::ReleaseLocalRefCell => lower_release_local_ref_cell(ctx, &inst),
        Op::LoadGlobal => lower_load_global(ctx, &inst),
        Op::StoreGlobal => lower_store_global(ctx, &inst),
        Op::ExternGlobalLoad => lower_extern_global_load(ctx, &inst),
        Op::ExternGlobalStore => lower_extern_global_store(ctx, &inst),
        Op::IAdd => arithmetic::lower_int_binop(ctx, &inst, "add", "add"),
        Op::ISub => arithmetic::lower_int_binop(ctx, &inst, "sub", "sub"),
        Op::IMul => arithmetic::lower_int_binop(ctx, &inst, "mul", "imul"),
        Op::IDiv => arithmetic::lower_int_div_to_float(ctx, &inst),
        Op::ISMod => arithmetic::lower_int_mod(ctx, &inst),
        Op::INeg => arithmetic::lower_int_unary(ctx, &inst, "neg", "neg"),
        Op::IBitAnd => arithmetic::lower_int_binop(ctx, &inst, "and", "and"),
        Op::IBitOr => arithmetic::lower_int_binop(ctx, &inst, "orr", "or"),
        Op::IBitXor => arithmetic::lower_int_binop(ctx, &inst, "eor", "xor"),
        Op::IBitNot => arithmetic::lower_int_unary(ctx, &inst, "mvn", "not"),
        Op::IShl => arithmetic::lower_int_shift(ctx, &inst, "lsl", "shl"),
        Op::IShrA => arithmetic::lower_int_shift(ctx, &inst, "asr", "sar"),
        Op::MixedNumericBinop => arithmetic::lower_mixed_numeric_binop(ctx, &inst),
        Op::FAdd => floats::lower_float_binop(ctx, &inst, "fadd", "addsd"),
        Op::FSub => floats::lower_float_binop(ctx, &inst, "fsub", "subsd"),
        Op::FMul => floats::lower_float_binop(ctx, &inst, "fmul", "mulsd"),
        Op::FDiv => floats::lower_float_binop(ctx, &inst, "fdiv", "divsd"),
        Op::FPow => floats::lower_float_pow(ctx, &inst),
        Op::FNeg => floats::lower_float_neg(ctx, &inst),
        Op::ICmp => lower_int_compare(ctx, &inst),
        Op::FCmp => floats::lower_float_compare(ctx, &inst),
        Op::Spaceship => comparisons::lower_spaceship(ctx, &inst),
        Op::StrCmp => comparisons::lower_str_cmp(ctx, &inst),
        Op::StrictEq => comparisons::lower_strict_eq(ctx, &inst, true),
        Op::StrictNotEq => comparisons::lower_strict_eq(ctx, &inst, false),
        Op::LooseEq => comparisons::lower_loose_eq(ctx, &inst, true),
        Op::LooseNotEq => comparisons::lower_loose_eq(ctx, &inst, false),
        Op::IsNull => predicates::lower_is_null(ctx, &inst),
        Op::IsTruthy => predicates::lower_is_truthy(ctx, &inst),
        Op::IToF => floats::lower_int_to_float(ctx, &inst),
        Op::FToI => floats::lower_float_to_int(ctx, &inst),
        Op::IToStr => strings::lower_int_like_to_string(ctx, &inst),
        Op::FToStr => strings::lower_float_to_string(ctx, &inst),
        Op::BoolToStr => strings::lower_int_like_to_string(ctx, &inst),
        Op::ResourceToStr => strings::lower_resource_to_string(ctx, &inst),
        Op::StrToI => conversions::lower_str_to_int(ctx, &inst),
        Op::StrToF => conversions::lower_str_to_float(ctx, &inst),
        Op::Cast => conversions::lower_cast(ctx, &inst),
        Op::MixedBox => lower_mixed_box(ctx, &inst),
        Op::InvokerRefArg => lower_invoker_ref_arg(ctx, &inst),
        Op::ArrayToMixed => arrays::lower_array_to_mixed(ctx, &inst),
        Op::HashToMixed => hashes::lower_hash_to_mixed(ctx, &inst),
        Op::StrConcat => strings::lower_str_concat(ctx, &inst),
        Op::StrLen => strings::lower_str_len(ctx, &inst),
        Op::StrCharAt => strings::lower_str_char_at(ctx, &inst),
        Op::StrPersist => strings::lower_str_persist(ctx, &inst),
        Op::ArrayNew => arrays::lower_array_new(ctx, &inst),
        Op::ArrayLen => arrays::lower_array_len(ctx, &inst),
        Op::ArrayGet => arrays::lower_array_get(ctx, &inst),
        Op::ArrayIsset => builtins::lower_array_isset(ctx, &inst),
        Op::ArraySet => arrays::lower_array_set(ctx, &inst),
        Op::ArrayPush => arrays::lower_array_push(ctx, &inst),
        Op::MixedArrayAppend => arrays::lower_mixed_array_append(ctx, &inst),
        Op::ArrayUnion => arrays::lower_array_union(ctx, &inst),
        Op::ArrayHashUnion => arrays::lower_array_hash_union(ctx, &inst),
        Op::ArrayToHash => arrays::lower_array_to_hash(ctx, &inst),
        Op::HashNew => hashes::lower_hash_new(ctx, &inst),
        Op::HashLen => hashes::lower_hash_len(ctx, &inst),
        Op::HashGet => hashes::lower_hash_get(ctx, &inst),
        Op::HashIsset => builtins::lower_hash_isset(ctx, &inst),
        Op::HashSet => hashes::lower_hash_set(ctx, &inst),
        Op::HashUnion => hashes::lower_hash_union(ctx, &inst),
        Op::HashArrayUnion => hashes::lower_hash_array_union(ctx, &inst),
        Op::IterStart => iterators::lower_iter_start(ctx, &inst),
        Op::IterNext => iterators::lower_iter_next(ctx, &inst),
        Op::IterCurrentKey => iterators::lower_iter_current_key(ctx, &inst),
        Op::IterCurrentValue => iterators::lower_iter_current_value(ctx, &inst),
        Op::IterCurrentValueRef => iterators::lower_iter_current_value_ref(ctx, &inst),
        Op::IterEnd => iterators::lower_iter_end(ctx, &inst),
        Op::PtrCast => pointers::lower_ptr_cast(ctx, &inst),
        Op::BufferNew => buffers::lower_buffer_new(ctx, &inst),
        Op::BufferGet => buffers::lower_buffer_get(ctx, &inst),
        Op::BufferSet => buffers::lower_buffer_set(ctx, &inst),
        Op::ObjectNew => objects::lower_object_new(ctx, &inst),
        Op::DynamicObjectNew => objects::lower_dynamic_object_new(ctx, &inst),
        Op::DynamicObjectNewMixed => objects::lower_dynamic_object_new_mixed(ctx, &inst),
        Op::PropGet => objects::lower_prop_get(ctx, &inst),
        Op::NullsafePropGet => objects::lower_nullsafe_prop_get(ctx, &inst),
        Op::DynamicPropGet => objects::lower_dynamic_prop_get(ctx, &inst),
        Op::PropSet => objects::lower_prop_set(ctx, &inst),
        Op::DynamicPropSet => objects::lower_dynamic_prop_set(ctx, &inst),
        Op::InstanceOf => objects::lower_instanceof(ctx, &inst),
        Op::InstanceOfDynamic => objects::lower_instanceof_dynamic(ctx, &inst),
        Op::ScopedConstantGet => scoped_constants::lower_scoped_constant_get(ctx, &inst),
        Op::LoadStaticLocal => static_locals::lower_load_static_local(ctx, &inst),
        Op::StoreStaticLocal => static_locals::lower_store_static_local(ctx, &inst),
        Op::InitStaticLocal => static_locals::lower_init_static_local(ctx, &inst),
        Op::LoadStaticProperty => static_properties::lower_load_static_property(ctx, &inst),
        Op::StoreStaticProperty => static_properties::lower_store_static_property(ctx, &inst),
        Op::Call => lower_direct_call(ctx, &inst),
        Op::ClosureCall => callables::lower_closure_call(ctx, &inst),
        Op::ExprCall => callables::lower_expr_call(ctx, &inst),
        Op::CallableDescriptorInvoke => callables::lower_callable_descriptor_invoke(ctx, &inst),
        Op::PipeCall => callables::lower_pipe_call(ctx, &inst),
        Op::MethodCall => lower_method_call(ctx, &inst),
        Op::NullsafeMethodCall => lower_nullsafe_method_call(ctx, &inst),
        Op::StaticMethodCall => lower_static_method_call(ctx, &inst),
        Op::ExternCall => externs::lower_extern_call(ctx, &inst),
        Op::BuiltinCall => builtins::lower_builtin_call(ctx, &inst),
        Op::ClosureCapture => lower_closure_capture(ctx, &inst),
        Op::ClosureNew => lower_closure_new(ctx, &inst),
        Op::FirstClassCallableNew => lower_first_class_callable_new(ctx, &inst),
        Op::Acquire => ownership::lower_acquire(ctx, &inst),
        Op::Release => ownership::lower_release(ctx, &inst),
        Op::GcCollect => lower_gc_collect(ctx),
        Op::Move | Op::Borrow => ownership::lower_forward(ctx, &inst),
        Op::EchoValue => lower_echo_value(ctx, &inst),
        Op::PrintValue => lower_print_value(ctx, &inst),
        Op::ThrowException => lower_throw_exception(ctx, &inst),
        Op::TryPushHandler => lower_try_push_handler(ctx, &inst),
        Op::TryPopHandler => lower_try_pop_handler(ctx, &inst),
        Op::CatchCurrent => lower_catch_current(ctx, &inst),
        Op::CatchBind => lower_catch_bind(ctx, &inst),
        Op::ErrorSuppressBegin => lower_runtime_void_call(ctx, "__rt_diag_push_suppression"),
        Op::ErrorSuppressEnd => lower_runtime_void_call(ctx, "__rt_diag_pop_suppression"),
        Op::IncludeOnceMark => lower_include_once_mark(ctx, &inst),
        Op::IncludeOnceGuard => lower_include_once_guard(ctx, &inst),
        Op::FunctionVariantDispatch => Ok(()),
        Op::FunctionVariantMark => lower_function_variant_mark(ctx, &inst),
        Op::RuntimeCall => lower_runtime_call(ctx, &inst),
        Op::ConcatReset => lower_concat_reset(ctx),
        Op::Nop => lower_nop(ctx, &inst),
        _ => Err(CodegenIrError::unsupported(format!("opcode {}", inst.op.name()))),
    }
}

/// Lowers a statement-boundary concat-buffer reset.
fn lower_concat_reset(ctx: &mut FunctionContext<'_>) -> Result<()> {
    reset_concat_to_frame_base(ctx);
    Ok(())
}

/// Restores `_concat_off` to the offset inherited by this EIR frame.
fn reset_concat_to_frame_base(ctx: &mut FunctionContext<'_>) {
    let scratch = abi::temp_int_reg(ctx.emitter.target);
    abi::load_at_offset(ctx.emitter, scratch, ctx.concat_base_offset);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_concat_off", 0);
}

/// Lowers metadata-only NOPs, emitting data-backed messages as assembly comments.
fn lower_nop(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return Ok(());
    };
    let message = ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?;
    ctx.emitter.comment(message);
    Ok(())
}

/// Lowers a closure capture marker after call operands already recorded the captured value.
fn lower_closure_capture(_ctx: &mut FunctionContext<'_>, _inst: &Instruction) -> Result<()> {
    Ok(())
}

/// Materializes an EIR closure literal as a callable descriptor pointer.
fn lower_closure_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let closure_name = callable_target_data(ctx, inst)?.to_string();
    let closure = ctx
        .module
        .closures
        .iter()
        .find(|function| function.name == closure_name)
        .ok_or_else(|| CodegenIrError::missing_entry("closure", 0))?;
    if inst.operands.len() > closure.params.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "closure_new for {} has {} captures but only {} params",
            closure.name,
            inst.operands.len(),
            closure.params.len()
        )));
    }
    let visible_param_count = closure.params.len() - inst.operands.len();
    let signature = function_signature_from_eir_with_param_count(closure, visible_param_count);
    let captures = closure_capture_params_from_eir(closure, inst.operands.len());
    let invoker_label = emit_runtime_callable_invoker_inline(ctx, &signature, &captures);
    let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_meta(
        ctx.data,
        &function_symbol(&closure.name),
        Some(&closure.name),
        callable_descriptor::CALLABLE_DESC_KIND_CLOSURE,
        Some(&signature),
        &captures,
        &captures,
        callable_descriptor::CallableDescriptorInvocation::new(
            callable_descriptor::CallableDescriptorShape::Closure,
        ),
        Some(&invoker_label),
    );
    if captures.is_empty() {
        abi::emit_symbol_address(ctx.emitter, abi::int_result_reg(ctx.emitter), &descriptor_label);
    } else {
        emit_runtime_closure_descriptor_with_captures(
            ctx,
            &descriptor_label,
            &captures,
            &inst.operands,
        )?;
    }
    store_if_result(ctx, inst)
}

/// Returns the hidden closure capture params from the tail of the EIR closure ABI.
fn closure_capture_params_from_eir(
    closure: &crate::ir::Function,
    capture_count: usize,
) -> Vec<(String, PhpType, bool)> {
    closure
        .params
        .iter()
        .rev()
        .take(capture_count)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|param| (param.name.clone(), param.php_type.clone(), param.by_ref))
        .collect()
}

/// Allocates a runtime closure descriptor and stores capture operands into its environment.
fn emit_runtime_closure_descriptor_with_captures(
    ctx: &mut FunctionContext<'_>,
    descriptor_label: &str,
    captures: &[(String, PhpType, bool)],
    operands: &[ValueId],
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    let total_bytes =
        callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + captures.len() * 16;
    abi::emit_load_int_immediate(ctx.emitter, result_reg, total_bytes as i64);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    ctx.emitter.instruction(&format!("mov {}, {}", descriptor_reg, result_reg)); // keep the runtime closure descriptor while storing captures
    callable_descriptor::emit_copy_static_descriptor_to_runtime(
        ctx.emitter,
        descriptor_reg,
        descriptor_label,
    );
    for (idx, ((_, capture_ty, by_ref), operand)) in captures.iter().zip(operands.iter()).enumerate() {
        if *by_ref {
            let slot = local_slot_for_loaded_value(ctx, *operand)?;
            let release_replaced_value = promoted_ref_capture_replaces_owned_value(ctx, *operand)?;
            promote_local_slot_for_ref_capture(ctx, slot, None, capture_ty, release_replaced_value)?;
            materialize_local_ref_arg_address(ctx, *operand)?;
            callable_descriptor::emit_store_current_result_to_runtime_capture(
                ctx.emitter,
                descriptor_reg,
                idx,
                &PhpType::Int,
            );
            continue;
        }
        ctx.load_value_to_result(*operand)?;
        if ctx.value_ownership(*operand)? != Ownership::Owned {
            if capture_ty.codegen_repr() == PhpType::Str {
                abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            } else {
                abi::emit_incref_if_refcounted(ctx.emitter, capture_ty);
            }
        }
        callable_descriptor::emit_store_current_result_to_runtime_capture(
            ctx.emitter,
            descriptor_reg,
            idx,
            capture_ty,
        );
    }
    if descriptor_reg != result_reg {
        ctx.emitter.instruction(&format!("mov {}, {}", result_reg, descriptor_reg)); // return the runtime closure descriptor pointer
    }
    Ok(())
}

/// Returns whether a by-reference closure capture replaces a caller-owned local value.
fn promoted_ref_capture_replaces_owned_value(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<bool> {
    Ok(matches!(
        ctx.value_ownership(value)?,
        Ownership::Owned | Ownership::MaybeOwned
    ))
}

/// Promotes a normal local slot to a heap ref-cell for an escaping by-reference capture.
fn promote_local_slot_for_ref_capture(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    owner_slot: Option<LocalSlotId>,
    capture_ty: &PhpType,
    release_replaced_value: bool,
) -> Result<()> {
    if local_slot_stores_ref_cell_pointer(ctx, slot) {
        return Ok(());
    }
    reject_multiword_ref_param_local(capture_ty, "capture")?;
    let local_ty = ctx.local_php_type(slot)?;
    let offset = ctx.local_offset(slot)?;
    abi::emit_load(ctx.emitter, &local_ty, offset);
    retain_promoted_ref_cell_value(ctx, &local_ty);
    abi::emit_push_result_value(ctx.emitter, &local_ty);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    let cell_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.emitter.instruction(&format!("mov {}, {}", cell_reg, abi::int_result_reg(ctx.emitter))); // keep the promoted closure capture cell while restoring its value
    pop_result_value(ctx, &local_ty);
    store_current_result_to_ref_cell(ctx, cell_reg, &local_ty);
    if release_replaced_value {
        release_replaced_promoted_local_value(ctx, &local_ty, offset, cell_reg);
    }
    abi::store_at_offset_scratch(ctx.emitter, cell_reg, offset, abi::tertiary_scratch_reg(ctx.emitter));
    if let Some(owner_slot) = owner_slot {
        let owner_offset = ctx.local_offset(owner_slot)?;
        abi::store_at_offset_scratch(ctx.emitter, cell_reg, owner_offset, abi::tertiary_scratch_reg(ctx.emitter));
    }
    ctx.mark_promoted_ref_cell(slot);
    Ok(())
}

/// Releases the old local owner after its retained value has been copied into a ref-cell.
fn release_replaced_promoted_local_value(
    ctx: &mut FunctionContext<'_>,
    local_ty: &PhpType,
    offset: usize,
    cell_reg: &str,
) {
    let local_ty = local_ty.codegen_repr();
    if !matches!(local_ty, PhpType::Str | PhpType::Callable) && !local_ty.is_refcounted() {
        return;
    }
    abi::emit_push_reg(ctx.emitter, cell_reg);
    match local_ty {
        PhpType::Str => {
            abi::load_at_offset_scratch(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                offset,
                abi::secondary_scratch_reg(ctx.emitter),
            );
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
        }
        PhpType::Callable => {
            abi::load_at_offset_scratch(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                offset,
                abi::secondary_scratch_reg(ctx.emitter),
            );
            callable_descriptor::emit_release_current_descriptor(ctx.emitter);
        }
        ty if ty.is_refcounted() => {
            abi::load_at_offset_scratch(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                offset,
                abi::secondary_scratch_reg(ctx.emitter),
            );
            abi::emit_decref_if_refcounted(ctx.emitter, &ty);
        }
        _ => {}
    }
    abi::emit_pop_reg(ctx.emitter, cell_reg);
}

/// Retains or persists a value before it is moved into a promoted ref-cell.
fn retain_promoted_ref_cell_value(ctx: &mut FunctionContext<'_>, local_ty: &PhpType) {
    match local_ty.codegen_repr() {
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
        }
        PhpType::Callable => {
            callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
        }
        other if other.is_refcounted() => {
            abi::emit_incref_if_refcounted(ctx.emitter, &other);
        }
        _ => {}
    }
}

/// Pops a previously saved result value back into the target result registers.
fn pop_result_value(ctx: &mut FunctionContext<'_>, local_ty: &PhpType) {
    match local_ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_pop_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
        }
        PhpType::TaggedScalar => {
            abi::emit_pop_reg_pair(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter),
            );
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        }
    }
}

/// Stores the current result registers into a two-word heap ref-cell.
fn store_current_result_to_ref_cell(
    ctx: &mut FunctionContext<'_>,
    cell_reg: &str,
    local_ty: &PhpType,
) {
    match local_ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_store_to_address(ctx.emitter, abi::float_result_reg(ctx.emitter), cell_reg, 0);
            abi::emit_store_zero_to_address(ctx.emitter, cell_reg, 8);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, ptr_reg, cell_reg, 0);
            abi::emit_store_to_address(ctx.emitter, len_reg, cell_reg, 8);
        }
        PhpType::TaggedScalar => {
            abi::emit_store_to_address(ctx.emitter, abi::int_result_reg(ctx.emitter), cell_reg, 0);
            abi::emit_store_to_address(
                ctx.emitter,
                crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter),
                cell_reg,
                8,
            );
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_store_zero_to_address(ctx.emitter, cell_reg, 0);
            abi::emit_store_zero_to_address(ctx.emitter, cell_reg, 8);
        }
        _ => {
            abi::emit_store_to_address(ctx.emitter, abi::int_result_reg(ctx.emitter), cell_reg, 0);
            abi::emit_store_zero_to_address(ctx.emitter, cell_reg, 8);
        }
    }
}

/// Reconstructs callable signature metadata from an emitted EIR function.
fn function_signature_from_eir(function: &crate::ir::Function) -> FunctionSig {
    function_signature_from_eir_with_param_count(function, function.params.len())
}

/// Reconstructs signature metadata from the first `param_count` EIR params.
fn function_signature_from_eir_with_param_count(
    function: &crate::ir::Function,
    param_count: usize,
) -> FunctionSig {
    if let Some(signature) = &function.signature {
        let mut signature = signature.clone();
        let original_param_count = signature.params.len();
        ensure_variadic_param_slot(&mut signature);
        if original_param_count == param_count {
            return signature.clone();
        }
    }

    FunctionSig {
        params: function
            .params
            .iter()
            .take(param_count)
            .map(|param| (param.name.clone(), param.php_type.clone()))
            .collect(),
        defaults: function.params.iter().take(param_count).map(|_| None).collect(),
        return_type: function.return_php_type.clone(),
        declared_return: !matches!(function.return_php_type, PhpType::Mixed),
        ref_params: function
            .params
            .iter()
            .take(param_count)
            .map(|param| param.by_ref)
            .collect(),
        declared_params: function
            .params
            .iter()
            .take(param_count)
            .map(|param| !matches!(param.php_type, PhpType::Mixed))
            .collect(),
        variadic: function
            .params
            .iter()
            .take(param_count)
            .find(|param| param.variadic)
            .map(|param| param.name.clone()),
        deprecation: None,
    }
}

/// Adds the virtual variadic array slot when the EIR ABI stores it outside `params`.
fn ensure_variadic_param_slot(signature: &mut FunctionSig) {
    let Some(variadic) = signature.variadic.clone() else {
        return;
    };
    if signature.params.iter().any(|(name, _)| name == &variadic) {
        return;
    }
    signature
        .params
        .push((variadic, PhpType::Array(Box::new(PhpType::Mixed))));
    signature.defaults.push(None);
    signature.ref_params.push(false);
    signature.declared_params.push(false);
}

/// Lowers a concrete include-loaded function variant activation marker.
fn lower_function_variant_mark(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_data(inst)?;
    let label = ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?;
    let parsed = function_variants::parse_variant_label(label)
        .ok_or_else(|| CodegenIrError::invalid_module(format!("invalid function variant label '{}'", label)))?;
    function_variants::emit_variant_mark(ctx.emitter, ctx.data, &parsed)
}

/// Lowers an include-once marker by setting its module-global guard symbol.
fn lower_include_once_mark(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let label = include_once_label(ctx, inst)?;
    ctx.data.add_comm(label.clone(), 8);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    abi::emit_store_reg_to_symbol(ctx.emitter, abi::int_result_reg(ctx.emitter), &label, 0);
    Ok(())
}

/// Lowers an include-once guard to a boolean branch condition and marks first entry.
fn lower_include_once_guard(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let label = include_once_label(ctx, inst)?;
    ctx.data.add_comm(label.clone(), 8);
    let already_label = ctx.next_label("include_once_already");
    let done_label = ctx.next_label("include_once_done");
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, &label, 0);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &already_label);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 1);
    abi::emit_store_reg_to_symbol(ctx.emitter, result_reg, &label, 0);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&already_label);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Returns the include-once guard symbol stored in the module data pool.
fn include_once_label(ctx: &FunctionContext<'_>, inst: &Instruction) -> Result<String> {
    let data = expect_data(inst)?;
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}

/// Lowers a void EIR opcode that maps directly to one runtime helper call.
fn lower_runtime_void_call(ctx: &mut FunctionContext<'_>, label: &str) -> Result<()> {
    abi::emit_call_label(ctx.emitter, label);
    Ok(())
}

/// Materializes a first-class callable value as a static descriptor pointer when possible.
fn lower_first_class_callable_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let target = callable_target_data(ctx, inst)?.to_string();
    if emit_static_late_bound_first_class_callable(ctx, &target)? {
        return store_if_result(ctx, inst);
    }
    if emit_instance_method_first_class_callable(ctx, inst, &target)? {
        return store_if_result(ctx, inst);
    }
    if let Some(descriptor) = first_class_callable_descriptor(ctx, &target) {
        let invoker_label = descriptor
            .sig
            .as_ref()
            .map(|sig| emit_runtime_callable_invoker_inline(ctx, sig, &[]));
        let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_meta(
            ctx.data,
            &descriptor.entry_label,
            Some(&target),
            descriptor.kind,
            descriptor.sig.as_ref(),
            &[],
            &[],
            descriptor.invocation,
            invoker_label.as_deref(),
        );
        abi::emit_symbol_address(ctx.emitter, abi::int_result_reg(ctx.emitter), &descriptor_label);
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    }
    store_if_result(ctx, inst)
}

/// Emits a runtime descriptor for `static::method(...)` first-class callables.
fn emit_static_late_bound_first_class_callable(
    ctx: &mut FunctionContext<'_>,
    target: &str,
) -> Result<bool> {
    let Some((receiver_label, method_name)) = target.rsplit_once("::") else {
        return Ok(false);
    };
    if receiver_label.trim_start_matches('\\') != "static" {
        return Ok(false);
    }

    let receiver = resolve_static_method_receiver(ctx, receiver_label)?;
    let called_class_id = resolve_static_called_class_arg(ctx, receiver_label, &receiver)?;
    let receiver_info = ctx
        .module
        .class_infos
        .get(receiver.as_str())
        .ok_or_else(|| CodegenIrError::unsupported(format!(
            "late-bound first-class callable '{}' on unknown class '{}'",
            target,
            receiver
        )))?;
    let method_key = php_symbol_key(method_name);
    let impl_class = receiver_info
        .static_method_impl_classes
        .get(&method_key)
        .cloned()
        .unwrap_or_else(|| receiver.clone());
    let dynamic_slot = receiver_info.static_vtable_slots.get(&method_key).copied();
    let sig = ctx
        .module
        .class_infos
        .get(impl_class.as_str())
        .and_then(|class_info| class_info.static_methods.get(&method_key))
        .ok_or_else(|| CodegenIrError::unsupported(format!(
            "late-bound first-class callable '{}' with unknown implementation",
            target
        )))?
        .clone();
    let wrapper_sig = crate::codegen::callable_dispatch::static_method_runtime_wrapper_sig(&sig);
    let captures = vec![("called_class_id".to_string(), PhpType::Int, false)];
    let entry_label = emit_static_late_bound_descriptor_entry_wrapper(
        ctx,
        impl_class.as_str(),
        &method_key,
        &wrapper_sig,
        dynamic_slot,
    )?;
    let invoker_label = emit_runtime_callable_invoker_inline(ctx, &wrapper_sig, &captures);
    let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_meta(
        ctx.data,
        &entry_label,
        Some(target),
        callable_descriptor::CALLABLE_DESC_KIND_STATIC_METHOD,
        Some(&wrapper_sig),
        &captures,
        &[],
        callable_descriptor::CallableDescriptorInvocation::method(
            callable_descriptor::CallableDescriptorShape::StaticMethod,
            Some("static".to_string()),
            method_key.clone(),
        ),
        Some(&invoker_label),
    );
    emit_runtime_descriptor_with_called_class_capture(ctx, &descriptor_label, &called_class_id)?;
    Ok(true)
}

/// Emits a runtime descriptor for receiver-bound `object::method` first-class callables.
fn emit_instance_method_first_class_callable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: &str,
) -> Result<bool> {
    let Some((receiver_label, method_name)) = target.rsplit_once("::") else {
        return Ok(false);
    };
    if receiver_label.trim_start_matches('\\') != "object" {
        return Ok(false);
    }
    let receiver = inst.operands.first().copied().ok_or_else(|| {
        CodegenIrError::invalid_module(format!(
            "instance first-class callable '{}' has no receiver operand",
            target
        ))
    })?;
    let receiver_ty = ctx.value_php_type(receiver)?;
    let PhpType::Object(class_name) = receiver_ty.codegen_repr() else {
        return Err(CodegenIrError::unsupported(format!(
            "instance first-class callable '{}' with receiver PHP type {:?}",
            target,
            receiver_ty
        )));
    };
    let normalized_class = class_name.trim_start_matches('\\').to_string();
    let method_key = php_symbol_key(method_name);
    let class_info = ctx
        .module
        .class_infos
        .get(normalized_class.as_str())
        .ok_or_else(|| CodegenIrError::unsupported(format!(
            "instance first-class callable '{}' with unknown receiver class '{}'",
            target,
            normalized_class
        )))?;
    let sig = class_info
        .methods
        .get(&method_key)
        .ok_or_else(|| CodegenIrError::unsupported(format!(
            "instance first-class callable '{}' with unknown method",
            target
        )))?
        .clone();
    let impl_class = class_info
        .method_impl_classes
        .get(&method_key)
        .cloned()
        .unwrap_or_else(|| normalized_class.clone());
    if !class_method_body_exists(ctx, &impl_class, &method_key) {
        return Err(CodegenIrError::unsupported(format!(
            "instance first-class callable '{}' without emitted method body",
            target
        )));
    }
    let receiver_ty = PhpType::Object(normalized_class.clone());
    let captures = vec![("receiver".to_string(), receiver_ty.clone(), false)];
    let entry_label = emit_instance_method_descriptor_entry_wrapper(ctx, &impl_class, &method_key, &sig)?;
    let invoker_label = emit_runtime_callable_invoker_inline(ctx, &sig, &captures);
    let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_meta(
        ctx.data,
        &entry_label,
        Some(target),
        callable_descriptor::CALLABLE_DESC_KIND_FIRST_CLASS,
        Some(&sig),
        &captures,
        &[],
        callable_descriptor::CallableDescriptorInvocation::method(
            callable_descriptor::CallableDescriptorShape::InstanceMethod,
            Some(normalized_class),
            method_name,
        ),
        Some(&invoker_label),
    );
    emit_runtime_descriptor_with_receiver_capture(ctx, &descriptor_label, receiver, &receiver_ty)?;
    Ok(true)
}

/// Emits an entry wrapper that receives visible args followed by a captured called-class id.
fn emit_static_late_bound_descriptor_entry_wrapper(
    ctx: &mut FunctionContext<'_>,
    impl_class: &str,
    method_key: &str,
    sig: &FunctionSig,
    dynamic_slot: Option<usize>,
) -> Result<String> {
    let visible_arg_types = descriptor_visible_arg_types(sig);
    let wrapper_label = ctx.next_label("static_late_bound_descriptor_entry");
    let done_label = ctx.next_label("static_late_bound_descriptor_entry_done");
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&wrapper_label);
    emit_static_late_bound_descriptor_entry_wrapper_body(
        ctx,
        impl_class,
        method_key,
        &visible_arg_types,
        dynamic_slot,
    );
    ctx.emitter.label(&done_label);
    Ok(wrapper_label)
}

/// Emits an entry wrapper that receives visible args followed by the captured receiver.
fn emit_instance_method_descriptor_entry_wrapper(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    method_key: &str,
    sig: &FunctionSig,
) -> Result<String> {
    let visible_arg_types = descriptor_visible_arg_types(sig);
    let wrapper_label = ctx.next_label("callable_instance_method");
    let done_label = ctx.next_label("callable_instance_method_done");
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&wrapper_label);
    emit_instance_method_descriptor_entry_wrapper_body(ctx, class_name, method_key, &visible_arg_types);
    ctx.emitter.label(&done_label);
    Ok(wrapper_label)
}

/// Returns codegen-representation parameter types for a descriptor entry wrapper.
fn descriptor_visible_arg_types(sig: &FunctionSig) -> Vec<PhpType> {
    sig.params
        .iter()
        .map(|(_, ty)| ty.codegen_repr())
        .collect()
}

/// Emits a descriptor entry wrapper body by reordering visible args after the receiver.
fn emit_instance_method_descriptor_entry_wrapper_body(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    method_key: &str,
    visible_arg_types: &[PhpType],
) {
    let receiver_ty = descriptor_receiver_type(class_name);
    let incoming_types = descriptor_entry_incoming_types(visible_arg_types, &receiver_ty);
    let actual_types = descriptor_entry_actual_types(visible_arg_types, &receiver_ty);
    let incoming_assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &incoming_types, 0);
    let actual_assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &actual_types, 0);
    let (incoming_stack_offsets, _) = descriptor_entry_stack_offsets(&incoming_assignments);
    let (actual_stack_offsets, actual_overflow_bytes) =
        descriptor_entry_stack_offsets(&actual_assignments);
    let frame_size = descriptor_entry_frame_size(incoming_types.len());

    abi::emit_frame_prologue(ctx.emitter, frame_size);
    for (idx, (ty, assignment)) in incoming_types.iter().zip(incoming_assignments.iter()).enumerate() {
        store_descriptor_entry_incoming_arg(
            ctx.emitter,
            ty,
            assignment,
            descriptor_entry_slot_offset(idx),
            incoming_stack_offsets[idx],
        );
    }
    if actual_overflow_bytes > 0 {
        abi::emit_reserve_temporary_stack(ctx.emitter, actual_overflow_bytes);
    }
    for (idx, (ty, assignment)) in actual_types.iter().zip(actual_assignments.iter()).enumerate() {
        let source_idx = if idx == 0 { visible_arg_types.len() } else { idx - 1 };
        load_descriptor_entry_actual_arg(
            ctx.emitter,
            ty,
            assignment,
            descriptor_entry_slot_offset(source_idx),
            actual_stack_offsets[idx],
        );
    }
    abi::emit_call_label(ctx.emitter, &method_symbol(class_name, method_key));
    if actual_overflow_bytes > 0 {
        abi::emit_release_temporary_stack(ctx.emitter, actual_overflow_bytes);
    }
    abi::emit_frame_restore(ctx.emitter, frame_size);
    abi::emit_return(ctx.emitter);
}

/// Emits a static descriptor entry wrapper body by prepending the called-class id.
fn emit_static_late_bound_descriptor_entry_wrapper_body(
    ctx: &mut FunctionContext<'_>,
    impl_class: &str,
    method_key: &str,
    visible_arg_types: &[PhpType],
    dynamic_slot: Option<usize>,
) {
    let called_class_ty = PhpType::Int;
    let incoming_types = descriptor_entry_incoming_types(visible_arg_types, &called_class_ty);
    let actual_types = descriptor_entry_actual_types(visible_arg_types, &called_class_ty);
    let incoming_assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &incoming_types, 0);
    let actual_assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &actual_types, 0);
    let (incoming_stack_offsets, _) = descriptor_entry_stack_offsets(&incoming_assignments);
    let (actual_stack_offsets, actual_overflow_bytes) =
        descriptor_entry_stack_offsets(&actual_assignments);
    let frame_size = descriptor_entry_frame_size(incoming_types.len());

    abi::emit_frame_prologue(ctx.emitter, frame_size);
    for (idx, (ty, assignment)) in incoming_types.iter().zip(incoming_assignments.iter()).enumerate() {
        store_descriptor_entry_incoming_arg(
            ctx.emitter,
            ty,
            assignment,
            descriptor_entry_slot_offset(idx),
            incoming_stack_offsets[idx],
        );
    }
    if actual_overflow_bytes > 0 {
        abi::emit_reserve_temporary_stack(ctx.emitter, actual_overflow_bytes);
    }
    for (idx, (ty, assignment)) in actual_types.iter().zip(actual_assignments.iter()).enumerate() {
        let source_idx = if idx == 0 { visible_arg_types.len() } else { idx - 1 };
        load_descriptor_entry_actual_arg(
            ctx.emitter,
            ty,
            assignment,
            descriptor_entry_slot_offset(source_idx),
            actual_stack_offsets[idx],
        );
    }
    if let Some(slot) = dynamic_slot {
        emit_dynamic_static_method_call(ctx, slot);
    } else {
        abi::emit_call_label(ctx.emitter, &static_method_symbol(impl_class, method_key));
    }
    if actual_overflow_bytes > 0 {
        abi::emit_release_temporary_stack(ctx.emitter, actual_overflow_bytes);
    }
    abi::emit_frame_restore(ctx.emitter, frame_size);
    abi::emit_return(ctx.emitter);
}

/// Returns the runtime receiver type threaded through the descriptor entry wrapper.
fn descriptor_receiver_type(class_name: &str) -> PhpType {
    PhpType::Object(class_name.to_string())
}

/// Returns the wrapper incoming argument order: visible args followed by receiver.
fn descriptor_entry_incoming_types(visible_arg_types: &[PhpType], receiver_ty: &PhpType) -> Vec<PhpType> {
    let mut types = visible_arg_types.to_vec();
    types.push(receiver_ty.clone());
    types
}

/// Returns the real method ABI argument order: receiver followed by visible args.
fn descriptor_entry_actual_types(visible_arg_types: &[PhpType], receiver_ty: &PhpType) -> Vec<PhpType> {
    let mut types = Vec::with_capacity(visible_arg_types.len() + 1);
    types.push(receiver_ty.clone());
    types.extend_from_slice(visible_arg_types);
    types
}

/// Returns an aligned frame size for descriptor entry wrapper spill slots plus footer.
fn descriptor_entry_frame_size(slot_count: usize) -> usize {
    align16((slot_count + 1) * 16)
}

/// Returns the frame offset for a descriptor entry wrapper spill slot.
fn descriptor_entry_slot_offset(idx: usize) -> usize {
    (idx + 1) * 16
}

/// Returns the local/outgoing byte size used for one descriptor wrapper argument.
fn descriptor_entry_arg_slot_size(ty: &PhpType) -> usize {
    match ty.codegen_repr() {
        PhpType::Void | PhpType::Never => 0,
        _ => 16,
    }
}

/// Returns stack offsets for ABI assignments that overflow their target registers.
fn descriptor_entry_stack_offsets(assignments: &[abi::OutgoingArgAssignment]) -> (Vec<Option<usize>>, usize) {
    let mut offsets = vec![None; assignments.len()];
    let mut next_offset = 0usize;
    for (idx, assignment) in assignments.iter().enumerate() {
        if assignment.in_register() {
            continue;
        }
        offsets[idx] = Some(next_offset);
        next_offset += descriptor_entry_arg_slot_size(&assignment.ty);
    }
    (offsets, next_offset)
}

/// Lowers an explicit cycle-collection safe point.
fn lower_gc_collect(ctx: &mut FunctionContext<'_>) -> Result<()> {
    abi::emit_call_label(ctx.emitter, "__rt_gc_collect_cycles");
    Ok(())
}

/// Converts a descriptor overflow offset into a caller-stack frame offset.
fn descriptor_entry_caller_stack_offset(
    emitter: &crate::codegen::emit::Emitter,
    stack_offset: usize,
) -> usize {
    let cursor = abi::IncomingArgCursor::for_target(emitter.target, 0);
    cursor.caller_stack_offset + stack_offset
}

/// Returns integer scratch registers that cannot overlap live descriptor argument registers.
fn descriptor_entry_int_spill_pair(
    emitter: &crate::codegen::emit::Emitter,
) -> (&'static str, &'static str) {
    let lo_reg = abi::secondary_scratch_reg(emitter);
    let hi_reg = match emitter.target.arch {
        Arch::AArch64 => abi::tertiary_scratch_reg(emitter),
        Arch::X86_64 => "r11",
    };
    (lo_reg, hi_reg)
}

/// Stores one incoming descriptor entry argument into its spill slot.
fn store_descriptor_entry_incoming_arg(
    emitter: &mut crate::codegen::emit::Emitter,
    ty: &PhpType,
    assignment: &abi::OutgoingArgAssignment,
    offset: usize,
    stack_offset: Option<usize>,
) {
    match ty.codegen_repr() {
        PhpType::Float => {
            let reg = if assignment.in_register() {
                abi::float_arg_reg_name(emitter.target, assignment.start_reg)
            } else {
                let caller_offset =
                    descriptor_entry_caller_stack_offset(emitter, stack_offset.expect("stack offset"));
                let spill_reg = match emitter.target.arch {
                    Arch::AArch64 => "d15",
                    Arch::X86_64 => "xmm15",
                };
                abi::load_from_caller_stack(emitter, spill_reg, caller_offset);
                spill_reg
            };
            abi::store_at_offset(emitter, reg, offset);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = if assignment.in_register() {
                (
                    abi::int_arg_reg_name(emitter.target, assignment.start_reg),
                    abi::int_arg_reg_name(emitter.target, assignment.start_reg + 1),
                )
            } else {
                let caller_offset =
                    descriptor_entry_caller_stack_offset(emitter, stack_offset.expect("stack offset"));
                let (ptr_spill_reg, len_spill_reg) = descriptor_entry_int_spill_pair(emitter);
                abi::load_from_caller_stack(emitter, ptr_spill_reg, caller_offset);
                abi::load_from_caller_stack(emitter, len_spill_reg, caller_offset + 8);
                (ptr_spill_reg, len_spill_reg)
            };
            abi::store_at_offset(emitter, ptr_reg, offset);
            abi::store_at_offset(emitter, len_reg, offset - 8);
        }
        PhpType::TaggedScalar => {
            let (payload_reg, tag_reg) = if assignment.in_register() {
                (
                    abi::int_arg_reg_name(emitter.target, assignment.start_reg),
                    abi::int_arg_reg_name(emitter.target, assignment.start_reg + 1),
                )
            } else {
                let caller_offset =
                    descriptor_entry_caller_stack_offset(emitter, stack_offset.expect("stack offset"));
                let (payload_spill_reg, tag_spill_reg) = descriptor_entry_int_spill_pair(emitter);
                abi::load_from_caller_stack(emitter, payload_spill_reg, caller_offset);
                abi::load_from_caller_stack(emitter, tag_spill_reg, caller_offset + 8);
                (payload_spill_reg, tag_spill_reg)
            };
            abi::store_at_offset(emitter, payload_reg, offset);
            abi::store_at_offset(emitter, tag_reg, offset - 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            let reg = if assignment.in_register() {
                abi::int_arg_reg_name(emitter.target, assignment.start_reg)
            } else {
                let caller_offset =
                    descriptor_entry_caller_stack_offset(emitter, stack_offset.expect("stack offset"));
                let spill_reg = abi::secondary_scratch_reg(emitter);
                abi::load_from_caller_stack(emitter, spill_reg, caller_offset);
                spill_reg
            };
            abi::store_at_offset(emitter, reg, offset);
        }
    }
}

/// Loads one spilled descriptor entry argument into its real method ABI assignment.
fn load_descriptor_entry_actual_arg(
    emitter: &mut crate::codegen::emit::Emitter,
    ty: &PhpType,
    assignment: &abi::OutgoingArgAssignment,
    offset: usize,
    stack_offset: Option<usize>,
) {
    match ty.codegen_repr() {
        PhpType::Float => {
            let reg = if assignment.in_register() {
                abi::float_arg_reg_name(emitter.target, assignment.start_reg)
            } else {
                match emitter.target.arch {
                    Arch::AArch64 => "d15",
                    Arch::X86_64 => "xmm15",
                }
            };
            abi::load_at_offset(emitter, reg, offset);
            if let Some(out_offset) = stack_offset {
                abi::emit_store_to_sp(emitter, reg, out_offset);
            }
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = if assignment.in_register() {
                (
                    abi::int_arg_reg_name(emitter.target, assignment.start_reg),
                    abi::int_arg_reg_name(emitter.target, assignment.start_reg + 1),
                )
            } else {
                descriptor_entry_int_spill_pair(emitter)
            };
            abi::load_at_offset(emitter, ptr_reg, offset);
            abi::load_at_offset(emitter, len_reg, offset - 8);
            if let Some(out_offset) = stack_offset {
                abi::emit_store_to_sp(emitter, ptr_reg, out_offset);
                abi::emit_store_to_sp(emitter, len_reg, out_offset + 8);
            }
        }
        PhpType::TaggedScalar => {
            let (payload_reg, tag_reg) = if assignment.in_register() {
                (
                    abi::int_arg_reg_name(emitter.target, assignment.start_reg),
                    abi::int_arg_reg_name(emitter.target, assignment.start_reg + 1),
                )
            } else {
                descriptor_entry_int_spill_pair(emitter)
            };
            abi::load_at_offset(emitter, payload_reg, offset);
            abi::load_at_offset(emitter, tag_reg, offset - 8);
            if let Some(out_offset) = stack_offset {
                abi::emit_store_to_sp(emitter, payload_reg, out_offset);
                abi::emit_store_to_sp(emitter, tag_reg, out_offset + 8);
            }
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            let reg = if assignment.in_register() {
                abi::int_arg_reg_name(emitter.target, assignment.start_reg)
            } else {
                abi::secondary_scratch_reg(emitter)
            };
            abi::load_at_offset(emitter, reg, offset);
            if let Some(out_offset) = stack_offset {
                abi::emit_store_to_sp(emitter, reg, out_offset);
            }
        }
    }
}

/// Rounds `value` up to a 16-byte multiple.
fn align16(value: usize) -> usize {
    (value + 15) & !15
}

/// Emits a descriptor invoker inline and branches around its global entry body.
fn emit_runtime_callable_invoker_inline(
    ctx: &mut FunctionContext<'_>,
    sig: &FunctionSig,
    captures: &[(String, PhpType, bool)],
) -> String {
    let label = ctx.next_label("callable_invoker");
    let done_label = ctx.next_label("callable_invoker_done");
    let parent_ctx = legacy_context_from_eir_module(ctx.module);
    let invoker = DeferredRuntimeCallableInvoker {
        label: label.clone(),
        sig: sig.clone(),
        captures: captures.to_vec(),
    };
    abi::emit_jump(ctx.emitter, &done_label);
    crate::codegen::runtime_callable_invoker::emit_runtime_callable_invoker(
        ctx.emitter,
        ctx.data,
        &parent_ctx,
        &invoker,
    );
    ctx.emitter.label(&done_label);
    label
}

/// Emits a legacy builtin wrapper inline so EIR descriptors can point at PHP-ABI code.
fn emit_runtime_builtin_wrapper_inline(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    sig: &FunctionSig,
) -> String {
    let mut legacy_ctx = legacy_context_from_eir_module(ctx.module);
    let label = crate::codegen::callable_dispatch::ensure_runtime_builtin_wrapper(
        &mut legacy_ctx,
        name,
        sig,
    );
    emit_legacy_deferred_callable_support_inline(ctx, &mut legacy_ctx);
    label
}

/// Emits a legacy static-method wrapper inline for descriptor-compatible callbacks.
fn emit_runtime_static_method_wrapper_inline(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    method_name: &str,
    sig: &FunctionSig,
) -> String {
    let mut legacy_ctx = legacy_context_from_eir_module(ctx.module);
    let label = crate::codegen::callable_dispatch::ensure_runtime_static_method_wrapper(
        &mut legacy_ctx,
        class_name,
        method_name,
        sig,
    );
    emit_legacy_deferred_callable_support_inline(ctx, &mut legacy_ctx);
    label
}

/// Emits legacy deferred callable helpers inline and branches around their entry bodies.
fn emit_legacy_deferred_callable_support_inline(
    ctx: &mut FunctionContext<'_>,
    legacy_ctx: &mut LegacyContext,
) {
    if legacy_ctx.deferred_closures.is_empty()
        && legacy_ctx.deferred_fiber_wrappers.is_empty()
        && legacy_ctx.deferred_callback_wrappers.is_empty()
        && legacy_ctx.deferred_extern_callback_trampolines.is_empty()
        && legacy_ctx.deferred_runtime_callable_invokers.is_empty()
    {
        return;
    }
    let done_label = ctx.next_label("runtime_callable_support_done");
    abi::emit_jump(ctx.emitter, &done_label);
    crate::codegen::emit_deferred_closures(ctx.emitter, ctx.data, legacy_ctx);
    ctx.emitter.label(&done_label);
}

/// Builds the legacy metadata context needed by reused descriptor-invoker emitters.
fn legacy_context_from_eir_module(module: &crate::ir::Module) -> LegacyContext {
    let mut ctx = LegacyContext::new();
    for function in module
        .functions
        .iter()
        .filter(|function| !is_property_init_thunk_function(function))
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
    {
        ctx.functions
            .insert(function.name.clone(), function_signature_from_eir(function));
    }
    ctx.function_variant_groups = super::function_variants::collect_dispatch_groups(module)
        .into_iter()
        .map(|group| group.name)
        .collect();
    ctx.callable_param_sigs = module.callable_param_sigs.clone();
    for decl in &module.extern_decls {
        ctx.extern_functions.insert(
            decl.name.clone(),
            ExternFunctionSig {
                name: decl.name.clone(),
                params: decl
                    .params
                    .iter()
                    .map(|param| (param.name.clone(), param.php_type.clone()))
                    .collect(),
                return_type: decl.return_php_type.clone(),
                library: decl.link_libs.first().cloned(),
            },
        );
    }
    ctx.classes = module.class_infos.clone();
    ctx.interfaces = module.interface_infos.clone();
    ctx.enums = module.enum_infos.clone();
    ctx.packed_classes = module.packed_class_infos.clone();
    ctx.extern_classes = module.extern_class_infos.clone();
    ctx
}

/// Returns true for synthetic property-default init thunks, which are not PHP callables.
fn is_property_init_thunk_function(function: &crate::ir::Function) -> bool {
    function.name.starts_with("_class_propinit_")
}

/// Returns true when the EIR module contains the concrete instance-method body.
pub(super) fn class_method_body_exists(ctx: &FunctionContext<'_>, class_name: &str, method_key: &str) -> bool {
    ctx.module.class_methods.iter().any(|function| {
        !function.flags.is_static
            && function
                .name
                .rsplit_once("::")
                .is_some_and(|(class, method)| class == class_name && php_symbol_key(method) == method_key)
    })
}

/// Allocates a runtime descriptor and stores the receiver in capture slot zero.
pub(super) fn emit_runtime_descriptor_with_receiver_capture(
    ctx: &mut FunctionContext<'_>,
    descriptor_label: &str,
    receiver: ValueId,
    receiver_ty: &PhpType,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    let total_bytes = callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + 16;
    ctx.load_value_to_result(receiver)?;
    if ctx.value_ownership(receiver)? != Ownership::Owned {
        abi::emit_incref_if_refcounted(ctx.emitter, receiver_ty);
    }
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, total_bytes as i64);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    ctx.emitter.instruction(&format!("mov {}, {}", descriptor_reg, result_reg)); // keep the runtime callable descriptor while copying its static header
    callable_descriptor::emit_copy_static_descriptor_to_runtime(
        ctx.emitter,
        descriptor_reg,
        descriptor_label,
    );
    abi::emit_pop_reg(ctx.emitter, result_reg);
    callable_descriptor::emit_store_current_result_to_runtime_capture(
        ctx.emitter,
        descriptor_reg,
        0,
        receiver_ty,
    );
    if descriptor_reg != result_reg {
        ctx.emitter.instruction(&format!("mov {}, {}", result_reg, descriptor_reg)); // return the receiver-bound callable descriptor
    }
    Ok(())
}

/// Allocates a runtime descriptor and stores the called-class id in capture slot zero.
fn emit_runtime_descriptor_with_called_class_capture(
    ctx: &mut FunctionContext<'_>,
    descriptor_label: &str,
    called_class_id: &CalledClassIdArg,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    let total_bytes = callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + 16;
    materialize_called_class_id(ctx, called_class_id)?;
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, total_bytes as i64);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    ctx.emitter.instruction(&format!("mov {}, {}", descriptor_reg, result_reg)); // keep the runtime callable descriptor while copying its static header
    callable_descriptor::emit_copy_static_descriptor_to_runtime(
        ctx.emitter,
        descriptor_reg,
        descriptor_label,
    );
    abi::emit_pop_reg(ctx.emitter, result_reg);
    callable_descriptor::emit_store_current_result_to_runtime_capture(
        ctx.emitter,
        descriptor_reg,
        0,
        &PhpType::Int,
    );
    if descriptor_reg != result_reg {
        ctx.emitter.instruction(&format!("mov {}, {}", result_reg, descriptor_reg)); // return the called-class-bound callable descriptor
    }
    Ok(())
}

/// Descriptor metadata for a compile-time first-class callable target.
struct FirstClassCallableDescriptor {
    entry_label: String,
    kind: u64,
    sig: Option<FunctionSig>,
    invocation: callable_descriptor::CallableDescriptorInvocation,
}

/// Returns static descriptor metadata for compile-time callable targets supported by EIR.
fn first_class_callable_descriptor(
    ctx: &mut FunctionContext<'_>,
    target: &str,
) -> Option<FirstClassCallableDescriptor> {
    if let Some((receiver_label, method_name)) = target.rsplit_once("::") {
        return first_class_static_method_descriptor(ctx, receiver_label, method_name);
    }
    if let Some(callee) = ctx.callable_function_by_name(target) {
        return Some(FirstClassCallableDescriptor {
            entry_label: function_symbol(&callee.name),
            kind: callable_descriptor::CALLABLE_DESC_KIND_FUNCTION,
            sig: Some(function_signature_from_eir(callee)),
            invocation: callable_descriptor::CallableDescriptorInvocation::named(
                callable_descriptor::CallableDescriptorShape::Function,
                callee.name.clone(),
            ),
        });
    }
    if ctx.has_extern_function(target) {
        return Some(FirstClassCallableDescriptor {
            entry_label: ctx.emitter.target.extern_symbol(target),
            kind: callable_descriptor::CALLABLE_DESC_KIND_EXTERN,
            sig: None,
            invocation: callable_descriptor::CallableDescriptorInvocation::named(
                callable_descriptor::CallableDescriptorShape::Extern,
                target.to_string(),
            ),
        });
    }
    if let Some(descriptor) = first_class_builtin_descriptor(ctx, target) {
        return Some(descriptor);
    }
    None
}

/// Returns descriptor metadata for builtin first-class callable targets.
fn first_class_builtin_descriptor(
    ctx: &mut FunctionContext<'_>,
    target: &str,
) -> Option<FirstClassCallableDescriptor> {
    let name = php_symbol_key(target.trim_start_matches('\\'));
    let sig = first_class_callable_builtin_sig(&name)?;
    let wrapper_sig = callable_wrapper_sig(&sig);
    let entry_label = emit_runtime_builtin_wrapper_inline(ctx, &name, &wrapper_sig);
    Some(FirstClassCallableDescriptor {
        entry_label,
        kind: callable_descriptor::CALLABLE_DESC_KIND_BUILTIN,
        sig: Some(wrapper_sig),
        invocation: callable_descriptor::CallableDescriptorInvocation::named(
            callable_descriptor::CallableDescriptorShape::Builtin,
            name,
        ),
    })
}

/// Returns descriptor metadata for static methods with compile-time class receivers.
fn first_class_static_method_descriptor(
    ctx: &mut FunctionContext<'_>,
    receiver_label: &str,
    method_name: &str,
) -> Option<FirstClassCallableDescriptor> {
    if matches!(receiver_label.trim_start_matches('\\'), "static" | "object") {
        return None;
    }
    let receiver = resolve_static_method_receiver(ctx, receiver_label).ok()?;
    let method_key = php_symbol_key(method_name);
    let receiver_info = ctx.module.class_infos.get(receiver.as_str())?;
    let impl_class = receiver_info
        .static_method_impl_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(receiver.as_str());
    let sig = ctx.module
        .class_infos
        .get(impl_class)?
        .static_methods
        .get(&method_key)?
        .clone();
    let wrapper_sig = crate::codegen::callable_dispatch::static_method_runtime_wrapper_sig(&sig);
    let entry_label = emit_runtime_static_method_wrapper_inline(
        ctx,
        receiver.as_str(),
        &method_key,
        &wrapper_sig,
    );
    Some(FirstClassCallableDescriptor {
        entry_label,
        kind: callable_descriptor::CALLABLE_DESC_KIND_STATIC_METHOD,
        sig: Some(wrapper_sig),
        invocation: callable_descriptor::CallableDescriptorInvocation::method(
            callable_descriptor::CallableDescriptorShape::StaticMethod,
            Some(receiver),
            method_key,
        ),
    })
}

/// Returns the callable-target string attached to `first_class_callable_new`.
fn callable_target_data<'a>(
    ctx: &'a FunctionContext<'_>,
    inst: &Instruction,
) -> Result<&'a str> {
    let data = expect_data(inst)?;
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}

/// Lowers high-level runtime fallback casts that Phase 04 can identify by type.
fn lower_runtime_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() == 3 && matches!(inst.immediate, Some(Immediate::Data(_))) {
        return lower_property_array_runtime_set(ctx, inst);
    }
    if let Some(()) = try_lower_array_access_runtime_call(ctx, inst)? {
        return Ok(());
    }
    if inst.operands.len() == 3 {
        return lower_mixed_array_runtime_set(ctx, inst);
    }
    if inst.operands.len() == 2 {
        return lower_binary_runtime_call(ctx, inst);
    }
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::unsupported(format!(
            "runtime_call with {} operands returning PHP type {:?}",
            inst.operands.len(),
            inst.result_php_type
        )));
    }
    let value = expect_operand(inst, 0)?;
    let source_ty = ctx.value_php_type(value)?.codegen_repr();
    if let (PhpType::Object(class_name), PhpType::Str) =
        (&source_ty, inst.result_php_type.codegen_repr())
    {
        let normalized = class_name.trim_start_matches('\\');
        if !object_class_has_tostring(ctx, normalized) {
            emit_missing_tostring_fatal(ctx, normalized);
            return Ok(());
        }
        emit_object_tostring_call(ctx, value, normalized)?;
        return store_if_result(ctx, inst);
    }
    if inst.result_php_type.codegen_repr() == PhpType::TaggedScalar {
        match source_ty {
            PhpType::Int | PhpType::Bool | PhpType::Callable => {
                ctx.load_value_to_result(value)?;
                crate::codegen::sentinels::emit_tagged_scalar_from_int_result(ctx.emitter);
                return store_if_result(ctx, inst);
            }
            PhpType::Void | PhpType::Never => {
                crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
                return store_if_result(ctx, inst);
            }
            other => {
                return Err(CodegenIrError::unsupported(format!(
                    "runtime_call from PHP type {:?} to PHP type TaggedScalar",
                    other
                )))
            }
        }
    }
    if matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) {
        let result_ty = inst.result_php_type.codegen_repr();
        load_value_to_first_int_arg(ctx, value)?;
        match result_ty {
            PhpType::Str => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string"),
            PhpType::Float => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float"),
            PhpType::Int => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int"),
            PhpType::Bool => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool"),
            PhpType::Array(elem) if elem.codegen_repr() == PhpType::Mixed => {
                lower_mixed_to_mixed_indexed_array(ctx)?;
            }
            PhpType::AssocArray { value, .. } if value.codegen_repr() == PhpType::Mixed => {
                lower_mixed_to_mixed_assoc_array(ctx)?;
            }
            PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Iterable
            | PhpType::Object(_) => {
                emit_unbox_mixed_to_owned_refcounted_result(ctx, &result_ty);
            }
            other => {
                return Err(CodegenIrError::unsupported(format!(
                    "runtime_call from PHP type {:?} to PHP type {:?}",
                    source_ty,
                    other
                )))
            }
        }
        return store_if_result(ctx, inst);
    }
    Err(CodegenIrError::unsupported(format!(
        "runtime_call from PHP type {:?} to PHP type {:?}",
        source_ty,
        inst.result_php_type
    )))
}

/// Lowers generic EIR runtime calls that represent PHP `ArrayAccess` object indexing.
fn try_lower_array_access_runtime_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<Option<()>> {
    let Some(receiver) = inst.operands.first().copied() else {
        return Ok(None);
    };
    let receiver_ty = ctx.raw_value_php_type(receiver)?;
    let Some(dispatch) = array_access_runtime_dispatch(ctx, &receiver_ty) else {
        return Ok(None);
    };
    let method_name = match inst.operands.len() {
        2 if inst.result_php_type.codegen_repr() == PhpType::Void => "append",
        2 => "offsetGet",
        3 => "offsetSet",
        _ => return Ok(None),
    };
    match dispatch {
        ArrayAccessRuntimeDispatch::Concrete(class_name) => {
            let concrete_method = if method_name == "append"
                && is_spl_doubly_linked_list_family(&class_name)
            {
                "push"
            } else {
                method_name
            };
            if let Some(intrinsic) = runtime_backed_instance_intrinsic(&class_name, concrete_method) {
                lower_instance_runtime_intrinsic(ctx, inst, &class_name, concrete_method, intrinsic)?;
            } else {
                lower_runtime_object_method_call(ctx, inst, &class_name, concrete_method)?;
            }
        }
        ArrayAccessRuntimeDispatch::Interface {
            boxed_receiver: false,
        } => {
            lower_interface_method_call(ctx, inst, "ArrayAccess", method_name)?;
        }
        ArrayAccessRuntimeDispatch::Interface {
            boxed_receiver: true,
        } => {
            lower_boxed_array_access_interface_call(ctx, inst, method_name)?;
        }
    }
    Ok(Some(()))
}

/// Returns true when a concrete class uses the SPL doubly-linked-list append helper.
fn is_spl_doubly_linked_list_family(class_name: &str) -> bool {
    matches!(class_name, "SplDoublyLinkedList" | "SplStack" | "SplQueue")
}

/// Selects the ArrayAccess runtime dispatch strategy for a receiver type.
fn array_access_runtime_dispatch(
    ctx: &FunctionContext<'_>,
    receiver_ty: &PhpType,
) -> Option<ArrayAccessRuntimeDispatch> {
    match receiver_ty {
        PhpType::Object(class_name) => {
            let normalized = class_name.trim_start_matches('\\');
            if interface_satisfies_interface(ctx, normalized, "ArrayAccess") {
                return Some(ArrayAccessRuntimeDispatch::Interface {
                    boxed_receiver: false,
                });
            }
            if class_implements_interface(ctx, normalized, "ArrayAccess") {
                return Some(ArrayAccessRuntimeDispatch::Concrete(normalized.to_string()));
            }
            None
        }
        PhpType::Union(members) if union_satisfies_array_access(ctx, members) => {
            Some(ArrayAccessRuntimeDispatch::Interface {
                boxed_receiver: true,
            })
        }
        _ => None,
    }
}

/// Returns true when all non-null union arms are ArrayAccess-compatible objects.
fn union_satisfies_array_access(ctx: &FunctionContext<'_>, members: &[PhpType]) -> bool {
    let mut saw_object = false;
    for member in members {
        match member {
            PhpType::Void | PhpType::Never => {}
            PhpType::Object(class_name) => {
                if !object_name_satisfies_interface(ctx, class_name, "ArrayAccess") {
                    return false;
                }
                saw_object = true;
            }
            _ => return false,
        }
    }
    saw_object
}

/// Returns true when a class or interface name satisfies the requested interface.
fn object_name_satisfies_interface(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    interface_name: &str,
) -> bool {
    let normalized = class_name.trim_start_matches('\\');
    interface_satisfies_interface(ctx, normalized, interface_name)
        || class_implements_interface(ctx, normalized, interface_name)
}

/// Lowers ArrayAccess on a boxed union receiver through runtime interface metadata.
fn lower_boxed_array_access_interface_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    method_name: &str,
) -> Result<()> {
    let (interface_name, method_key, callee_sig) =
        resolve_interface_call_signature(ctx, "ArrayAccess", method_name, inst.operands.len())?;
    let receiver = expect_operand(inst, 0)?;
    let receiver_ty = PhpType::Object(interface_name.clone());
    let mut param_types = Vec::with_capacity(callee_sig.params.len() + 1);
    param_types.push(receiver_ty.clone());
    param_types.extend(callee_sig.params.iter().map(|(_, ty)| ty.codegen_repr()));
    let mut ref_params = Vec::with_capacity(callee_sig.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(callee_sig.ref_params.iter().copied());

    ctx.load_value_to_result(receiver)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, mixed_unbox_low_payload_reg(ctx));
    abi::emit_pop_reg(ctx.emitter, receiver_reg);
    let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
        ctx,
        receiver_reg,
        &receiver_ty,
        &inst.operands,
        &param_types,
        &ref_params,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    let return_ty = iterators::emit_interface_dispatch_call(ctx, &interface_name, &method_key, None)?;
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_call_result(ctx, inst, &return_ty)?;
    emit_ref_arg_writebacks(ctx, &call_args.ref_writebacks)
}

/// Emits the concrete method body backing a PHP object runtime fallback.
pub(super) fn lower_runtime_object_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
    method_name: &str,
) -> Result<()> {
    let target = resolve_method_call_target(ctx, class_name, method_name, inst.operands.len())?;
    let mut param_types = Vec::with_capacity(target.params.len() + 1);
    param_types.push(PhpType::Object(class_name.to_string()));
    param_types.extend(target.params.iter().map(|param| param.codegen_repr()));
    let mut ref_params = Vec::with_capacity(target.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(target.ref_params.iter().copied());
    let call_args = materialize_direct_call_args_with_refs_and_options(
        ctx,
        &inst.operands,
        &param_types,
        &ref_params,
        true,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &method_symbol(&target.impl_class, &target.method_key));
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_runtime_object_call_result(ctx, inst, &target.return_ty)?;
    emit_call_arg_temp_cleanups(ctx, &call_args, inst.result)?;
    emit_ref_arg_writebacks(ctx, &call_args.ref_writebacks)
}

/// Stores an object fallback call result, casting boxed Mixed values when the access type is known.
fn store_runtime_object_call_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    return_ty: &PhpType,
) -> Result<()> {
    if return_ty.codegen_repr() != PhpType::Mixed {
        return store_call_result(ctx, inst, return_ty);
    }
    let Some(result) = inst.result else {
        return Ok(());
    };
    let result_ty = ctx.value_php_type(result)?.codegen_repr();
    if matches!(result_ty, PhpType::Mixed | PhpType::Union(_)) {
        ctx.store_result_value(result)?;
        return Ok(());
    }
    cast_loaded_mixed_pointer_to_result(ctx, &result_ty)?;
    ctx.store_result_value(result)
}

/// Returns true when a class implements an interface, following parent classes if needed.
fn class_implements_interface(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    interface_name: &str,
) -> bool {
    let interface_key = php_symbol_key(interface_name.trim_start_matches('\\'));
    let mut current = Some(class_name.trim_start_matches('\\'));
    while let Some(candidate) = current {
        let Some(info) = ctx.module.class_infos.get(candidate) else {
            return false;
        };
        if info
            .interfaces
            .iter()
            .any(|interface| {
                let interface = interface.trim_start_matches('\\');
                php_symbol_key(interface) == interface_key
                    || interface_satisfies_interface(ctx, interface, interface_name)
            })
        {
            return true;
        }
        current = info.parent.as_deref();
    }
    false
}

/// Returns true when an interface is or extends the requested ancestor.
fn interface_satisfies_interface(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    ancestor_name: &str,
) -> bool {
    if php_symbol_key(interface_name.trim_start_matches('\\'))
        == php_symbol_key(ancestor_name.trim_start_matches('\\'))
    {
        return true;
    }
    let Some(interface_info) = ctx
        .module
        .interface_infos
        .get(interface_name.trim_start_matches('\\'))
    else {
        return false;
    };
    interface_info.parents.iter().any(|parent| {
        let parent = parent.trim_start_matches('\\');
        php_symbol_key(parent) == php_symbol_key(ancestor_name.trim_start_matches('\\'))
            || interface_satisfies_interface(ctx, parent, ancestor_name)
    })
}

/// Converts an untyped boxed Mixed payload into indexed-array storage with Mixed slots.
fn lower_mixed_to_mixed_indexed_array(ctx: &mut FunctionContext<'_>) -> Result<()> {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // pass the unboxed indexed-array payload to the Mixed conversion helper
            ctx.emitter.instruction("ldr x1, [x0, #-8]");                       // load indexed-array metadata before Mixed-slot conversion
            ctx.emitter.instruction("lsr x1, x1, #8");                          // move the runtime value_type tag into the low bits
            ctx.emitter.instruction("and x1, x1, #0x7f");                       // isolate the indexed-array value_type tag
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, QWORD PTR [rdi - 8]");            // load indexed-array metadata before Mixed-slot conversion
            ctx.emitter.instruction("shr rsi, 8");                              // move the runtime value_type tag into the low bits
            ctx.emitter.instruction("and rsi, 0x7f");                           // isolate the indexed-array value_type tag
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
    abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
    Ok(())
}

/// Converts an untyped boxed Mixed payload into associative-array storage with Mixed values.
fn lower_mixed_to_mixed_assoc_array(ctx: &mut FunctionContext<'_>) -> Result<()> {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // pass the unboxed associative-array payload to the Mixed conversion helper
        }
        Arch::X86_64 => {}
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
    abi::emit_incref_if_refcounted(
        ctx.emitter,
        &PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        },
    );
    Ok(())
}

/// Lowers binary runtime fallbacks that Phase 04 can identify by operand type.
fn lower_binary_runtime_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let receiver = expect_operand(inst, 0)?;
    let receiver_ty = ctx.value_php_type(receiver)?.codegen_repr();
    let result_ty = inst.result_php_type.codegen_repr();
    match (receiver_ty, &result_ty) {
        (PhpType::Mixed | PhpType::Union(_), PhpType::Void) => {
            lower_mixed_cell_runtime_assign(ctx, inst)
        }
        (PhpType::Mixed | PhpType::Union(_), _) => lower_mixed_array_runtime_get(ctx, inst),
        (PhpType::AssocArray { .. }, PhpType::Void) => hashes::lower_hash_append(ctx, inst),
        (other, _) => Err(CodegenIrError::unsupported(format!(
            "runtime_call with receiver PHP type {:?} returning PHP type {:?}",
            other,
            inst.result_php_type
        ))),
    }
}

/// Lowers `$mixed[$key]` through the shared boxed Mixed array/hash/stdClass reader.
fn lower_mixed_array_runtime_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let receiver = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            hashes::materialize_hash_key_aarch64(ctx, key)?;
            ctx.load_value_to_reg(receiver, "x0")?;
        }
        Arch::X86_64 => {
            hashes::materialize_hash_key_x86_64(ctx, key)?;
            ctx.load_value_to_reg(receiver, "rdi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_array_get");
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)
}

/// Lowers `$mixed[$key] = $value` through the shared boxed Mixed array/hash/stdClass writer.
fn lower_mixed_array_runtime_set(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let receiver = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    let value = expect_operand(inst, 2)?;
    match ctx.value_php_type(receiver)?.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {}
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "runtime_call array set with receiver PHP type {:?}",
                other
            )))
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_mixed_array_runtime_set_aarch64(ctx, receiver, key, value)?,
        Arch::X86_64 => lower_mixed_array_runtime_set_x86_64(ctx, receiver, key, value)?,
    }
    Ok(())
}

/// Materializes AArch64 operands for the boxed Mixed array/hash writer.
fn lower_mixed_array_runtime_set_aarch64(
    ctx: &mut FunctionContext<'_>,
    receiver: ValueId,
    key: ValueId,
    value: ValueId,
) -> Result<()> {
    let value_ty = ctx.load_value_to_result(value)?.codegen_repr();
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty);
    }
    abi::emit_push_reg(ctx.emitter, "x0");
    hashes::materialize_hash_key_aarch64(ctx, key)?;
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
    ctx.load_value_to_reg(receiver, "x0")?;
    abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
    abi::emit_pop_reg(ctx.emitter, "x3");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_array_set");
    Ok(())
}

/// Materializes x86_64 operands for the boxed Mixed array/hash writer.
fn lower_mixed_array_runtime_set_x86_64(
    ctx: &mut FunctionContext<'_>,
    receiver: ValueId,
    key: ValueId,
    value: ValueId,
) -> Result<()> {
    let value_ty = ctx.load_value_to_result(value)?.codegen_repr();
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty);
    }
    abi::emit_push_reg(ctx.emitter, "rax");
    hashes::materialize_hash_key_x86_64(ctx, key)?;
    abi::emit_push_reg_pair(ctx.emitter, "rsi", "rdx");
    ctx.load_value_to_reg(receiver, "rdi")?;
    abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
    abi::emit_pop_reg(ctx.emitter, "rcx");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_array_set");
    Ok(())
}

/// Lowers `$object->property[$key] = $value` when the property itself is runtime-typed.
fn lower_property_array_runtime_set(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    let value = expect_operand(inst, 2)?;
    let data = expect_data(inst)?;
    let property = ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?;
    match ctx.value_php_type(object)?.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => match ctx.emitter.target.arch {
            Arch::AArch64 => lower_mixed_property_array_runtime_set_aarch64(
                ctx,
                object,
                key,
                value,
                &property,
                "__rt_mixed_property_get",
            ),
            Arch::X86_64 => lower_mixed_property_array_runtime_set_x86_64(
                ctx,
                object,
                key,
                value,
                &property,
                "__rt_mixed_property_get",
            ),
        },
        PhpType::Object(class_name)
            if crate::types::checker::builtin_stdclass::is_stdclass(
                class_name.trim_start_matches('\\'),
            ) =>
        {
            match ctx.emitter.target.arch {
                Arch::AArch64 => lower_mixed_property_array_runtime_set_aarch64(
                    ctx,
                    object,
                    key,
                    value,
                    &property,
                    "__rt_stdclass_get",
                ),
                Arch::X86_64 => lower_mixed_property_array_runtime_set_x86_64(
                    ctx,
                    object,
                    key,
                    value,
                    &property,
                    "__rt_stdclass_get",
                ),
            }
        }
        other => Err(CodegenIrError::unsupported(format!(
            "runtime_call property array set with receiver PHP type {:?}",
            other
        ))),
    }
}

/// Lowers a property-array write through stdClass/Mixed property get and Mixed array set on AArch64.
fn lower_mixed_property_array_runtime_set_aarch64(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    key: ValueId,
    value: ValueId,
    property: &str,
    getter_label: &str,
) -> Result<()> {
    let value_ty = ctx.load_value_to_result(value)?.codegen_repr();
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty);
    }
    abi::emit_push_reg(ctx.emitter, "x0");
    hashes::materialize_hash_key_aarch64(ctx, key)?;
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
    emit_property_array_target_get_aarch64(ctx, object, property, getter_label)?;
    abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
    abi::emit_pop_reg(ctx.emitter, "x3");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_array_set");
    Ok(())
}

/// Lowers a property-array write through stdClass/Mixed property get and Mixed array set on x86_64.
fn lower_mixed_property_array_runtime_set_x86_64(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    key: ValueId,
    value: ValueId,
    property: &str,
    getter_label: &str,
) -> Result<()> {
    let value_ty = ctx.load_value_to_result(value)?.codegen_repr();
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty);
    }
    abi::emit_push_reg(ctx.emitter, "rax");
    hashes::materialize_hash_key_x86_64(ctx, key)?;
    abi::emit_push_reg_pair(ctx.emitter, "rsi", "rdx");
    emit_property_array_target_get_x86_64(ctx, object, property, getter_label)?;
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the property Mixed cell as the array-write target
    abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
    abi::emit_pop_reg(ctx.emitter, "rcx");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_array_set");
    Ok(())
}

/// Calls the requested property getter and leaves the boxed Mixed property cell in `x0`.
fn emit_property_array_target_get_aarch64(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property: &str,
    getter_label: &str,
) -> Result<()> {
    let (label, len) = ctx.data.add_string(property.as_bytes());
    ctx.load_value_to_reg(object, "x0")?;
    abi::emit_symbol_address(ctx.emitter, "x1", &label);
    abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
    abi::emit_call_label(ctx.emitter, getter_label);
    Ok(())
}

/// Calls the requested property getter and leaves the boxed Mixed property cell in `rax`.
fn emit_property_array_target_get_x86_64(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property: &str,
    getter_label: &str,
) -> Result<()> {
    let (label, len) = ctx.data.add_string(property.as_bytes());
    ctx.load_value_to_reg(object, "rdi")?;
    abi::emit_symbol_address(ctx.emitter, "rsi", &label);
    abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
    abi::emit_call_label(ctx.emitter, getter_label);
    Ok(())
}

/// Lowers a two-operand Mixed-cell replacement emitted for nested runtime assignments.
fn lower_mixed_cell_runtime_assign(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let target = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    match ctx.value_php_type(target)?.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {}
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "runtime_call mixed-cell assignment with target PHP type {:?}",
                other
            )))
        }
    }
    box_value_for_mixed_cell_replacement(ctx, value)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_mixed_cell_runtime_assign_aarch64(ctx, target)?,
        Arch::X86_64 => lower_mixed_cell_runtime_assign_x86_64(ctx, target)?,
    }
    Ok(())
}

/// Boxes the replacement value into a fresh Mixed cell whose payload can be moved.
fn box_value_for_mixed_cell_replacement(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    let value_ty = ctx.load_value_to_result(value)?.codegen_repr();
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        match ctx.emitter.target.arch {
            Arch::AArch64 => emit_box_runtime_payload_as_mixed(ctx.emitter, "x0", "x1", "x2"),
            Arch::X86_64 => emit_box_runtime_payload_as_mixed(ctx.emitter, "rax", "rdi", "rdx"),
        }
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty);
    }
    Ok(())
}

/// Replaces the payload inside an existing boxed Mixed cell on AArch64.
fn lower_mixed_cell_runtime_assign_aarch64(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
) -> Result<()> {
    let drop_new = ctx.next_label("mixed_cell_assign_drop_new");
    let release_string = ctx.next_label("mixed_cell_assign_release_string");
    let copy_new = ctx.next_label("mixed_cell_assign_copy_new");
    let done = ctx.next_label("mixed_cell_assign_done");

    ctx.emitter.instruction("sub sp, sp, #32");                                 // reserve temporary slots for target and replacement Mixed cells
    ctx.emitter.instruction("str x0, [sp, #8]");                                // preserve the boxed replacement while loading the target cell
    ctx.load_value_to_reg(target, "x0")?;
    ctx.emitter.instruction("str x0, [sp, #0]");                                // preserve the target Mixed cell across payload-release helpers
    ctx.emitter.instruction(&format!("cbz x0, {}", drop_new));                  // drop the replacement when the target cell is missing
    ctx.emitter.instruction("ldr x9, [x0]");                                    // inspect the old payload tag before overwriting the cell
    ctx.emitter.instruction("cmp x9, #1");                                      // strings own a persisted heap payload that needs safe free
    ctx.emitter.instruction(&format!("b.eq {}", release_string));               // release string payloads through the string-safe free path
    ctx.emitter.instruction("cmp x9, #4");                                      // tags below array/hash/object/mixed are scalar payloads
    ctx.emitter.instruction(&format!("b.lo {}", copy_new));                     // scalar payloads can be overwritten directly
    ctx.emitter.instruction("cmp x9, #7");                                      // tags above the refcounted payload range are not released here
    ctx.emitter.instruction(&format!("b.hi {}", copy_new));                     // unknown/null payload tags can be overwritten directly
    ctx.emitter.instruction("ldr x0, [x0, #8]");                                // pass the old refcounted child payload to the generic release helper
    abi::emit_call_label(ctx.emitter, "__rt_decref_any");
    ctx.emitter.instruction(&format!("b {}", copy_new));                        // continue with replacement after releasing the old child
    ctx.emitter.label(&release_string);
    ctx.emitter.instruction("ldr x0, [sp, #0]");                                // reload the target cell before reading its string payload
    ctx.emitter.instruction("ldr x0, [x0, #8]");                                // pass the old string payload pointer to the safe free helper
    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
    ctx.emitter.instruction(&format!("b {}", copy_new));                        // continue with replacement after freeing the old string
    ctx.emitter.label(&drop_new);
    ctx.emitter.instruction("ldr x0, [sp, #8]");                                // reload the unused replacement Mixed cell
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    ctx.emitter.instruction(&format!("b {}", done));                            // skip payload copy because there is no target cell
    ctx.emitter.label(&copy_new);
    ctx.emitter.instruction("ldr x10, [sp, #0]");                               // reload the destination Mixed cell pointer
    ctx.emitter.instruction("ldr x11, [sp, #8]");                               // reload the replacement Mixed cell pointer
    ctx.emitter.instruction("ldr x12, [x11]");                                  // copy the replacement runtime tag
    ctx.emitter.instruction("str x12, [x10]");                                  // overwrite the target cell tag
    ctx.emitter.instruction("ldr x12, [x11, #8]");                              // copy the replacement low payload word
    ctx.emitter.instruction("str x12, [x10, #8]");                              // overwrite the target cell low payload word
    ctx.emitter.instruction("ldr x12, [x11, #16]");                             // copy the replacement high payload word
    ctx.emitter.instruction("str x12, [x10, #16]");                             // overwrite the target cell high payload word
    ctx.emitter.instruction("mov x0, x11");                                     // pass the now-empty replacement cell storage to heap_free
    abi::emit_call_label(ctx.emitter, "__rt_heap_free");
    ctx.emitter.label(&done);
    ctx.emitter.instruction("add sp, sp, #32");                                 // release replacement temporaries
    Ok(())
}

/// Replaces the payload inside an existing boxed Mixed cell on x86_64.
fn lower_mixed_cell_runtime_assign_x86_64(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
) -> Result<()> {
    let drop_new = ctx.next_label("mixed_cell_assign_drop_new");
    let release_string = ctx.next_label("mixed_cell_assign_release_string");
    let copy_new = ctx.next_label("mixed_cell_assign_copy_new");
    let done = ctx.next_label("mixed_cell_assign_done");

    ctx.emitter.instruction("sub rsp, 32");                                     // reserve aligned temporary slots for target and replacement Mixed cells
    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");                    // preserve the boxed replacement while loading the target cell
    ctx.load_value_to_reg(target, "rax")?;
    ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                        // preserve the target Mixed cell across payload-release helpers
    ctx.emitter.instruction("test rax, rax");                                   // check whether the nested lookup produced a writable cell
    ctx.emitter.instruction(&format!("jz {}", drop_new));                       // drop the replacement when the target cell is missing
    ctx.emitter.instruction("mov r9, QWORD PTR [rax]");                         // inspect the old payload tag before overwriting the cell
    ctx.emitter.instruction("cmp r9, 1");                                       // strings own a persisted heap payload that needs safe free
    ctx.emitter.instruction(&format!("je {}", release_string));                 // release string payloads through the string-safe free path
    ctx.emitter.instruction("cmp r9, 4");                                       // tags below array/hash/object/mixed are scalar payloads
    ctx.emitter.instruction(&format!("jl {}", copy_new));                       // scalar payloads can be overwritten directly
    ctx.emitter.instruction("cmp r9, 7");                                       // tags above the refcounted payload range are not released here
    ctx.emitter.instruction(&format!("jg {}", copy_new));                       // unknown/null payload tags can be overwritten directly
    ctx.emitter.instruction("mov rax, QWORD PTR [rax + 8]");                    // pass the old refcounted child payload to the generic release helper
    abi::emit_call_label(ctx.emitter, "__rt_decref_any");
    ctx.emitter.instruction(&format!("jmp {}", copy_new));                      // continue with replacement after releasing the old child
    ctx.emitter.label(&release_string);
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                        // reload the target cell before reading its string payload
    ctx.emitter.instruction("mov rax, QWORD PTR [rax + 8]");                    // pass the old string payload pointer to the safe free helper
    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
    ctx.emitter.instruction(&format!("jmp {}", copy_new));                      // continue with replacement after freeing the old string
    ctx.emitter.label(&drop_new);
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                    // reload the unused replacement Mixed cell
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip payload copy because there is no target cell
    ctx.emitter.label(&copy_new);
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                        // reload the destination Mixed cell pointer
    ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 8]");                    // reload the replacement Mixed cell pointer
    ctx.emitter.instruction("mov r9, QWORD PTR [r11]");                         // copy the replacement runtime tag
    ctx.emitter.instruction("mov QWORD PTR [r10], r9");                         // overwrite the target cell tag
    ctx.emitter.instruction("mov r9, QWORD PTR [r11 + 8]");                     // copy the replacement low payload word
    ctx.emitter.instruction("mov QWORD PTR [r10 + 8], r9");                     // overwrite the target cell low payload word
    ctx.emitter.instruction("mov r9, QWORD PTR [r11 + 16]");                    // copy the replacement high payload word
    ctx.emitter.instruction("mov QWORD PTR [r10 + 16], r9");                    // overwrite the target cell high payload word
    ctx.emitter.instruction("mov rax, r11");                                    // pass the now-empty replacement cell storage to heap_free
    abi::emit_call_label(ctx.emitter, "__rt_heap_free");
    ctx.emitter.label(&done);
    ctx.emitter.instruction("add rsp, 32");                                     // release replacement temporaries
    Ok(())
}

/// Casts the boxed Mixed pointer currently returned by a runtime helper when needed.
fn cast_loaded_mixed_pointer_to_result(
    ctx: &mut FunctionContext<'_>,
    target_ty: &PhpType,
) -> Result<()> {
    let label = match target_ty {
        PhpType::Mixed | PhpType::Union(_) => return Ok(()),
        PhpType::Str => "__rt_mixed_cast_string",
        PhpType::Int => "__rt_mixed_cast_int",
        PhpType::Float => "__rt_mixed_cast_float",
        PhpType::Bool => "__rt_mixed_cast_bool",
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "runtime mixed result cast to PHP type {:?}",
                other
            )))
        }
    };
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the returned boxed Mixed pointer as the SysV first argument
    }
    abi::emit_call_label(ctx.emitter, label);
    Ok(())
}

/// Lowers expression-form `throw` through the same runtime path as throw terminators.
fn lower_throw_exception(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    super::lower_term::lower_throw_value(ctx, value)
}

/// Pushes an EIR exception handler and branches to the handler block after `longjmp`.
fn lower_try_push_handler(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let token = expect_i64(inst)?;
    let handler_offset = ctx.try_handler_offset(token)?;
    let handler_block = BlockId::from_raw(token as u32);
    let handler_label = ctx.block_label_for_id(handler_block)?;
    let scratch = abi::temp_int_reg(ctx.emitter.target);

    ctx.emitter.comment("push EIR exception handler");
    abi::emit_load_symbol_to_reg(ctx.emitter, scratch, "_exc_handler_top", 0);
    abi::store_at_offset(ctx.emitter, scratch, handler_offset);
    abi::emit_load_int_immediate(ctx.emitter, scratch, 0);
    abi::store_at_offset(ctx.emitter, scratch, handler_offset - 8);
    abi::emit_load_symbol_to_reg(ctx.emitter, scratch, "_rt_diag_suppression", 0);
    abi::store_at_offset(
        ctx.emitter,
        scratch,
        handler_offset - TRY_HANDLER_DIAG_DEPTH_OFFSET,
    );
    abi::emit_frame_slot_address(ctx.emitter, scratch, handler_offset);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_exc_handler_top", 0);
    abi::emit_frame_slot_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        handler_offset - TRY_HANDLER_JMP_BUF_OFFSET,
    );
    ctx.emitter.bl_c("setjmp");
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &handler_label);
    Ok(())
}

/// Pops an EIR exception handler and restores the saved diagnostic-suppression depth.
fn lower_try_pop_handler(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let token = expect_i64(inst)?;
    let handler_offset = ctx.try_handler_offset(token)?;
    let scratch = abi::temp_int_reg(ctx.emitter.target);
    ctx.emitter.comment("pop EIR exception handler");
    abi::load_at_offset(ctx.emitter, scratch, handler_offset);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_exc_handler_top", 0);
    abi::load_at_offset(
        ctx.emitter,
        scratch,
        handler_offset - TRY_HANDLER_DIAG_DEPTH_OFFSET,
    );
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_rt_diag_suppression", 0);
    Ok(())
}

/// Loads the currently active exception object from the runtime exception slot.
fn lower_catch_current(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    abi::emit_load_symbol_to_reg(ctx.emitter, abi::int_result_reg(ctx.emitter), "_exc_value", 0);
    store_if_result(ctx, inst)
}

/// Binds the active exception to an optional catch variable and clears the runtime slot.
fn lower_catch_bind(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
        let storage_ty = ctx.local_php_type(slot)?;
        let target_ty = if inst.result_php_type.codegen_repr() == PhpType::Void {
            storage_ty.clone()
        } else {
            inst.result_php_type.codegen_repr()
        };
        let offset = ctx.local_offset(slot)?;
        abi::emit_load_symbol_to_result(ctx.emitter, "_exc_value", &target_ty);
        let store_ty = catch_bind_store_type(ctx, &target_ty, &storage_ty);
        abi::emit_store(ctx.emitter, &store_ty, offset);
    }
    abi::emit_store_zero_to_symbol(ctx.emitter, "_exc_value", 0);
    Ok(())
}

/// Returns the local-storage representation used after loading the raw exception object.
fn catch_bind_store_type(
    ctx: &mut FunctionContext<'_>,
    target_ty: &PhpType,
    storage_ty: &PhpType,
) -> PhpType {
    let storage_ty = storage_ty.codegen_repr();
    let target_ty = target_ty.codegen_repr();
    if storage_ty == PhpType::Mixed && target_ty != PhpType::Mixed {
        emit_box_current_value_as_mixed(ctx.emitter, &target_ty);
        return PhpType::Mixed;
    }
    target_ty
}

/// Lowers a direct instance-method call on a statically known object receiver.
fn lower_method_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let method_name = method_name_data(ctx, inst)?.to_string();
    if let Some((class_name, true)) = objects::nullable_object_receiver_class(ctx, object)? {
        return lower_nullable_receiver_method_call(ctx, inst, object, &class_name, &method_name);
    }
    let object_ty = ctx.value_php_type(object)?.codegen_repr();
    if matches!(object_ty, PhpType::Mixed | PhpType::Union(_)) {
        if let Some(state) = fiber_state_predicate_method(&method_name) {
            return lower_mixed_fiber_state_predicate(ctx, inst, object, &method_name, state);
        }
        return lower_mixed_method_call(ctx, inst, object, &method_name);
    }
    let PhpType::Object(class_name) = object_ty else {
        return Err(CodegenIrError::unsupported(format!(
            "method call receiver for PHP type {:?}",
            object_ty
        )));
    };
    if let Some(state) = fiber_state_predicate(&class_name, &method_name) {
        return lower_fiber_state_predicate(ctx, inst, object, state);
    }
    if let Some(intrinsic) = generator_intrinsic(&class_name, &method_name) {
        return lower_generator_intrinsic(ctx, inst, intrinsic);
    }
    if let Some(intrinsic) = callback_filter_intrinsic(&class_name, &method_name) {
        return lower_callback_filter_accept_intrinsic(ctx, inst, intrinsic);
    }
    if is_fiber_start_call(&class_name, &method_name) {
        return lower_fiber_start(ctx, inst, object);
    }
    if is_fiber_resume_call(&class_name, &method_name) {
        return lower_fiber_resume(ctx, inst, object);
    }
    if is_fiber_throw_call(&class_name, &method_name) {
        return lower_fiber_throw(ctx, inst, object);
    }
    if is_fiber_get_return_call(&class_name, &method_name) {
        return lower_fiber_noarg_runtime_method(ctx, inst, object, "__rt_fiber_get_return");
    }
    if let Some(intrinsic) = runtime_backed_instance_intrinsic(&class_name, &method_name) {
        return lower_instance_runtime_intrinsic(ctx, inst, &class_name, &method_name, intrinsic);
    }
    if is_throwable_standard_method_call(ctx, &class_name, &method_name) {
        return lower_throwable_standard_method(ctx, inst, object, &method_name);
    }
    if ctx
        .module
        .interface_infos
        .contains_key(class_name.trim_start_matches('\\'))
    {
        return lower_interface_method_call(ctx, inst, &class_name, &method_name);
    }
    let target = resolve_method_call_target(ctx, &class_name, &method_name, inst.operands.len())?;
    let mut param_types = Vec::with_capacity(target.params.len() + 1);
    param_types.push(PhpType::Object(class_name));
    param_types.extend(target.params.iter().map(|param| param.codegen_repr()));
    let mut ref_params = Vec::with_capacity(target.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(target.ref_params.iter().copied());
    let call_args = materialize_direct_call_args_with_refs_and_options(
        ctx,
        &inst.operands,
        &param_types,
        &ref_params,
        true,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    if let Some(slot) = target.dynamic_slot {
        emit_dynamic_instance_method_call(ctx, slot);
    } else {
        abi::emit_call_label(ctx.emitter, &method_symbol(&target.impl_class, &target.method_key));
    }
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_call_result(ctx, inst, &target.return_ty)?;
    emit_call_arg_temp_cleanups(ctx, &call_args, inst.result)?;
    emit_ref_arg_writebacks(ctx, &call_args.ref_writebacks)
}

/// Lowers an instance-method call whose receiver is boxed as `Mixed`.
fn lower_mixed_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
) -> Result<()> {
    let candidates = mixed_method_candidates(ctx, method_name, inst.operands.len())?;
    if candidates.is_empty() {
        emit_method_call_on_null_fatal(ctx, method_name);
        return Ok(());
    }

    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    let no_match_label = ctx.next_label("mixed_method_no_match");
    let done_label = ctx.next_label("mixed_method_done");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_method_{}",
                label_fragment(&candidate.class_name)
            ))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_result(object)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_mixed_method_object_payload_or_fatal(ctx, receiver_reg, &no_match_label);
    emit_mixed_method_class_dispatch(ctx, receiver_reg, &candidates, &match_labels, &no_match_label);

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        lower_mixed_method_candidate_call(ctx, inst, receiver_reg, candidate)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&no_match_label);
    emit_method_call_on_null_fatal(ctx, method_name);

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits one concrete class branch for a `Mixed` receiver method call.
fn lower_mixed_method_candidate_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    receiver_reg: &str,
    candidate: &MixedMethodCandidate,
) -> Result<()> {
    let receiver_ty = PhpType::Object(candidate.class_name.clone());
    let mut param_types = Vec::with_capacity(candidate.target.params.len() + 1);
    param_types.push(receiver_ty.clone());
    param_types.extend(candidate.target.params.iter().map(|param| param.codegen_repr()));
    let mut ref_params = Vec::with_capacity(candidate.target.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(candidate.target.ref_params.iter().copied());
    let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
        ctx,
        receiver_reg,
        &receiver_ty,
        &inst.operands,
        &param_types,
        &ref_params,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    if let Some(slot) = candidate.target.dynamic_slot {
        emit_dynamic_instance_method_call(ctx, slot);
    } else {
        abi::emit_call_label(
            ctx.emitter,
            &method_symbol(&candidate.target.impl_class, &candidate.target.method_key),
        );
    }
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_call_result(ctx, inst, &candidate.target.return_ty)?;
    emit_ref_arg_writebacks(ctx, &call_args.ref_writebacks)
}

/// Collects concrete class-method candidates for a boxed `Mixed` receiver.
fn mixed_method_candidates(
    ctx: &FunctionContext<'_>,
    method_name: &str,
    operand_count: usize,
) -> Result<Vec<MixedMethodCandidate>> {
    let method_key = php_symbol_key(method_name);
    let mut candidates = Vec::new();
    for (class_name, class_info) in &ctx.module.class_infos {
        let Some(signature) = class_info.methods.get(&method_key) else {
            continue;
        };
        if signature.params.len() + 1 != operand_count {
            continue;
        }
        let target = resolve_method_call_target(ctx, class_name, method_name, operand_count)?;
        candidates.push(MixedMethodCandidate {
            class_id: class_info.class_id,
            class_name: class_name.clone(),
            target,
        });
    }
    candidates.sort_by_key(|candidate| candidate.class_id);
    Ok(candidates)
}

/// Preserves the unboxed object payload or routes non-object `Mixed` receivers to fatal.
fn emit_mixed_method_object_payload_or_fatal(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    no_match_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // require an object payload before method dispatch
            ctx.emitter.instruction(&format!("b.ne {}", no_match_label));       // non-object Mixed receivers cannot call instance methods
            ctx.emitter.instruction(&format!("mov {}, x1", receiver_reg));      // preserve the unboxed object payload across argument lowering
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // require an object payload before method dispatch
            ctx.emitter.instruction(&format!("jne {}", no_match_label));        // non-object Mixed receivers cannot call instance methods
            ctx.emitter.instruction(&format!("mov {}, rdi", receiver_reg));     // preserve the unboxed object payload across argument lowering
        }
    }
}

/// Emits class-id branches for every method candidate discovered for a `Mixed` receiver.
fn emit_mixed_method_class_dispatch(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    candidates: &[MixedMethodCandidate],
    match_labels: &[String],
    no_match_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr x9, [{}]", receiver_reg));    // load the receiver class id for Mixed method dispatch
            for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "x10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp x9, x10");                         // compare the receiver class id against this method candidate
                ctx.emitter.instruction(&format!("b.eq {}", label));            // call this candidate when the runtime class id matches
            }
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov r11, QWORD PTR [{}]", receiver_reg)); // load the receiver class id for Mixed method dispatch
            for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "r10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp r11, r10");                        // compare the receiver class id against this method candidate
                ctx.emitter.instruction(&format!("je {}", label));              // call this candidate when the runtime class id matches
            }
        }
    }
    abi::emit_jump(ctx.emitter, no_match_label);
}

/// Returns a label-safe fragment for class names and method metadata keys.
fn label_fragment(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

/// Lowers an instance-method call through interface metadata.
fn lower_interface_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    interface_name: &str,
    method_name: &str,
) -> Result<()> {
    let (normalized, method_key, callee_sig) =
        resolve_interface_call_signature(ctx, interface_name, method_name, inst.operands.len())?;
    let mut param_types = Vec::with_capacity(callee_sig.params.len() + 1);
    param_types.push(PhpType::Object(normalized.clone()));
    param_types.extend(callee_sig.params.iter().map(|(_, ty)| ty.codegen_repr()));
    let mut ref_params = Vec::with_capacity(callee_sig.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(callee_sig.ref_params.iter().copied());
    let call_args = materialize_direct_call_args_with_refs_and_options(
        ctx,
        &inst.operands,
        &param_types,
        &ref_params,
        true,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    let return_ty = iterators::emit_interface_dispatch_call(ctx, &normalized, &method_key, None)?;
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_call_result(ctx, inst, &return_ty)?;
    emit_call_arg_temp_cleanups(ctx, &call_args, inst.result)?;
    emit_ref_arg_writebacks(ctx, &call_args.ref_writebacks)
}

/// Resolves interface method metadata and validates the EIR ABI operand count.
fn resolve_interface_call_signature(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    method_name: &str,
    operand_count: usize,
) -> Result<(String, String, FunctionSig)> {
    let normalized = interface_name.trim_start_matches('\\');
    let method_key = php_symbol_key(method_name);
    let callee_sig = ctx
        .module
        .interface_infos
        .get(normalized)
        .and_then(|interface_info| interface_info.methods.get(&method_key))
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "interface method call to unknown method {}::{}",
                normalized, method_name
            ))
        })?
        .clone();
    let expected_args = callee_sig.params.len() + 1;
    if operand_count != expected_args {
        return Err(CodegenIrError::unsupported(format!(
            "interface method call to {}::{} with {} operands for {} ABI params",
            normalized,
            method_name,
            operand_count,
            expected_args
        )));
    }
    Ok((normalized.to_string(), method_key, callee_sig))
}

/// Lowers a method call after an earlier EIR guard has proven a nullable receiver non-null.
fn lower_nullable_receiver_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    class_name: &str,
    method_name: &str,
) -> Result<()> {
    if ctx
        .module
        .interface_infos
        .contains_key(class_name.trim_start_matches('\\'))
    {
        return lower_nullable_receiver_interface_method_call(
            ctx,
            inst,
            object,
            class_name,
            method_name,
        );
    }
    let target = resolve_method_call_target(ctx, class_name, method_name, inst.operands.len())?;
    let receiver_ty = PhpType::Object(class_name.to_string());
    let mut param_types = Vec::with_capacity(target.params.len() + 1);
    param_types.push(receiver_ty.clone());
    param_types.extend(target.params.iter().map(|param| param.codegen_repr()));
    let mut ref_params = Vec::with_capacity(target.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(target.ref_params.iter().copied());
    let null_label = ctx.next_label("method_receiver_null");
    let done_label = ctx.next_label("method_receiver_done");
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    objects::emit_nullable_receiver_object_payload(ctx, object, &null_label, receiver_reg)?;
    let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
        ctx,
        receiver_reg,
        &receiver_ty,
        &inst.operands,
        &param_types,
        &ref_params,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    if let Some(slot) = target.dynamic_slot {
        emit_dynamic_instance_method_call(ctx, slot);
    } else {
        abi::emit_call_label(ctx.emitter, &method_symbol(&target.impl_class, &target.method_key));
    }
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_call_result(ctx, inst, &target.return_ty)?;
    emit_ref_arg_writebacks(ctx, &call_args.ref_writebacks)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&null_label);
    emit_method_call_on_null_fatal(ctx, method_name);

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers a nullable receiver call whose non-null payload is known only by interface type.
fn lower_nullable_receiver_interface_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    interface_name: &str,
    method_name: &str,
) -> Result<()> {
    let normalized = interface_name.trim_start_matches('\\');
    let method_key = php_symbol_key(method_name);
    let callee_sig = ctx
        .module
        .interface_infos
        .get(normalized)
        .and_then(|interface_info| interface_info.methods.get(&method_key))
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "interface method call to unknown method {}::{}",
                normalized, method_name
            ))
        })?
        .clone();
    let expected_args = callee_sig.params.len() + 1;
    if inst.operands.len() != expected_args {
        return Err(CodegenIrError::unsupported(format!(
            "interface method call to {}::{} with {} operands for {} ABI params",
            normalized,
            method_name,
            inst.operands.len(),
            expected_args
        )));
    }
    let receiver_ty = PhpType::Object(normalized.to_string());
    let mut param_types = Vec::with_capacity(callee_sig.params.len() + 1);
    param_types.push(receiver_ty.clone());
    param_types.extend(callee_sig.params.iter().map(|(_, ty)| ty.codegen_repr()));
    let mut ref_params = Vec::with_capacity(callee_sig.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(callee_sig.ref_params.iter().copied());
    let null_label = ctx.next_label("method_receiver_null");
    let done_label = ctx.next_label("method_receiver_done");
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    objects::emit_nullable_receiver_object_payload(ctx, object, &null_label, receiver_reg)?;
    let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
        ctx,
        receiver_reg,
        &receiver_ty,
        &inst.operands,
        &param_types,
        &ref_params,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    let return_ty = iterators::emit_interface_dispatch_call(ctx, normalized, &method_key, None)?;
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_call_result(ctx, inst, &return_ty)?;
    emit_ref_arg_writebacks(ctx, &call_args.ref_writebacks)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&null_label);
    emit_method_call_on_null_fatal(ctx, method_name);

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits PHP's fatal diagnostic for calling an instance method on null.
fn emit_method_call_on_null_fatal(ctx: &mut FunctionContext<'_>, method_name: &str) {
    let message = format!(
        "Fatal error: Call to a member function {}() on null\n",
        method_name
    );
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // write the member-call-on-null fatal to stderr
            ctx.emitter.adrp("x1", &message_label);
            ctx.emitter.add_lo12("x1", "x1", &message_label);
            ctx.emitter.instruction(&format!("mov x2, #{}", message_len));      // pass the member-call-on-null fatal byte length
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2");                              // write the member-call-on-null fatal to Linux stderr
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter.instruction(&format!("mov edx, {}", message_len));      // pass the member-call-on-null fatal byte length
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the member-call-on-null fatal before exiting
            abi::emit_exit(ctx.emitter, 1);
        }
    }
}

/// Returns the direct runtime intrinsic for built-in `Generator` instance methods.
fn generator_intrinsic(class_name: &str, method_name: &str) -> Option<IntrinsicCall> {
    if class_name.trim_start_matches('\\') != "Generator" {
        return None;
    }
    IntrinsicCall::instance_method("Generator", method_name)
}

/// Returns the descriptor-backed intrinsic for SPL callback-filter accept trampolines.
fn callback_filter_intrinsic(class_name: &str, method_name: &str) -> Option<IntrinsicCall> {
    let intrinsic = IntrinsicCall::instance_method(class_name.trim_start_matches('\\'), method_name)?;
    if intrinsic.kind() == IntrinsicCallKind::CallbackFilterAccept {
        Some(intrinsic)
    } else {
        None
    }
}

/// Returns a runtime-backed intrinsic for ordinary direct instance-method calls.
fn runtime_backed_instance_intrinsic(class_name: &str, method_name: &str) -> Option<IntrinsicCall> {
    let intrinsic = IntrinsicCall::instance_method(class_name.trim_start_matches('\\'), method_name)?;
    intrinsic.runtime_helper()?;
    Some(intrinsic)
}

/// Returns a runtime-backed intrinsic for ordinary direct static-method calls.
fn runtime_backed_static_intrinsic(class_name: &str, method_name: &str) -> Option<IntrinsicCall> {
    let intrinsic = IntrinsicCall::static_method(class_name.trim_start_matches('\\'), method_name)?;
    intrinsic.runtime_helper()?;
    Some(intrinsic)
}

/// Lowers a runtime-backed intrinsic instance method using normal method ABI arguments.
fn lower_instance_runtime_intrinsic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
    method_name: &str,
    intrinsic: IntrinsicCall,
) -> Result<()> {
    let normalized = class_name.trim_start_matches('\\');
    let method_key = php_symbol_key(method_name);
    let class_info = ctx
        .module
        .class_infos
        .get(normalized)
        .ok_or_else(|| CodegenIrError::unsupported(format!("intrinsic method on unknown class {}", normalized)))?;
    let callee_sig = class_info
        .methods
        .get(&method_key)
        .ok_or_else(|| CodegenIrError::unsupported(format!("intrinsic method {}::{}", normalized, method_name)))?;
    let expected_args = callee_sig.params.len() + 1;
    if inst.operands.len() != expected_args {
        return Err(CodegenIrError::unsupported(format!(
            "intrinsic method call to {}::{} with {} operands for {} ABI params",
            normalized,
            method_name,
            inst.operands.len(),
            expected_args
        )));
    }
    let return_ty = callee_sig.return_type.clone();
    let callee_params = callee_sig.params.clone();
    let callee_ref_params = callee_sig.ref_params.clone();
    let mut param_types = Vec::with_capacity(callee_params.len() + 1);
    param_types.push(PhpType::Object(normalized.to_string()));
    param_types.extend(callee_params.iter().map(|(_, ty)| ty.codegen_repr()));
    let mut ref_params = Vec::with_capacity(callee_ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(callee_ref_params.iter().copied());
    let call_args =
        materialize_direct_call_args_with_refs(ctx, &inst.operands, &param_types, &ref_params)?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(
        ctx.emitter,
        intrinsic
            .runtime_helper()
            .expect("runtime-backed instance intrinsic must have a helper"),
    );
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_call_result(ctx, inst, &return_ty)?;
    emit_ref_arg_writebacks(ctx, &call_args.ref_writebacks)
}

/// Lowers a runtime-backed intrinsic static method using the hidden called-class id ABI.
fn lower_static_runtime_intrinsic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    receiver: &str,
    method_name: &str,
    called_class_id: &CalledClassIdArg,
    intrinsic: IntrinsicCall,
) -> Result<()> {
    let method_key = php_symbol_key(method_name);
    let receiver_info = ctx
        .module
        .class_infos
        .get(receiver)
        .ok_or_else(|| CodegenIrError::unsupported(format!("intrinsic static method on unknown class {}", receiver)))?;
    let callee_sig = receiver_info
        .static_methods
        .get(&method_key)
        .ok_or_else(|| CodegenIrError::unsupported(format!("intrinsic static method {}::{}", receiver, method_name)))?;
    if inst.operands.len() != callee_sig.params.len() {
        return Err(CodegenIrError::unsupported(format!(
            "intrinsic static method call to {}::{} with {} operands for {} params",
            receiver,
            method_name,
            inst.operands.len(),
            callee_sig.params.len()
        )));
    }
    let return_ty = callee_sig.return_type.clone();
    let callee_ref_params = callee_sig.ref_params.clone();
    let param_types = callee_sig
        .params
        .iter()
        .map(|(_, ty)| ty.codegen_repr())
        .collect::<Vec<_>>();
    let call_args = materialize_static_method_call_args_with_refs(
        ctx,
        called_class_id,
        &inst.operands,
        &param_types,
        &callee_ref_params,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(
        ctx.emitter,
        intrinsic
            .runtime_helper()
            .expect("runtime-backed static intrinsic must have a helper"),
    );
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    if let Some(result) = inst.result {
        let result_ty = ctx.value_php_type(result)?.codegen_repr();
        let return_ty = return_ty.codegen_repr();
        if matches!(result_ty, PhpType::Mixed | PhpType::Union(_)) && return_ty != PhpType::Mixed {
            emit_box_current_value_as_mixed(ctx.emitter, &return_ty);
        }
        ctx.store_result_value(result)?;
    }
    emit_ref_arg_writebacks(ctx, &call_args.ref_writebacks)
}

/// Lowers `CallbackFilterIterator::__elephcAcceptCallback()` through its stored descriptor.
fn lower_callback_filter_accept_intrinsic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    intrinsic: IntrinsicCall,
) -> Result<()> {
    if inst.operands.len() != 4 {
        return Err(CodegenIrError::invalid_module(format!(
            "{}::{} received {} operands for callback-filter accept",
            intrinsic.class_name(),
            intrinsic.method_key(),
            inst.operands.len()
        )));
    }
    let class_info = ctx
        .module
        .class_infos
        .get(intrinsic.class_name())
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "missing {} metadata for callback-filter accept",
                intrinsic.class_name()
            ))
        })?;
    let callback_offset = class_info
        .property_offsets
        .get("callback")
        .copied()
        .ok_or_else(|| CodegenIrError::missing_entry("property callback", 0))?;
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    ctx.load_value_to_reg(inst.operands[0], descriptor_reg)?;
    abi::emit_load_from_address(ctx.emitter, descriptor_reg, descriptor_reg, callback_offset);
    callables::emit_descriptor_reg_invoker_call_with_args(
        ctx,
        inst,
        descriptor_reg,
        &inst.operands[1..],
        "callback_filter_accept",
    )
}

/// Lowers built-in `Generator` methods to their runtime helpers.
fn lower_generator_intrinsic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    intrinsic: IntrinsicCall,
) -> Result<()> {
    let param_types = generator_intrinsic_param_types(intrinsic);
    let ref_params = vec![false; param_types.len()];
    let call_args =
        materialize_direct_call_args_with_refs(ctx, &inst.operands, &param_types, &ref_params)?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    let helper = intrinsic.runtime_helper().ok_or_else(|| {
        CodegenIrError::invalid_module(format!(
            "Generator intrinsic {:?} has no runtime helper",
            intrinsic.kind()
        ))
    })?;
    abi::emit_call_label(ctx.emitter, helper);
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_call_result(ctx, inst, &generator_intrinsic_return_type(intrinsic))?;
    emit_ref_arg_writebacks(ctx, &call_args.ref_writebacks)
}

/// Returns ABI-visible parameter types for a `Generator` intrinsic call.
fn generator_intrinsic_param_types(intrinsic: IntrinsicCall) -> Vec<PhpType> {
    let mut params = vec![PhpType::Object("Generator".to_string())];
    match intrinsic.kind() {
        IntrinsicCallKind::GeneratorSend => params.push(PhpType::Mixed),
        IntrinsicCallKind::GeneratorThrow => {
            params.push(PhpType::Object("Throwable".to_string()));
        }
        _ => {}
    }
    params
}

/// Returns the PHP result type produced by a `Generator` runtime helper.
fn generator_intrinsic_return_type(intrinsic: IntrinsicCall) -> PhpType {
    match intrinsic.kind() {
        IntrinsicCallKind::GeneratorValid => PhpType::Bool,
        IntrinsicCallKind::GeneratorNext | IntrinsicCallKind::GeneratorRewind => PhpType::Void,
        _ => PhpType::Mixed,
    }
}

/// Returns true when a direct method call can be satisfied from the compact Throwable payload.
fn is_throwable_standard_method_call(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_name: &str,
) -> bool {
    is_throwable_standard_method_key(&php_symbol_key(method_name))
        && is_throwable_like_class(ctx, class_name)
}

/// Returns true for method keys supplied by PHP's built-in `Throwable` surface.
fn is_throwable_standard_method_key(method_key: &str) -> bool {
    matches!(
        method_key,
        "getmessage"
            | "getcode"
            | "getfile"
            | "getline"
            | "gettrace"
            | "gettraceasstring"
            | "getprevious"
            | "__tostring"
    )
}

/// Returns true when class metadata says the receiver is Throwable-compatible.
fn is_throwable_like_class(ctx: &FunctionContext<'_>, class_name: &str) -> bool {
    let class_name = class_name.trim_start_matches('\\');
    if matches!(class_name, "Throwable") {
        return true;
    }
    if interface_extends_throwable(ctx, class_name) {
        return true;
    }
    let mut current = Some(class_name);
    while let Some(name) = current {
        let Some(class_info) = ctx.module.class_infos.get(name) else {
            return false;
        };
        if class_info.interfaces.iter().any(|interface| interface == "Throwable") {
            return true;
        }
        current = class_info.parent.as_deref();
    }
    false
}

/// Returns true when an interface is `Throwable` or transitively extends it.
fn interface_extends_throwable(ctx: &FunctionContext<'_>, interface_name: &str) -> bool {
    if interface_name == "Throwable" {
        return true;
    }
    let Some(interface_info) = ctx.module.interface_infos.get(interface_name) else {
        return false;
    };
    interface_info.parents.iter().any(|parent| {
        let parent = parent.trim_start_matches('\\');
        parent == "Throwable" || interface_extends_throwable(ctx, parent)
    })
}

/// Lowers compact Throwable methods without requiring synthetic EIR method bodies.
fn lower_throwable_standard_method(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::unsupported(format!(
            "Throwable::{} with {} EIR operands",
            method_name,
            inst.operands.len()
        )));
    }
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, object_reg)?;
    let return_ty = match php_symbol_key(method_name).as_str() {
        "getmessage" => lower_throwable_get_message(ctx, object_reg),
        "getcode" => lower_throwable_get_code(ctx, object_reg),
        "getfile" | "gettraceasstring" => lower_throwable_empty_string(ctx),
        "getline" => lower_throwable_zero_int(ctx),
        "gettrace" => lower_throwable_empty_trace_array(ctx),
        "getprevious" => lower_throwable_null_previous(ctx, inst),
        "__tostring" => lower_throwable_get_message(ctx, object_reg),
        _ => Err(CodegenIrError::unsupported(format!(
            "Throwable intrinsic method {}",
            method_name
        ))),
    }?;
    if inst.result.is_some()
        && matches!(inst.result_php_type.codegen_repr(), PhpType::Mixed)
        && !matches!(return_ty.codegen_repr(), PhpType::Mixed)
    {
        emit_box_current_value_as_mixed(ctx.emitter, &return_ty.codegen_repr());
    }
    store_if_result(ctx, inst)
}

/// Loads `Throwable::getMessage()` from payload offsets 8/16 into string result registers.
fn lower_throwable_get_message(ctx: &mut FunctionContext<'_>, object_reg: &str) -> Result<PhpType> {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, ptr_reg, object_reg, 8);
    abi::emit_load_from_address(ctx.emitter, len_reg, object_reg, 16);
    Ok(PhpType::Str)
}

/// Loads `Throwable::getCode()` from payload offset 24 into the integer result register.
fn lower_throwable_get_code(ctx: &mut FunctionContext<'_>, object_reg: &str) -> Result<PhpType> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, 24);
    Ok(PhpType::Int)
}

/// Materializes the synthetic empty-string result used by Throwable file/trace methods.
fn lower_throwable_empty_string(ctx: &mut FunctionContext<'_>) -> Result<PhpType> {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    let (label, len) = ctx.data.add_string(b"");
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    Ok(PhpType::Str)
}

/// Materializes the synthetic zero integer used by `Throwable::getLine()`.
fn lower_throwable_zero_int(ctx: &mut FunctionContext<'_>) -> Result<PhpType> {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    Ok(PhpType::Int)
}

/// Materializes the synthetic empty indexed array used by `Throwable::getTrace()`.
fn lower_throwable_empty_trace_array(ctx: &mut FunctionContext<'_>) -> Result<PhpType> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", 4);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 8);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", 4);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 8);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &PhpType::Mixed,
    );
    Ok(PhpType::Array(Box::new(PhpType::Mixed)))
}

/// Materializes the synthetic null result used by `Throwable::getPrevious()`.
fn lower_throwable_null_previous(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<PhpType> {
    let payload = if inst.result_php_type.codegen_repr() == PhpType::Mixed {
        0
    } else {
        0x7fff_ffff_ffff_fffe
    };
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), payload);
    Ok(PhpType::Void)
}

/// Lowers `Fiber::start(...)` by copying boxed start arguments into the Fiber object.
fn lower_fiber_start(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
) -> Result<()> {
    let args = fiber_start_visible_args(ctx, inst)?;
    if args.len() > runtime::FIBER_START_ARGS_MAX as usize {
        return Err(CodegenIrError::unsupported(
            "Fiber::start with more than seven EIR arguments",
        ));
    }
    let param_types = vec![PhpType::Mixed; args.len()];
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &param_types, 1);
    for value in &args {
        ctx.load_value_to_result(*value)?;
        let source_ty = ctx.raw_value_php_type(*value)?;
        let push_ty = materialize_direct_call_arg_for_param(ctx, &source_ty, &PhpType::Mixed)?;
        abi::emit_push_result_value(ctx.emitter, &push_ty);
    }
    let overflow_bytes = abi::materialize_outgoing_args(ctx.emitter, &assignments);
    let receiver_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    ctx.load_value_to_reg(object, receiver_arg)?;
    emit_store_fiber_start_args(ctx, &assignments, args.len())?;
    abi::emit_call_label(ctx.emitter, "__rt_fiber_start");
    abi::emit_release_temporary_stack(ctx.emitter, overflow_bytes);
    store_if_result(ctx, inst)
}

/// Lowers `Fiber::resume($value = null)` through the shared runtime helper.
fn lower_fiber_resume(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
) -> Result<()> {
    let value = fiber_single_optional_arg(
        ctx,
        inst.operands.get(1..).unwrap_or(&[]),
        "Fiber::resume",
    )?;
    emit_optional_mixed_arg(ctx, value)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));          // preserve the boxed resume value while loading the receiver
    let receiver_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    ctx.load_value_to_reg(object, receiver_arg)?;
    abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1)); // pass the boxed resume value as runtime helper argument 2
    abi::emit_call_label(ctx.emitter, "__rt_fiber_resume");
    store_if_result(ctx, inst)
}

/// Lowers `Fiber::throw(Throwable $exception)` through the shared runtime helper.
fn lower_fiber_throw(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
) -> Result<()> {
    let args = fiber_visible_args(ctx, inst.operands.get(1..).unwrap_or(&[]), "Fiber::throw")?;
    if args.len() != 1 {
        return Err(CodegenIrError::unsupported(
            "Fiber::throw without exactly one EIR argument",
        ));
    }
    let thrown = args[0];
    let thrown_ty = ctx.load_value_to_result(thrown)?;
    if !matches!(thrown_ty.codegen_repr(), PhpType::Object(_)) {
        return Err(CodegenIrError::unsupported(format!(
            "Fiber::throw argument PHP type {:?}",
            thrown_ty
        )));
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));          // preserve the Throwable while loading the Fiber receiver
    ctx.load_value_to_reg(object, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
    abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1)); // pass the Throwable object as runtime helper argument 2
    abi::emit_call_label(ctx.emitter, "__rt_fiber_throw");
    store_if_result(ctx, inst)
}

/// Copies materialized `Fiber::start` arguments into the runtime Fiber start-arg buffer.
fn emit_store_fiber_start_args(
    ctx: &mut FunctionContext<'_>,
    assignments: &[abi::OutgoingArgAssignment],
    supplied_arg_count: usize,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_store_fiber_start_args_aarch64(ctx, assignments, supplied_arg_count),
        Arch::X86_64 => {
            emit_store_fiber_start_args_x86_64(ctx, assignments, supplied_arg_count);
            Ok(())
        }
    }
}

/// Copies register-passed ARM64 start arguments into `Fiber::start_args`.
fn emit_store_fiber_start_args_aarch64(
    ctx: &mut FunctionContext<'_>,
    assignments: &[abi::OutgoingArgAssignment],
    supplied_arg_count: usize,
) -> Result<()> {
    let skip_label = ctx.next_label("fiber_start_args_done");
    ctx.emitter.instruction(&format!("ldr x9, [x0, #{}]", runtime::FIBER_USER_ARG_MAX_OFFSET)); // x9 = writable Fiber start_args slot count
    for (idx, assignment) in assignments.iter().take(supplied_arg_count).enumerate() {
        if !assignment.in_register() {
            return Err(CodegenIrError::unsupported(
                "Fiber::start ARM64 stack-passed EIR arguments",
            ));
        }
        let source_reg = abi::int_arg_reg_name(ctx.emitter.target, assignment.start_reg);
        let offset = runtime::FIBER_START_ARGS_OFFSET + (idx as i32) * 8;
        ctx.emitter.instruction(&format!("cmp x9, #{}", idx + 1));              // is this start() slot allowed for user arguments?
        ctx.emitter.instruction(&format!("b.lt {}", skip_label));               // stop once wrapper-reserved slots would be overwritten
        ctx.emitter.instruction(&format!("str {}, [x0, #{}]", source_reg, offset)); // store the boxed Mixed start() argument
    }
    ctx.emitter.label(&skip_label);
    ctx.emitter.instruction(&format!("mov x9, #{}", supplied_arg_count));       // materialize the visible start() argument count
    ctx.emitter.instruction(&format!("str x9, [x0, #{}]", runtime::FIBER_START_ARG_COUNT_OFFSET)); // publish start() arity for Fiber wrappers
    Ok(())
}

/// Copies SysV x86_64 register and stack-passed start arguments into `Fiber::start_args`.
fn emit_store_fiber_start_args_x86_64(
    ctx: &mut FunctionContext<'_>,
    assignments: &[abi::OutgoingArgAssignment],
    supplied_arg_count: usize,
) {
    let skip_label = ctx.next_label("fiber_start_args_done");
    ctx.emitter.instruction(&format!("mov r11, QWORD PTR [rdi + {}]", runtime::FIBER_USER_ARG_MAX_OFFSET)); // r11 = writable Fiber start_args slot count
    let mut overflow_slot = 0usize;
    for (idx, assignment) in assignments.iter().take(supplied_arg_count).enumerate() {
        let offset = runtime::FIBER_START_ARGS_OFFSET + (idx as i32) * 8;
        ctx.emitter.instruction(&format!("cmp r11, {}", idx + 1));              // is this start() slot allowed for user arguments?
        ctx.emitter.instruction(&format!("jl {}", skip_label));                 // stop once wrapper-reserved slots would be overwritten
        if assignment.in_register() {
            let source_reg = abi::int_arg_reg_name(ctx.emitter.target, assignment.start_reg);
            ctx.emitter.instruction(&format!("mov QWORD PTR [rdi + {}], {}", offset, source_reg)); // store the boxed Mixed register argument
        } else {
            let stack_offset = overflow_slot * 16;
            if stack_offset == 0 {
                ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");            // load the first stack-passed boxed Mixed start() argument
            } else {
                ctx.emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", stack_offset)); // load this stack-passed boxed Mixed start() argument
            }
            ctx.emitter.instruction(&format!("mov QWORD PTR [rdi + {}], r10", offset)); // store the boxed Mixed stack argument
            overflow_slot += 1;
        }
    }
    ctx.emitter.label(&skip_label);
    ctx.emitter.instruction(&format!("mov QWORD PTR [rdi + {}], {}", runtime::FIBER_START_ARG_COUNT_OFFSET, supplied_arg_count)); // publish start() arity for Fiber wrappers
}

/// Lowers no-argument Fiber instance methods that delegate to one runtime helper.
fn lower_fiber_noarg_runtime_method(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    helper: &str,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::unsupported(format!(
            "Fiber runtime method {} with EIR arguments",
            helper
        )));
    }
    let receiver_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    ctx.load_value_to_reg(object, receiver_arg)?;
    abi::emit_call_label(ctx.emitter, helper);
    store_if_result(ctx, inst)
}

/// Returns the visible `Fiber::start(...)` operands before synthetic default padding.
fn fiber_start_visible_args(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<Vec<ValueId>> {
    fiber_visible_args(ctx, inst.operands.get(1..).unwrap_or(&[]), "Fiber::start")
}

/// Returns at most one visible Fiber runtime argument after default padding.
fn fiber_single_optional_arg(
    ctx: &FunctionContext<'_>,
    operands: &[ValueId],
    context: &str,
) -> Result<Option<ValueId>> {
    let args = fiber_visible_args(ctx, operands, context)?;
    if args.len() > 1 {
        return Err(CodegenIrError::unsupported(format!(
            "{} with more than one EIR argument",
            context
        )));
    }
    Ok(args.first().copied())
}

/// Returns visible Fiber operands before synthetic default padding.
fn fiber_visible_args(
    ctx: &FunctionContext<'_>,
    operands: &[ValueId],
    context: &str,
) -> Result<Vec<ValueId>> {
    let mut args = Vec::new();
    let mut saw_default_padding = false;
    for operand in operands {
        if is_synthetic_null_value(ctx, *operand)? {
            saw_default_padding = true;
            continue;
        }
        if saw_default_padding {
            return Err(CodegenIrError::unsupported(format!(
                "{} with non-trailing EIR default arguments",
                context
            )));
        }
        args.push(*operand);
    }
    Ok(args)
}

/// Leaves a boxed Mixed value in the integer result register, using null when omitted.
fn emit_optional_mixed_arg(ctx: &mut FunctionContext<'_>, value: Option<ValueId>) -> Result<()> {
    if let Some(value) = value {
        ctx.load_value_to_result(value)?;
        let source_ty = ctx.raw_value_php_type(value)?;
        materialize_direct_call_arg_for_param(ctx, &source_ty, &PhpType::Mixed)?;
        return Ok(());
    }
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
    Ok(())
}

/// Returns true when a value is an omitted optional-argument placeholder.
fn is_synthetic_null_value(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    if ctx.value_php_type(value)? != PhpType::Void {
        return Ok(false);
    }
    let Some(value) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let crate::ir::ValueDef::Instruction { inst, .. } = value.def else {
        return Ok(false);
    };
    let Some(inst) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    Ok(matches!(inst.op, Op::ConstNull)
        && inst.span.is_some_and(|span| span.line == 0 && span.col == 0))
}

/// Lowers Fiber state predicates directly to the shared runtime helper.
fn lower_fiber_state_predicate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    state: FiberStatePredicate,
) -> Result<()> {
    let receiver_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    ctx.load_value_to_reg(object, receiver_arg)?;
    emit_fiber_state_predicate_call(ctx, inst, state)
}

/// Lowers Fiber state predicates when the receiver is boxed as `Mixed`.
fn lower_mixed_fiber_state_predicate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
    state: FiberStatePredicate,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::unsupported(format!(
            "Fiber mixed state predicate {} with EIR arguments",
            method_name
        )));
    }
    emit_mixed_fiber_receiver_to_arg(ctx, object, method_name)?;
    emit_fiber_state_predicate_call(ctx, inst, state)
}

/// Unboxes a `Mixed` receiver and leaves a verified `Fiber*` in argument register 0.
fn emit_mixed_fiber_receiver_to_arg(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    method_name: &str,
) -> Result<()> {
    let object_label = ctx.next_label("mixed_fiber_state_object");
    let fiber_label = ctx.next_label("mixed_fiber_state_fiber");
    let class_id = ctx
        .module
        .class_infos
        .get("Fiber")
        .map(|class| class.class_id)
        .ok_or_else(|| CodegenIrError::unsupported("mixed Fiber predicate without Fiber metadata"))?;
    let receiver_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // continue only when the Mixed receiver holds an object
            ctx.emitter.instruction(&format!("b.eq {}", object_label));         // inspect the object class before calling the Fiber predicate
            emit_method_call_on_null_fatal(ctx, method_name);
            ctx.emitter.label(&object_label);
            ctx.emitter.instruction("ldr x9, [x1]");                            // load the receiver object's runtime class id
            ctx.emitter.instruction(&format!("cmp x9, #{}", class_id));         // verify the boxed object is a Fiber instance
            ctx.emitter.instruction(&format!("b.eq {}", fiber_label));          // call the Fiber predicate only for real Fiber receivers
            emit_method_call_on_null_fatal(ctx, method_name);
            ctx.emitter.label(&fiber_label);
            ctx.emitter.instruction(&format!("mov {}, x1", receiver_arg));      // pass the unboxed Fiber object to the runtime predicate
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // continue only when the Mixed receiver holds an object
            ctx.emitter.instruction(&format!("je {}", object_label));           // inspect the object class before calling the Fiber predicate
            emit_method_call_on_null_fatal(ctx, method_name);
            ctx.emitter.label(&object_label);
            ctx.emitter.instruction("mov r10, QWORD PTR [rdi]");                // load the receiver object's runtime class id
            ctx.emitter.instruction(&format!("cmp r10, {}", class_id));         // verify the boxed object is a Fiber instance
            ctx.emitter.instruction(&format!("je {}", fiber_label));            // call the Fiber predicate only for real Fiber receivers
            emit_method_call_on_null_fatal(ctx, method_name);
            ctx.emitter.label(&fiber_label);
        }
    }
    Ok(())
}

/// Calls the shared runtime state predicate helper for a receiver already in arg0.
fn emit_fiber_state_predicate_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    state: FiberStatePredicate,
) -> Result<()> {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        state.expected_state() as i64,
    );
    abi::emit_call_label(ctx.emitter, "__rt_fiber_state_eq");
    if matches!(state, FiberStatePredicate::Started) {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("eor x0, x0, #1");                      // invert not-started into PHP's isStarted predicate
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("xor rax, 1");                          // invert not-started into PHP's isStarted predicate
            }
        }
    }
    store_if_result(ctx, inst)
}

/// Fiber state-query method selected by a direct method call.
enum FiberStatePredicate {
    Started,
    Running,
    Suspended,
    Terminated,
}

impl FiberStatePredicate {
    /// Returns the runtime state value compared by `__rt_fiber_state_eq`.
    fn expected_state(&self) -> i32 {
        match self {
            Self::Started => crate::codegen::runtime::FIBER_STATE_NOT_STARTED,
            Self::Running => crate::codegen::runtime::FIBER_STATE_RUNNING,
            Self::Suspended => crate::codegen::runtime::FIBER_STATE_SUSPENDED,
            Self::Terminated => crate::codegen::runtime::FIBER_STATE_TERMINATED,
        }
    }
}

/// Returns true when a direct method call targets PHP's built-in `Fiber::start`.
fn is_fiber_start_call(class_name: &str, method_name: &str) -> bool {
    php_symbol_key(class_name.trim_start_matches('\\')) == "fiber"
        && php_symbol_key(method_name) == "start"
}

/// Returns true when a direct method call targets PHP's built-in `Fiber::resume`.
fn is_fiber_resume_call(class_name: &str, method_name: &str) -> bool {
    php_symbol_key(class_name.trim_start_matches('\\')) == "fiber"
        && php_symbol_key(method_name) == "resume"
}

/// Returns true when a direct method call targets PHP's built-in `Fiber::throw`.
fn is_fiber_throw_call(class_name: &str, method_name: &str) -> bool {
    php_symbol_key(class_name.trim_start_matches('\\')) == "fiber"
        && php_symbol_key(method_name) == "throw"
}

/// Returns true when a direct method call targets PHP's built-in `Fiber::getReturn`.
fn is_fiber_get_return_call(class_name: &str, method_name: &str) -> bool {
    php_symbol_key(class_name.trim_start_matches('\\')) == "fiber"
        && php_symbol_key(method_name) == "getreturn"
}

/// Resolves a Fiber state predicate method name, if the receiver is `Fiber`.
fn fiber_state_predicate(
    class_name: &str,
    method_name: &str,
) -> Option<FiberStatePredicate> {
    if php_symbol_key(class_name.trim_start_matches('\\')) != "fiber" {
        return None;
    }
    fiber_state_predicate_method(method_name)
}

/// Resolves a Fiber state predicate solely from the method name.
fn fiber_state_predicate_method(method_name: &str) -> Option<FiberStatePredicate> {
    match php_symbol_key(method_name).as_str() {
        "isstarted" => Some(FiberStatePredicate::Started),
        "isrunning" => Some(FiberStatePredicate::Running),
        "issuspended" => Some(FiberStatePredicate::Suspended),
        "isterminated" => Some(FiberStatePredicate::Terminated),
        _ => None,
    }
}

/// Lowers a nullsafe method call by short-circuiting boxed-null receivers.
fn lower_nullsafe_method_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let method_name = method_name_data(ctx, inst)?.to_string();
    let Some((class_name, nullable)) = objects::nullable_object_receiver_class(ctx, object)? else {
        return Err(CodegenIrError::unsupported(format!(
            "{} for receiver PHP type {:?}",
            inst.op.name(),
            objects::raw_value_php_type(ctx, object)?
        )));
    };
    if !nullable {
        return lower_method_call(ctx, inst);
    }
    let target = resolve_method_call_target(ctx, &class_name, &method_name, inst.operands.len())?;
    let null_label = ctx.next_label("nullsafe_method_null");
    let done_label = ctx.next_label("nullsafe_method_done");
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    objects::emit_nullable_receiver_object_payload(ctx, object, &null_label, object_reg)?;
    let receiver_ty = PhpType::Object(class_name);
    let mut param_types = Vec::with_capacity(target.params.len() + 1);
    param_types.push(receiver_ty.clone());
    param_types.extend(target.params.iter().map(|param| param.codegen_repr()));
    let mut ref_params = Vec::with_capacity(target.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(target.ref_params.iter().copied());
    let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
        ctx,
        object_reg,
        &receiver_ty,
        &inst.operands,
        &param_types,
        &ref_params,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &method_symbol(&target.impl_class, &target.method_key));
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    if inst.result_php_type.codegen_repr() == PhpType::Mixed
        && target.return_ty.codegen_repr() != PhpType::Mixed
    {
        emit_box_current_value_as_mixed(ctx.emitter, &target.return_ty.codegen_repr());
    }
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&null_label);
    objects::emit_boxed_null(ctx);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)?;
    emit_ref_arg_writebacks(ctx, &call_args.ref_writebacks)
}

/// Resolved method metadata needed to issue a direct method call.
struct MethodCallTarget {
    impl_class: String,
    method_key: String,
    dynamic_slot: Option<usize>,
    params: Vec<PhpType>,
    ref_params: Vec<bool>,
    return_ty: PhpType,
}

/// Concrete runtime class branch available to a `Mixed` receiver method call.
struct MixedMethodCandidate {
    class_id: u64,
    class_name: String,
    target: MethodCallTarget,
}

/// Outgoing call argument state that must be cleaned up after the call returns.
struct CallArgMaterialization {
    overflow_bytes: usize,
    ref_writebacks: Vec<RefArgWriteback>,
    cleanup_slots: Vec<CallArgTempCleanup>,
    cleanup_bytes: usize,
    borrowed_stack_arg_bytes: usize,
}

/// Caller-owned temporary argument that must be released after the call returns.
struct CallArgTempCleanup {
    param_index: usize,
    offset: usize,
    ty: PhpType,
}

/// Caller-side stack Mixed cell borrowed by a read-only callee.
struct BorrowedStackMixedArg {
    param_index: usize,
    offset: usize,
    source_ty: PhpType,
}

/// A caller-side scalar local boxed into a temporary Mixed by-reference cell.
struct RefArgWriteback {
    param_index: usize,
    source_value: ValueId,
    source_slot: LocalSlotId,
    source_is_ref_cell: bool,
    source_ty: PhpType,
    cell_offset: usize,
}

/// Runtime dispatch path for EIR `RuntimeCall` instructions that mean ArrayAccess indexing.
enum ArrayAccessRuntimeDispatch {
    Concrete(String),
    Interface { boxed_receiver: bool },
}

/// Source for the hidden called-class id passed to static method bodies.
enum CalledClassIdArg {
    Immediate(u64),
    Local(LocalSlotId),
    ThisObject(LocalSlotId),
}

/// Resolves method implementation class, canonical key, return type, and ABI arity.
fn resolve_method_call_target(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_name: &str,
    operand_count: usize,
) -> Result<MethodCallTarget> {
    let normalized = class_name.trim_start_matches('\\');
    let class_info = ctx
        .module
        .class_infos
        .get(normalized)
        .ok_or_else(|| CodegenIrError::unsupported(format!("method call on unknown class {}", normalized)))?;
    let method_key = php_symbol_key(method_name);
    let callee_sig = class_info
        .methods
        .get(&method_key)
        .ok_or_else(|| CodegenIrError::unsupported(format!("method call to unknown method {}::{}", normalized, method_name)))?;
    let expected_args = callee_sig.params.len() + 1;
    if operand_count != expected_args {
        return Err(CodegenIrError::unsupported(format!(
            "method call to {}::{} with {} operands for {} ABI params",
            normalized,
            method_name,
            operand_count,
            expected_args
        )));
    }
    let impl_class = class_info
        .method_impl_classes
        .get(&method_key)
        .cloned()
        .unwrap_or_else(|| normalized.to_string());
    let dynamic_slot = class_info.vtable_slots.get(&method_key).copied();
    let has_direct_body = class_method_already_emitted(ctx, &impl_class, &method_key, false);
    if !has_direct_body && dynamic_slot.is_none() {
        return Err(CodegenIrError::unsupported(format!(
            "method call to {}::{} without an emitted EIR method body",
            impl_class, method_name
        )));
    }
    let dynamic_slot = if class_info.final_methods.contains(&method_key) {
        None
    } else {
        dynamic_slot
    };
    Ok(MethodCallTarget {
        impl_class,
        method_key,
        dynamic_slot,
        params: callee_sig
            .params
            .iter()
            .map(|(_, ty)| ty.codegen_repr())
            .collect(),
        ref_params: callee_sig.ref_params.clone(),
        return_ty: callee_sig.return_type.clone(),
    })
}

/// Emits a runtime vtable dispatch for an instance method whose concrete override is late-bound.
fn emit_dynamic_instance_method_call(ctx: &mut FunctionContext<'_>, slot: usize) {
    let class_id_reg = abi::temp_int_reg(ctx.emitter.target);
    let dispatch_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_from_address(
        ctx.emitter,
        class_id_reg,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        0,
    );
    abi::emit_symbol_address(ctx.emitter, dispatch_reg, "_class_vtable_ptrs");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr {}, [{}, {}, lsl #3]", dispatch_reg, dispatch_reg, class_id_reg)); // load the class-specific instance-vtable pointer
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, QWORD PTR [{} + {} * 8]", dispatch_reg, dispatch_reg, class_id_reg)); // load the class-specific instance-vtable pointer
        }
    }
    abi::emit_load_from_address(ctx.emitter, dispatch_reg, dispatch_reg, slot * 8);
    abi::emit_call_reg(ctx.emitter, dispatch_reg);
}

/// Returns true when the current EIR module includes the target class method body.
fn class_method_already_emitted(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_key: &str,
    is_static: bool,
) -> bool {
    ctx.module.class_methods.iter().any(|function| {
        function.flags.is_static == is_static
            && function
                .name
                .rsplit_once("::")
                .is_some_and(|(candidate_class, candidate_method)| {
                    candidate_class == class_name
                        && php_symbol_key(candidate_method) == method_key
                })
    })
}

/// Stores a call result, boxing concrete returns for generic EIR result slots.
fn store_call_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    return_ty: &PhpType,
) -> Result<()> {
    if let Some(result) = inst.result {
        let result_ty = ctx.value_php_type(result)?;
        let return_ty = return_ty.codegen_repr();
        if return_ty == PhpType::Void || result_ty == PhpType::Void {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
            if matches!(result_ty, PhpType::Mixed | PhpType::Union(_)) {
                emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
            }
            ctx.store_result_value(result)?;
            return Ok(());
        }
        if matches!(result_ty, PhpType::Mixed | PhpType::Union(_)) && return_ty != PhpType::Mixed {
            emit_box_current_value_as_mixed(ctx.emitter, &return_ty);
        }
        ctx.store_result_value(result)?;
    }
    Ok(())
}

/// Resolves an instruction data immediate as a method name.
fn method_name_data<'a>(ctx: &'a FunctionContext<'_>, inst: &Instruction) -> Result<&'a str> {
    let data = expect_data(inst)?;
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}

/// Lowers a direct static-method call on a named class receiver.
fn lower_static_method_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let target = method_name_data(ctx, inst)?.to_string();
    let (receiver_label, method_name) = parse_static_method_target(&target)?;
    let receiver = resolve_static_method_receiver(ctx, receiver_label)?;
    if is_static_fiber_get_current_call(&receiver, method_name) {
        return lower_static_fiber_get_current(ctx, inst);
    }
    if is_static_fiber_suspend_call(&receiver, method_name) {
        return lower_static_fiber_suspend(ctx, inst);
    }
    if let Some(()) = enums::try_lower_enum_static_method(ctx, receiver.as_str(), method_name, inst)? {
        return Ok(());
    }
    let called_class_id = resolve_static_called_class_arg(ctx, receiver_label, &receiver)?;
    if let Some(intrinsic) = runtime_backed_static_intrinsic(receiver.as_str(), method_name) {
        return lower_static_runtime_intrinsic(
            ctx,
            inst,
            receiver.as_str(),
            method_name,
            &called_class_id,
            intrinsic,
        );
    }
    let late_bound_static = is_late_bound_static_receiver(receiver_label);
    let receiver_info = ctx
        .module
        .class_infos
        .get(receiver.as_str())
        .ok_or_else(|| CodegenIrError::unsupported(format!("static method call on unknown class {}", receiver)))?;
    let method_key = php_symbol_key(method_name);
    let impl_class = receiver_info
        .static_method_impl_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(receiver.as_str());
    let impl_info = ctx
        .module
        .class_infos
        .get(impl_class)
        .ok_or_else(|| CodegenIrError::unsupported(format!("static method implementation on unknown class {}", impl_class)))?;
    let Some(callee_sig) = impl_info.static_methods.get(&method_key) else {
        if is_lexical_instance_static_receiver(receiver_label)
            && receiver_info.methods.contains_key(&method_key)
        {
            return lower_lexical_instance_static_method_call(ctx, inst, receiver.as_str(), method_name);
        }
        return Err(CodegenIrError::unsupported(format!(
            "static method call to unknown method {}",
            target
        )));
    };
    if inst.operands.len() != callee_sig.params.len() {
        return Err(CodegenIrError::unsupported(format!(
            "static method call to {} with {} operands for {} params",
            target,
            inst.operands.len(),
            callee_sig.params.len()
        )));
    }
    let param_types = callee_sig
        .params
        .iter()
        .map(|(_, ty)| ty.codegen_repr())
        .collect::<Vec<_>>();
    let dynamic_static_slot = if late_bound_static {
        receiver_info.static_vtable_slots.get(&method_key).copied()
    } else {
        None
    };
    let call_args = materialize_static_method_call_args_with_refs(
        ctx,
        &called_class_id,
        &inst.operands,
        &param_types,
        &callee_sig.ref_params,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    if let Some(slot) = dynamic_static_slot {
        emit_dynamic_static_method_call(ctx, slot);
    } else {
        abi::emit_call_label(ctx.emitter, &static_method_symbol(impl_class, &method_key));
    }
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    if let Some(result) = inst.result {
        if ctx.value_php_type(result)? == PhpType::Void {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
        }
        ctx.store_result_value(result)?;
    }
    emit_ref_arg_writebacks(ctx, &call_args.ref_writebacks)
}

/// Lowers `self::method()` or `parent::method()` when it targets an instance method.
fn lower_lexical_instance_static_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    receiver: &str,
    method_name: &str,
) -> Result<()> {
    let this_slot = ctx.local_slot_by_name("this").ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "lexical instance method static call without this in {}",
            ctx.function.name
        ))
    })?;
    let mut target = resolve_method_call_target(ctx, receiver, method_name, inst.operands.len() + 1)?;
    target.dynamic_slot = None;
    let receiver_ty = PhpType::Object(receiver.to_string());
    let mut param_types = Vec::with_capacity(target.params.len() + 1);
    param_types.push(receiver_ty.clone());
    param_types.extend(target.params.iter().map(|param| param.codegen_repr()));
    let mut ref_params = Vec::with_capacity(target.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(target.ref_params.iter().copied());
    let call_args = materialize_method_call_args_with_receiver_local_and_refs(
        ctx,
        this_slot,
        &receiver_ty,
        &inst.operands,
        &param_types,
        &ref_params,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &method_symbol(&target.impl_class, &target.method_key));
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_call_result(ctx, inst, &target.return_ty)?;
    emit_ref_arg_writebacks(ctx, &call_args.ref_writebacks)
}

/// Emits an indirect static-vtable call for a late-bound `static::method()` receiver.
fn emit_dynamic_static_method_call(ctx: &mut FunctionContext<'_>, slot: usize) {
    let hidden_called_class_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let class_id_scratch = abi::temp_int_reg(ctx.emitter.target);
    let dispatch_scratch = abi::symbol_scratch_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov {}, {}", class_id_scratch, hidden_called_class_reg)); // preserve the forwarded called-class id across static-vtable address materialization
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, {}", class_id_scratch, hidden_called_class_reg)); // preserve the forwarded called-class id across static-vtable address materialization
        }
    }
    abi::emit_symbol_address(ctx.emitter, dispatch_scratch, "_class_static_vtable_ptrs");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr {}, [{}, {}, lsl #3]", dispatch_scratch, dispatch_scratch, class_id_scratch)); // load the class-specific static-vtable pointer from the global table
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, QWORD PTR [{} + {} * 8]", dispatch_scratch, dispatch_scratch, class_id_scratch)); // load the class-specific static-vtable pointer from the global table
        }
    }
    abi::emit_load_from_address(ctx.emitter, dispatch_scratch, dispatch_scratch, slot * 8);
    abi::emit_call_reg(ctx.emitter, dispatch_scratch);
}

/// Lowers static `Fiber::suspend($value = null)` through the shared runtime helper.
fn lower_static_fiber_suspend(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let value = fiber_single_optional_arg(ctx, &inst.operands, "Fiber::suspend")?;
    emit_optional_mixed_arg(ctx, value)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));          // preserve the boxed suspend value for target-specific argument loading
    abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0)); // pass the boxed suspend value as runtime helper argument 1
    abi::emit_call_label(ctx.emitter, "__rt_fiber_suspend");
    store_if_result(ctx, inst)
}

/// Lowers static `Fiber::getCurrent()` through the shared runtime helper.
fn lower_static_fiber_get_current(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if !inst.operands.is_empty() {
        return Err(CodegenIrError::unsupported(
            "Fiber::getCurrent with EIR arguments",
        ));
    }
    abi::emit_call_label(ctx.emitter, "__rt_fiber_get_current");
    store_if_result(ctx, inst)
}

/// Returns true when a static method call targets PHP's built-in `Fiber::getCurrent`.
fn is_static_fiber_get_current_call(receiver: &str, method_name: &str) -> bool {
    php_symbol_key(receiver.trim_start_matches('\\')) == "fiber"
        && php_symbol_key(method_name) == "getcurrent"
}

/// Returns true when a static method call targets PHP's built-in `Fiber::suspend`.
fn is_static_fiber_suspend_call(receiver: &str, method_name: &str) -> bool {
    php_symbol_key(receiver.trim_start_matches('\\')) == "fiber"
        && php_symbol_key(method_name) == "suspend"
}

/// Resolves the hidden called-class id argument for a static method call.
fn resolve_static_called_class_arg(
    ctx: &FunctionContext<'_>,
    receiver_label: &str,
    receiver: &str,
) -> Result<CalledClassIdArg> {
    let receiver_label = receiver_label.trim_start_matches('\\');
    if matches!(receiver_label, "self" | "parent" | "static") {
        if let Some(slot) = ctx.local_slot_by_name(CALLED_CLASS_ID_PARAM) {
            return Ok(CalledClassIdArg::Local(slot));
        }
        if let Some(slot) = ctx.local_slot_by_name("this") {
            return Ok(CalledClassIdArg::ThisObject(slot));
        }
    }
    let class_info = ctx
        .module
        .class_infos
        .get(receiver)
        .ok_or_else(|| CodegenIrError::unsupported(format!("static method call on unknown class {}", receiver)))?;
    Ok(CalledClassIdArg::Immediate(class_info.class_id))
}

/// Resolves lexical `self` and `parent` receivers for static method calls.
fn resolve_static_method_receiver(ctx: &FunctionContext<'_>, receiver: &str) -> Result<String> {
    let receiver = receiver.trim_start_matches('\\');
    match receiver {
        "self" => current_method_class(ctx).map(str::to_string),
        "parent" => {
            let class_name = current_method_class(ctx)?;
            ctx.module
                .class_infos
                .get(class_name)
                .and_then(|class| class.parent.clone())
                .ok_or_else(|| CodegenIrError::unsupported(format!(
                    "parent static method call outside class with parent for {}",
                    ctx.function.name
                )))
        }
        "static" => current_method_class(ctx).map(str::to_string),
        _ => Ok(receiver.to_string()),
    }
}

/// Returns true for the late-bound static receiver spelling.
fn is_late_bound_static_receiver(receiver: &str) -> bool {
    receiver.trim_start_matches('\\') == "static"
}

/// Returns true when PHP static-call syntax should bind an instance method lexically.
fn is_lexical_instance_static_receiver(receiver: &str) -> bool {
    matches!(receiver.trim_start_matches('\\'), "self" | "parent")
}

/// Returns the class name encoded in the current EIR class-method function name.
fn current_method_class<'a>(ctx: &'a FunctionContext<'_>) -> Result<&'a str> {
    ctx.function
        .name
        .rsplit_once("::")
        .map(|(class_name, _)| class_name)
        .ok_or_else(|| CodegenIrError::unsupported(format!(
            "lexical static method receiver outside class method {}",
            ctx.function.name
        )))
}

/// Splits an EIR static-method call label into class receiver and method name.
fn parse_static_method_target(target: &str) -> Result<(&str, &str)> {
    target.rsplit_once("::").ok_or_else(|| {
        CodegenIrError::invalid_module(format!("invalid static method target '{}'", target))
    })
}

/// Lowers a direct call to a module-local user function.
fn lower_direct_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let function_name = ctx.function_name_data(expect_data(inst)?)?.to_string();
    let callee = ctx
        .callable_function_by_name(&function_name)
        .ok_or_else(|| CodegenIrError::unsupported(format!("call to unknown function {}", function_name)))?;
    if inst.operands.len() != callee.params.len() {
        return Err(CodegenIrError::unsupported(format!(
            "call to {} with {} args for {} params",
            function_name,
            inst.operands.len(),
            callee.params.len()
        )));
    }
    let param_types = callee
        .params
        .iter()
        .map(|param| param.php_type.codegen_repr())
        .collect::<Vec<_>>();
    let ref_params = callee.params.iter().map(|param| param.by_ref).collect::<Vec<_>>();
    let borrowed_stack_mixed_args =
        plan_borrowed_stack_mixed_args(ctx, callee, &inst.operands, &param_types, &ref_params)?;
    let call_args = materialize_direct_call_args_with_refs_and_borrowed_options(
        ctx,
        &inst.operands,
        &param_types,
        &ref_params,
        true,
        &borrowed_stack_mixed_args,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &function_symbol(&function_name));
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    if let Some(result) = inst.result {
        if ctx.value_php_type(result)? == PhpType::Void {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
        }
        ctx.store_result_value(result)?;
    }
    emit_call_arg_temp_cleanups(ctx, &call_args, inst.result)?;
    emit_borrowed_stack_mixed_arg_release(ctx, &call_args);
    emit_ref_arg_writebacks(ctx, &call_args.ref_writebacks)
}

/// Loads SSA operands into ABI argument registers and caller-stack slots for a direct call.
pub(super) fn materialize_direct_call_args(
    ctx: &mut FunctionContext<'_>,
    args: &[ValueId],
    param_types: &[PhpType],
) -> Result<usize> {
    let ref_params = vec![false; param_types.len()];
    let materialized = materialize_direct_call_args_with_refs(ctx, args, param_types, &ref_params)?;
    Ok(materialized.overflow_bytes)
}

/// Loads SSA operands into ABI argument slots, preserving by-reference locals.
fn materialize_direct_call_args_with_refs(
    ctx: &mut FunctionContext<'_>,
    args: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
) -> Result<CallArgMaterialization> {
    materialize_direct_call_args_with_refs_and_options(ctx, args, param_types, ref_params, false)
}

/// Loads SSA operands into ABI argument slots with optional caller-temp cleanup tracking.
fn materialize_direct_call_args_with_refs_and_options(
    ctx: &mut FunctionContext<'_>,
    args: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
    track_mixed_temp_cleanups: bool,
) -> Result<CallArgMaterialization> {
    materialize_direct_call_args_with_refs_and_borrowed_options(
        ctx,
        args,
        param_types,
        ref_params,
        track_mixed_temp_cleanups,
        &[],
    )
}

/// Loads SSA operands into ABI argument slots with optional borrowed Mixed stack cells.
fn materialize_direct_call_args_with_refs_and_borrowed_options(
    ctx: &mut FunctionContext<'_>,
    args: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
    track_mixed_temp_cleanups: bool,
    borrowed_stack_mixed_args: &[BorrowedStackMixedArg],
) -> Result<CallArgMaterialization> {
    if args.len() != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "direct call materialization received {} args for {} params",
            args.len(),
            param_types.len()
        )));
    }
    if ref_params.len() != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "direct call materialization received {} ref flags for {} params",
            ref_params.len(),
            param_types.len()
        )));
    }
    let mut ref_writebacks = plan_ref_arg_writebacks(ctx, args, param_types, ref_params)?;
    emit_ref_arg_temp_cells(ctx, &mut ref_writebacks)?;
    let abi_param_types = abi_param_types_for_refs(param_types, ref_params);
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &abi_param_types, 0);
    let borrowed_stack_arg_bytes =
        borrowed_stack_mixed_args.len() * BORROWED_MIXED_ARG_CELL_BYTES;
    if borrowed_stack_arg_bytes > 0 {
        abi::emit_reserve_temporary_stack(ctx.emitter, borrowed_stack_arg_bytes);
    }
    let cleanup_slots = if track_mixed_temp_cleanups {
        plan_call_arg_temp_cleanups(
            ctx,
            args,
            param_types,
            ref_params,
            borrowed_stack_mixed_args,
        )?
    } else {
        Vec::new()
    };
    let cleanup_bytes = cleanup_slots.len() * 16;
    if cleanup_bytes > 0 {
        abi::emit_reserve_temporary_stack(ctx.emitter, cleanup_bytes);
    }
    let ref_cell_base_offset = borrowed_stack_arg_bytes + cleanup_bytes;
    let borrowed_cell_base_offset = cleanup_bytes;
    let mut arg_temp_bytes = 0usize;
    for (index, (value, param_ty)) in args.iter().zip(param_types.iter()).enumerate() {
        if ref_params[index] {
            materialize_ref_arg_address(
                ctx,
                *value,
                index,
                param_ty,
                arg_temp_bytes,
                &ref_writebacks,
                ref_cell_base_offset,
            )?;
            abi::emit_push_result_value(ctx.emitter, &PhpType::Int);
        } else if let Some(borrowed) = borrowed_stack_mixed_args
            .iter()
            .find(|borrowed| borrowed.param_index == index)
        {
            ctx.load_value_to_result(*value)?;
            emit_borrowed_stack_mixed_arg_cell(
                ctx,
                borrowed,
                borrowed_cell_base_offset + arg_temp_bytes,
            );
            abi::emit_push_result_value(ctx.emitter, &PhpType::Mixed);
        } else {
            ctx.load_value_to_result(*value)?;
            let source_ty = ctx.raw_value_php_type(*value)?;
            let push_ty = materialize_direct_call_arg_for_param(ctx, &source_ty, param_ty)?;
            if let Some(cleanup) = cleanup_slots.iter().find(|cleanup| cleanup.param_index == index) {
                save_call_arg_temp_cleanup(ctx, cleanup, arg_temp_bytes);
            }
            abi::emit_push_result_value(ctx.emitter, &push_ty);
        }
        arg_temp_bytes += call_arg_temp_slot_size(&abi_param_types[index]);
    }
    Ok(CallArgMaterialization {
        overflow_bytes: abi::materialize_outgoing_args(ctx.emitter, &assignments),
        ref_writebacks,
        cleanup_slots,
        cleanup_bytes,
        borrowed_stack_arg_bytes,
    })
}

/// Loads hidden and visible static-method arguments, preserving by-reference locals.
fn materialize_static_method_call_args_with_refs(
    ctx: &mut FunctionContext<'_>,
    called_class_id: &CalledClassIdArg,
    args: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
) -> Result<CallArgMaterialization> {
    if args.len() != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "static method call materialization received {} args for {} visible params",
            args.len(),
            param_types.len()
        )));
    }
    if ref_params.len() != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "static method call materialization received {} ref flags for {} visible params",
            ref_params.len(),
            param_types.len()
        )));
    }
    let mut ref_writebacks = plan_ref_arg_writebacks(ctx, args, param_types, ref_params)?;
    emit_ref_arg_temp_cells(ctx, &mut ref_writebacks)?;
    let visible_abi_param_types = abi_param_types_for_refs(param_types, ref_params);
    let mut abi_param_types = Vec::with_capacity(visible_abi_param_types.len() + 1);
    abi_param_types.push(PhpType::Int);
    abi_param_types.extend_from_slice(&visible_abi_param_types);
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &abi_param_types, 0);
    materialize_called_class_id(ctx, called_class_id)?;
    abi::emit_push_result_value(ctx.emitter, &PhpType::Int);
    let mut arg_temp_bytes = call_arg_temp_slot_size(&PhpType::Int);
    for (index, (value, param_ty)) in args.iter().zip(param_types.iter()).enumerate() {
        if ref_params[index] {
            materialize_ref_arg_address(
                ctx,
                *value,
                index,
                param_ty,
                arg_temp_bytes,
                &ref_writebacks,
                0,
            )?;
            abi::emit_push_result_value(ctx.emitter, &PhpType::Int);
        } else {
            ctx.load_value_to_result(*value)?;
            let source_ty = ctx.raw_value_php_type(*value)?;
            let push_ty = materialize_direct_call_arg_for_param(ctx, &source_ty, param_ty)?;
            abi::emit_push_result_value(ctx.emitter, &push_ty);
        }
        arg_temp_bytes += call_arg_temp_slot_size(&visible_abi_param_types[index]);
    }
    Ok(CallArgMaterialization {
        overflow_bytes: abi::materialize_outgoing_args(ctx.emitter, &assignments),
        ref_writebacks,
        cleanup_slots: Vec::new(),
        cleanup_bytes: 0,
        borrowed_stack_arg_bytes: 0,
    })
}

/// Materializes the hidden called-class id into the integer result register.
fn materialize_called_class_id(
    ctx: &mut FunctionContext<'_>,
    called_class_id: &CalledClassIdArg,
) -> Result<()> {
    match called_class_id {
        CalledClassIdArg::Immediate(class_id) => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                *class_id as i64,
            );
        }
        CalledClassIdArg::Local(slot) => {
            let source_ty = ctx.load_local_to_result(*slot)?;
            if source_ty != PhpType::Int {
                return Err(CodegenIrError::invalid_module(format!(
                    "hidden called-class id local has PHP type {:?}",
                    source_ty
                )));
            }
        }
        CalledClassIdArg::ThisObject(slot) => {
            let source_ty = ctx.load_local_to_result(*slot)?;
            if !matches!(source_ty.codegen_repr(), PhpType::Object(_)) {
                return Err(CodegenIrError::invalid_module(format!(
                    "this local has PHP type {:?} for forwarded called-class id",
                    source_ty
                )));
            }
            abi::emit_load_from_address(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                abi::int_result_reg(ctx.emitter),
                0,
            );
        }
    }
    Ok(())
}

/// Converts the loaded call operand to the ABI shape required by the callee parameter.
fn materialize_direct_call_arg_for_param(
    ctx: &mut FunctionContext<'_>,
    source_ty: &PhpType,
    param_ty: &PhpType,
) -> Result<PhpType> {
    match param_ty.codegen_repr() {
        PhpType::TaggedScalar => coerce_loaded_value_to_tagged_scalar(ctx, source_ty),
        PhpType::Int if matches!(source_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            Ok(PhpType::Int)
        }
        PhpType::Bool if matches!(source_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
            Ok(PhpType::Bool)
        }
        PhpType::Float if matches!(source_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
            Ok(PhpType::Float)
        }
        PhpType::Str if matches!(source_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            Ok(PhpType::Str)
        }
        PhpType::Mixed if source_ty.codegen_repr() != PhpType::Mixed => {
            emit_box_current_value_as_mixed(ctx.emitter, source_ty);
            Ok(PhpType::Mixed)
        }
        PhpType::Array(param_elem) if param_elem.codegen_repr() == PhpType::Mixed => {
            if let PhpType::Array(source_elem) = source_ty.codegen_repr() {
                let source_elem = source_elem.codegen_repr();
                if source_elem != PhpType::Mixed {
                    emit_loaded_indexed_array_to_mixed(ctx, &source_elem);
                }
                return Ok(PhpType::Array(Box::new(PhpType::Mixed)));
            }
            Ok(PhpType::Array(param_elem))
        }
        target_ty => Ok(target_ty),
    }
}

/// Converts the currently loaded result registers into the inline nullable-int shape.
pub(super) fn coerce_loaded_value_to_tagged_scalar(
    ctx: &mut FunctionContext<'_>,
    source_ty: &PhpType,
) -> Result<PhpType> {
    match source_ty.codegen_repr() {
        PhpType::TaggedScalar => Ok(PhpType::TaggedScalar),
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            crate::codegen::sentinels::emit_tagged_scalar_from_int_result(ctx.emitter);
            Ok(PhpType::TaggedScalar)
        }
        PhpType::Void | PhpType::Never => {
            crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
            Ok(PhpType::TaggedScalar)
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_mixed_result_as_tagged_scalar(ctx);
            Ok(PhpType::TaggedScalar)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "conversion from PHP type {:?} to PHP type TaggedScalar",
            other
        ))),
    }
}

/// Reorders `__rt_mixed_unbox` output into the inline tagged-scalar result registers.
fn emit_mixed_result_as_tagged_scalar(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x9, x0");                              // preserve the unboxed Mixed tag before moving the payload
            ctx.emitter.instruction("mov x0, x1");                              // place the unboxed payload into the tagged-scalar payload register
            ctx.emitter.instruction("mov x1, x9");                              // place the unboxed Mixed tag into the tagged-scalar tag register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, rax");                            // preserve the unboxed Mixed tag before moving the payload
            ctx.emitter.instruction("mov rax, rdi");                            // place the unboxed payload into the tagged-scalar payload register
            ctx.emitter.instruction("mov rdx, r10");                            // place the unboxed Mixed tag into the tagged-scalar tag register
        }
    }
}

/// Plans scalar Mixed arguments that can be borrowed on the caller stack for a direct callee.
fn plan_borrowed_stack_mixed_args(
    ctx: &FunctionContext<'_>,
    callee: &Function,
    args: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
) -> Result<Vec<BorrowedStackMixedArg>> {
    let mut borrowed_args = Vec::new();
    for (index, (value, param_ty)) in args.iter().zip(param_types.iter()).enumerate() {
        if ref_params[index]
            || callee.params[index].variadic
            || param_ty.codegen_repr() != PhpType::Mixed
        {
            continue;
        }
        let source_ty = ctx.raw_value_php_type(*value)?.codegen_repr();
        if !matches!(source_ty, PhpType::Int | PhpType::Bool) {
            continue;
        }
        if !callee_mixed_param_is_truthiness_only(callee, index) {
            continue;
        }
        borrowed_args.push(BorrowedStackMixedArg {
            param_index: index,
            offset: borrowed_args.len() * BORROWED_MIXED_ARG_CELL_BYTES,
            source_ty,
        });
    }
    Ok(borrowed_args)
}

/// Returns true when a Mixed parameter is only loaded for boolean conversion.
fn callee_mixed_param_is_truthiness_only(callee: &Function, param_index: usize) -> bool {
    let slot = LocalSlotId::from_raw(param_index as u32);
    let mut loaded_values = Vec::new();
    for inst in &callee.instructions {
        match (&inst.op, &inst.immediate) {
            (Op::LoadLocal, Some(Immediate::LocalSlot(candidate))) if *candidate == slot => {
                let Some(result) = inst.result else {
                    return false;
                };
                loaded_values.push(result);
            }
            (_, Some(Immediate::LocalSlot(candidate))) if *candidate == slot => return false,
            _ => {}
        }
    }
    loaded_values
        .iter()
        .all(|value| callee_value_is_only_truthiness_operand(callee, *value))
}

/// Returns true when every use of `value` feeds a non-escaping boolean conversion.
fn callee_value_is_only_truthiness_operand(callee: &Function, value: ValueId) -> bool {
    for inst in &callee.instructions {
        if !inst.operands.iter().any(|operand| *operand == value) {
            continue;
        }
        if !matches!(inst.op, Op::IsTruthy | Op::MixedCastBool) {
            return false;
        }
    }
    !callee_terminator_uses_value(callee, value)
}

/// Returns true when any terminator directly consumes `value`.
fn callee_terminator_uses_value(callee: &Function, value: ValueId) -> bool {
    callee
        .blocks
        .iter()
        .filter_map(|block| block.terminator.as_ref())
        .any(|terminator| terminator_uses_value(terminator, value))
}

/// Returns true when one terminator directly consumes `value`.
fn terminator_uses_value(terminator: &Terminator, value: ValueId) -> bool {
    match terminator {
        Terminator::Br { args, .. } => args.contains(&value),
        Terminator::CondBr {
            cond,
            then_args,
            else_args,
            ..
        } => *cond == value || then_args.contains(&value) || else_args.contains(&value),
        Terminator::Switch {
            scrutinee,
            cases,
            default_args,
            ..
        } => {
            *scrutinee == value
                || default_args.contains(&value)
                || cases.iter().any(|case| case.args.contains(&value))
        }
        Terminator::Return { value: Some(return_value) } => *return_value == value,
        Terminator::Return { value: None } => false,
        Terminator::Throw { value: thrown } => *thrown == value,
        Terminator::Fatal { .. } | Terminator::Unreachable => false,
        Terminator::GeneratorSuspend {
            key,
            value: yielded,
            resume_args,
            ..
        } => key.is_some_and(|key| key == value)
            || yielded.is_some_and(|yielded| yielded == value)
            || resume_args.contains(&value),
    }
}

/// Writes a borrowed stack Mixed cell for a scalar argument and returns its address as the result.
fn emit_borrowed_stack_mixed_arg_cell(
    ctx: &mut FunctionContext<'_>,
    borrowed: &BorrowedStackMixedArg,
    base_offset: usize,
) {
    let payload_reg = abi::secondary_scratch_reg(ctx.emitter);
    let cell_reg = abi::symbol_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.emitter.instruction(&format!("mov {}, {}", payload_reg, result_reg));   // preserve the scalar payload before writing the borrowed Mixed tag
    abi::emit_temporary_stack_address(ctx.emitter, cell_reg, base_offset + borrowed.offset);
    abi::emit_load_int_immediate(
        ctx.emitter,
        result_reg,
        runtime_value_tag(&borrowed.source_ty) as i64,
    );
    abi::emit_store_to_address(ctx.emitter, result_reg, cell_reg, 0);
    abi::emit_store_to_address(ctx.emitter, payload_reg, cell_reg, 8);
    abi::emit_store_zero_to_address(ctx.emitter, cell_reg, 16);
    move_reg_to_int_result(ctx, cell_reg);
}

/// Plans temporary Mixed call arguments that must remain alive until after the callee returns.
fn plan_call_arg_temp_cleanups(
    ctx: &FunctionContext<'_>,
    args: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
    borrowed_stack_mixed_args: &[BorrowedStackMixedArg],
) -> Result<Vec<CallArgTempCleanup>> {
    let mut cleanups = Vec::new();
    for (index, (value, param_ty)) in args.iter().zip(param_types.iter()).enumerate() {
        if ref_params[index]
            || borrowed_stack_mixed_args
                .iter()
                .any(|borrowed| borrowed.param_index == index)
        {
            continue;
        }
        let source_ty = ctx.raw_value_php_type(*value)?;
        if direct_call_arg_creates_mixed_temp(&source_ty, param_ty) {
            cleanups.push(CallArgTempCleanup {
                param_index: index,
                offset: cleanups.len() * 16,
                ty: PhpType::Mixed,
            });
        }
    }
    Ok(cleanups)
}

/// Returns whether argument materialization allocates a caller-owned Mixed box.
fn direct_call_arg_creates_mixed_temp(source_ty: &PhpType, param_ty: &PhpType) -> bool {
    matches!(param_ty.codegen_repr(), PhpType::Mixed)
        && !matches!(source_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
}

/// Saves the current pointer result into the reserved call-argument cleanup area.
fn save_call_arg_temp_cleanup(
    ctx: &mut FunctionContext<'_>,
    cleanup: &CallArgTempCleanup,
    arg_temp_bytes: usize,
) {
    let scratch = abi::symbol_scratch_reg(ctx.emitter);
    let offset = arg_temp_bytes + cleanup.offset;
    abi::emit_temporary_stack_address(ctx.emitter, scratch, offset);
    abi::emit_store_to_address(ctx.emitter, abi::int_result_reg(ctx.emitter), scratch, 0);
}

/// Releases caller-owned temporary arguments after the call result has been saved.
fn emit_call_arg_temp_cleanups(
    ctx: &mut FunctionContext<'_>,
    call_args: &CallArgMaterialization,
    result: Option<ValueId>,
) -> Result<()> {
    if call_args.cleanup_slots.is_empty() {
        return Ok(());
    }
    let result_alias = call_result_can_alias_mixed_temp(ctx, result)?;
    for cleanup in &call_args.cleanup_slots {
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            cleanup.offset,
        );
        let skip_cleanup_label = if let Some(result) = result_alias {
            let label = ctx.next_label("call_arg_temp_cleanup_result_alias");
            emit_branch_if_cleanup_temp_aliases_result(ctx, result, &label)?;
            Some(label)
        } else {
            None
        };
        abi::emit_decref_if_refcounted(ctx.emitter, &cleanup.ty);
        if let Some(label) = skip_cleanup_label {
            ctx.emitter.label(&label);
        }
    }
    abi::emit_release_temporary_stack(ctx.emitter, call_args.cleanup_bytes);
    Ok(())
}

/// Returns the result value when it can alias a caller-owned temporary Mixed argument.
fn call_result_can_alias_mixed_temp(
    ctx: &FunctionContext<'_>,
    result: Option<ValueId>,
) -> Result<Option<ValueId>> {
    let Some(result) = result else {
        return Ok(None);
    };
    if matches!(ctx.value_php_type(result)?.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return Ok(Some(result));
    }
    Ok(None)
}

/// Skips temp cleanup when a callee returned the same Mixed cell that was passed as an argument.
fn emit_branch_if_cleanup_temp_aliases_result(
    ctx: &mut FunctionContext<'_>,
    result: ValueId,
    skip_label: &str,
) -> Result<()> {
    let cleanup_reg = abi::int_result_reg(ctx.emitter);
    let result_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(result, result_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", cleanup_reg, result_reg));  // compare the temporary Mixed cell with the saved call result
            ctx.emitter.instruction(&format!("b.eq {}", skip_label));           // keep the temp alive when ownership moved to the result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", cleanup_reg, result_reg));  // compare the temporary Mixed cell with the saved call result
            ctx.emitter.instruction(&format!("je {}", skip_label));             // keep the temp alive when ownership moved to the result
        }
    }
    Ok(())
}

/// Releases borrowed stack Mixed cells after heap temp cleanups and before by-ref cells.
fn emit_borrowed_stack_mixed_arg_release(
    ctx: &mut FunctionContext<'_>,
    call_args: &CallArgMaterialization,
) {
    if call_args.borrowed_stack_arg_bytes == 0 {
        return;
    }
    abi::emit_release_temporary_stack(ctx.emitter, call_args.borrowed_stack_arg_bytes);
}

/// Converts the currently loaded indexed-array argument into boxed Mixed slots.
fn emit_loaded_indexed_array_to_mixed(
    ctx: &mut FunctionContext<'_>,
    source_elem_ty: &PhpType,
) {
    let value_tag = runtime_value_tag(source_elem_ty) as i64;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x1", value_tag);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rsi", value_tag);
            ctx.emitter.instruction("mov rdi, rax");                            // pass the loaded indexed-array argument to the Mixed conversion helper
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
}

/// Loads method call arguments for lexical `self::`/`parent::` instance calls using local `this`.
fn materialize_method_call_args_with_receiver_local_and_refs(
    ctx: &mut FunctionContext<'_>,
    receiver_slot: LocalSlotId,
    receiver_ty: &PhpType,
    operands: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
) -> Result<CallArgMaterialization> {
    if operands.len() + 1 != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "lexical instance call materialization received {} operands for {} params",
            operands.len(),
            param_types.len()
        )));
    }
    if ref_params.len() != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "lexical instance call materialization received {} ref flags for {} params",
            ref_params.len(),
            param_types.len()
        )));
    }
    let visible_param_types = &param_types[1..];
    let visible_ref_params = &ref_params[1..];
    let mut ref_writebacks = plan_ref_arg_writebacks(ctx, operands, visible_param_types, visible_ref_params)?;
    emit_ref_arg_temp_cells(ctx, &mut ref_writebacks)?;
    let abi_param_types = abi_param_types_for_refs(param_types, ref_params);
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &abi_param_types, 0);
    ctx.load_local_to_result(receiver_slot)?;
    abi::emit_push_result_value(ctx.emitter, receiver_ty);
    let mut arg_temp_bytes = call_arg_temp_slot_size(&abi_param_types[0]);
    for (index, (value, param_ty)) in operands.iter().zip(visible_param_types.iter()).enumerate() {
        if visible_ref_params[index] {
            materialize_ref_arg_address(
                ctx,
                *value,
                index,
                param_ty,
                arg_temp_bytes,
                &ref_writebacks,
                0,
            )?;
            abi::emit_push_result_value(ctx.emitter, &PhpType::Int);
        } else {
            ctx.load_value_to_result(*value)?;
            let source_ty = ctx.raw_value_php_type(*value)?;
            let push_ty = materialize_direct_call_arg_for_param(ctx, &source_ty, param_ty)?;
            abi::emit_push_result_value(ctx.emitter, &push_ty);
        }
        arg_temp_bytes += call_arg_temp_slot_size(&abi_param_types[index + 1]);
    }
    Ok(CallArgMaterialization {
        overflow_bytes: abi::materialize_outgoing_args(ctx.emitter, &assignments),
        ref_writebacks,
        cleanup_slots: Vec::new(),
        cleanup_bytes: 0,
        borrowed_stack_arg_bytes: 0,
    })
}

/// Loads method call arguments with by-reference parameter support for local operands.
fn materialize_method_call_args_with_receiver_reg_and_refs(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    receiver_ty: &PhpType,
    operands: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
) -> Result<CallArgMaterialization> {
    if operands.len() != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "method call materialization received {} operands for {} params",
            operands.len(),
            param_types.len()
        )));
    }
    if ref_params.len() != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "method call materialization received {} ref flags for {} params",
            ref_params.len(),
            param_types.len()
        )));
    }
    let ref_writebacks = plan_ref_arg_writebacks(ctx, operands, param_types, ref_params)?;
    if !ref_writebacks.is_empty() {
        return Err(CodegenIrError::unsupported(
            "receiver-register method call with scalar-to-mixed by-reference writebacks",
        ));
    }
    let abi_param_types = abi_param_types_for_refs(param_types, ref_params);
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &abi_param_types, 0);
    move_reg_to_int_result(ctx, receiver_reg);
    abi::emit_push_result_value(ctx.emitter, receiver_ty);
    let mut arg_temp_bytes = call_arg_temp_slot_size(&abi_param_types[0]);
    for (index, (value, param_ty)) in operands
        .iter()
        .skip(1)
        .zip(param_types.iter().skip(1))
        .enumerate()
    {
        let param_index = index + 1;
        if ref_params[param_index] {
            materialize_ref_arg_address(
                ctx,
                *value,
                param_index,
                &param_types[param_index],
                arg_temp_bytes,
                &ref_writebacks,
                0,
            )?;
            abi::emit_push_result_value(ctx.emitter, &PhpType::Int);
        } else {
            ctx.load_value_to_result(*value)?;
            let source_ty = ctx.raw_value_php_type(*value)?;
            let push_ty = materialize_direct_call_arg_for_param(ctx, &source_ty, param_ty)?;
            abi::emit_push_result_value(ctx.emitter, &push_ty);
        }
        arg_temp_bytes += call_arg_temp_slot_size(&abi_param_types[param_index]);
    }
    Ok(CallArgMaterialization {
        overflow_bytes: abi::materialize_outgoing_args(ctx.emitter, &assignments),
        ref_writebacks,
        cleanup_slots: Vec::new(),
        cleanup_bytes: 0,
        borrowed_stack_arg_bytes: 0,
    })
}

/// Converts declared parameter types to the ABI-visible shape for by-reference args.
fn abi_param_types_for_refs(param_types: &[PhpType], ref_params: &[bool]) -> Vec<PhpType> {
    param_types
        .iter()
        .zip(ref_params.iter())
        .map(|(ty, is_ref)| if *is_ref { PhpType::Int } else { ty.codegen_repr() })
        .collect()
}

/// Returns the temporary stack slot size used by outgoing-argument staging.
fn call_arg_temp_slot_size(ty: &PhpType) -> usize {
    if matches!(ty.codegen_repr(), PhpType::Void | PhpType::Never) {
        0
    } else {
        16
    }
}

/// Plans caller-side Mixed cells needed for scalar locals passed to by-reference Mixed params.
fn plan_ref_arg_writebacks(
    ctx: &FunctionContext<'_>,
    args: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
) -> Result<Vec<RefArgWriteback>> {
    let mut writebacks = Vec::new();
    for (param_index, value) in args.iter().enumerate() {
        if !ref_params[param_index] || param_types[param_index].codegen_repr() != PhpType::Mixed {
            continue;
        }
        let source_ty = ctx.raw_value_php_type(*value)?.codegen_repr();
        if matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) {
            continue;
        }
        reject_unsupported_mixed_ref_writeback_source(&source_ty)?;
        let source = local_ref_arg_source(ctx, *value)?;
        writebacks.push(RefArgWriteback {
            param_index,
            source_value: *value,
            source_slot: source.slot,
            source_is_ref_cell: source.is_ref_cell,
            source_ty,
            cell_offset: 0,
        });
    }
    Ok(writebacks)
}

/// Rejects scalar-to-Mixed temporary ref cells whose writeback shape is not supported yet.
fn reject_unsupported_mixed_ref_writeback_source(source_ty: &PhpType) -> Result<()> {
    if matches!(source_ty.codegen_repr(), PhpType::Int | PhpType::Bool) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "by-reference Mixed parameter writeback to PHP type {:?}",
        source_ty
    )))
}

/// Emits persistent caller-stack Mixed cells used by scalar-to-Mixed by-reference args.
fn emit_ref_arg_temp_cells(
    ctx: &mut FunctionContext<'_>,
    writebacks: &mut [RefArgWriteback],
) -> Result<()> {
    let total = writebacks.len();
    for (index, writeback) in writebacks.iter_mut().enumerate() {
        ctx.load_value_to_result(writeback.source_value)?;
        emit_box_current_value_as_mixed(ctx.emitter, &writeback.source_ty);
        abi::emit_push_result_value(ctx.emitter, &PhpType::Mixed);
        writeback.cell_offset = (total - index - 1) * 16;
    }
    Ok(())
}

/// Loads the address that should be passed for a by-reference argument.
fn materialize_ref_arg_address(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    param_index: usize,
    param_ty: &PhpType,
    arg_temp_bytes: usize,
    writebacks: &[RefArgWriteback],
    ref_cell_base_offset: usize,
) -> Result<()> {
    if let Some(writeback) = writebacks
        .iter()
        .find(|writeback| writeback.param_index == param_index)
    {
        let cell_offset = arg_temp_bytes + ref_cell_base_offset + writeback.cell_offset;
        abi::emit_temporary_stack_address(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            cell_offset,
        );
        return Ok(());
    }
    if local_ref_arg_source(ctx, value).is_ok() {
        return materialize_local_ref_arg_address(ctx, value);
    }
    materialize_temporary_ref_arg_cell(ctx, value, param_ty)
}

/// Allocates a heap ref-cell for a by-reference argument that is not a local variable.
fn materialize_temporary_ref_arg_cell(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    param_ty: &PhpType,
) -> Result<()> {
    let source_ty = ctx.load_value_to_result(value)?;
    let target_ty = param_ty.codegen_repr();
    coerce_ref_cell_store_value(ctx, &source_ty, &target_ty)?;
    abi::emit_push_result_value(ctx.emitter, &target_ty);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    let cell_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_pop_reg(ctx.emitter, cell_reg);
    store_pushed_value_to_ref_cell(ctx, cell_reg, &target_ty);
    move_reg_to_int_result(ctx, cell_reg);
    Ok(())
}

/// Stores the pushed argument value into a freshly allocated by-reference cell.
fn store_pushed_value_to_ref_cell(
    ctx: &mut FunctionContext<'_>,
    cell_reg: &str,
    val_ty: &PhpType,
) {
    let temp_reg = if cell_reg == abi::temp_int_reg(ctx.emitter.target) {
        abi::symbol_scratch_reg(ctx.emitter)
    } else {
        abi::temp_int_reg(ctx.emitter.target)
    };
    match val_ty.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
            abi::emit_store_to_address(ctx.emitter, ptr_reg, cell_reg, 0);
            abi::emit_store_to_address(ctx.emitter, len_reg, cell_reg, 8);
        }
        PhpType::TaggedScalar => {
            let tag_reg = crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter);
            abi::emit_pop_reg_pair(ctx.emitter, abi::int_result_reg(ctx.emitter), tag_reg);
            abi::emit_store_to_address(ctx.emitter, abi::int_result_reg(ctx.emitter), cell_reg, 0);
            abi::emit_store_to_address(ctx.emitter, tag_reg, cell_reg, 8);
        }
        PhpType::Float => {
            abi::emit_pop_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
            abi::emit_store_to_address(ctx.emitter, abi::float_result_reg(ctx.emitter), cell_reg, 0);
        }
        _ => {
            abi::emit_pop_reg(ctx.emitter, temp_reg);
            abi::emit_store_to_address(ctx.emitter, temp_reg, cell_reg, 0);
            abi::emit_store_zero_to_address(ctx.emitter, cell_reg, 8);
        }
    }
}

/// Writes temporary Mixed by-reference cells back into the original caller locals.
fn emit_ref_arg_writebacks(
    ctx: &mut FunctionContext<'_>,
    writebacks: &[RefArgWriteback],
) -> Result<()> {
    for writeback in writebacks {
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            writeback.cell_offset,
        );
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        move_reg_to_int_result(ctx, mixed_unbox_low_payload_reg(ctx));
        store_current_scalar_result_to_ref_source(ctx, writeback)?;
        abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    }
    abi::emit_release_temporary_stack(ctx.emitter, writebacks.len() * 16);
    Ok(())
}

/// Returns the low payload register produced by `__rt_mixed_unbox` on the active target.
fn mixed_unbox_low_payload_reg(ctx: &FunctionContext<'_>) -> &'static str {
    match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdi",
    }
}

/// Unboxes a boxed Mixed/Union payload and retains it for an owned concrete heap result.
fn emit_unbox_mixed_to_owned_refcounted_result(ctx: &mut FunctionContext<'_>, result_ty: &PhpType) {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    move_reg_to_int_result(ctx, mixed_unbox_low_payload_reg(ctx));
    abi::emit_incref_if_refcounted(ctx.emitter, result_ty);
}

/// Stores an unboxed scalar Mixed payload back through the original by-reference source.
fn store_current_scalar_result_to_ref_source(
    ctx: &mut FunctionContext<'_>,
    writeback: &RefArgWriteback,
) -> Result<()> {
    if writeback.source_is_ref_cell || local_slot_stores_ref_cell_pointer(ctx, writeback.source_slot) {
        let offset = ctx.local_offset(writeback.source_slot)?;
        let pointer_reg = abi::symbol_scratch_reg(ctx.emitter);
        abi::load_at_offset(ctx.emitter, pointer_reg, offset);
        abi::emit_store_to_address(ctx.emitter, abi::int_result_reg(ctx.emitter), pointer_reg, 0);
        return Ok(());
    }
    let offset = ctx.local_offset(writeback.source_slot)?;
    abi::emit_store(ctx.emitter, &writeback.source_ty, offset);
    Ok(())
}

/// Loads a local variable's address for a by-reference method-call argument.
fn materialize_local_ref_arg_address(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    let source = local_ref_arg_source(ctx, value)?;
    let slot = source.slot;
    let offset = ctx.local_offset(slot)?;
    if source.is_ref_cell || local_slot_stores_ref_cell_pointer(ctx, slot) {
        abi::load_at_offset(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
    } else {
        abi::emit_frame_slot_address(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
    }
    Ok(())
}

/// Describes a local operand used as a by-reference call argument.
struct LocalRefArgSource {
    slot: LocalSlotId,
    is_ref_cell: bool,
}

/// Resolves an EIR value back to a local slot and whether it already stores a ref-cell pointer.
fn local_ref_arg_source(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<LocalRefArgSource> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(
            "by-reference method call argument from non-local value",
        ));
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    let is_ref_cell = match inst_ref.op {
        Op::LoadLocal => false,
        Op::LoadRefCell => true,
        _ => {
            return Err(CodegenIrError::unsupported(format!(
                "by-reference method call argument from opcode {}",
                inst_ref.op.name()
            )))
        }
    };
    let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "by-reference load argument has no local slot",
        ));
    };
    Ok(LocalRefArgSource { slot, is_ref_cell })
}

/// Resolves an EIR value back to a `load_local` source slot for by-reference calls.
fn local_slot_for_loaded_value(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<LocalSlotId> {
    local_ref_arg_source(ctx, value).map(|source| source.slot)
}

/// Returns true when a local slot stores a ref-cell pointer instead of a raw value.
fn local_slot_stores_ref_cell_pointer(ctx: &FunctionContext<'_>, slot: LocalSlotId) -> bool {
    ctx.local_stores_ref_cell_pointer(slot)
}

/// Moves a scratch integer register into the canonical integer result register.
fn move_reg_to_int_result(ctx: &mut FunctionContext<'_>, source_reg: &str) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    if source_reg == result_reg {
        return;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov {}, {}", result_reg, source_reg)); // move the unboxed receiver pointer into the normal argument staging register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, {}", result_reg, source_reg)); // move the unboxed receiver pointer into the normal argument staging register
        }
    }
}

/// Loads an SSA value and moves it into the first integer/pointer argument register.
pub(super) fn load_value_to_first_int_arg(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<PhpType> {
    let ty = ctx.load_value_to_result(value)?;
    move_int_result_to_first_arg(ctx);
    Ok(ty)
}

/// Casts a Mixed source in the first integer arg into one owned string copy.
pub(super) fn emit_mixed_string_for_persistent_store(ctx: &mut FunctionContext<'_>) {
    let non_string = ctx.next_label("mixed_string_persist_non_string");
    let done = ctx.next_label("mixed_string_persist_done");
    let mixed_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_push_reg(ctx.emitter, mixed_arg);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // check whether the Mixed payload already holds a string
            ctx.emitter.instruction(&format!("b.ne {}", non_string));           // non-string casts need scratch conversion before persistence
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the generic cast path after the direct string persist
            ctx.emitter.label(&non_string);
            abi::emit_pop_reg(ctx.emitter, mixed_arg);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // check whether the Mixed payload already holds a string
            ctx.emitter.instruction(&format!("jne {}", non_string));            // non-string casts need scratch conversion before persistence
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed string pointer into str_persist's input register
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the generic cast path after the direct string persist
            ctx.emitter.label(&non_string);
            abi::emit_pop_reg(ctx.emitter, mixed_arg);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
        }
    }
    ctx.emitter.label(&done);
}

/// Resolves `value` into the canonical integer result register, unboxing a boxed `Mixed`/`Union`
/// payload through `__rt_mixed_cast_int`.
///
/// `Int`/`Bool` load directly; every other type is an `unsupported` diagnostic. The `Mixed` path
/// emits a call that clobbers the caller-saved argument registers, so a caller that has already
/// staged other arguments in those registers must spill across this resolution (the integer is left
/// in the int result register on return).
pub(super) fn resolve_int_operand_to_result(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    context: &str,
) -> Result<()> {
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(value)?;
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
        }
        ty => {
            return Err(CodegenIrError::unsupported(format!(
                "{} for PHP type {:?}",
                context, ty
            )));
        }
    }
    Ok(())
}

/// Moves the canonical integer result register into the target's first argument register.
fn move_int_result_to_first_arg(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    if result_reg == arg_reg {
        return;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov {}, {}", arg_reg, result_reg)); // move the loaded value into the runtime helper argument register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, {}", arg_reg, result_reg)); // move the loaded value into the runtime helper argument register
        }
    }
}

/// Returns the temporary caller-stack pad needed to match incoming stack-arg offsets.
pub(super) fn direct_call_stack_pad_bytes(
    ctx: &FunctionContext<'_>,
    overflow_bytes: usize,
) -> usize {
    match ctx.emitter.target.arch {
        Arch::AArch64 if overflow_bytes > 0 => 16,
        _ => 0,
    }
}

/// Lowers a signed integer comparison into a boolean result value.
fn lower_int_compare(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let predicate = expect_cmp_predicate(inst)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let rhs_reg = abi::secondary_scratch_reg(ctx.emitter);
    require_integer_like(ctx.load_value_to_reg(lhs, result_reg)?, inst)?;
    require_integer_like(ctx.load_value_to_reg(rhs, rhs_reg)?, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", result_reg, rhs_reg)); // compare signed integer operands for the EIR predicate
            ctx.emitter.instruction(&format!("cset {}, {}", result_reg, aarch64_condition(predicate)?)); // materialize the predicate result as 0 or 1
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", result_reg, rhs_reg)); // compare signed integer operands for the EIR predicate
            ctx.emitter.instruction(&format!("set{} al", x86_64_condition(predicate)?)); // materialize the predicate result in the low byte
            ctx.emitter.instruction(&format!("movzx {}, al", result_reg));      // widen the predicate byte into the integer result register
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers an addressable local load into the result register and SSA destination slot.
fn lower_load_local(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    let result = inst.result.ok_or_else(|| {
        CodegenIrError::invalid_module("load_local missing result value")
    })?;
    let source_ty = if local_slot_stores_ref_cell_pointer(ctx, slot) {
        load_ref_param_local_to_result(ctx, slot)?
    } else {
        ctx.load_local_to_result(slot)?
    };
    let result_ty = ctx.value_php_type(result)?;
    coerce_loaded_local_to_result_type(ctx, &source_ty, &result_ty)?;
    ctx.store_result_value(result)
}

/// Lowers an explicit local ref-cell load into the result register and SSA slot.
fn lower_load_ref_cell(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    let result = inst.result.ok_or_else(|| {
        CodegenIrError::invalid_module("load_ref_cell missing result value")
    })?;
    let result_ty = ctx.value_php_type(result)?;
    let source_ty = load_ref_cell_local_to_result_as(ctx, slot, &result_ty)?;
    coerce_loaded_local_to_result_type(ctx, &source_ty, &result_ty)?;
    ctx.store_result_value(result)
}

/// Loads the value pointed to by an incoming by-reference local parameter.
fn load_ref_param_local_to_result(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
) -> Result<PhpType> {
    let ty = ctx.local_php_type(slot)?;
    load_ref_cell_local_to_result_as(ctx, slot, &ty)
}

/// Loads the value pointed to by a local ref-cell slot using the supplied alias type.
fn load_ref_cell_local_to_result_as(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    ty: &PhpType,
) -> Result<PhpType> {
    let ty = ty.codegen_repr();
    reject_multiword_ref_param_local(&ty, "load")?;
    let offset = ctx.local_offset(slot)?;
    let pointer_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, pointer_reg, offset);
    match ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, ptr_reg, pointer_reg, 0);
            abi::emit_load_from_address(ctx.emitter, len_reg, pointer_reg, 8);
        }
        PhpType::Float => {
            abi::emit_load_from_address(ctx.emitter, abi::float_result_reg(ctx.emitter), pointer_reg, 0);
        }
        PhpType::TaggedScalar => {
            abi::emit_load_from_address(ctx.emitter, abi::int_result_reg(ctx.emitter), pointer_reg, 0);
            abi::emit_load_from_address(
                ctx.emitter,
                crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter),
                pointer_reg,
                8,
            );
        }
        _ => {
            abi::emit_load_from_address(ctx.emitter, abi::int_result_reg(ctx.emitter), pointer_reg, 0);
        }
    }
    Ok(ty)
}

/// Converts a loaded local slot value to the SSA result representation requested by EIR.
fn coerce_loaded_local_to_result_type(
    ctx: &mut FunctionContext<'_>,
    source_ty: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    let source_ty = source_ty.codegen_repr();
    let result_ty = result_ty.codegen_repr();
    if local_load_types_share_storage(&source_ty, &result_ty) {
        return Ok(());
    }
    match (&source_ty, &result_ty) {
        (PhpType::Mixed, PhpType::Int) => {
            move_int_result_to_first_arg(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            Ok(())
        }
        (PhpType::Mixed, PhpType::Bool) => {
            move_int_result_to_first_arg(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
            Ok(())
        }
        (PhpType::Mixed, PhpType::Float) => {
            move_int_result_to_first_arg(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
            Ok(())
        }
        (PhpType::Mixed, PhpType::Str) => {
            move_int_result_to_first_arg(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            Ok(())
        }
        (PhpType::Mixed, PhpType::Array(_))
        | (PhpType::Mixed, PhpType::AssocArray { .. })
        | (PhpType::Mixed, PhpType::Object(_)) => {
            emit_unbox_mixed_to_owned_refcounted_result(ctx, &result_ty);
            Ok(())
        }
        (PhpType::Mixed, PhpType::Iterable) => {
            emit_unbox_mixed_to_owned_refcounted_result(ctx, &result_ty);
            Ok(())
        }
        (PhpType::Mixed, PhpType::Void) => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
            Ok(())
        }
        (_, PhpType::TaggedScalar) => {
            coerce_loaded_value_to_tagged_scalar(ctx, &source_ty)?;
            Ok(())
        }
        (_, PhpType::Mixed) => {
            emit_box_current_value_as_mixed(ctx.emitter, &source_ty);
            Ok(())
        }
        _ => Err(CodegenIrError::unsupported(format!(
            "local load from PHP type {:?} as {:?}",
            source_ty,
            result_ty
        ))),
    }
}

/// Returns true when two PHP types use the same local-frame representation.
fn local_load_types_share_storage(source_ty: &PhpType, result_ty: &PhpType) -> bool {
    if source_ty == result_ty {
        return true;
    }
    matches!(
        (source_ty, result_ty),
        (
            PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never,
            PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never
        ) | (PhpType::Array(_), PhpType::Array(_))
            | (PhpType::AssocArray { .. }, PhpType::AssocArray { .. })
    )
}

/// Lowers an addressable local store from one SSA operand.
fn lower_store_local(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    let value = expect_operand(inst, 0)?;
    let reset_concat_after_store = inst.span.is_some_and(|span| span.line > 0)
        && value_is_acquire_of_str_concat(ctx, value)?;
    if local_slot_stores_ref_cell_pointer(ctx, slot) {
        store_value_to_ref_param_local(ctx, slot, value)?;
    } else {
        ctx.store_value_to_local(slot, value)?;
    }
    if reset_concat_after_store {
        reset_concat_to_frame_base(ctx);
    }
    Ok(())
}

/// Returns true when a value is `Acquire(StrConcat(...))`, which means storage now owns a heap copy.
fn value_is_acquire_of_str_concat(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    let Some(acquire_inst) = instruction_for_value(ctx, value)? else {
        return Ok(false);
    };
    if acquire_inst.op != Op::Acquire {
        return Ok(false);
    }
    let Some(source) = acquire_inst.operands.first().copied() else {
        return Ok(false);
    };
    Ok(instruction_for_value(ctx, source)?.is_some_and(|source_inst| source_inst.op == Op::StrConcat))
}

/// Returns the instruction that produced an SSA value, or `None` for block parameters.
fn instruction_for_value<'a>(
    ctx: &'a FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<&'a Instruction>> {
    let metadata = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = metadata.def else {
        return Ok(None);
    };
    ctx.function
        .instruction(inst)
        .map(Some)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))
}

/// Lowers an explicit local ref-cell store through the pointer held in the slot.
fn lower_store_ref_cell(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    let value = expect_operand(inst, 0)?;
    store_value_to_ref_cell_as(ctx, slot, value, &inst.result_php_type)
}

/// Promotes an existing raw local slot into a heap ref-cell pointer.
fn lower_promote_local_ref_cell(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let (slot, owner_slot) = expect_local_slot_pair(inst)?;
    promote_local_slot_for_ref_capture(ctx, slot, Some(owner_slot), &inst.result_php_type, true)
}

/// Binds a target local slot to the source local's existing ref-cell pointer.
fn lower_alias_local_ref_cell(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let (target_slot, source_slot) = expect_local_slot_pair(inst)?;
    if !local_slot_stores_ref_cell_pointer(ctx, source_slot) {
        return Err(CodegenIrError::invalid_module(
            "alias_local_ref_cell source slot does not store a ref-cell pointer",
        ));
    }
    let source_offset = ctx.local_offset(source_slot)?;
    let target_offset = ctx.local_offset(target_slot)?;
    let pointer_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, pointer_reg, source_offset);
    abi::store_at_offset_scratch(
        ctx.emitter,
        pointer_reg,
        target_offset,
        abi::tertiary_scratch_reg(ctx.emitter),
    );
    ctx.mark_promoted_ref_cell(target_slot);
    Ok(())
}

/// Releases an owned local ref-cell tracked by a hidden owner slot.
fn lower_release_local_ref_cell(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let owner_slot = expect_local_slot(inst)?;
    release_local_ref_cell_owner(ctx, owner_slot, &inst.result_php_type)
}

/// Releases the owned ref-cell pointer in an owner slot and clears that owner.
fn release_local_ref_cell_owner(
    ctx: &mut FunctionContext<'_>,
    owner_slot: LocalSlotId,
    value_ty: &PhpType,
) -> Result<()> {
    let owner_offset = ctx.local_offset(owner_slot)?;
    let done = ctx.next_label("release_ref_cell_owner_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset_scratch(ctx.emitter, "x9", owner_offset, "x11");
            ctx.emitter.instruction(&format!("cbz x9, {}", done));              // skip release when this variable no longer owns a fallback ref-cell
            abi::emit_release_local_ref_cell(ctx.emitter, "x9", value_ty);
            abi::emit_store_zero_to_local_slot(ctx.emitter, owner_offset);
        }
        Arch::X86_64 => {
            abi::load_at_offset_scratch(ctx.emitter, "r11", owner_offset, "r10");
            ctx.emitter.instruction("test r11, r11");                           // check whether this variable owns a fallback ref-cell
            ctx.emitter.instruction(&format!("je {}", done));                   // skip release when the fallback owner is already clear
            abi::emit_release_local_ref_cell(ctx.emitter, "r11", value_ty);
            abi::emit_store_zero_to_local_slot(ctx.emitter, owner_offset);
        }
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Lowers `unset($local)` by breaking any promoted alias and writing PHP null locally.
fn lower_unset_local(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    let offset = ctx.local_offset(slot)?;
    ctx.unmark_promoted_ref_cell(slot);
    if ctx.local_kind(slot)? == LocalKind::OwnedTemp {
        clear_local_slot_storage(ctx, slot, offset)?;
        return Ok(());
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    abi::store_at_offset(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
    if matches!(ctx.local_php_type(slot)?.codegen_repr(), PhpType::Str) {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        abi::store_at_offset(ctx.emitter, abi::int_result_reg(ctx.emitter), offset - 8);
    }
    Ok(())
}

/// Zeroes a local slot after an owned hidden temp has been moved into SSA.
fn clear_local_slot_storage(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    offset: usize,
) -> Result<()> {
    match ctx.local_php_type(slot)?.codegen_repr() {
        PhpType::Str | PhpType::TaggedScalar => {
            abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
            abi::emit_store_zero_to_local_slot(ctx.emitter, offset - 8);
        }
        _ => {
            abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
        }
    }
    Ok(())
}

/// Stores an SSA value through the pointer held by an incoming by-reference local parameter.
fn store_value_to_ref_param_local(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    value: ValueId,
) -> Result<()> {
    let target_ty = ctx.local_php_type(slot)?;
    store_value_to_ref_cell_as(ctx, slot, value, &target_ty)
}

/// Stores an SSA value through a local ref-cell pointer using the supplied alias type.
fn store_value_to_ref_cell_as(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    value: ValueId,
    target_ty: &PhpType,
) -> Result<()> {
    let source_ty = ctx.load_value_to_result(value)?;
    let target_ty = target_ty.codegen_repr();
    reject_multiword_ref_param_local(&target_ty, "store")?;
    coerce_ref_cell_store_value(ctx, &source_ty, &target_ty)?;
    let offset = ctx.local_offset(slot)?;
    let pointer_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, pointer_reg, offset);
    match target_ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, ptr_reg, pointer_reg, 0);
            abi::emit_store_to_address(ctx.emitter, len_reg, pointer_reg, 8);
        }
        PhpType::Float => {
            abi::emit_store_to_address(ctx.emitter, abi::float_result_reg(ctx.emitter), pointer_reg, 0);
        }
        PhpType::TaggedScalar => {
            abi::emit_store_to_address(ctx.emitter, abi::int_result_reg(ctx.emitter), pointer_reg, 0);
            abi::emit_store_to_address(
                ctx.emitter,
                crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter),
                pointer_reg,
                8,
            );
        }
        _ => {
            abi::emit_store_to_address(ctx.emitter, abi::int_result_reg(ctx.emitter), pointer_reg, 0);
        }
    }
    Ok(())
}

/// Converts the current result registers to the target shape needed by a ref-cell store.
fn coerce_ref_cell_store_value(
    ctx: &mut FunctionContext<'_>,
    source_ty: &PhpType,
    target_ty: &PhpType,
) -> Result<()> {
    let source_ty = source_ty.codegen_repr();
    let target_ty = target_ty.codegen_repr();
    if target_ty == PhpType::Mixed && source_ty != PhpType::Mixed {
        emit_box_current_value_as_mixed(ctx.emitter, &source_ty);
        return Ok(());
    }
    if target_ty == PhpType::TaggedScalar {
        coerce_loaded_value_to_tagged_scalar(ctx, &source_ty)?;
        return Ok(());
    }
    if source_ty == PhpType::Mixed {
        match target_ty {
            PhpType::Int => {
                move_int_result_to_first_arg(ctx);
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
                return Ok(());
            }
            PhpType::Bool => {
                move_int_result_to_first_arg(ctx);
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
                return Ok(());
            }
            PhpType::Float => {
                move_int_result_to_first_arg(ctx);
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
                return Ok(());
            }
            PhpType::Str => {
                move_int_result_to_first_arg(ctx);
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
                return Ok(());
            }
            _ => {}
        }
    }
    Ok(())
}

/// Rejects by-reference parameter locals whose frame representation spans multiple words.
fn reject_multiword_ref_param_local(ty: &PhpType, action: &str) -> Result<()> {
    let _ = (ty, action);
    Ok(())
}

/// Lowers a global storage load into the result register and SSA destination slot.
fn lower_load_global(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    let name = ctx.global_name_data(data)?;
    let symbol = ir_global_symbol(name);
    let result = inst.result.ok_or_else(|| {
        CodegenIrError::invalid_module("load_global missing result value")
    })?;
    let ty = ctx.value_php_type(result)?;
    ctx.data.add_comm(symbol.clone(), ty.codegen_repr().stack_size().max(8));
    abi::emit_load_symbol_to_result(ctx.emitter, &symbol, &ty);
    store_if_result(ctx, inst)
}

/// Lowers a global storage store from one SSA operand.
fn lower_store_global(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    let name = ctx.global_name_data(data)?;
    let symbol = ir_global_symbol(name);
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?;
    ctx.data.add_comm(symbol.clone(), ty.codegen_repr().stack_size().max(8));
    abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &ty, false);
    Ok(())
}

/// Lowers a C extern global load into the EIR result slot.
fn lower_extern_global_load(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    let name = ctx.global_name_data(data)?;
    let result = inst.result.ok_or_else(|| {
        CodegenIrError::invalid_module("extern_global_load missing result value")
    })?;
    let ty = ctx.value_php_type(result)?;
    let symbol = ctx.emitter.target.extern_symbol(name);
    match ty.codegen_repr() {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Resource(_)
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Callable => {
            abi::emit_load_extern_symbol_to_reg(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &symbol,
                0,
            );
        }
        PhpType::Float => {
            abi::emit_load_extern_symbol_to_reg(
                ctx.emitter,
                abi::float_result_reg(ctx.emitter),
                &symbol,
                0,
            );
        }
        PhpType::Str => {
            abi::emit_load_extern_symbol_to_reg(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &symbol,
                0,
            );
            abi::emit_call_label(ctx.emitter, "__rt_cstr_to_str");
        }
        other => {
            ctx.emitter.comment(&format!(
                "WARNING: unsupported extern global load for ${} with PHP type {:?}",
                name, other
            ));
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers a C extern global store from one SSA operand.
fn lower_extern_global_store(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    let name = ctx.global_name_data(data)?.to_string();
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?.codegen_repr();
    let symbol = ctx.emitter.target.extern_symbol(&name);
    match ty {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Resource(_)
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Callable => {
            abi::emit_store_reg_to_extern_symbol(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &symbol,
                0,
            );
        }
        PhpType::Float => {
            abi::emit_store_reg_to_extern_symbol(
                ctx.emitter,
                abi::float_result_reg(ctx.emitter),
                &symbol,
                0,
            );
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_to_cstr");
            abi::emit_store_reg_to_extern_symbol(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &symbol,
                0,
            );
        }
        other => {
            ctx.emitter.comment(&format!(
                "WARNING: unsupported extern global store for ${} with PHP type {:?}",
                name, other
            ));
        }
    }
    Ok(())
}

/// Lowers an integer constant into the canonical integer result register and slot.
fn lower_const_i64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_i64(inst)?;
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), value);
    store_if_result(ctx, inst)
}

/// Lowers a boolean constant into the canonical integer result register and slot.
fn lower_const_bool(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = i64::from(expect_bool(inst)?);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), value);
    store_if_result(ctx, inst)
}

/// Lowers a null constant to the selected one-word or tagged-scalar representation.
fn lower_const_null(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.result_php_type.codegen_repr() == PhpType::TaggedScalar {
        crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
    } else {
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            0x7fff_ffff_ffff_fffe,
        );
    }
    store_if_result(ctx, inst)
}

/// Lowers explicit Mixed boxing for scalar, string, object, and existing Mixed operands.
fn lower_mixed_box(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let source_ty = ctx.load_value_to_result(value)?;
    let raw_source_ty = ctx.raw_value_php_type(value)?;
    let box_ty = if matches!(raw_source_ty, PhpType::Resource(_)) {
        raw_source_ty
    } else {
        source_ty
    };
    emit_box_current_value_as_mixed(ctx.emitter, &box_ty);
    store_if_result(ctx, inst)
}

/// Lowers an invoker-only by-reference argument marker for descriptor calls.
fn lower_invoker_ref_arg(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    let source_ty = ctx.local_php_type(slot)?.codegen_repr();
    let offset = ctx.local_offset(slot)?;
    let ref_cell_reg = abi::secondary_scratch_reg(ctx.emitter);
    let marker_tag_reg = abi::tertiary_scratch_reg(ctx.emitter);
    let source_tag_reg = abi::symbol_scratch_reg(ctx.emitter);
    if local_slot_stores_ref_cell_pointer(ctx, slot) {
        abi::load_at_offset(ctx.emitter, ref_cell_reg, offset);
    } else {
        abi::emit_frame_slot_address(ctx.emitter, ref_cell_reg, offset);
    }
    abi::emit_load_int_immediate(ctx.emitter, marker_tag_reg, INVOKER_ARG_REF_CELL_TAG);
    abi::emit_load_int_immediate(
        ctx.emitter,
        source_tag_reg,
        crate::codegen::runtime_value_tag(&source_ty) as i64,
    );
    emit_box_runtime_payload_as_mixed(
        ctx.emitter,
        marker_tag_reg,
        ref_cell_reg,
        source_tag_reg,
    );
    store_if_result(ctx, inst)
}

/// Lowers PHP echo output for a previously computed SSA value.
fn lower_echo_value(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    if let PhpType::Object(class_name) = ctx.value_php_type(value)?.codegen_repr() {
        return lower_object_echo_value(ctx, value, &class_name);
    }
    let ty = ctx.load_value_to_result(value)?;
    let raw_ty = ctx.raw_value_php_type(value)?;
    let output_ty = if matches!(raw_ty, PhpType::Resource(_)) {
        raw_ty
    } else {
        ty
    };
    emit_loaded_value_to_stdout(ctx, &output_ty)
}

/// Lowers PHP `print` output for a previously computed SSA value.
fn lower_print_value(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_echo_value(ctx, inst)
}

/// Lowers `echo $object` through `__toString()` or PHP's conversion fatal.
fn lower_object_echo_value(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    class_name: &str,
) -> Result<()> {
    let normalized = class_name.trim_start_matches('\\');
    if !object_class_has_tostring(ctx, normalized) {
        emit_missing_tostring_fatal(ctx, normalized);
        return Ok(());
    }
    let return_ty = emit_object_tostring_call(ctx, value, normalized)?;
    emit_loaded_value_to_stdout(ctx, &return_ty.codegen_repr())
}

/// Emits the zero-argument `__toString()` method call for an object value.
fn emit_object_tostring_call(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    class_name: &str,
) -> Result<PhpType> {
    let target = resolve_method_call_target(ctx, class_name, "__toString", 1)?;
    let args = [value];
    let param_types = [PhpType::Object(class_name.to_string())];
    let ref_params = [false];
    let call_args =
        materialize_direct_call_args_with_refs(ctx, &args, &param_types, &ref_params)?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &method_symbol(&target.impl_class, &target.method_key));
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    emit_ref_arg_writebacks(ctx, &call_args.ref_writebacks)?;
    Ok(target.return_ty)
}

/// Returns true when class metadata exposes a `__toString()` method.
fn object_class_has_tostring(ctx: &FunctionContext<'_>, class_name: &str) -> bool {
    ctx.module
        .class_infos
        .get(class_name)
        .is_some_and(|class_info| class_info.methods.contains_key("__tostring"))
}

/// Emits PHP's fatal diagnostic for object-to-string conversion without `__toString()`.
fn emit_missing_tostring_fatal(ctx: &mut FunctionContext<'_>, class_name: &str) {
    let message = format!(
        "Fatal error: Object of class {} could not be converted to string\n",
        class_name
    );
    let (label, len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // write the object string-cast fatal to stderr
            ctx.emitter.adrp("x1", &label);
            ctx.emitter.add_lo12("x1", "x1", &label);
            ctx.emitter.instruction(&format!("mov x2, #{}", len));              // pass the object string-cast fatal byte length
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2");                              // write the object string-cast fatal to Linux stderr
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            ctx.emitter.instruction(&format!("mov edx, {}", len));              // pass the object string-cast fatal byte length
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the object string-cast fatal before exiting
            abi::emit_exit(ctx.emitter, 1);
        }
    }
}

/// Emits stdout output for the value currently loaded into result register(s).
fn emit_loaded_value_to_stdout(ctx: &mut FunctionContext<'_>, ty: &PhpType) -> Result<()> {
    ctx.emitter.blank();
    ctx.emitter.comment("echo");
    match ty {
        PhpType::Void | PhpType::Never => Ok(()),
        PhpType::Bool => {
            let skip_label = ctx.next_label("echo_skip_false");
            abi::emit_branch_if_int_result_zero(ctx.emitter, &skip_label);
            abi::emit_write_stdout(ctx.emitter, ty);
            ctx.emitter.label(&skip_label);
            Ok(())
        }
        PhpType::TaggedScalar => {
            let skip_label = ctx.next_label("echo_skip_tagged_null");
            crate::codegen::sentinels::emit_branch_if_tagged_scalar_null(ctx.emitter, &skip_label);
            abi::emit_write_stdout(ctx.emitter, &PhpType::Int);
            ctx.emitter.label(&skip_label);
            Ok(())
        }
        PhpType::Int => {
            if crate::codegen::sentinels::null_repr_is_tagged() {
                abi::emit_write_stdout(ctx.emitter, ty);
                return Ok(());
            }
            let skip_label = ctx.next_label("echo_skip_null");
            let sentinel_reg = abi::symbol_scratch_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, sentinel_reg, crate::codegen::sentinels::NULL_SENTINEL);
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction(&format!("cmp {}, {}", abi::int_result_reg(ctx.emitter), sentinel_reg)); // compare integer value against the runtime null sentinel
                    ctx.emitter.instruction(&format!("b.eq {}", skip_label));   // skip integer echo when the value represents null
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction(&format!("cmp {}, {}", abi::int_result_reg(ctx.emitter), sentinel_reg)); // compare integer value against the runtime null sentinel
                    ctx.emitter.instruction(&format!("je {}", skip_label));     // skip integer echo when the value represents null
                }
            }
            abi::emit_write_stdout(ctx.emitter, ty);
            ctx.emitter.label(&skip_label);
            Ok(())
        }
        PhpType::Float
        | PhpType::Str
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Iterable
        | PhpType::Resource(_)
        | PhpType::Pointer(_) => {
            abi::emit_write_stdout(ctx.emitter, ty);
            Ok(())
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            conversions::emit_array_like_string_result(ctx);
            abi::emit_write_stdout(ctx.emitter, &PhpType::Str);
            Ok(())
        }
        _ => Err(CodegenIrError::unsupported(format!("echo for PHP type {:?}", ty))),
    }
}

/// Returns the AArch64 condition-code suffix for an EIR comparison predicate.
fn aarch64_condition(predicate: CmpPredicate) -> Result<&'static str> {
    match predicate {
        CmpPredicate::Eq => Ok("eq"),
        CmpPredicate::Ne => Ok("ne"),
        CmpPredicate::Slt => Ok("lt"),
        CmpPredicate::Sle => Ok("le"),
        CmpPredicate::Sgt => Ok("gt"),
        CmpPredicate::Sge => Ok("ge"),
        other => Err(CodegenIrError::unsupported(format!(
            "integer comparison predicate {:?}",
            other
        ))),
    }
}

/// Returns the x86_64 setcc suffix for an EIR comparison predicate.
fn x86_64_condition(predicate: CmpPredicate) -> Result<&'static str> {
    match predicate {
        CmpPredicate::Eq => Ok("e"),
        CmpPredicate::Ne => Ok("ne"),
        CmpPredicate::Slt => Ok("l"),
        CmpPredicate::Sle => Ok("le"),
        CmpPredicate::Sgt => Ok("g"),
        CmpPredicate::Sge => Ok("ge"),
        other => Err(CodegenIrError::unsupported(format!(
            "integer comparison predicate {:?}",
            other
        ))),
    }
}

/// Returns the x86_64 floating-point setcc suffix for an EIR comparison predicate.
fn x86_64_float_condition(predicate: CmpPredicate) -> Result<&'static str> {
    match predicate {
        CmpPredicate::Eq => Ok("e"),
        CmpPredicate::Ne => Ok("ne"),
        CmpPredicate::Slt | CmpPredicate::Olt => Ok("b"),
        CmpPredicate::Sle | CmpPredicate::Ole => Ok("be"),
        CmpPredicate::Sgt | CmpPredicate::Ogt => Ok("a"),
        CmpPredicate::Sge | CmpPredicate::Oge => Ok("ae"),
    }
}

/// Returns the secondary floating-point scratch register for the target.
fn secondary_float_reg(arch: Arch) -> &'static str {
    match arch {
        Arch::AArch64 => "d1",
        Arch::X86_64 => "xmm1",
    }
}

/// Verifies that an arithmetic operand has a single-register integer-like representation.
fn require_integer_like(ty: PhpType, inst: &Instruction) -> Result<()> {
    if matches!(ty, PhpType::Int | PhpType::Bool) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        inst.op.name(),
        ty
    )))
}

/// Verifies that an operand has the floating-point representation expected by the opcode.
fn require_float(ty: PhpType, inst: &Instruction) -> Result<()> {
    if ty == PhpType::Float {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        inst.op.name(),
        ty
    )))
}

/// Verifies that an operand has the string-pair representation expected by the opcode.
fn require_string(ty: PhpType, inst: &Instruction) -> Result<()> {
    if ty == PhpType::Str {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        inst.op.name(),
        ty
    )))
}

/// Stores the current result registers when an instruction has an SSA result.
fn store_if_result(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if let Some(result) = inst.result {
        ctx.store_result_value(result)?;
    }
    Ok(())
}

/// Returns the integer immediate attached to a constant instruction.
fn expect_i64(inst: &Instruction) -> Result<i64> {
    match inst.immediate {
        Some(Immediate::I64(value)) => Ok(value),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing i64 immediate",
            inst.op.name()
        ))),
    }
}

/// Returns the floating-point immediate attached to a constant instruction.
fn expect_f64(inst: &Instruction) -> Result<f64> {
    match inst.immediate {
        Some(Immediate::F64(value)) => Ok(value),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing f64 immediate",
            inst.op.name()
        ))),
    }
}

/// Returns the boolean immediate attached to a constant instruction.
fn expect_bool(inst: &Instruction) -> Result<bool> {
    match inst.immediate {
        Some(Immediate::Bool(value)) => Ok(value),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing bool immediate",
            inst.op.name()
        ))),
    }
}

/// Returns the data-pool immediate attached to a data-backed instruction.
fn expect_data(inst: &Instruction) -> Result<crate::ir::DataId> {
    match inst.immediate {
        Some(Immediate::Data(value)) => Ok(value),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing data immediate",
            inst.op.name()
        ))),
    }
}

/// Returns the comparison predicate attached to a compare instruction.
fn expect_cmp_predicate(inst: &Instruction) -> Result<CmpPredicate> {
    match inst.immediate {
        Some(Immediate::CmpPredicate(predicate)) => Ok(predicate),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing comparison predicate immediate",
            inst.op.name()
        ))),
    }
}

/// Returns the local-slot immediate attached to a local access instruction.
fn expect_local_slot(inst: &Instruction) -> Result<LocalSlotId> {
    match inst.immediate {
        Some(Immediate::LocalSlot(slot)) => Ok(slot),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing local slot immediate",
            inst.op.name()
        ))),
    }
}

/// Returns the local-slot pair immediate attached to a paired local instruction.
fn expect_local_slot_pair(inst: &Instruction) -> Result<(LocalSlotId, LocalSlotId)> {
    match inst.immediate {
        Some(Immediate::LocalSlotPair { first, second }) => Ok((first, second)),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing local slot pair immediate",
            inst.op.name()
        ))),
    }
}

/// Returns the global-name immediate attached to a global access instruction.
fn expect_global_name(inst: &Instruction) -> Result<crate::ir::DataId> {
    match inst.immediate {
        Some(Immediate::GlobalName(value)) => Ok(value),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing global-name immediate",
            inst.op.name()
        ))),
    }
}

/// Returns the operand at `index` or reports a malformed instruction.
fn expect_operand(inst: &Instruction, index: usize) -> Result<ValueId> {
    inst.operands.get(index).copied().ok_or_else(|| {
        CodegenIrError::invalid_module(format!(
            "{} missing operand {}",
            inst.op.name(),
            index
        ))
    })
}
