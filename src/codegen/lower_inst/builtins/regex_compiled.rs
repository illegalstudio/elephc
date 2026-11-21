//! Purpose:
//! Lowers the internal compiled-pattern regex builtins the `preg_grep()` prelude calls:
//! `__elephc_preg_compile()`, `__elephc_preg_match_compiled()` and `__elephc_preg_free()`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions` for the three typed runtime targets.
//!
//! Key details:
//! - Only `__elephc_preg_compile()` clears the `preg_last_error()` state, like php-src, which
//!   resets it once per `preg_grep()` call. It records `PREG_INTERNAL_ERROR` when the
//!   pattern is invalid or uses the removed `/e` modifier and returns the `0` handle.
//! - `__elephc_preg_match_compiled()` leaves the state alone on a match or a failed match,
//!   so a nested `preg_*` error an entry's `__toString()` recorded stays visible; the
//!   `__rt_preg_exec` wrapper records the call's own match error and the helper returns `-1`.
//! - Handles are opaque PCRE2 shim pointers carried as PHP ints inside the prelude only.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;

/// Lowers `__elephc_preg_compile(pattern)`: resets the error state and compiles the pattern.
pub(crate) fn lower_preg_compile(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_preg_compile", 1)?;
    super::regex_errors::emit_clear_error(ctx);
    let pattern = super::expect_operand(inst, 0)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => load_string(ctx, pattern, "x1", "x2")?,
        Arch::X86_64 => load_string(ctx, pattern, "rdi", "rsi")?,
    }
    abi::emit_call_label(ctx.emitter, "__rt_preg_compile");
    let compiled = ctx.next_label("preg_compile_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {compiled}"));           // a non-null handle means the pattern compiled
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the pattern compile into a handle?
            ctx.emitter.instruction(&format!("jnz {compiled}"));                // a non-null handle means the pattern compiled
        }
    }
    super::regex_errors::emit_record_internal_error(ctx);
    ctx.emitter.label(&compiled);
    super::store_if_result(ctx, inst)
}

/// Lowers `__elephc_preg_match_compiled(regex, subject)` to `1`, `0`, or `-1` on a match error.
pub(crate) fn lower_preg_match_compiled(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_preg_match_compiled", 2)?;
    let regex = super::expect_operand(inst, 0)?;
    let subject = super::expect_operand(inst, 1)?;
    // -- the subject first: loading the integer handle afterwards leaves its registers intact --
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string(ctx, subject, "x1", "x2")?;
            load_handle(ctx, regex, "x0")?;
        }
        Arch::X86_64 => {
            load_string(ctx, subject, "rsi", "rdx")?;
            load_handle(ctx, regex, "rdi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_preg_match_compiled");
    super::store_if_result(ctx, inst)
}

/// Lowers `__elephc_preg_free(regex)` by releasing the compiled pattern.
pub(crate) fn lower_preg_free(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_preg_free", 1)?;
    let regex = super::expect_operand(inst, 0)?;
    let reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    load_handle(ctx, regex, reg)?;
    abi::emit_call_label(ctx.emitter, "__rt_preg_free");
    super::store_if_result(ctx, inst)
}

/// Loads a string operand into the given pointer/length registers.
fn load_string(ctx: &mut FunctionContext<'_>, value: ValueId, ptr_reg: &str, len_reg: &str) -> Result<()> {
    let ty = ctx.value_php_type(value)?;
    if ty != PhpType::Str {
        return Err(CodegenIrError::unsupported(format!(
            "compiled preg string operand for PHP type {ty:?}"
        )));
    }
    ctx.load_string_value_to_regs(value, ptr_reg, len_reg)
}

/// Loads an integer pattern handle operand into `reg`.
fn load_handle(ctx: &mut FunctionContext<'_>, value: ValueId, reg: &str) -> Result<()> {
    let ty = ctx.load_value_to_reg(value, reg)?;
    if ty != PhpType::Int {
        return Err(CodegenIrError::unsupported(format!(
            "compiled preg handle operand for PHP type {ty:?}"
        )));
    }
    Ok(())
}
