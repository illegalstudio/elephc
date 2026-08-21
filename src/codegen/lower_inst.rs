//! Purpose:
//! Lowers individual EIR instructions into target-aware assembly snippets.
//! Starts with scalar constants and output needed for the first executable smoke test.
//!
//! Called from:
//! - `crate::codegen::block_emit`.
//!
//! Key details:
//! - Results are written to fixed value-placement slots immediately after definition.
//! - Unsupported opcodes fail explicitly instead of silently emitting invalid code.

use crate::codegen::platform::Arch;
use crate::codegen::{
    abi, callable_descriptor, callable_invoker_args, emit_box_current_owned_value_as_mixed,
    emit_box_current_value_as_mixed, emit_box_runtime_payload_as_mixed, runtime,
    runtime_value_tag,
};
use crate::intrinsics::{IntrinsicCall, IntrinsicCallKind};
use crate::ir::{
    BlockId, Builder, CmpPredicate, Function, FunctionParam, Immediate, InstId, Instruction,
    IrType, LocalKind, LocalSlotId, Module, Op, Ownership, Terminator, ValueDef, ValueId,
};
use crate::names::{
    function_symbol, ir_global_symbol, method_symbol, php_symbol_key,
    static_method_symbol,
};
use crate::types::{callable_wrapper_sig, first_class_callable_builtin_sig, FunctionSig, PhpType};

use super::context::FunctionContext;
use super::function_variants;
use super::{CodegenIrError, Result};

mod arithmetic;
mod arrays;
mod buffers;
mod checked_int_to_int;
mod runtime_functions;
pub(crate) mod builtins;
mod callables;
mod comparisons;
mod conversions;
mod enums;
mod exceptions;
mod externs;
mod floats;
mod hashes;
mod internal_extensions;
mod iterators;
mod objects;
mod ownership;
mod pointers;
mod predicates;
mod property_values;
mod receiver_place;
mod runtime_calls;
mod scoped_constants;
mod static_locals;
mod static_properties;
mod strings;
mod array_access_runtime;
mod call_cleanup;
mod call_operands;
mod callable_descriptors;
mod core_closures;
mod core_includes;
mod core_misc;
mod descriptor_arguments;
mod descriptor_entries;
mod descriptor_metadata;
mod direct_calls;
mod exception_instructions;
mod fiber_methods;
mod generator_instructions;
mod globals_constants;
mod instruction_helpers;
mod local_loads;
mod local_stores;
mod method_call_types;
mod method_dispatch;
mod method_intrinsics;
mod method_resolution;
mod mixed_array_runtime;
mod mixed_property_runtime;
mod output_values;
mod reference_arguments;
mod runtime_wrappers;
mod static_method_calls;
mod throwable_methods;

use array_access_runtime::*;
use call_cleanup::*;
use call_operands::*;
use callable_descriptors::*;
use core_closures::*;
use core_includes::*;
use core_misc::*;
use descriptor_arguments::*;
use descriptor_entries::*;
use descriptor_metadata::*;
use direct_calls::*;
use exception_instructions::*;
use fiber_methods::*;
use generator_instructions::*;
use globals_constants::*;
use instruction_helpers::*;
use local_loads::*;
use local_stores::*;
use method_call_types::*;
use method_dispatch::*;
use method_intrinsics::*;
use method_resolution::*;
use mixed_array_runtime::*;
use mixed_property_runtime::*;
use output_values::*;
use reference_arguments::*;
use runtime_wrappers::*;
use static_method_calls::*;
use throwable_methods::*;

pub(super) use array_access_runtime::lower_runtime_object_method_call;
pub(super) use call_operands::{
    direct_call_stack_pad_bytes, emit_mixed_string_for_persistent_store,
    load_value_to_first_int_arg, resolve_int_operand_to_result,
};
pub(in crate::codegen) use builtins::emit_count_countable_guard_from_result;
pub(in crate::codegen) use conversions::{
    emit_mixed_string_dispatch_from_result, MixedStringContextMode,
};
pub(super) use core_closures::function_signature_from_eir;
pub(super) use descriptor_entries::emit_static_method_descriptor_entry_wrapper;
pub(super) use descriptor_metadata::{
    class_method_body_exists, emit_runtime_descriptor_with_receiver_capture,
};
pub(super) use direct_calls::{
    coerce_loaded_value_to_tagged_scalar, materialize_direct_call_args,
};
pub(super) use instruction_helpers::instruction_strict_php_profile;
pub(super) use local_loads::coerce_loaded_local_to_result_type;
pub(super) use runtime_wrappers::{
    emit_runtime_builtin_wrapper_inline, emit_runtime_extern_wrapper_inline,
    runtime_builtin_wrapper_sig,
};

