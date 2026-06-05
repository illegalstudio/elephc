//! Purpose:
//! Lowers date/time system builtins for the EIR backend.
//! Marshals already-evaluated EIR operands into the shared runtime helpers.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Time builtins are effectful and must reuse the target-aware runtime
//!   helpers rather than duplicating libc/syscall behavior in the EIR backend.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};

/// Lowers `date(format, timestamp?)` through the shared formatter runtime helper.
pub(super) fn lower_date(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "date", 1, 2)?;
    let format = expect_operand(inst, 0)?;
    let timestamp = inst
        .operands
        .get(1)
        .copied();

    load_date_timestamp(ctx, timestamp)?;
    load_date_format(ctx, format)?;
    abi::emit_call_label(ctx.emitter, "__rt_date");
    store_if_result(ctx, inst)
}

/// Lowers `microtime()`/`microtime(true)` through the shared runtime helper.
pub(super) fn lower_microtime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "microtime", 0, 1)?;
    abi::emit_call_label(ctx.emitter, "__rt_microtime");
    store_if_result(ctx, inst)
}

/// Lowers `mktime(hour, minute, second, month, day, year)` through the runtime helper.
pub(super) fn lower_mktime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "mktime", 6)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            materialize_integer_arg(ctx, expect_operand(inst, 0)?, "x0", "mktime hour")?;
            materialize_integer_arg(ctx, expect_operand(inst, 1)?, "x1", "mktime minute")?;
            materialize_integer_arg(ctx, expect_operand(inst, 2)?, "x2", "mktime second")?;
            materialize_integer_arg(ctx, expect_operand(inst, 3)?, "x3", "mktime month")?;
            materialize_integer_arg(ctx, expect_operand(inst, 4)?, "x4", "mktime day")?;
            materialize_integer_arg(ctx, expect_operand(inst, 5)?, "x5", "mktime year")?;
        }
        Arch::X86_64 => {
            materialize_integer_arg(ctx, expect_operand(inst, 0)?, "rdi", "mktime hour")?;
            materialize_integer_arg(ctx, expect_operand(inst, 1)?, "rsi", "mktime minute")?;
            materialize_integer_arg(ctx, expect_operand(inst, 2)?, "rdx", "mktime second")?;
            materialize_integer_arg(ctx, expect_operand(inst, 3)?, "rcx", "mktime month")?;
            materialize_integer_arg(ctx, expect_operand(inst, 4)?, "r8", "mktime day")?;
            materialize_integer_arg(ctx, expect_operand(inst, 5)?, "r9", "mktime year")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mktime");
    store_if_result(ctx, inst)
}

/// Lowers `strtotime(datetime)` through the shared parser runtime helper.
pub(super) fn lower_strtotime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "strtotime", 1)?;
    let datetime = expect_operand(inst, 0)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            require_string(ctx.value_php_type(datetime)?, "strtotime datetime")?;
            ctx.load_string_value_to_regs(datetime, "x1", "x2")?;
        }
        Arch::X86_64 => {
            require_string(ctx.value_php_type(datetime)?, "strtotime datetime")?;
            ctx.load_string_value_to_regs(datetime, "rdi", "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_strtotime");
    store_if_result(ctx, inst)
}

/// Lowers `time()` through the shared wall-clock runtime helper.
pub(super) fn lower_time(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "time", 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_time");
    store_if_result(ctx, inst)
}

/// Loads a `date()` timestamp or the `-1` current-time sentinel into the integer result register.
fn load_date_timestamp(
    ctx: &mut FunctionContext<'_>,
    timestamp: Option<ValueId>,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let Some(timestamp) = timestamp else {
        abi::emit_load_int_immediate(ctx.emitter, result_reg, -1);
        return Ok(());
    };
    match ctx.value_php_type(timestamp)? {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, result_reg, -1);
            Ok(())
        }
        ty => {
            require_integer_like(ty, "date timestamp")?;
            ctx.load_value_to_result(timestamp)?;
            Ok(())
        }
    }
}

/// Loads a `date()` format string into the runtime helper's string argument registers.
fn load_date_format(ctx: &mut FunctionContext<'_>, format: ValueId) -> Result<()> {
    require_string(ctx.value_php_type(format)?, "date format")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.load_string_value_to_regs(format, "x1", "x2"),
        Arch::X86_64 => ctx.load_string_value_to_regs(format, "rdi", "rsi"),
    }
}

/// Loads one integer-like runtime argument into a caller-selected register.
fn materialize_integer_arg(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    reg: &str,
    context: &str,
) -> Result<()> {
    require_integer_like(ctx.load_value_to_reg(value, reg)?, context)
}

/// Verifies a value can be passed as a date/time integer option.
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

/// Verifies a value can be passed as a date/time string argument.
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

/// Verifies that the builtin call has between the expected lowered operand counts.
fn ensure_arg_count_between(
    inst: &Instruction,
    name: &str,
    min: usize,
    max: usize,
) -> Result<()> {
    if (min..=max).contains(&inst.operands.len()) {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {} to {} args, got {}",
        name,
        min,
        max,
        inst.operands.len()
    )))
}
