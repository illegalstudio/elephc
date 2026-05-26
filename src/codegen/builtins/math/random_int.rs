//! Purpose:
//! Emits PHP `random_int` random-number builtin calls.
//! Delegates entropy and range handling to runtime helpers while producing PHP integer results.
//!
//! Called from:
//! - `crate::codegen::builtins::math::emit()`.
//!
//! Key details:
//! - Random helpers are effectful and must not be treated as pure by callers or optimizer assumptions.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `random_int($min, $max)` builtin.
///
/// Produces a cryptographically secure random integer in the inclusive range
/// [$min, $max]. The implementation evaluates `$min` first, then `$max`, pushes
/// the minimum to preserve it while evaluating the maximum, then calls the runtime
/// helper `__rt_random_uniform` to obtain a uniform offset in the half-open range
/// [0, max-min+1). The offset is shifted by `$min` to restore the inclusive interval.
///
/// # Arguments
/// - `_name`: the builtin name (unused, already resolved to `random_int`).
/// - `args`: exactly two expressions: the minimum and maximum bounds.
/// - `emitter`: target-specific instruction emitter.
/// - `ctx`: codegen context with variable layout and target info.
/// - `data`: mutable data section for constants/labels.
///
/// # Returns
/// `Some(PhpType::Int)` indicating the result is a PHP integer.
/// The value is placed in the primary integer register (`x0` on ARM64, `rax` on x86_64)
/// per the target ABI.
///
/// # ABI constraints
/// - `emit_expr` for each argument may clobber the primary integer register.
/// - A scratch register (`x9`/`r9`) holds the preserved minimum across calls.
/// - `__rt_random_uniform` is called with the exclusive upper bound in `rdi`/`x0`
///   and returns a uniform value in the primary integer register.
///
/// # Panics
/// Panics if `args.len() != 2`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("random_int()");
    // -- random_int(min, max): cryptographically secure random in [min, max] --
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the inclusive minimum while evaluating the inclusive maximum expression
    emit_expr(&args[1], emitter, ctx, data);
    match emitter.target.arch {
        Arch::X86_64 => {
            abi::emit_pop_reg(emitter, "r9");                                   // restore the inclusive minimum into a scratch register before forming the random range on SysV x86_64
            emitter.instruction("sub rax, r9");                                 // compute the inclusive range width as max - min in the active integer result register
            emitter.instruction("add rax, 1");                                  // widen the exclusive upper bound to max - min + 1 before sampling a uniform offset
            emitter.instruction("mov rdi, rax");                                // move the exclusive upper bound into the first SysV integer argument register for __rt_random_uniform
            abi::emit_call_label(emitter, "__rt_random_uniform");               // draw a uniform random offset in the half-open range [0, max - min + 1)
            emitter.instruction("add rax, r9");                                 // shift the sampled offset back into the caller-visible inclusive [min, max] interval
        }
        _ => {
            abi::emit_pop_reg(emitter, "x9");                                   // restore the inclusive minimum into a scratch register before forming the random range on AArch64
            emitter.instruction("sub x0, x0, x9");                              // compute the inclusive range width as max - min in the active integer result register
            emitter.instruction("add x0, x0, #1");                              // widen the exclusive upper bound to max - min + 1 before sampling a uniform offset
            abi::emit_push_reg(emitter, "x9");                                  // preserve the inclusive minimum across the random helper call that reuses the primary integer result register
            abi::emit_call_label(emitter, "__rt_random_uniform");               // draw a uniform random offset in the half-open range [0, max - min + 1)
            abi::emit_pop_reg(emitter, "x9");                                   // restore the saved inclusive minimum after the random helper returns the sampled offset
            emitter.instruction("add x0, x0, x9");                              // shift the sampled offset back into the caller-visible inclusive [min, max] interval
        }
    }
    Some(PhpType::Int)
}
