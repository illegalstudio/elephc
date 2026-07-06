//! Purpose:
//! Emits PHP `round` numeric builtin calls backed by floating-point/libm-style helpers.
//! Marshals integer or float operands into the target ABI and records the numeric return type.
//!
//! Called from:
//! - `crate::codegen_support::builtins::math::emit()`.
//!
//! Key details:
//! - NaN, infinity, rounding, and division edge cases must remain PHP-compatible with type-checker signatures.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `round(value [, precision])` builtin.
///
/// ## Single-argument form (`args.len() == 1`)
/// Emits `frinta` (AArch64) or `call round` (x86_64) directly on the value,
/// after promoting integers to float and normalising `Mixed`/`Union` payloads.
///
/// ## Two-argument form (`args.len() == 2`)
/// Scales the value by `10^precision`, rounds to the nearest integer with ties
/// away from zero, then divides back by the multiplier.  This produces PHP's
/// documented rounding behaviour for non-zero precision.
///
/// # Arguments
/// * `_name`  – builtin name (unused; the caller dispatches by name).
/// * `args`   – expression tree for `value` and optionally `precision`.
/// * `emitter`– target assembly emitter.
/// * `ctx`    – codegen context (types, frame layout, etc.).
/// * `data`   – data section for literal pools.
///
/// # Returns
/// `Some(PhpType::Float)` because `round()` always returns a float in PHP.
///
/// # ABI notes
/// AArch64: input in `x0`/`d0`, result in `d0`.  x86_64: input in `rax`/`xmm0`,
/// result in `xmm0`.  `Mixed` payloads are normalised via `__rt_mixed_cast_float`
/// before any operation.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("round()");

    if args.len() == 1 {
        let ty = emit_expr(&args[0], emitter, ctx, data);
        match emitter.target.arch {
            Arch::AArch64 => {
                if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
                    abi::emit_call_label(emitter, "__rt_mixed_cast_float");     // normalize boxed numeric/null payloads to a floating-point round() input
                } else if ty != PhpType::Float {
                    emitter.instruction("scvtf d0, x0");                        // convert the round() input to float when it is an integer
                }
                emitter.instruction("frinta d0, d0");                           // round to nearest with ties away from zero on AArch64
            }
            Arch::X86_64 => {
                if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
                    abi::emit_call_label(emitter, "__rt_mixed_cast_float");     // normalize boxed numeric/null payloads to a floating-point round() input
                } else if ty != PhpType::Float {
                    emitter.instruction("cvtsi2sd xmm0, rax");                  // convert the round() input to float when it is an integer
                }
                emitter.instruction("call round");                              // delegate PHP-compatible nearest-integer rounding to libc round() on linux-x86_64
            }
        }
    } else {
        let ty = emit_expr(&args[0], emitter, ctx, data);
        match emitter.target.arch {
            Arch::AArch64 => {
                if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
                    abi::emit_call_label(emitter, "__rt_mixed_cast_float");     // normalize boxed numeric/null payloads to a floating-point round() input
                } else if ty != PhpType::Float {
                    emitter.instruction("scvtf d0, x0");                        // convert the round() value to float when it is an integer
                }
                emitter.instruction("str d0, [sp, #-16]!");                     // preserve the original value while computing the precision multiplier

                let t1 = emit_expr(&args[1], emitter, ctx, data);
                if t1 == PhpType::Float {
                    emitter.instruction("fcvtzs x0, d0");                       // convert the precision argument from float to integer before pow()
                }

                emitter.instruction("scvtf d1, x0");                            // convert the integer precision into the pow() exponent floating-point register
                emitter.instruction("str d1, [sp, #-16]!");                     // preserve the floating precision exponent while materializing the pow() base
                emitter.instruction("fmov d0, #10.0");                          // materialize 10.0 as the pow() base for precision scaling
                emitter.instruction("ldr d1, [sp], #16");                       // restore the floating precision exponent into the second pow() argument register
                emitter.bl_c("pow");                                            // compute 10^precision through the C library pow() helper

                emitter.instruction("ldr d1, [sp], #16");                       // restore the original value after pow() returns the precision multiplier
                emitter.instruction("fmul d1, d1, d0");                         // scale the original value by the precision multiplier before rounding
                emitter.instruction("str d0, [sp, #-16]!");                     // preserve the precision multiplier for the final division step
                emitter.instruction("frinta d0, d1");                           // round the scaled value to the nearest integer with ties away from zero
                emitter.instruction("ldr d1, [sp], #16");                       // restore the precision multiplier for the final division step
                emitter.instruction("fdiv d0, d0, d1");                         // divide the rounded scaled value back down by the precision multiplier
            }
            Arch::X86_64 => {
                if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
                    abi::emit_call_label(emitter, "__rt_mixed_cast_float");     // normalize boxed numeric/null payloads to a floating-point round() input
                } else if ty != PhpType::Float {
                    emitter.instruction("cvtsi2sd xmm0, rax");                  // convert the round() value to float when it is an integer
                }
                abi::emit_push_float_reg(emitter, "xmm0");                      // preserve the original value while computing the precision multiplier

                let t1 = emit_expr(&args[1], emitter, ctx, data);
                if t1 == PhpType::Float {
                    emitter.instruction("cvttsd2si rax, xmm0");                 // convert the precision argument from float to integer before pow()
                }

                emitter.instruction("cvtsi2sd xmm1, rax");                      // convert the integer precision into the second pow() floating-point argument register
                emitter.instruction("mov rax, 0x4024000000000000");             // materialize the IEEE-754 bit pattern for 10.0 before calling pow()
                emitter.instruction("movq xmm0, rax");                          // move 10.0 into the first pow() floating-point argument register
                emitter.instruction("call pow");                                // compute 10^precision through the libc pow() helper on linux-x86_64
                abi::emit_pop_float_reg(emitter, "xmm1");                       // restore the original value into the left-hand floating-point scratch register
                emitter.instruction("mulsd xmm1, xmm0");                        // scale the original value by the precision multiplier before rounding
                abi::emit_push_float_reg(emitter, "xmm0");                      // preserve the precision multiplier for the final division step
                emitter.instruction("movsd xmm0, xmm1");                        // move the scaled value into the first libc round() floating-point argument register
                emitter.instruction("call round");                              // round the scaled value through libc round() to preserve PHP rounding semantics
                abi::emit_pop_float_reg(emitter, "xmm1");                       // restore the precision multiplier into the left-hand floating-point scratch register
                emitter.instruction("divsd xmm0, xmm1");                        // divide the rounded scaled value back down by the precision multiplier
            }
        }
    }

    Some(PhpType::Float)
}
