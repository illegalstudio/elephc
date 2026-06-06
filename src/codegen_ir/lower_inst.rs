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

use crate::codegen::{abi, callable_descriptor, emit_box_current_value_as_mixed, runtime};
use crate::codegen::context::{TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET};
use crate::codegen::platform::Arch;
use crate::ir::{BlockId, CmpPredicate, Immediate, InstId, Instruction, LocalSlotId, Op, ValueId};
use crate::names::{
    function_symbol, ir_global_symbol, method_symbol, php_symbol_key, static_method_symbol,
};
use crate::types::{FunctionSig, PhpType};

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
        Op::LoadGlobal => lower_load_global(ctx, &inst),
        Op::StoreGlobal => lower_store_global(ctx, &inst),
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
        Op::StrToI => conversions::lower_str_to_int(ctx, &inst),
        Op::StrToF => conversions::lower_str_to_float(ctx, &inst),
        Op::Cast => conversions::lower_cast(ctx, &inst),
        Op::MixedBox => lower_mixed_box(ctx, &inst),
        Op::StrConcat => strings::lower_str_concat(ctx, &inst),
        Op::StrLen => strings::lower_str_len(ctx, &inst),
        Op::StrCharAt => strings::lower_str_char_at(ctx, &inst),
        Op::StrPersist => strings::lower_str_persist(ctx, &inst),
        Op::ArrayNew => arrays::lower_array_new(ctx, &inst),
        Op::ArrayLen => arrays::lower_array_len(ctx, &inst),
        Op::ArrayGet => arrays::lower_array_get(ctx, &inst),
        Op::ArraySet => arrays::lower_array_set(ctx, &inst),
        Op::ArrayPush => arrays::lower_array_push(ctx, &inst),
        Op::ArrayUnion => arrays::lower_array_union(ctx, &inst),
        Op::HashNew => hashes::lower_hash_new(ctx, &inst),
        Op::HashLen => hashes::lower_hash_len(ctx, &inst),
        Op::HashGet => hashes::lower_hash_get(ctx, &inst),
        Op::HashSet => hashes::lower_hash_set(ctx, &inst),
        Op::IterStart => iterators::lower_iter_start(ctx, &inst),
        Op::IterNext => iterators::lower_iter_next(ctx, &inst),
        Op::IterCurrentKey => iterators::lower_iter_current_key(ctx, &inst),
        Op::IterCurrentValue => iterators::lower_iter_current_value(ctx, &inst),
        Op::IterEnd => iterators::lower_iter_end(ctx, &inst),
        Op::PtrCast => pointers::lower_ptr_cast(ctx, &inst),
        Op::BufferNew => buffers::lower_buffer_new(ctx, &inst),
        Op::BufferGet => buffers::lower_buffer_get(ctx, &inst),
        Op::BufferSet => buffers::lower_buffer_set(ctx, &inst),
        Op::ObjectNew => objects::lower_object_new(ctx, &inst),
        Op::DynamicObjectNew => objects::lower_dynamic_object_new(ctx, &inst),
        Op::PropGet => objects::lower_prop_get(ctx, &inst),
        Op::NullsafePropGet => objects::lower_nullsafe_prop_get(ctx, &inst),
        Op::DynamicPropGet => objects::lower_dynamic_prop_get(ctx, &inst),
        Op::PropSet => objects::lower_prop_set(ctx, &inst),
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
        Op::Nop => Ok(()),
        _ => Err(CodegenIrError::unsupported(format!("opcode {}", inst.op.name()))),
    }
}

/// Lowers a by-value closure capture marker.
fn lower_closure_capture(_ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.immediate.is_some() {
        return Err(CodegenIrError::unsupported(
            "by-reference closure captures in the EIR backend",
        ));
    }
    Ok(())
}

/// Materializes an EIR closure literal as a static callable descriptor pointer.
fn lower_closure_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let closure_name = callable_target_data(ctx, inst)?.to_string();
    let closure = ctx
        .module
        .closures
        .iter()
        .find(|function| function.name == closure_name)
        .ok_or_else(|| CodegenIrError::missing_entry("closure", 0))?;
    let signature = function_signature_from_eir(closure);
    callable_descriptor::emit_load_descriptor_address_with_meta(
        ctx.emitter,
        ctx.data,
        abi::int_result_reg(ctx.emitter),
        &function_symbol(&closure.name),
        Some(&closure.name),
        callable_descriptor::CALLABLE_DESC_KIND_CLOSURE,
        Some(&signature),
        &[],
        &[],
        callable_descriptor::CallableDescriptorInvocation::new(
            callable_descriptor::CallableDescriptorShape::Closure,
        ),
    );
    store_if_result(ctx, inst)
}

