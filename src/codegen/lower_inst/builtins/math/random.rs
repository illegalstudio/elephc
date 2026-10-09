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
//! - `mt_rand()` and `rand()` draw from php's Mersenne Twister (`__rt_mt_*`, seeded by
//!   `mt_srand()` / `srand()`), `random_int()` from the CSPRNG chain (`__rt_random_*`), the split
//!   php-src makes. Without arguments the two twister builtins return the draw shifted right by
//!   one, php's `genrand_int31`, so they never exceed `mt_getrandmax()`.

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
        0 => {
            abi::emit_call_label(ctx.emitter, "__rt_mt_u32");
            match ctx.emitter.target.arch {
                Arch::AArch64 => ctx.emitter.instruction("lsr w0, w0, #1"),     // php's genrand_int31: drop the low bit
                Arch::X86_64 => ctx.emitter.instruction("shr eax, 1"),          // php's genrand_int31: drop the low bit
            }
        }
        2 => lower_random_range(ctx, inst, name, policy, RangeSource::Twister)?,
        count => {
            return Err(CodegenIrError::invalid_module(format!(
                "{} expected 0 or 2 args, got {}",
                name, count
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// php's `PHP_MT_RAND_MAX`, the largest value `mt_rand()` and `rand()` return without a range.
const PHP_MT_RAND_MAX: i64 = 2_147_483_647;

/// php's `MT_RAND_PHP` mode argument, which selects the deprecated legacy twist.
const MT_RAND_PHP_ARGUMENT: i64 = 1;

/// php-src's verbatim deprecation for seeding the legacy `MT_RAND_PHP` variant.
const MT_RAND_PHP_DEPRECATION: &str = "Deprecated: The MT_RAND_PHP variant of Mt19937 is deprecated\n";

/// Lowers `mt_getrandmax()` and its alias `getrandmax()` to php's fixed maximum.
pub(crate) fn lower_getrandmax(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), PHP_MT_RAND_MAX);
    store_if_result(ctx, inst)
}

/// Lowers `mt_srand(?int $seed = null, int $mode = MT_RAND_MT19937)` and its alias `srand()`.
///
/// The mode is settled first, so `MT_RAND_PHP`'s deprecation is raised before the seed is drawn
/// or read, as php raises it before seeding. A missing or null seed is drawn from the CSPRNG,
/// php's `php_random_mt19937_seed_default`; any other seed is converted the way php's int
/// parameter converts it and truncated to 32 bits by `__rt_mt_seed`.
pub(crate) fn lower_mt_srand(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let result = abi::int_result_reg(ctx.emitter);
    match inst.operands.get(1).copied() {
        Some(mode) => load_numeric_as_int(ctx, mode, "mt_srand")?,
        None => abi::emit_load_int_immediate(ctx.emitter, result, 0),
    }
    let standard = ctx.next_label("mt_srand_standard");
    let mode_ready = ctx.next_label("mt_srand_mode_ready");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp x0, #{MT_RAND_PHP_ARGUMENT}"));  // was the legacy MT_RAND_PHP variant requested?
            ctx.emitter.instruction(&format!("b.ne {standard}"));               // any other mode is MT_RAND_MT19937, as in php
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp rax, {MT_RAND_PHP_ARGUMENT}"));  // was the legacy MT_RAND_PHP variant requested?
            ctx.emitter.instruction(&format!("jne {standard}"));                // any other mode is MT_RAND_MT19937, as in php
        }
    }
    emit_static_deprecation(ctx, MT_RAND_PHP_DEPRECATION);
    abi::emit_load_int_immediate(ctx.emitter, result, crate::codegen_support::runtime::MT_MODE_PHP);
    abi::emit_jump(ctx.emitter, &mode_ready);
    ctx.emitter.label(&standard);
    abi::emit_load_int_immediate(ctx.emitter, result, crate::codegen_support::runtime::MT_MODE_MT19937);
    ctx.emitter.label(&mode_ready);
    abi::emit_push_reg(ctx.emitter, result);
    load_seed(ctx, inst.operands.first().copied())?;
    let seed_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let mode_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_reg_move(ctx.emitter, seed_arg, result);
    abi::emit_pop_reg(ctx.emitter, mode_arg);
    abi::emit_call_label(ctx.emitter, "__rt_mt_seed");
    abi::emit_load_int_immediate(ctx.emitter, result, crate::codegen_support::sentinels::NULL_SENTINEL);
    if inst.result.is_some()
        && matches!(
            inst.result_php_type.codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
    {
        crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
    }
    store_if_result(ctx, inst)
}

/// Loads `mt_srand()`'s seed into the integer result register, drawing one from the CSPRNG for a
/// missing or null seed.
fn load_seed(ctx: &mut FunctionContext<'_>, seed: Option<ValueId>) -> Result<()> {
    let Some(seed) = seed else {
        abi::emit_call_label(ctx.emitter, "__rt_random_u32");
        return Ok(());
    };
    let random = ctx.next_label("mt_srand_random_seed");
    let ready = ctx.next_label("mt_srand_seed_ready");
    match ctx.value_php_type(seed)?.codegen_repr() {
        PhpType::Void | PhpType::Never => {
            abi::emit_call_label(ctx.emitter, "__rt_random_u32");
            return Ok(());
        }
        // A nullable int carries its runtime tag beside the payload.
        PhpType::TaggedScalar => {
            ctx.load_value_to_result(seed)?;
            let null = crate::codegen_support::sentinels::TAGGED_SCALAR_TAG_NULL;
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction(&format!("cmp x1, #{null}"));       // is the nullable seed null?
                    ctx.emitter.instruction(&format!("b.eq {random}"));         // yes: php seeds from the CSPRNG
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction(&format!("cmp rdx, {null}"));       // is the nullable seed null?
                    ctx.emitter.instruction(&format!("je {random}"));           // yes: php seeds from the CSPRNG
                }
            }
        }
        // A boxed seed is null when its payload tag is null; otherwise it converts like `(int)`.
        PhpType::Mixed | PhpType::Union(_) => {
            let result = abi::int_result_reg(ctx.emitter);
            ctx.load_value_to_result(seed)?;
            abi::emit_push_reg(ctx.emitter, result);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            // Compare the tag before the box comes back off the stack: the pop does not touch the
            // flags, and on AArch64 it lands in the same register the tag is in.
            match ctx.emitter.target.arch {
                Arch::AArch64 => ctx.emitter.instruction("cmp x0, #8"),         // is the boxed seed null?
                Arch::X86_64 => ctx.emitter.instruction("cmp rax, 8"),          // is the boxed seed null?
            }
            abi::emit_pop_reg(ctx.emitter, result);
            match ctx.emitter.target.arch {
                Arch::AArch64 => ctx.emitter.instruction(&format!("b.eq {random}")), // yes: php seeds from the CSPRNG
                Arch::X86_64 => ctx.emitter.instruction(&format!("je {random}")),    // yes: php seeds from the CSPRNG
            }
            // `__rt_mixed_cast_int` reads the box from the result register (`x0` / `rax`).
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
        }
        _ => load_numeric_as_int(ctx, seed, "mt_srand")?,
    }
    abi::emit_jump(ctx.emitter, &ready);
    ctx.emitter.label(&random);
    abi::emit_call_label(ctx.emitter, "__rt_random_u32");
    ctx.emitter.label(&ready);
    Ok(())
}

