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
use super::{expect_operand, load_value_to_first_int_arg, store_if_result};

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

/// Lowers `sleep(seconds)` through the target's C library symbol.
pub(super) fn lower_sleep(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_blocking_c_call(ctx, inst, "sleep", "sleep seconds")
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

/// Lowers `usleep(microseconds)` through the target's C library symbol.
pub(super) fn lower_usleep(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_blocking_c_call(ctx, inst, "usleep", "usleep microseconds")
}

/// Lowers `getenv(name)` through the target-aware environment lookup helper.
pub(super) fn lower_getenv(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "getenv", 1)?;
    let name = expect_operand(inst, 0)?;
    require_string(ctx.load_value_to_result(name)?.codegen_repr(), "getenv name")?;
    abi::emit_call_label(ctx.emitter, "__rt_getenv");
    store_if_result(ctx, inst)
}

/// Lowers `php_uname(mode?)` through the target-aware uname runtime helper.
pub(super) fn lower_php_uname(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "php_uname", 0, 1)?;
    if let Some(mode) = inst.operands.first().copied() {
        require_string(ctx.load_value_to_result(mode)?.codegen_repr(), "php_uname mode")?;
    } else {
        let (label, len) = ctx.data.add_string(b"a");
        let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
        abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
        abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    }
    abi::emit_call_label(ctx.emitter, "__rt_php_uname");
    store_if_result(ctx, inst)
}

/// Lowers `exec(command)` by capturing shell stdout through the shared runtime helper.
pub(super) fn lower_exec(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_shell_exec_like(ctx, inst, "exec")
}

/// Lowers `shell_exec(command)` by capturing shell stdout through the shared runtime helper.
pub(super) fn lower_shell_exec(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_shell_exec_like(ctx, inst, "shell_exec")
}

/// Lowers `system(command)` through libc `system()` and returns the legacy empty string result.
pub(super) fn lower_system(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_direct_system_call(ctx, inst, "system", true)
}

/// Lowers `passthru(command)` through libc `system()` for direct stdout passthrough.
pub(super) fn lower_passthru(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_direct_system_call(ctx, inst, "passthru", false)
}

/// Lowers shell-capturing process builtins that return a PHP string.
fn lower_shell_exec_like(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let command = expect_operand(inst, 0)?;
    require_string(ctx.load_value_to_result(command)?.codegen_repr(), "shell command")?;
    abi::emit_call_label(ctx.emitter, "__rt_shell_exec");
    store_if_result(ctx, inst)
}

/// Lowers stdout-passthrough process builtins that execute a command via libc `system()`.
fn lower_direct_system_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    returns_empty_string: bool,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let command = expect_operand(inst, 0)?;
    require_string(ctx.load_value_to_result(command)?.codegen_repr(), "system command")?;
    abi::emit_call_label(ctx.emitter, "__rt_cstr");
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the null-terminated shell command to libc system()
    }
    ctx.emitter.bl_c("system");
    if returns_empty_string {
        emit_empty_string_result(ctx);
    }
    store_if_result(ctx, inst)
}

/// Materializes the legacy empty-string return value used after `system()`.
fn emit_empty_string_result(ctx: &mut FunctionContext<'_>) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
}

/// Lowers a one-argument blocking libc call that receives an integer duration.
fn lower_unary_blocking_c_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    context: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let duration = expect_operand(inst, 0)?;
    require_integer_like(load_value_to_first_int_arg(ctx, duration)?, context)?;
    ctx.emitter.bl_c(name);
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