/// Reconstructs callable signature metadata from an emitted EIR function.
fn function_signature_from_eir(function: &crate::ir::Function) -> FunctionSig {
    FunctionSig {
        params: function
            .params
            .iter()
            .map(|param| (param.name.clone(), param.php_type.clone()))
            .collect(),
        defaults: function.params.iter().map(|_| None).collect(),
        return_type: function.return_php_type.clone(),
        declared_return: !matches!(function.return_php_type, PhpType::Mixed),
        ref_params: function.params.iter().map(|param| param.by_ref).collect(),
        declared_params: function
            .params
            .iter()
            .map(|param| !matches!(param.php_type, PhpType::Mixed))
            .collect(),
        variadic: function
            .params
            .iter()
            .find(|param| param.variadic)
            .map(|param| param.name.clone()),
        deprecation: None,
    }
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
    if let Some((entry_label, kind, invocation)) = first_class_callable_descriptor(ctx, &target) {
        callable_descriptor::emit_load_descriptor_address_with_meta(
            ctx.emitter,
            ctx.data,
            abi::int_result_reg(ctx.emitter),
            &entry_label,
            Some(&target),
            kind,
            None,
            &[],
            &[],
            invocation,
        );
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    }
    store_if_result(ctx, inst)
}

/// Returns static descriptor metadata for compile-time callable targets supported by EIR.
fn first_class_callable_descriptor(
    ctx: &FunctionContext<'_>,
    target: &str,
) -> Option<(String, u64, callable_descriptor::CallableDescriptorInvocation)> {
    if let Some((receiver_label, method_name)) = target.rsplit_once("::") {
        return first_class_static_method_descriptor(ctx, receiver_label, method_name);
    }
    if let Some(callee) = ctx.callable_function_by_name(target) {
        return Some((
            function_symbol(&callee.name),
            callable_descriptor::CALLABLE_DESC_KIND_FUNCTION,
            callable_descriptor::CallableDescriptorInvocation::named(
                callable_descriptor::CallableDescriptorShape::Function,
                callee.name.clone(),
            ),
        ));
    }
    if ctx.has_extern_function(target) {
        return Some((
            ctx.emitter.target.extern_symbol(target),
            callable_descriptor::CALLABLE_DESC_KIND_EXTERN,
            callable_descriptor::CallableDescriptorInvocation::named(
                callable_descriptor::CallableDescriptorShape::Extern,
                target.to_string(),
            ),
        ));
    }
    None
}

