//! Purpose:
//! Lowers random integer math builtins for the EIR backend.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::math`.
//!
//! Key details:
//! - Range arguments are evaluated by AST-to-EIR in PHP source order; this module
//!   reloads the SSA slots and preserves the lower bound across runtime helper calls.
//! - The three range builtins disagree about an inverted range, and each follows php-src:
//!   `random_int()` and `mt_rand()` raise a catchable `ValueError` with their own wording,
//!   while `rand()` silently swaps the bounds. Without the guard the width `max - min + 1`
//!   went negative and `__rt_random_uniform` returned an unbounded garbage integer.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::super::context::FunctionContext;
use super::super::{expect_operand, store_if_result};

/// What a random-range builtin does when `$min` turns out to be greater than `$max`.
#[derive(Clone, Copy)]
enum InvertedRangePolicy {
    /// `rand()` silently samples the swapped `[max, min]` range, exactly like php-src.
    Swap,
    /// `random_int()` and `mt_rand()` raise a catchable `ValueError` carrying this message.
    Throw(&'static str),
}

/// php-src's verbatim `ValueError` wording for `random_int()` with `$min > $max`.
const RANDOM_INT_INVERTED_RANGE_MESSAGE: &str =
    "random_int(): Argument #1 ($min) must be less than or equal to argument #2 ($max)";

/// php-src's verbatim `ValueError` wording for `mt_rand()` with `$min > $max`.
const MT_RAND_INVERTED_RANGE_MESSAGE: &str =
    "mt_rand(): Argument #2 ($max) must be greater than or equal to argument #1 ($min)";

/// Lowers `rand()` and `mt_rand()` with either zero args or an inclusive range.
pub(crate) fn lower_rand(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    let policy = if name == "mt_rand" {
        InvertedRangePolicy::Throw(MT_RAND_INVERTED_RANGE_MESSAGE)
    } else {
        InvertedRangePolicy::Swap
    };
    match inst.operands.len() {
        0 => abi::emit_call_label(ctx.emitter, "__rt_random_u32"),
        2 => lower_random_range(ctx, inst, name, policy)?,
        count => {
            return Err(CodegenIrError::invalid_module(format!(
                "{} expected 0 or 2 args, got {}",
                name, count
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `random_int()` over an inclusive integer range.
pub(crate) fn lower_random_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "random_int", 2)?;
    lower_random_range(
        ctx,
        inst,
        "random_int",
        InvertedRangePolicy::Throw(RANDOM_INT_INVERTED_RANGE_MESSAGE),
    )?;
    store_if_result(ctx, inst)
}

/// Emits the shared inclusive-range lowering for random integer builtins.
fn lower_random_range(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    policy: InvertedRangePolicy,
) -> Result<()> {
    let min = expect_operand(inst, 0)?;
    let max = expect_operand(inst, 1)?;
    load_numeric_as_int(ctx, min, name)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    load_numeric_as_int(ctx, max, name)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_aarch64_random_range(ctx, policy),
        Arch::X86_64 => emit_x86_64_random_range(ctx, policy),
    }
}

/// Emits the AArch64 range normalization and runtime call.
fn emit_aarch64_random_range(
    ctx: &mut FunctionContext<'_>,
    policy: InvertedRangePolicy,
) -> Result<()> {
    abi::emit_pop_reg(ctx.emitter, "x9");
    emit_inverted_range_policy(ctx, policy, "x9", "x0");
    // INCLUSIVE width, deliberately not `+ 1`. The exclusive form wraps to zero for the full
    // 64-bit range, and the old 32-bit helper truncated any width that was a multiple of 2^32 —
    // either way the bound arrived as zero and every draw came back as `min`.
    ctx.emitter.instruction("sub x0, x0, x9");                                  // compute the inclusive range width as max - min
    abi::emit_push_reg(ctx.emitter, "x9");
    abi::emit_call_label(ctx.emitter, "__rt_random_uniform64");
    abi::emit_pop_reg(ctx.emitter, "x9");
    ctx.emitter.instruction("add x0, x0, x9");                                  // shift the sampled offset back into the caller-visible range
    Ok(())
}

/// Emits the x86_64 range normalization and runtime call.
fn emit_x86_64_random_range(
    ctx: &mut FunctionContext<'_>,
    policy: InvertedRangePolicy,
) -> Result<()> {
    abi::emit_pop_reg(ctx.emitter, "r9");
    emit_inverted_range_policy(ctx, policy, "r9", "rax");
    // See the AArch64 half: the width stays INCLUSIVE so it cannot wrap, and the 64-bit sampler
    // receives all of it rather than the low half.
    ctx.emitter.instruction("sub rax, r9");                                     // compute the inclusive range width as max - min
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the inclusive upper bound to the random helper
    abi::emit_call_label(ctx.emitter, "__rt_random_uniform64");
    ctx.emitter.instruction("add rax, r9");                                     // shift the sampled offset back into the caller-visible range
    Ok(())
}

/// Normalizes or rejects an inverted `[min, max]` range before the width is computed.
///
/// `min_reg` and `max_reg` still hold the materialized bounds, so a swap is a plain register
/// exchange and a rejection is a compare plus the shared `ValueError` sequence. Letting an
/// inverted range through would make `max - min + 1` non-positive, and `__rt_random_uniform`
/// treats that as an unbounded modulus, which is where the garbage `random_int(10, 5)` value
/// came from.
fn emit_inverted_range_policy(
    ctx: &mut FunctionContext<'_>,
    policy: InvertedRangePolicy,
    min_reg: &str,
    max_reg: &str,
) {
    match policy {
        InvertedRangePolicy::Throw(message) => {
            let ok_label = ctx.next_label("random_range_ok");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction(
                        &format!("cmp {}, {}", min_reg, max_reg)
                    );                                                          // is the requested range inverted?
                    ctx.emitter.instruction(&format!("b.le {}", ok_label));     // an ordered range samples normally
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction(
                        &format!("cmp {}, {}", min_reg, max_reg)
                    );                                                          // is the requested range inverted?
                    ctx.emitter.instruction(&format!("jle {}", ok_label));      // an ordered range samples normally
                }
            }
            crate::codegen::lower_inst::exceptions::emit_value_error(ctx, message);
            ctx.emitter.label(&ok_label);
        }
        InvertedRangePolicy::Swap => {
            let ok_label = ctx.next_label("random_range_ordered");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction(
                        &format!("cmp {}, {}", min_reg, max_reg)
                    );                                                          // is the requested range inverted?
                    ctx.emitter.instruction(&format!("b.le {}", ok_label));     // an ordered range needs no swap
                    ctx.emitter.instruction(&format!("mov x10, {}", min_reg));  // park the larger bound while the pair is exchanged
                    ctx.emitter.instruction(
                        &format!("mov {}, {}", min_reg, max_reg)
                    );                                                          // the smaller bound becomes the range minimum
                    ctx.emitter.instruction(&format!("mov {}, x10", max_reg));  // the larger bound becomes the range maximum
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction(
                        &format!("cmp {}, {}", min_reg, max_reg)
                    );                                                          // is the requested range inverted?
                    ctx.emitter.instruction(&format!("jle {}", ok_label));      // an ordered range needs no swap
                    ctx.emitter.instruction(
                        &format!("xchg {}, {}", min_reg, max_reg)
                    );                                                          // exchange the inverted bounds so the width stays positive
                }
            }
            ctx.emitter.label(&ok_label);
        }
    }
}

/// Loads a numeric range operand and normalizes values into the integer result register.
fn load_numeric_as_int(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    name: &str,
) -> Result<()> {
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Int | PhpType::Bool => Ok(()),
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            Ok(())
        }
        PhpType::Float => {
            abi::emit_float_result_to_int_result(ctx.emitter);
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}