const CALLED_CLASS_ID_PARAM: &str = "__elephc_called_class_id";
const BORROWED_MIXED_ARG_CELL_BYTES: usize = 32;

/// Emits the module-wide DOM XPath callable resolvers when the runtime bridge is required.
///
/// Native DOM callbacks reference these symbols even when wrapper construction or
/// boxed dispatch reaches the bridge before a direct internal-extension call.
pub(super) fn ensure_dom_xpath_callable_resolvers(
    ctx: &mut FunctionContext<'_>,
) -> Result<()> {
    if ctx.module.required_runtime_features.dom_bridge {
        internal_extensions::emit_dom_xpath_callable_resolver(ctx)?;
    }
    Ok(())
}

/// Lowers one EIR instruction by opcode.
pub(super) fn lower_instruction(ctx: &mut FunctionContext<'_>, inst_id: InstId) -> Result<()> {
    ctx.begin_instruction(inst_id);
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
        Op::LoadCalledClassId => strings::lower_load_called_class_id(ctx, &inst),
        Op::LoadLocal => lower_load_local(ctx, &inst),
        Op::StoreLocal => lower_store_local(ctx, &inst),
        Op::UnsetLocal => lower_unset_local(ctx, &inst),
        Op::LoadRefCell => lower_load_ref_cell(ctx, &inst),
        Op::StoreRefCell => lower_store_ref_cell(ctx, &inst),
        Op::PromoteLocalRefCell => lower_promote_local_ref_cell(ctx, &inst),
        Op::AliasLocalRefCell => lower_alias_local_ref_cell(ctx, &inst),
        Op::ReleaseLocalRefCell => lower_release_local_ref_cell(ctx, &inst),
        Op::ReleaseLocalSlot => lower_release_local_slot(ctx, inst_id, &inst),
        Op::LoadGlobal => lower_load_global(ctx, &inst),
        Op::StoreGlobal => lower_store_global(ctx, &inst),
        Op::ExternGlobalLoad => lower_extern_global_load(ctx, &inst),
        Op::ExternGlobalStore => lower_extern_global_store(ctx, &inst),
        Op::IAdd => arithmetic::lower_int_binop(ctx, &inst, "add", "add"),
        Op::ISub => arithmetic::lower_int_binop(ctx, &inst, "sub", "sub"),
        Op::IMul => arithmetic::lower_int_binop(ctx, &inst, "mul", "imul"),
        Op::ICheckedAdd => arithmetic::lower_int_checked_binop(ctx, &inst, "__rt_int_add_checked"),
        Op::ICheckedSub => arithmetic::lower_int_checked_binop(ctx, &inst, "__rt_int_sub_checked"),
        Op::ICheckedMul => arithmetic::lower_int_checked_binop(ctx, &inst, "__rt_int_mul_checked"),
        Op::ICheckedAddToInt => checked_int_to_int::lower_checked_int_to_int(
            ctx,
            &inst,
            checked_int_to_int::CheckedIntOp::Add,
        ),
        Op::ICheckedSubToInt => checked_int_to_int::lower_checked_int_to_int(
            ctx,
            &inst,
            checked_int_to_int::CheckedIntOp::Sub,
        ),
        Op::ICheckedMulToInt => checked_int_to_int::lower_checked_int_to_int(
            ctx,
            &inst,
            checked_int_to_int::CheckedIntOp::Mul,
        ),
        Op::ICheckedPow => arithmetic::lower_int_checked_binop(ctx, &inst, "__rt_int_pow_checked"),
        Op::IDiv => arithmetic::lower_int_div_to_float(ctx, &inst),
        Op::ISMod => arithmetic::lower_int_mod(ctx, &inst),
        Op::INeg => arithmetic::lower_int_unary(ctx, &inst, "neg", "neg"),
        Op::IBitAnd => arithmetic::lower_int_binop(ctx, &inst, "and", "and"),
        Op::IBitOr => arithmetic::lower_int_binop(ctx, &inst, "orr", "or"),
        Op::IBitXor => arithmetic::lower_int_binop(ctx, &inst, "eor", "xor"),
        Op::IBitNot => arithmetic::lower_int_unary(ctx, &inst, "mvn", "not"),
        Op::IShl => arithmetic::lower_int_shift(ctx, &inst, true),
        Op::IShrA => arithmetic::lower_int_shift(ctx, &inst, false),
        Op::MixedNumericBinop => arithmetic::lower_mixed_numeric_binop(ctx, &inst),
        Op::StrIncDec => strings::lower_str_inc_dec(ctx, &inst),
        Op::FAdd => floats::lower_float_binop(ctx, &inst, "fadd", "addsd"),
        Op::FSub => floats::lower_float_binop(ctx, &inst, "fsub", "subsd"),
        Op::FMul => floats::lower_float_binop(ctx, &inst, "fmul", "mulsd"),
        Op::FDiv => arithmetic::lower_float_div(ctx, &inst),
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
        Op::TypePredicate => builtins::lower_type_predicate(ctx, &inst),
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
        Op::MixedClone => lower_mixed_clone(ctx, &inst),
        Op::MixedUnbox => lower_mixed_unbox(ctx, &inst),
        Op::InvokerRefArg => lower_invoker_ref_arg(ctx, &inst),
        Op::ArrayToMixed => arrays::lower_array_to_mixed(ctx, &inst),
        Op::HashToMixed => hashes::lower_hash_to_mixed(ctx, &inst),
        Op::StrConcat => strings::lower_str_concat(ctx, &inst),
        Op::StrLen => strings::lower_str_len(ctx, &inst),
        Op::StrCharAt => strings::lower_str_char_at(ctx, &inst),
        Op::StrPersist => strings::lower_str_persist(ctx, &inst),
        Op::ArrayNew => arrays::lower_array_new(ctx, &inst),
        Op::ArrayLen => arrays::lower_array_len(ctx, &inst),
        Op::ArrayGet => arrays::lower_array_get(ctx, &inst, true),
        Op::ArrayGetSilent => arrays::lower_array_get(ctx, &inst, false),
        Op::ArrayGetForWrite => arrays::lower_array_get_for_write(ctx, &inst),
        Op::ArrayIsset => builtins::lower_array_isset(ctx, &inst),
        Op::ArrayElemAddr => arrays::lower_array_elem_addr(ctx, &inst),
        Op::ArraySet => arrays::lower_array_set(ctx, &inst),
        Op::SlotDetach => arrays::lower_slot_detach(ctx, &inst),
        Op::ArraySetMixedKey => arrays::lower_array_set_mixed_key(ctx, &inst),
        Op::ArrayGetMixedKey => arrays::lower_array_get_mixed_key(ctx, &inst, true),
        Op::ArrayGetMixedKeySilent => arrays::lower_array_get_mixed_key(ctx, &inst, false),
        Op::ArrayPush => arrays::lower_array_push(ctx, &inst),
        Op::MixedArrayAppend => arrays::lower_mixed_array_append(ctx, &inst),
        Op::ArrayUnion => arrays::lower_array_union(ctx, &inst),
        Op::ArrayHashUnion => arrays::lower_array_hash_union(ctx, &inst),
        Op::ArrayToHash => arrays::lower_array_to_hash(ctx, &inst),
        Op::HashNew => hashes::lower_hash_new(ctx, &inst),
        Op::HashLen => hashes::lower_hash_len(ctx, &inst),
        Op::HashGet => hashes::lower_hash_get(ctx, &inst, true),
        Op::HashGetForWrite => hashes::lower_hash_get_for_write(ctx, &inst),
        Op::HashGetSilent => hashes::lower_hash_get(ctx, &inst, false),
        Op::HashIsset => builtins::lower_hash_isset(ctx, &inst),
        Op::HashSet => hashes::lower_hash_set(ctx, &inst),
        Op::HashUnset => hashes::lower_hash_unset(ctx, &inst),
        Op::HashUnion => hashes::lower_hash_union(ctx, &inst),
        Op::HashArrayUnion => hashes::lower_hash_array_union(ctx, &inst),
        Op::HashSpread => hashes::lower_hash_spread(ctx, &inst),
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
        Op::ObjectCloneShallow => objects::lower_object_clone_shallow(ctx, &inst),
        Op::DynamicObjectNew => objects::lower_dynamic_object_new(ctx, &inst),
        Op::DynamicObjectNewMixed => objects::lower_dynamic_object_new_mixed(ctx, &inst),
        Op::DynamicObjectNewWithoutConstructorMixed => {
            objects::lower_dynamic_object_new_without_constructor_mixed(ctx, &inst)
        }
        Op::CallablePtr => builtins::pointers::lower_elephc_callable_ptr(ctx, &inst),
        Op::NormalizeCallable => builtins::pointers::lower_elephc_normalize_callable(ctx, &inst),
        Op::PdoAdapterAddr => builtins::pointers::lower_elephc_pdo_adapter_addr(ctx, &inst),
        Op::DynamicClassHasConstructor => {
            builtins::system::lower_elephc_class_has_constructor(ctx, &inst)
        }
        Op::DynamicPdoStatementClassStatus => {
            builtins::system::lower_elephc_pdo_statement_class_status(ctx, &inst)
        }
        Op::DynamicPdoCalledClassStatus => {
            builtins::system::lower_elephc_pdo_called_class_status(ctx, &inst)
        }
        Op::DynamicPdoStatementConstructorCall => {
            builtins::system::lower_elephc_invoke_pdo_statement_constructor(ctx, &inst)
        }
        Op::DynamicPdoStatementInitialize => {
            builtins::system::lower_elephc_initialize_pdo_statement(ctx, &inst)
        }
        Op::PropGet => objects::lower_prop_get(ctx, &inst),
        Op::PropGetForWrite => objects::lower_prop_get_for_write(ctx, &inst),
        Op::PropInitialized => objects::lower_prop_initialized(ctx, &inst),
        Op::LoadPropRefCell => objects::lower_load_prop_ref_cell(ctx, &inst),
        Op::LoadArrayElemRefCell => arrays::lower_load_array_elem_ref_cell(ctx, &inst),
        Op::BindRefCellPtr => lower_bind_ref_cell_ptr(ctx, &inst),
        Op::NullsafePropGet => objects::lower_nullsafe_prop_get(ctx, &inst),
        Op::DynamicPropGet => objects::lower_dynamic_prop_get(ctx, &inst),
        Op::PropSet => objects::lower_prop_set(ctx, &inst),
        Op::PropUnset => objects::lower_prop_unset(ctx, &inst),
        Op::DynamicPropSet => objects::lower_dynamic_prop_set(ctx, &inst),
        Op::InstanceOf => objects::lower_instanceof(ctx, &inst),
        Op::InstanceOfDynamic => objects::lower_instanceof_dynamic(ctx, &inst),
        Op::ScopedConstantGet => scoped_constants::lower_scoped_constant_get(ctx, &inst),
        Op::LoadStaticLocal => static_locals::lower_load_static_local(ctx, &inst),
        Op::StoreStaticLocal => static_locals::lower_store_static_local(ctx, &inst),
        Op::InitStaticLocal => static_locals::lower_init_static_local(ctx, &inst),
        Op::LoadStaticProperty => static_properties::lower_load_static_property(ctx, &inst),
        Op::StoreStaticProperty => static_properties::lower_store_static_property(ctx, &inst),
        Op::StaticPropInitialized => {
            static_properties::lower_static_property_initialized(ctx, &inst)
        }
        Op::LoadReflectionStaticProperty => {
            static_properties::lower_load_reflection_static_property(ctx, &inst)
        }
        Op::ReflectionStaticPropertyInitialized => {
            static_properties::lower_reflection_static_property_initialized(ctx, &inst)
        }
        Op::StoreReflectionStaticProperty => {
            static_properties::lower_store_reflection_static_property(ctx, &inst)
        }
        Op::Call => lower_direct_call(ctx, &inst),
        Op::ClosureBind => builtins::lower_closure_bind(ctx, &inst),
        Op::ClosureCall => callables::lower_closure_call(ctx, &inst),
        Op::ExprCall => callables::lower_expr_call(ctx, &inst),
        Op::CallableDescriptorInvoke => callables::lower_callable_descriptor_invoke(ctx, &inst),
        Op::PipeCall => callables::lower_pipe_call(ctx, &inst),
        Op::MethodCall => lower_method_call(ctx, &inst),
        Op::NullsafeMethodCall => lower_nullsafe_method_call(ctx, &inst),
        Op::StaticMethodCall => lower_static_method_call(ctx, &inst),
        Op::EvalStaticMethodCall => lower_eval_static_method_call(ctx, &inst),
        Op::EnumBackingStringToInt => enums::lower_enum_backing_string_to_int(ctx, &inst),
        Op::EnumBackingMixedToInt => enums::lower_enum_backing_mixed_to_int(ctx, &inst),
        Op::ExternCall => externs::lower_extern_call(ctx, &inst),
        Op::InternalExtensionCall => {
            internal_extensions::lower_internal_extension_call(ctx, &inst)
        }
        Op::LanguageConstructCall => builtins::lower_language_construct_call(ctx, &inst),
        Op::EvalLiteralCall => builtins::lower_eval_literal_call(ctx, &inst),
        Op::EvalScopeGet => builtins::lower_eval_scope_get(ctx, &inst),
        Op::EvalScopeSet => builtins::lower_eval_scope_set(ctx, &inst),
        Op::EvalFunctionCall => builtins::lower_eval_function_call(ctx, &inst),
        Op::EvalFunctionCallArray => builtins::lower_eval_function_call_array(ctx, &inst),
        Op::EvalObjectNew => builtins::lower_eval_object_new(ctx, &inst),
        Op::EvalFunctionExists => builtins::lower_eval_function_exists(ctx, &inst),
        Op::EvalClassExists => builtins::lower_eval_class_exists(ctx, &inst),
        Op::EvalConstantExists => builtins::lower_eval_constant_exists(ctx, &inst),
        Op::EvalConstantFetch => builtins::lower_eval_constant_fetch(ctx, &inst),
        Op::ClosureCapture => lower_closure_capture(ctx, &inst),
        Op::ClosureNew => lower_closure_new(ctx, &inst),
        Op::FirstClassCallableNew => lower_first_class_callable_new(ctx, &inst),
        Op::Acquire => ownership::lower_acquire(ctx, &inst),
        Op::Release => ownership::lower_release(ctx, &inst),
        Op::ReleaseUnlessAliases => ownership::lower_release_unless_aliases(ctx, &inst),
        Op::GcCollect => lower_gc_collect(ctx),
        Op::Move | Op::Borrow => ownership::lower_forward(ctx, &inst),
        Op::EchoValue => lower_echo_value(ctx, &inst),
        Op::PrintValue => lower_print_value(ctx, &inst),
        Op::ThrowException => lower_throw_exception(ctx, &inst),
        Op::ThrowError => lower_throw_error(ctx, &inst),
        Op::ThrowErrorValue => lower_throw_error_value(ctx, &inst),
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
        Op::MixedArrayGetForWrite => lower_mixed_array_runtime_get(ctx, &inst, true),
        Op::GeneratorYield => lower_generator_yield(ctx, &inst),
        Op::GeneratorYieldFrom => lower_generator_yield_from(ctx, &inst),
        Op::ConcatReset => lower_concat_reset(ctx),
        Op::Nop => lower_nop(ctx, &inst),
        _ => Err(CodegenIrError::unsupported(format!(
            "opcode {}",
            inst.op.name()
        ))),
    }
}

