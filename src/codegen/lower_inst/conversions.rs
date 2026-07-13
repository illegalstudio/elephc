//! Purpose:
//! Lowers scalar EIR conversion opcodes, including explicit PHP casts.
//! Bridges direct coercion opcodes and `Cast` immediates to existing runtime helpers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Concrete scalar casts stay inline; Mixed numeric casts delegate to boxed runtime helpers.
//! - String numeric parsing delegates to shared runtime routines.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::{Immediate, Instruction, IrType, ValueId};
use crate::names::method_symbol;
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{
    direct_call_stack_pad_bytes, emit_dynamic_instance_method_call,
    emit_mixed_method_class_dispatch, expect_operand, load_value_to_first_int_arg,
    lower_runtime_object_method_call, materialize_method_call_args_with_receiver_reg_and_refs,
    mixed_method_candidates, predicates, store_if_result, strings,
};
use crate::codegen::{CodegenIrError, Result};

/// Lowers a string-to-integer conversion through PHP string cast rules.
pub(super) fn lower_str_to_int(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?;
    if ty != PhpType::Str {
        return Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            inst.op.name(),
            ty
        )));
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_to_int");
    store_if_result(ctx, inst)
}

/// Lowers a string-to-float conversion through PHP numeric string parsing.
pub(super) fn lower_str_to_float(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?;
    if ty != PhpType::Str {
        return Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            inst.op.name(),
            ty
        )));
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_to_number");
    store_if_result(ctx, inst)
}

/// Lowers explicit scalar casts based on the target storage immediate and result PHP type.
pub(super) fn lower_cast(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    match expect_cast_target(inst)? {
        IrType::I64 if inst.result_php_type == PhpType::Bool => predicates::lower_is_truthy(ctx, inst),
        IrType::I64 => lower_cast_to_int(ctx, inst),
        IrType::F64 => lower_cast_to_float(ctx, inst),
        IrType::Str => lower_cast_to_string(ctx, inst),
        target => Err(CodegenIrError::unsupported(format!(
            "cast to EIR type {:?}",
            target
        ))),
    }
}

/// Lowers an explicit cast to PHP int for concrete scalar operands.
fn lower_cast_to_int(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let raw_ty = ctx.raw_value_php_type(value)?;
    if matches!(raw_ty, PhpType::Resource(_)) {
        ctx.load_value_to_result(value)?;
        emit_resource_display_id_to_int(ctx);
        return store_if_result(ctx, inst);
    }
    match raw_ty.codegen_repr() {
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(value)?;
        }
        PhpType::TaggedScalar => {
            ctx.load_value_to_result(value)?;
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            abi::emit_float_result_to_int_result(ctx.emitter);
        }
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_str_to_int");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            predicates::emit_array_truthiness(ctx, value)?;
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "int cast for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers an explicit cast to PHP float for concrete scalar operands.
fn lower_cast_to_float(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let raw_ty = ctx.raw_value_php_type(value)?;
    if matches!(raw_ty, PhpType::Resource(_)) {
        ctx.load_value_to_result(value)?;
        emit_resource_display_id_to_int(ctx);
        abi::emit_int_result_to_float_result(ctx.emitter);
        return store_if_result(ctx, inst);
    }
    match raw_ty.codegen_repr() {
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
        }
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(value)?;
            abi::emit_int_result_to_float_result(ctx.emitter);
        }
        PhpType::TaggedScalar => {
            ctx.load_value_to_result(value)?;
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
            abi::emit_int_result_to_float_result(ctx.emitter);
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_int_result_to_float_result(ctx.emitter);
        }
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_str_to_number");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            predicates::emit_array_truthiness(ctx, value)?;
            abi::emit_int_result_to_float_result(ctx.emitter);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "float cast for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers an explicit cast to PHP string for concrete scalar operands.
pub(super) fn lower_cast_to_string(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let raw_ty = ctx.raw_value_php_type(value)?;
    if matches!(raw_ty, PhpType::Resource(_)) {
        ctx.load_value_to_result(value)?;
        abi::emit_call_label(ctx.emitter, "__rt_resource_to_string");
        return store_if_result(ctx, inst);
    }
    match raw_ty.codegen_repr() {
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            store_if_result(ctx, inst)
        }
        PhpType::Float => strings::lower_float_to_string(ctx, inst),
        PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never | PhpType::TaggedScalar => {
            strings::lower_int_like_to_string(ctx, inst)
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_mixed_string_context_result(ctx, value)?;
            store_if_result(ctx, inst)
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            lower_array_like_to_string(ctx, inst)
        }
        PhpType::Object(class_name) => lower_object_to_string(ctx, inst, &class_name),
        other => Err(CodegenIrError::unsupported(format!(
            "string cast for PHP type {:?}",
            other
        ))),
    }
}

