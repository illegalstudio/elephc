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
use crate::ir::{Immediate, Instruction, IrType};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{
    expect_operand, load_value_to_first_int_arg, lower_runtime_object_method_call, predicates,
    store_if_result, strings,
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
fn lower_cast_to_string(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
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