/// Returns descriptor metadata for static methods with compile-time class receivers.
fn first_class_static_method_descriptor(
    ctx: &FunctionContext<'_>,
    receiver_label: &str,
    method_name: &str,
) -> Option<(String, u64, callable_descriptor::CallableDescriptorInvocation)> {
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
    ctx.module
        .class_infos
        .get(impl_class)?
        .static_methods
        .get(&method_key)?;
    Some((
        static_method_symbol(impl_class, &method_key),
        callable_descriptor::CALLABLE_DESC_KIND_STATIC_METHOD,
        callable_descriptor::CallableDescriptorInvocation::method(
            callable_descriptor::CallableDescriptorShape::StaticMethod,
            Some(receiver),
            method_key,
        ),
    ))
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
    if matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) {
        let result_ty = inst.result_php_type.codegen_repr();
        load_value_to_first_int_arg(ctx, value)?;
        match result_ty {
            PhpType::Str => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string"),
            PhpType::Float => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float"),
            PhpType::Int => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int"),
            PhpType::Bool => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool"),
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

/// Lowers binary runtime fallbacks that Phase 04 can identify by operand type.
fn lower_binary_runtime_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let receiver = expect_operand(inst, 0)?;
    match ctx.value_php_type(receiver)?.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => lower_mixed_array_runtime_get(ctx, inst),
        other => Err(CodegenIrError::unsupported(format!(
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

/// Pops an EIR exception handler by restoring its saved previous handler pointer.
fn lower_try_pop_handler(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let token = expect_i64(inst)?;
    let handler_offset = ctx.try_handler_offset(token)?;
    let scratch = abi::temp_int_reg(ctx.emitter.target);
    ctx.emitter.comment("pop EIR exception handler");
    abi::load_at_offset(ctx.emitter, scratch, handler_offset);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_exc_handler_top", 0);
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
        let target_ty = ctx.local_php_type(slot)?;
        let offset = ctx.local_offset(slot)?;
        abi::emit_load_symbol_to_result(ctx.emitter, "_exc_value", &target_ty);
        abi::emit_store(ctx.emitter, &target_ty, offset);
    }
    abi::emit_store_zero_to_symbol(ctx.emitter, "_exc_value", 0);
    Ok(())
}

/// Lowers a direct instance-method call on a statically known object receiver.
fn lower_method_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let method_name = method_name_data(ctx, inst)?.to_string();
    let object_ty = ctx.value_php_type(object)?;
    let PhpType::Object(class_name) = object_ty else {
        return Err(CodegenIrError::unsupported(format!(
            "method call receiver for PHP type {:?}",
            object_ty
        )));
    };
    if let Some(state) = fiber_state_predicate(&class_name, &method_name) {
        return lower_fiber_state_predicate(ctx, inst, object, state);
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
    if is_throwable_payload_getter_call(ctx, &class_name, &method_name) {
        return lower_throwable_payload_getter(ctx, inst, object, &method_name);
    }
    let target = resolve_method_call_target(ctx, &class_name, &method_name, inst.operands.len())?;
    let mut param_types = Vec::with_capacity(target.params.len() + 1);
    param_types.push(PhpType::Object(class_name));
    param_types.extend(target.params.iter().map(|param| param.codegen_repr()));
    let overflow_bytes = materialize_direct_call_args(ctx, &inst.operands, &param_types)?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &method_symbol(&target.impl_class, &target.method_key));
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, overflow_bytes);
    store_call_result(ctx, inst, &target.return_ty)
}

/// Returns true when a direct method call can read PHP's compact Throwable payload.
fn is_throwable_payload_getter_call(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_name: &str,
) -> bool {
    matches!(php_symbol_key(method_name).as_str(), "getmessage" | "getcode")
        && is_throwable_like_class(ctx, class_name)
}

/// Returns true when class metadata says the receiver is Throwable-compatible.
fn is_throwable_like_class(ctx: &FunctionContext<'_>, class_name: &str) -> bool {
    let class_name = class_name.trim_start_matches('\\');
    if matches!(class_name, "Throwable") {
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

/// Lowers compact Throwable getters without requiring synthetic EIR method bodies.
fn lower_throwable_payload_getter(
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
    let overflow_bytes = materialize_method_call_args_with_receiver_reg(
        ctx,
        object_reg,
        &receiver_ty,
        &inst.operands,
        &param_types,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &method_symbol(&target.impl_class, &target.method_key));
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, overflow_bytes);
    if inst.result_php_type.codegen_repr() == PhpType::Mixed
        && target.return_ty.codegen_repr() != PhpType::Mixed
    {
        emit_box_current_value_as_mixed(ctx.emitter, &target.return_ty.codegen_repr());
    }
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&null_label);
    objects::emit_boxed_null(ctx);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Resolved method metadata needed to issue a direct method call.
struct MethodCallTarget {
    impl_class: String,
    method_key: String,
    params: Vec<PhpType>,
    return_ty: PhpType,
}

/// Source for the hidden called-class id passed to static method bodies.
enum CalledClassIdArg {
    Immediate(u64),
    Local(LocalSlotId),
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
    if !class_method_already_emitted(ctx, &impl_class, &method_key, false) {
        return Err(CodegenIrError::unsupported(format!(
            "method call to {}::{} without an emitted EIR method body",
            impl_class, method_name
        )));
    }
    Ok(MethodCallTarget {
        impl_class,
        method_key,
        params: callee_sig
            .params
            .iter()
            .map(|(_, ty)| ty.codegen_repr())
            .collect(),
        return_ty: callee_sig.return_type.clone(),
    })
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

/// Stores a call result, materializing PHP null for `void` returns when needed.
fn store_call_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    return_ty: &PhpType,
) -> Result<()> {
    if let Some(result) = inst.result {
        if return_ty.codegen_repr() == PhpType::Void || ctx.value_php_type(result)? == PhpType::Void {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
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
    let callee_sig = impl_info
        .static_methods
        .get(&method_key)
        .ok_or_else(|| CodegenIrError::unsupported(format!("static method call to unknown method {}", target)))?;
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
    let overflow_bytes =
        materialize_static_method_call_args(ctx, &called_class_id, &inst.operands, &param_types)?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &static_method_symbol(impl_class, &method_key));
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, overflow_bytes);
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
    Ok(())
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
        "static" => Err(CodegenIrError::unsupported(
            "static method call with late-bound receiver static",
        )),
        _ => Ok(receiver.to_string()),
    }
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
    let overflow_bytes = materialize_direct_call_args(ctx, &inst.operands, &param_types)?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &function_symbol(&function_name));
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, overflow_bytes);
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
    Ok(())
}

