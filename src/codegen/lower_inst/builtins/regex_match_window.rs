//! Purpose:
//! Lowers the `$flags` / `$offset` forms of `preg_match()` and the `$offset` argument of
//! `preg_match_all()` onto the shared `__rt_preg_match_all_capture` helper.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::regex::lower_preg_match()`.
//! - `crate::codegen::lower_inst::builtins::regex::lower_preg_match_all()`.
//!
//! Key details:
//! - EIR lowering gives an extended `preg_match()` exactly five operands and the
//!   plain form at most three (`ir_lower::expr::preg_match_args`), so the operand count
//!   alone selects this path and matches the checker's `array<mixed>` `$matches` type.
//! - An extended `preg_match()` runs the capture helper in SET_ORDER with the private
//!   single-match bit: it stops after one match and returns that row, trimmed of trailing
//!   unmatched groups unless `PREG_UNMATCHED_AS_NULL` is set, like php-src.
//! - php-src rejects any `preg_match()` flag in the low order byte with a `ValueError`
//!   and ignores other unknown bits, so only `PREG_OFFSET_CAPTURE` and
//!   `PREG_UNMATCHED_AS_NULL` reach the helper.
//! - A boxed `$flags` / `$offset` (for example `$m[0][1] + 1`) is cast to an integer
//!   before the string arguments are materialized, because the cast helper call would
//!   clobber the argument registers; the casts wait on the temporary stack.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::Result;
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::regex;

/// Mirrors the runtime's private single-match bit (`PREG_CAPTURE_FIRST_ONLY`).
const PREG_CAPTURE_FIRST_ONLY: i64 = 1 << 29;
const PREG_SET_ORDER: i64 = 2;
const PREG_MATCH_FORWARDED_FLAGS: i64 = 256 | 512;
const PREG_MATCH_ORDER_BYTE: i64 = 0xff;

/// Lowers `preg_match(pattern, subject, &$matches, flags, offset)`.
pub(super) fn lower_preg_match_extended(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let pattern = super::expect_operand(inst, 0)?;
    let subject = super::expect_operand(inst, 1)?;
    let matches_slot = regex::optional_matches_local_slot(ctx, super::expect_operand(inst, 2)?)?;
    let flags = super::expect_operand(inst, 3)?;
    let offset = super::expect_operand(inst, 4)?;
    let casts = precast_int_args(ctx, Some(flags), Some(offset))?;
    regex::load_pattern_and_subject(ctx, pattern, subject)?;
    load_flags(ctx, Some(flags), &casts)?;
    emit_preg_match_flag_guard(ctx);
    load_offset_arg(ctx, Some(offset), &casts)?;
    abi::emit_call_label(ctx.emitter, "__rt_preg_match_all_capture");
    match matches_slot {
        Some(slot) => regex::store_matches_array_on_success(
            ctx,
            slot,
            &PhpType::Array(Box::new(PhpType::Mixed)),
        )?,
        None => discard_matches_array(ctx),
    }
    regex::box_count_or_false(ctx)?;
    super::store_if_result(ctx, inst)
}

/// Raises PHP's `ValueError` for an order flag, then keeps only the flags php-src honours.
///
/// The flags register then carries SET_ORDER plus the private single-match bit, which is
/// the capture helper's `preg_match()` mode.
fn emit_preg_match_flag_guard(ctx: &mut FunctionContext<'_>) {
    let invalid = ctx.next_label("preg_match_flags_invalid");
    let done = ctx.next_label("preg_match_flags_done");
    let mode = PREG_SET_ORDER | PREG_CAPTURE_FIRST_ONLY;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("tst x5, #{}", PREG_MATCH_ORDER_BYTE)); // order flags are invalid for preg_match()
            ctx.emitter.instruction(&format!("b.ne {}", invalid));              // raise PHP's ValueError
            ctx.emitter.instruction(&format!("and x5, x5, #{}", PREG_MATCH_FORWARDED_FLAGS)); // keep OFFSET_CAPTURE and UNMATCHED_AS_NULL
            abi::emit_load_int_immediate(ctx.emitter, "x9", mode);
            ctx.emitter.instruction("orr x5, x5, x9");                          // request the single-match SET_ORDER row
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test r8, {}", PREG_MATCH_ORDER_BYTE)); // order flags are invalid for preg_match()
            ctx.emitter.instruction(&format!("jnz {}", invalid));               // raise PHP's ValueError
            ctx.emitter.instruction(&format!("and r8, {}", PREG_MATCH_FORWARDED_FLAGS)); // keep OFFSET_CAPTURE and UNMATCHED_AS_NULL
            ctx.emitter.instruction(&format!("or r8, {}", mode));               // request the single-match SET_ORDER row
        }
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&invalid);
    crate::codegen::lower_inst::exceptions::emit_value_error(
        ctx,
        "preg_match(): Argument #4 ($flags) must be a PREG_* constant",
    );
    ctx.emitter.label(&done);
}

/// Which of `$flags` / `$offset` were cast from a boxed value onto the temporary stack.
#[derive(Clone, Copy, Default)]
pub(super) struct PrecastInts {
    flags: bool,
    offset: bool,
}

/// Casts boxed `$flags` / `$offset` operands to integers and pushes them, offset first.
///
/// Must run before any argument register is loaded. `load_flags()` and
/// `load_offset_arg()` pop them in the opposite order.
pub(super) fn precast_int_args(
    ctx: &mut FunctionContext<'_>,
    flags: Option<ValueId>,
    offset: Option<ValueId>,
) -> Result<PrecastInts> {
    let mut casts = PrecastInts::default();
    for (value, pushed) in [(offset, &mut casts.offset), (flags, &mut casts.flags)] {
        let Some(value) = value else { continue };
        if !matches!(ctx.raw_value_php_type(value)?.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            continue;
        }
        ctx.load_value_to_result(value)?;
        abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        *pushed = true;
    }
    Ok(casts)
}

/// Loads `$flags` into the capture helper's flags register, popping a pre-cast value.
pub(super) fn load_flags(
    ctx: &mut FunctionContext<'_>,
    flags: Option<ValueId>,
    casts: &PrecastInts,
) -> Result<()> {
    let reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x5",
        Arch::X86_64 => "r8",
    };
    if casts.flags {
        abi::emit_pop_reg(ctx.emitter, reg);
        return Ok(());
    }
    regex::load_flags_arg(ctx, flags, reg)
}

/// Loads the optional `$offset` into the capture helper's offset register (default `0`).
///
/// Called after the flag guard, which uses the x86_64 offset register as scratch.
pub(super) fn load_offset_arg(
    ctx: &mut FunctionContext<'_>,
    offset: Option<ValueId>,
    casts: &PrecastInts,
) -> Result<()> {
    let reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x6",
        Arch::X86_64 => "r9",
    };
    if casts.offset {
        abi::emit_pop_reg(ctx.emitter, reg);
        return Ok(());
    }
    let Some(offset) = offset else {
        abi::emit_load_int_immediate(ctx.emitter, reg, 0);
        return Ok(());
    };
    regex::require_integer_like(ctx.load_value_to_reg(offset, reg)?, "preg offset")
}

/// Releases the capture helper's array when the call has no `$matches` destination.
pub(super) fn discard_matches_array(ctx: &mut FunctionContext<'_>) {
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // pass the unused matches array to the release helper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdx");                            // pass the unused matches array to the release helper
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    abi::emit_pop_reg(ctx.emitter, result);
}
