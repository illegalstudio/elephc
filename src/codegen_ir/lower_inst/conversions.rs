//! Purpose:
//! Lowers scalar EIR conversion opcodes, including explicit PHP casts.
//! Bridges direct coercion opcodes and `Cast` immediates to existing runtime helpers.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Concrete scalar casts stay inline; Mixed numeric casts delegate to boxed runtime helpers.
//! - String numeric parsing delegates to the shared runtime routines used by the legacy backend paths.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::{Immediate, Instruction, IrType};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_operand, load_value_to_first_int_arg, predicates, store_if_result, strings};
use crate::codegen_ir::{CodegenIrError, Result};

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
    match ctx.raw_value_php_type(value)? {
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(value)?;
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
        PhpType::Resource(_) => {
            ctx.load_value_to_result(value)?;
            emit_resource_display_id_to_int(ctx);
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
    match ctx.raw_value_php_type(value)? {
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
        }
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(value)?;
            abi::emit_int_result_to_float_result(ctx.emitter);
        }
        PhpType::Resource(_) => {
            ctx.load_value_to_result(value)?;
            emit_resource_display_id_to_int(ctx);
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
    match ctx.raw_value_php_type(value)? {
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            store_if_result(ctx, inst)
        }
        PhpType::Float => strings::lower_float_to_string(ctx, inst),
        PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never => {
            strings::lower_int_like_to_string(ctx, inst)
        }
        PhpType::Resource(_) => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_resource_to_string");
            store_if_result(ctx, inst)
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            store_if_result(ctx, inst)
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            lower_array_like_to_string(ctx, inst)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "string cast for PHP type {:?}",
            other
        ))),
    }
}

/// Lowers array-like PHP values to the literal string used by PHP casts.
fn lower_array_like_to_string(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let (label, len) = ctx.data.add_string(b"Array");
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    store_if_result(ctx, inst)
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
