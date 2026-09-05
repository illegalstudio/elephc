//! Purpose:
//! Lowers PHP `mb_strimwidth()` through the shared `__rt_mb_strimwidth` helper.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions` dispatch group 10.
//!
//! Key details:
//! - Materializes string/start/width plus optional marker and nullable encoding.
//! - Omitted/null encoding becomes a null pointer plus zero length; the runtime
//!   treats that as UTF-8 and rejects unknown names with a catchable `ValueError`.

use super::*;

/// Lowers `mb_strimwidth(string, start, width, trim_marker = "", encoding = null)`.
pub(crate) fn lower_mb_strimwidth(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count_between(inst, "mb_strimwidth", 3, 5)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_mb_strimwidth_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_mb_strimwidth_x86_64(ctx, inst)?,
    }
    abi::emit_call_label(ctx.emitter, "__rt_mb_strimwidth");
    store_if_result(ctx, inst)
}

/// Materializes AArch64 `__rt_mb_strimwidth` arguments.
///
/// Encoding length lives in `x0` so the optional encoding pointer can occupy `x7`.
/// Earlier operands are parked on the stack because later loads clobber them.
fn lower_mb_strimwidth_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_string_arg_to_regs(ctx, inst, 0, "mb_strimwidth", "x1", "x2")?;
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
    let start = expect_operand(inst, 1)?;
    load_as_int(ctx, start, "mb_strimwidth start")?;
    abi::emit_push_reg_pair(ctx.emitter, "x0", "xzr");
    let width = expect_operand(inst, 2)?;
    load_as_int(ctx, width, "mb_strimwidth width")?;
    abi::emit_push_reg_pair(ctx.emitter, "x0", "xzr");
    load_optional_mb_strimwidth_marker(ctx, inst, "x1", "x2")?;
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
    load_optional_mb_strimwidth_encoding(ctx, inst, "x7", "x0")?;
    abi::emit_pop_reg_pair(ctx.emitter, "x5", "x6");
    abi::emit_pop_reg_pair(ctx.emitter, "x4", "x9");
    abi::emit_pop_reg_pair(ctx.emitter, "x3", "x9");
    abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
    Ok(())
}

/// Materializes x86_64 `__rt_mb_strimwidth` arguments.
///
/// The helper reads `rax`/`rdx` (string), `rcx` (start), `r8` (width), `r9`/`r10`
/// (marker), and `r11`/`rdi` (optional encoding pointer/length).
fn lower_mb_strimwidth_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_string_arg_to_regs(ctx, inst, 0, "mb_strimwidth", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    let start = expect_operand(inst, 1)?;
    load_as_int(ctx, start, "mb_strimwidth start")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rax");
    let width = expect_operand(inst, 2)?;
    load_as_int(ctx, width, "mb_strimwidth width")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rax");
    load_optional_mb_strimwidth_marker(ctx, inst, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_optional_mb_strimwidth_encoding(ctx, inst, "r11", "rdi")?;
    abi::emit_pop_reg_pair(ctx.emitter, "r9", "r10");
    abi::emit_pop_reg_pair(ctx.emitter, "r8", "rsi");
    abi::emit_pop_reg_pair(ctx.emitter, "rcx", "rsi");
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Loads the optional trim marker, or a zero-length string when it is omitted.
fn load_optional_mb_strimwidth_marker(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    ptr_reg: &str,
    len_reg: &str,
) -> Result<()> {
    let Some(marker) = inst.operands.get(3).copied() else {
        abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
        abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
        return Ok(());
    };
    load_value_as_string_to_regs(ctx, marker, "mb_strimwidth trim_marker", ptr_reg, len_reg)
}

/// Loads the nullable optional encoding into a pointer/length pair.
fn load_optional_mb_strimwidth_encoding(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    ptr_reg: &str,
    len_reg: &str,
) -> Result<()> {
    let Some(encoding) = inst.operands.get(4).copied() else {
        abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
        abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
        return Ok(());
    };
    if matches!(ctx.value_php_type(encoding)?, PhpType::Void | PhpType::Never) {
        abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
        abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
        return Ok(());
    }
    load_value_as_string_to_regs(ctx, encoding, "mb_strimwidth encoding", ptr_reg, len_reg)
}