/// Keeps the modular runtime-call dispatcher authoritative while recognizing the
/// fourth SimpleXML fetch-for-write operand introduced by the DOM lowering.
fn lower_runtime_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if let Some(Immediate::RuntimeCall(target)) = inst.immediate {
        return runtime_calls::lower(ctx, inst, target);
    }
    if inst.operands.len() == 3 && matches!(inst.immediate, Some(Immediate::Data(_))) {
        return array_access_runtime::lower_runtime_call(ctx, inst);
    }
    if let Some(()) = try_lower_array_access_runtime_call(ctx, inst)? {
        return Ok(());
    }
    if inst.operands.len() == 4 {
        return lower_mixed_array_runtime_get(ctx, inst, false);
    }
    array_access_runtime::lower_runtime_call(ctx, inst)
}

/// Routes boxed SimpleXML dimension reads through its native object handler.
fn lower_mixed_array_runtime_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    for_write: bool,
) -> Result<()> {
    let candidates = mixed_simplexml_candidates(ctx);
    if candidates.is_empty() || inst.operands.len() < 4 {
        return mixed_array_runtime::lower_mixed_array_runtime_get(ctx, inst, for_write);
    }

    let receiver = expect_operand(inst, 0)?;
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    let fallback_label = ctx.next_label("mixed_dimension_fallback");
    let done_label = ctx.next_label("mixed_dimension_done");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_dimension_{}",
                label_fragment(&candidate.class_name)
            ))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_result(receiver)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_mixed_method_object_payload_or_fatal(ctx, receiver_reg, &fallback_label);
    emit_mixed_simplexml_class_dispatch(
        ctx,
        receiver_reg,
        &candidates,
        &match_labels,
        &fallback_label,
    );

    let opcode = crate::internal_extensions::operation_registry()
        .object_handler("simplexml", "read_dimension")
        .ok_or_else(|| {
            CodegenIrError::invalid_module("missing SimpleXML read_dimension object handler")
        })?
        .opcode;
    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        let native_inst = Instruction {
            operands: vec![inst.operands[0], inst.operands[1], inst.operands[3]],
            ..inst.clone()
        };
        internal_extensions::lower_mixed_receiver_internal_extension_call(
            ctx,
            &native_inst,
            receiver_reg,
            opcode,
            &PhpType::Object(candidate.class_name.clone()),
        )?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&fallback_label);
    mixed_array_runtime::lower_mixed_array_runtime_get(ctx, inst, for_write)?;
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Preserves the modular method dispatcher except for native wrapper methods that
/// have no EIR body and therefore must use the versioned internal-extension ABI.
fn lower_method_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    if !matches!(
        ctx.value_php_type(object)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return method_dispatch::lower_method_call(ctx, inst);
    }
    let method_name = method_name_data(ctx, inst)?.to_string();
    let candidates = mixed_method_candidates(ctx, &method_name, inst.operands.len())?;
    if !candidates
        .iter()
        .any(|candidate| mixed_receiver_internal_extension_method_opcode(ctx, candidate).is_some())
    {
        return method_dispatch::lower_method_call(ctx, inst);
    }
    lower_mixed_internal_extension_method_call(ctx, inst, object, &method_name, candidates)
}

