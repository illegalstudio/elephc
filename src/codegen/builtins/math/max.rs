//! Purpose:
//! Emits PHP `max` numeric builtin calls.
//! Handles scalar argument lowering and returns the PHP numeric type promised by signature checking.
//!
//! Called from:
//! - `crate::codegen::builtins::math::emit()`.
//!
//! Key details:
//! - Integer-vs-float result selection must stay aligned with PHP semantics and local type inference.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `max()` builtin with scalar numeric arguments.
///
/// Iterates over all arguments, maintaining the current maximum in the ABI result
/// register. Each subsequent argument is evaluated and compared, with the larger
/// value written back into the result register. Float arguments trigger promotion
/// of all prior integer candidates before comparison.
///
/// # Arguments
/// * `_name` — unused; present for dispatcher uniformity with other builtins
/// * `args` — evaluated left-to-right; must contain at least one expression
/// * `emitter` — receives the comparison/selection instructions; carries target arch
/// * `ctx` — variable and type context for expression emission
/// * `data` — data section for any literals or runtime constants
///
/// # Returns
/// `Some(PhpType::Float)` if any argument was a float (all prior ints promoted);
/// `Some(PhpType::Int)` if all arguments were integers.
///
/// # ABI behavior
/// * AArch64: integer results in `x0`; floats in `d0`; scratch register `x1`/`d1`
/// * X86_64: integer results in `rax`; floats in `xmm0`; scratch register `r9`/`xmm1`
///
/// # Side effects
/// * Stack: one 16-byte slot is pushed per iteration to preserve the running maximum
///   while the next candidate is evaluated; pops are architecture-aware (int vs float)
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("max()");

    // -- evaluate first arg --
    let t0 = emit_expr(&args[0], emitter, ctx, data);
    let mut any_float = t0 == PhpType::Float;

    for (i, arg) in args.iter().enumerate().skip(1) {
        // -- push current maximum onto stack --
        if any_float {
            if i == 1 && t0 != PhpType::Float {
                abi::emit_int_result_to_float_result(emitter);                  // normalize the first max() operand into the active floating-point result register before it becomes the running floating maximum
            }
            abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));  // preserve the current floating maximum while the next candidate expression is evaluated
        } else {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // preserve the current integer maximum while the next candidate expression is evaluated
        }

        let ti = emit_expr(arg, emitter, ctx, data);

        if any_float || ti == PhpType::Float {
            // -- float comparison path --
            if ti != PhpType::Float {
                abi::emit_int_result_to_float_result(emitter);                  // normalize the new max() candidate into the active floating-point result register before the floating comparison
            }
            if !any_float {
                match emitter.target.arch {
                    Arch::AArch64 => {
                        abi::emit_pop_reg(emitter, "x9");                       // restore the previous integer maximum before promoting it into the floating comparison path
                        emitter.instruction("scvtf d1, x9");                    // convert the previous integer maximum into the secondary AArch64 floating-point scratch register
                    }
                    Arch::X86_64 => {
                        abi::emit_pop_reg(emitter, "r9");                       // restore the previous integer maximum before promoting it into the floating comparison path
                        emitter.instruction("cvtsi2sd xmm1, r9");               // convert the previous integer maximum into the secondary x86_64 floating-point scratch register
                    }
                }
            } else {
                match emitter.target.arch {
                    Arch::AArch64 => {
                        abi::emit_pop_float_reg(emitter, "d1");                 // restore the previous floating maximum into the secondary AArch64 floating-point scratch register
                    }
                    Arch::X86_64 => {
                        abi::emit_pop_float_reg(emitter, "xmm1");               // restore the previous floating maximum into the secondary x86_64 floating-point scratch register
                    }
                }
            }
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("fmax d0, d1, d0");                     // compute the larger of the previous and new floating candidates in the standard AArch64 floating-point result register
                }
                Arch::X86_64 => {
                    emitter.instruction("maxsd xmm1, xmm0");                    // compute the larger of the previous and new floating candidates in the secondary x86_64 floating-point scratch register
                    emitter.instruction("movsd xmm0, xmm1");                    // move the updated floating maximum back into the standard x86_64 floating-point result register
                }
            }
            any_float = true;
        } else {
            // -- integer comparison path --
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("ldr x1, [sp], #16");                   // restore the previous integer maximum into the AArch64 scratch register before the scalar comparison
                    emitter.instruction("cmp x1, x0");                          // compare the previous integer maximum against the new integer candidate
                    emitter.instruction("csel x0, x1, x0, gt");                 // select the larger integer value into the standard AArch64 integer result register
                }
                Arch::X86_64 => {
                    abi::emit_pop_reg(emitter, "r9");                           // restore the previous integer maximum into a scratch register before the scalar comparison
                    emitter.instruction("cmp r9, rax");                         // compare the previous integer maximum against the new integer candidate
                    emitter.instruction("cmovg rax, r9");                       // keep the larger integer value in the standard x86_64 integer result register
                }
            }
        }
    }

    if any_float {
        Some(PhpType::Float)
    } else {
        Some(PhpType::Int)
    }
}
