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
use crate::codegen_support::try_handlers::TRY_HANDLER_SAVED_DEPTHS;
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
mod checked_numeric_chain;
mod runtime_functions;
pub(crate) mod builtins;
mod callables;
mod comparisons;
mod conversions;
mod enums;
mod exceptions;
mod mixed_narrowing;
mod externs;
mod floats;
mod hashes;
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

/// Lowers one EIR instruction by opcode.

/// Publishes the source line php would name in a diagnostic raised by this instruction.
///
/// php ends every warning with ` in FILE on line N`, and the line is the CALL SITE's — which only
/// the lowering knows. `__rt_diag_warning` reads what is published here when it writes the line
/// out, so the run-time helpers that actually compose the message need no span of their own.
///
/// Only an instruction whose effects admit `MAY_WARN` pays for it, which is what keeps this off
/// the arithmetic and the loads: a program that cannot warn emits none of these stores. The
/// string is rendered here rather than formatted at run time because both halves are constants at
/// this point, and because the obvious run-time formatter, `__rt_itoa`, writes through the shared
/// concat buffer — a warning raised mid-concatenation would corrupt the string being built.
///
/// Scratch registers are free at an instruction boundary: this backend gives every SSA value a
/// stack slot, so nothing of the function's own is live in one here.
fn publish_diagnostic_location(ctx: &mut FunctionContext<'_>, inst: &Instruction) {
    if !inst.effects.contains(crate::ir::Effects::MAY_WARN) {
        return;
    }
    let Some(span) = inst.span else {
        return;
    };
    publish_diagnostic_line(ctx, span.line);
}

/// Publishes one already-known source line as the location the next diagnostic will name.
///
/// Split out of `publish_diagnostic_location` because not every diagnostic is raised BY an
/// instruction: php's `$http_response_header` deprecation is raised while compiling the file and
/// is emitted from the main prologue, where there is no instruction to read a span from, yet php
/// still names the line of the mention that caused it.
pub(super) fn publish_diagnostic_line(ctx: &mut FunctionContext<'_>, line: u32) {
    if line == 0 {
        return;
    }
    let (label, len) = ctx.data.add_string(format!(" on line {line}\n").as_bytes());
    // NOT the primary scratch: `emit_store_reg_to_symbol` borrows that one to materialize the
    // symbol's own address, so handing it the value to store overwrites the value first.
    let ptr_reg = abi::secondary_scratch_reg(ctx.emitter);
    let len_reg = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_store_reg_to_symbol(ctx.emitter, ptr_reg, "_rt_diag_loc_ptr", 0);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    abi::emit_store_reg_to_symbol(ctx.emitter, len_reg, "_rt_diag_loc_len", 0);
}

pub(super) fn lower_instruction(ctx: &mut FunctionContext<'_>, inst_id: InstId) -> Result<()> {
    ctx.begin_instruction(inst_id);
    let inst = ctx
        .function
        .instruction(inst_id)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst_id.as_raw()))?;
    publish_diagnostic_location(ctx, &inst);
    match inst.op {
        Op::ConstI64 => lower_const_i64(ctx, &inst),
        Op::ConstF64 => floats::lower_const_f64(ctx, &inst),
        Op::ConstBool => lower_const_bool(ctx, &inst),
        Op::ConstNull => lower_const_null(ctx, &inst),
        Op::ConstStr => strings::lower_const_str(ctx, &inst),
        Op::ConstClassName => strings::lower_const_class_name(ctx, &inst),
        Op::LoadCalledClassId => strings::lower_load_called_class_id(ctx, &inst),
        Op::LoadLocal => lower_load_local(ctx, &inst),
        Op::WarnedNull => globals_constants::lower_warned_null(ctx, &inst),
        Op::StoreLocal => lower_store_local(ctx, &inst),
        Op::UnsetLocal => lower_unset_local(ctx, &inst),
        Op::ZeroLocalSlot => lower_zero_local_slot(ctx, &inst),
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
        Op::ICheckedNumericChainToInt => {
            checked_numeric_chain::lower_checked_numeric_chain_to_int(ctx, &inst)
        }
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
        Op::PhpRelCmp => comparisons::lower_php_rel_cmp(ctx, &inst),
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
        Op::PackedFieldMixedToInt => objects::lower_packed_field_mixed_to_int(ctx, &inst),
        Op::ReturnBoundaryMixedToInt => {
            mixed_narrowing::lower_return_boundary_mixed_to_int(ctx, &inst)
        }
        Op::ExternCall => externs::lower_extern_call(ctx, &inst),
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