/// Dispatches a boxed receiver across both ordinary EIR methods and bodyless native methods.
fn lower_mixed_internal_extension_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
    candidates: Vec<MixedMethodCandidate>,
) -> Result<()> {
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    let non_object_label = ctx.next_label("mixed_method_non_object");
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
    emit_mixed_method_object_payload_or_fatal(ctx, receiver_reg, &non_object_label);
    emit_mixed_method_class_dispatch(
        ctx,
        receiver_reg,
        &candidates,
        &match_labels,
        &no_match_label,
    );

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        if let Some(opcode) = mixed_receiver_internal_extension_method_opcode(ctx, candidate) {
            internal_extensions::lower_mixed_receiver_internal_extension_call(
                ctx,
                inst,
                receiver_reg,
                opcode,
                &inst.result_php_type,
            )?;
        } else {
            method_dispatch::lower_mixed_method_candidate_call(
                ctx,
                inst,
                receiver_reg,
                candidate,
                method_name,
            )?;
        }
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&no_match_label);
    if builtins::has_eval_context(ctx) {
        builtins::lower_eval_method_call(ctx, inst, object, method_name)?;
        abi::emit_jump(ctx.emitter, &done_label);
    } else {
        emit_method_call_on_null_fatal(ctx, method_name);
    }
    ctx.emitter.label(&non_object_label);
    emit_method_call_on_null_fatal(ctx, method_name);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Collects ordinary method candidates plus bodyless native-wrapper candidates.