/// Leaves a string result for a boxed Mixed value, dispatching objects through `__toString()`.
pub(super) fn emit_mixed_string_context_result(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    emit_mixed_string_context(ctx, value, MixedStringContextMode::Result)
}

/// Writes a boxed Mixed value to stdout, dispatching objects through `__toString()`.
pub(super) fn emit_mixed_string_context_stdout(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    emit_mixed_string_context(ctx, value, MixedStringContextMode::Stdout)
}

/// Describes whether a Mixed string context should leave a string result or write it.
enum MixedStringContextMode {
    Result,
    Stdout,
}

/// Handles PHP string contexts for boxed Mixed values with an object-aware branch.
fn emit_mixed_string_context(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    mode: MixedStringContextMode,
) -> Result<()> {
    let candidates = mixed_method_candidates(ctx, "__toString", 1)?;
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    let object_label = ctx.next_label("mixed_string_object");
    let no_match_label = ctx.next_label("mixed_string_no_match");
    let done_label = ctx.next_label("mixed_string_done");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_string_{}",
                super::label_fragment(&candidate.class_name)
            ))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_result(value)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_unboxed_mixed_object(ctx, &object_label);
    emit_mixed_string_scalar_fallback(ctx, &mode)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&object_label);
    discard_preserved_mixed_pointer(ctx);
    move_unboxed_mixed_object_payload(ctx, receiver_reg);
    emit_mixed_method_class_dispatch(
        ctx,
        receiver_reg,
        &candidates,
        &match_labels,
        &no_match_label,
    );

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        let return_ty = emit_mixed_tostring_candidate_call(ctx, value, receiver_reg, candidate)?;
        coerce_tostring_return_to_string_result(ctx, &return_ty)?;
        if matches!(mode, MixedStringContextMode::Stdout) {
            abi::emit_write_stdout(ctx.emitter, &PhpType::Str);
        }
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&no_match_label);
    emit_mixed_missing_tostring_fatal(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Branches to the object path when `__rt_mixed_unbox` returned an object tag.
fn emit_branch_if_unboxed_mixed_object(ctx: &mut FunctionContext<'_>, object_label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // check whether the boxed Mixed value contains an object
            ctx.emitter.instruction(&format!("b.eq {}", object_label));         // dispatch object string contexts through __toString
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // check whether the boxed Mixed value contains an object
            ctx.emitter.instruction(&format!("je {}", object_label));           // dispatch object string contexts through __toString
        }
    }
}

/// Runs the existing scalar Mixed string behavior after restoring the original box.
fn emit_mixed_string_scalar_fallback(
    ctx: &mut FunctionContext<'_>,
    mode: &MixedStringContextMode,
) -> Result<()> {
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match mode {
        MixedStringContextMode::Result => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
        }
        MixedStringContextMode::Stdout => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_write_stdout");
        }
    }
    Ok(())
}

/// Discards the saved boxed Mixed pointer once the object branch no longer needs it.
fn discard_preserved_mixed_pointer(ctx: &mut FunctionContext<'_>) {
    abi::emit_pop_reg(ctx.emitter, abi::temp_int_reg(ctx.emitter.target));
}