/// Raises a static diagnostic through the shared warning channel.
fn emit_static_deprecation(ctx: &mut FunctionContext<'_>, message: &str) {
    let (label, len) = ctx.data.add_string(message.as_bytes());
    let (ptr, length) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rdi", "rsi"),
    };
    abi::emit_symbol_address(ctx.emitter, ptr, &label);
    abi::emit_load_int_immediate(ctx.emitter, length, len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
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
        RangeSource::Csprng,
    )?;
    store_if_result(ctx, inst)
}

/// Which generator a range builtin draws from.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RangeSource {
    /// `random_int()`: the CSPRNG, which no seed can make predictable.
    Csprng,
    /// `mt_rand()` / `rand()`: php's Mersenne Twister, through `__rt_mt_rand_common`, which
    /// also applies the deprecated `MT_RAND_PHP` scaling when that mode was seeded.
    Twister,
}

/// Emits the shared inclusive-range lowering for random integer builtins.
fn lower_random_range(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    policy: InvertedRangePolicy,
    source: RangeSource,
) -> Result<()> {
    let min = expect_operand(inst, 0)?;
    let max = expect_operand(inst, 1)?;
    load_numeric_as_int(ctx, min, name)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    load_numeric_as_int(ctx, max, name)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_aarch64_random_range(ctx, policy, source),
        Arch::X86_64 => emit_x86_64_random_range(ctx, policy, source),
    }
}

/// Emits the AArch64 range normalization and runtime call.
fn emit_aarch64_random_range(
    ctx: &mut FunctionContext<'_>,
    policy: InvertedRangePolicy,
    source: RangeSource,
) -> Result<()> {
    abi::emit_pop_reg(ctx.emitter, "x9");
    emit_inverted_range_policy(ctx, policy, "x9", "x0");
    if source == RangeSource::Twister {
        ctx.emitter.instruction("mov x1, x0");                                  // pass max as the second bound
        ctx.emitter.instruction("mov x0, x9");                                  // pass min as the first bound
        abi::emit_call_label(ctx.emitter, "__rt_mt_rand_common");
        return Ok(());
    }
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
    source: RangeSource,
) -> Result<()> {
    abi::emit_pop_reg(ctx.emitter, "r9");
    emit_inverted_range_policy(ctx, policy, "r9", "rax");
    if source == RangeSource::Twister {
        ctx.emitter.instruction("mov rdi, r9");                                 // pass min as the first bound
        ctx.emitter.instruction("mov rsi, rax");                                // pass max as the second bound
        abi::emit_call_label(ctx.emitter, "__rt_mt_rand_common");
        return Ok(());
    }
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
        // A `mixed` or union bound (a parameter the caller could not type, #754) is coerced the
        // way `(int)` does: an int passes through, a float truncates, a numeric string parses.
        // The box stays owned by its value; the call's operand release handles it afterwards.
        PhpType::Mixed | PhpType::Union(_) => {
            let result_reg = abi::int_result_reg(ctx.emitter);
            let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
            if result_reg != arg_reg {
                abi::emit_reg_move(ctx.emitter, arg_reg, result_reg);
            }
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}