fn mixed_method_candidates(
    ctx: &FunctionContext<'_>,
    method_name: &str,
    operand_count: usize,
) -> Result<Vec<MixedMethodCandidate>> {
    let method_key = php_symbol_key(method_name);
    let mut candidates = method_dispatch::mixed_method_candidates(
        ctx,
        method_name,
        operand_count,
    )?;
    for (class_name, class_info) in &ctx.module.class_infos {
        if candidates
            .iter()
            .any(|candidate| candidate.class_id == class_info.class_id)
        {
            continue;
        }
        let Some(signature) = class_info.methods.get(&method_key) else {
            continue;
        };
        let regular_param_count = crate::types::call_args::regular_param_count(signature);
        let supplied_count = operand_count.saturating_sub(1);
        let arity_matches = supplied_count == signature.params.len()
            || signature
                .variadic
                .as_ref()
                .is_some_and(|_| supplied_count >= regular_param_count);
        if !arity_matches {
            continue;
        }
        let impl_class = class_info
            .method_impl_classes
            .get(&method_key)
            .cloned()
            .unwrap_or_else(|| class_name.clone());
        if crate::internal_extensions::operation_registry()
            .method(&impl_class, &method_key)
            .filter(|operation| !operation.static_operation)
            .is_none()
        {
            continue;
        }
        candidates.push(MixedMethodCandidate {
            class_id: class_info.class_id,
            class_name: class_name.clone(),
            target: MethodCallTarget {
                impl_class,
                method_key: method_key.clone(),
                dynamic_slot: None,
                params: signature
                    .params
                    .iter()
                    .map(|(_, ty)| ty.codegen_repr())
                    .collect(),
                ref_params: signature.ref_params.clone(),
                return_ty: signature.return_type.clone(),
                by_ref_return: signature.by_ref_return,
            },
        });
    }
    candidates.sort_by_key(|candidate| candidate.class_id);
    Ok(candidates)
}