/// Loads SSA operands into ABI argument registers and caller-stack slots for a direct call.
pub(super) fn materialize_direct_call_args(
    ctx: &mut FunctionContext<'_>,
    args: &[ValueId],
    param_types: &[PhpType],
) -> Result<usize> {
    if args.len() != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "direct call materialization received {} args for {} params",
            args.len(),
            param_types.len()
        )));
    }
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, param_types, 0);
    for (value, param_ty) in args.iter().zip(param_types.iter()) {
        ctx.load_value_to_result(*value)?;
        let source_ty = ctx.raw_value_php_type(*value)?;
        let push_ty = materialize_direct_call_arg_for_param(ctx, &source_ty, param_ty)?;
        abi::emit_push_result_value(ctx.emitter, &push_ty);
    }
    Ok(abi::materialize_outgoing_args(ctx.emitter, &assignments))
}

/// Loads the hidden called-class id plus visible operands for an EIR static method call.
fn materialize_static_method_call_args(
    ctx: &mut FunctionContext<'_>,
    called_class_id: &CalledClassIdArg,
    args: &[ValueId],
    param_types: &[PhpType],
) -> Result<usize> {
    if args.len() != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "static method call materialization received {} args for {} visible params",
            args.len(),
            param_types.len()
        )));
    }
    let mut abi_param_types = Vec::with_capacity(param_types.len() + 1);
    abi_param_types.push(PhpType::Int);
    abi_param_types.extend_from_slice(param_types);
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &abi_param_types, 0);
    materialize_called_class_id(ctx, called_class_id)?;
    abi::emit_push_result_value(ctx.emitter, &PhpType::Int);
    for (value, param_ty) in args.iter().zip(param_types.iter()) {
        ctx.load_value_to_result(*value)?;
        let source_ty = ctx.raw_value_php_type(*value)?;
        let push_ty = materialize_direct_call_arg_for_param(ctx, &source_ty, param_ty)?;
        abi::emit_push_result_value(ctx.emitter, &push_ty);
    }
    Ok(abi::materialize_outgoing_args(ctx.emitter, &assignments))
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
        PhpType::Mixed if source_ty.codegen_repr() != PhpType::Mixed => {
            emit_box_current_value_as_mixed(ctx.emitter, source_ty);
            Ok(PhpType::Mixed)
        }
        target_ty => Ok(target_ty),
    }
}

/// Loads call arguments with an already-unboxed receiver as the first ABI argument.
fn materialize_method_call_args_with_receiver_reg(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    receiver_ty: &PhpType,
    operands: &[ValueId],
    param_types: &[PhpType],
) -> Result<usize> {
    if operands.len() != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "method call materialization received {} operands for {} params",
            operands.len(),
            param_types.len()
        )));
    }
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, param_types, 0);
    move_reg_to_int_result(ctx, receiver_reg);
    abi::emit_push_result_value(ctx.emitter, receiver_ty);
    for (value, param_ty) in operands.iter().skip(1).zip(param_types.iter().skip(1)) {
        ctx.load_value_to_result(*value)?;
        let source_ty = ctx.raw_value_php_type(*value)?;
        let push_ty = materialize_direct_call_arg_for_param(ctx, &source_ty, param_ty)?;
        abi::emit_push_result_value(ctx.emitter, &push_ty);
    }
    Ok(abi::materialize_outgoing_args(ctx.emitter, &assignments))
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
fn direct_call_stack_pad_bytes(ctx: &FunctionContext<'_>, overflow_bytes: usize) -> usize {
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
    let source_ty = ctx.load_local_to_result(slot)?;
    let result_ty = ctx.value_php_type(result)?;
    coerce_loaded_local_to_result_type(ctx, &source_ty, &result_ty)?;
    ctx.store_result_value(result)
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
        (PhpType::Mixed, PhpType::Void) => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
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
            | (PhpType::Mixed, PhpType::Object(_))
    )
}

/// Lowers an addressable local store from one SSA operand.
fn lower_store_local(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    let value = expect_operand(inst, 0)?;
    ctx.store_value_to_local(slot, value)
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

/// Lowers a null constant to the runtime null sentinel and stores it in the result slot.
fn lower_const_null(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
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

/// Lowers PHP echo output for a previously computed SSA value.
fn lower_echo_value(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
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
        PhpType::Int => {
            let skip_label = ctx.next_label("echo_skip_null");
            let sentinel_reg = abi::symbol_scratch_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, sentinel_reg, 0x7fff_ffff_ffff_fffe);
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
        | PhpType::Pointer(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. } => {
            abi::emit_write_stdout(ctx.emitter, ty);
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