/// Moves the unboxed object payload into the callee-saved receiver dispatch register.
fn move_unboxed_mixed_object_payload(ctx: &mut FunctionContext<'_>, receiver_reg: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov {}, x1", receiver_reg));      // preserve the unboxed object pointer for __toString dispatch
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, rdi", receiver_reg));     // preserve the unboxed object pointer for __toString dispatch
        }
    }
}

/// Emits one concrete `__toString()` candidate call for a boxed Mixed object.
fn emit_mixed_tostring_candidate_call(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    receiver_reg: &str,
    candidate: &super::MixedMethodCandidate,
) -> Result<PhpType> {
    let receiver_ty = PhpType::Object(candidate.class_name.clone());
    let mut param_types = Vec::with_capacity(candidate.target.params.len() + 1);
    param_types.push(receiver_ty.clone());
    param_types.extend(candidate.target.params.iter().map(|param| param.codegen_repr()));
    let mut ref_params = Vec::with_capacity(candidate.target.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(candidate.target.ref_params.iter().copied());
    let operands = [value];
    let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
        ctx,
        receiver_reg,
        &receiver_ty,
        &operands,
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
    Ok(candidate.target.return_ty.clone())
}

/// Normalizes a `__toString()` return into a string result pair.
fn coerce_tostring_return_to_string_result(
    ctx: &mut FunctionContext<'_>,
    return_ty: &PhpType,
) -> Result<()> {
    match return_ty.codegen_repr() {
        PhpType::Str => Ok(()),
        PhpType::Mixed | PhpType::Union(_) => {
            super::cast_loaded_mixed_pointer_to_result(ctx, &PhpType::Str)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "__toString return value for PHP type {:?}",
            other
        ))),
    }
}

/// Emits a fatal when a boxed Mixed object has no matching public `__toString()`.
fn emit_mixed_missing_tostring_fatal(ctx: &mut FunctionContext<'_>) {
    let (label, len) = ctx
        .data
        .add_string(b"Fatal error: Object could not be converted to string\n");
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

/// Lowers an object string cast through `__toString()` or PHP's conversion fatal.
fn lower_object_to_string(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
) -> Result<()> {
    let normalized = class_name.trim_start_matches('\\');
    if object_class_has_tostring(ctx, normalized) {
        return lower_runtime_object_method_call(ctx, inst, normalized, "__toString");
    }
    emit_missing_tostring_fatal(ctx, normalized);
    Ok(())
}

/// Returns true when class metadata exposes a `__toString()` method.
fn object_class_has_tostring(ctx: &FunctionContext<'_>, class_name: &str) -> bool {
    ctx.module
        .class_infos
        .get(class_name)
        .is_some_and(|class_info| class_info.methods.contains_key("__tostring"))
}

/// Emits PHP's fatal diagnostic for object-to-string casts without `__toString()`.
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

/// Lowers array-like PHP values to the literal string used by PHP casts.
fn lower_array_like_to_string(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    emit_array_like_string_result(ctx);
    store_if_result(ctx, inst)
}

/// Materializes PHP's array-to-string placeholder in the active string result registers.
pub(super) fn emit_array_like_string_result(ctx: &mut FunctionContext<'_>) {
    let (label, len) = ctx.data.add_string(b"Array");
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
}

/// Converts the loaded native resource payload into PHP's one-based display id.
fn emit_resource_display_id_to_int(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("add x0, x0, #1");                          // convert native resource payload to PHP's one-based display id
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("add rax, 1");                              // convert native resource payload to PHP's one-based display id
        }
    }
}

/// Returns the cast target immediate attached to a `Cast` instruction.
fn expect_cast_target(inst: &Instruction) -> Result<IrType> {
    match inst.immediate {
        Some(Immediate::CastTarget(target)) => Ok(target),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing cast target immediate",
            inst.op.name()
        ))),
    }
}