/// Resolves a bodyless internal-wrapper method to its locked native opcode.
fn mixed_receiver_internal_extension_method_opcode(
    ctx: &FunctionContext<'_>,
    candidate: &MixedMethodCandidate,
) -> Option<u32> {
    if class_method_already_emitted(
        ctx,
        &candidate.target.impl_class,
        &candidate.target.method_key,
        false,
    ) || !crate::internal_extensions::is_native_wrapper_class(&candidate.target.impl_class)
    {
        return None;
    }
    crate::internal_extensions::operation_registry()
        .method(&candidate.target.impl_class, &candidate.target.method_key)
        .filter(|operation| !operation.static_operation)
        .map(|operation| operation.opcode)
}

/// Walks emitted and locked internal parents for native wrapper compatibility checks.
fn mixed_internal_extension_class_is_a(
    ctx: &FunctionContext<'_>,
    source_class: &str,
    target_class: &str,
) -> bool {
    let target_key = php_symbol_key(target_class.trim_start_matches('\\'));
    let mut current = Some(source_class.trim_start_matches('\\').to_string());
    while let Some(class_name) = current {
        if php_symbol_key(class_name.trim_start_matches('\\')) == target_key {
            return true;
        }
        current = ctx
            .module
            .class_infos
            .get(class_name.as_str())
            .and_then(|class_info| class_info.parent.clone())
            .or_else(|| {
                crate::internal_extensions::registry()
                    .class(&class_name)
                    .and_then(|class| class.parent.clone())
            });
    }
    false
}

