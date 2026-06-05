//! Purpose:
//! Lowers simple PCRE-style regex builtins for the EIR backend.
//! Bridges already-evaluated EIR operands to the shared target-aware regex runtime helpers.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - `preg_match()` capture arrays and `preg_replace_callback()` remain explicit future work.
//! - `preg_split()` forces boxed Mixed element slots so dynamic flags cannot mismatch layout.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;

const PREG_SPLIT_FORCE_MIXED_RESULT: i64 = 1 << 30;

/// Lowers `preg_match(pattern, subject)` through the shared regex runtime helper.
pub(super) fn lower_preg_match(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "preg_match", 2)?;
    let pattern = super::expect_operand(inst, 0)?;
    let subject = super::expect_operand(inst, 1)?;
    load_pattern_and_subject(ctx, pattern, subject)?;
    abi::emit_call_label(ctx.emitter, "__rt_preg_match");
    super::store_if_result(ctx, inst)
}

/// Lowers `preg_match_all(pattern, subject)` through the shared regex runtime helper.
pub(super) fn lower_preg_match_all(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "preg_match_all", 2)?;
    let pattern = super::expect_operand(inst, 0)?;
    let subject = super::expect_operand(inst, 1)?;
    load_pattern_and_subject(ctx, pattern, subject)?;
    abi::emit_call_label(ctx.emitter, "__rt_preg_match_all");
    super::store_if_result(ctx, inst)
}

/// Lowers `preg_replace(pattern, replacement, subject)` through the regex replacement helper.
pub(super) fn lower_preg_replace(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "preg_replace", 3)?;
    let pattern = super::expect_operand(inst, 0)?;
    let replacement = super::expect_operand(inst, 1)?;
    let subject = super::expect_operand(inst, 2)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_arg(ctx, pattern, "x1", "x2", "preg_replace pattern")?;
            load_string_arg(ctx, replacement, "x3", "x4", "preg_replace replacement")?;
            load_string_arg(ctx, subject, "x5", "x6", "preg_replace subject")?;
        }
        Arch::X86_64 => {
            load_string_arg(ctx, pattern, "rdi", "rsi", "preg_replace pattern")?;
            load_string_arg(ctx, replacement, "rdx", "rcx", "preg_replace replacement")?;
            load_string_arg(ctx, subject, "r8", "r9", "preg_replace subject")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_preg_replace");
    super::store_if_result(ctx, inst)
}

/// Lowers `preg_split(pattern, subject, limit?, flags?)` through the regex split helper.
pub(super) fn lower_preg_split(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count_between(inst, "preg_split", 2, 4)?;
    let pattern = super::expect_operand(inst, 0)?;
    let subject = super::expect_operand(inst, 1)?;
    let limit = inst.operands.get(2).copied();
    let flags = inst.operands.get(3).copied();
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_arg(ctx, pattern, "x1", "x2", "preg_split pattern")?;
            load_string_arg(ctx, subject, "x3", "x4", "preg_split subject")?;
            load_limit_arg(ctx, limit, "x5")?;
            load_flags_arg(ctx, flags, "x6")?;
            ctx.emitter.instruction(&format!("orr x6, x6, #{}", PREG_SPLIT_FORCE_MIXED_RESULT)); // force boxed-Mixed split slots for EIR result layout
        }
        Arch::X86_64 => {
            load_string_arg(ctx, pattern, "rdi", "rsi", "preg_split pattern")?;
            load_string_arg(ctx, subject, "rdx", "rcx", "preg_split subject")?;
            load_limit_arg(ctx, limit, "r8")?;
            load_flags_arg(ctx, flags, "r9")?;
            ctx.emitter.instruction(&format!("or r9, {}", PREG_SPLIT_FORCE_MIXED_RESULT)); // force boxed-Mixed split slots for EIR result layout
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_preg_split");
    super::store_if_result(ctx, inst)
}

/// Loads pattern and subject string operands into the regex runtime ABI registers.
fn load_pattern_and_subject(
    ctx: &mut FunctionContext<'_>,
    pattern: ValueId,
    subject: ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_arg(ctx, pattern, "x1", "x2", "preg pattern")?;
            load_string_arg(ctx, subject, "x3", "x4", "preg subject")
        }
        Arch::X86_64 => {
            load_string_arg(ctx, pattern, "rdi", "rsi", "preg pattern")?;
            load_string_arg(ctx, subject, "rdx", "rcx", "preg subject")
        }
    }
}

/// Loads a string operand into an explicit pointer/length register pair.
fn load_string_arg(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    ptr_reg: &str,
    len_reg: &str,
    context: &str,
) -> Result<()> {
    require_string(ctx.value_php_type(value)?, context)?;
    ctx.load_string_value_to_regs(value, ptr_reg, len_reg)
}

/// Loads the optional `preg_split()` limit, using PHP's default `-1`.
fn load_limit_arg(
    ctx: &mut FunctionContext<'_>,
    limit: Option<ValueId>,
    reg: &str,
) -> Result<()> {
    let Some(limit) = limit else {
        abi::emit_load_int_immediate(ctx.emitter, reg, -1);
        return Ok(());
    };
    require_integer_like(ctx.load_value_to_reg(limit, reg)?, "preg_split limit")
}

/// Loads the optional `preg_split()` flags, using PHP's default `0`.
fn load_flags_arg(
    ctx: &mut FunctionContext<'_>,
    flags: Option<ValueId>,
    reg: &str,
) -> Result<()> {
    let Some(flags) = flags else {
        abi::emit_load_int_immediate(ctx.emitter, reg, 0);
        return Ok(());
    };
    require_integer_like(ctx.load_value_to_reg(flags, reg)?, "preg_split flags")
}

/// Verifies that a regex string operand is statically string-shaped.
fn require_string(ty: PhpType, context: &str) -> Result<()> {
    if ty == PhpType::Str {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        context,
        ty
    )))
}

/// Verifies that a regex integer option is statically integer-like.
fn require_integer_like(ty: PhpType, context: &str) -> Result<()> {
    if matches!(ty, PhpType::Int | PhpType::Bool) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        context,
        ty
    )))
}