/// Concrete SimpleXML runtime class reachable through a boxed receiver.
pub(super) struct MixedSimpleXmlCandidate {
    pub(super) class_id: u64,
    pub(super) class_name: String,
}

/// Collects every concrete SimpleXML wrapper class emitted in the current module.
pub(super) fn mixed_simplexml_candidates(
    ctx: &FunctionContext<'_>,
) -> Vec<MixedSimpleXmlCandidate> {
    let mut candidates = ctx
        .module
        .class_infos
        .iter()
        .filter(|(class_name, _)| {
            mixed_internal_extension_class_is_a(ctx, class_name, "SimpleXMLElement")
        })
        .map(|(class_name, class_info)| MixedSimpleXmlCandidate {
            class_id: class_info.class_id,
            class_name: class_name.clone(),
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| candidate.class_id);
    candidates
}

/// Emits exact runtime class-id branches for boxed SimpleXML receivers.
fn emit_mixed_simplexml_class_dispatch(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    candidates: &[MixedSimpleXmlCandidate],
    match_labels: &[String],
    no_match_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x9, [{}]", receiver_reg));           // load the receiver class id for dynamic SimpleXML dispatch
            for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "x10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp x9, x10");                         // compare against this concrete SimpleXML wrapper class
                ctx.emitter.instruction(&format!("b.eq {}", label));            // route matching wrappers through the native object handler
            }
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov r11, QWORD PTR [{}]", receiver_reg)); // load the receiver class id for dynamic SimpleXML dispatch
            for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "r10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp r11, r10");                        // compare against this concrete SimpleXML wrapper class
                ctx.emitter.instruction(&format!("je {}", label));              // route matching wrappers through the native object handler
            }
        }
    }
    abi::emit_jump(ctx.emitter, no_match_label);
}
