//! Purpose:
//! Emits PHP `pow` numeric builtin calls backed by floating-point/libm-style helpers.
//! Marshals integer or float operands into the target ABI and records the numeric return type.
//!
//! Called from:
//! - `crate::codegen::builtins::math::emit()`.
//!
//! Key details:
//! - NaN, infinity, rounding, and division edge cases must remain PHP-compatible with type-checker signatures.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits PHP `pow(base, exponent)` as a call to the platform's libc `pow()`.
///
/// Both operands are evaluated, converted to floating-point if needed, and
/// passed to `pow()` following the target ABI (AArch64: d0/d1, X86_64: xmm0/xmm1).
/// The base is saved to a scratch float register before the exponent is evaluated,
/// then restored to the first argument register after exponent evaluation. The
/// result is always `PhpType::Float`. The `_name` parameter is unused for this
/// builtin and is accepted only to match the emitter dispatch signature.
///
/// # Arguments
/// * `_name` - Unused; present only to match the builtin emitter dispatch.
/// * `args` - Must contain exactly 2 expressions: base and exponent.
/// * `emitter` - Target-specific instruction emission.
/// * `ctx` - Codegen context carrying variable layout and class metadata.
/// * `data` - Data section for relocations and constant materialization.
///
/// # Returns
/// `Some(PhpType::Float)` on success; `None` is not produced by this emitter
/// but is returned to satisfy the emitter trait signature.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("pow()");
    // -- evaluate base, save it, evaluate exponent, call C pow() --
    let t0 = emit_expr(&args[0], emitter, ctx, data);
    let t0_mixed = matches!(t0, PhpType::Mixed | PhpType::Union(_));
    match emitter.target.arch {
        Arch::AArch64 => {
            if t0_mixed {
                // The base is a boxed Mixed cell pointer, not a scalar; cast it to a double
                // through the runtime so `scvtf` does not convert the pointer itself.
                abi::emit_call_label(emitter, "__rt_mixed_cast_float");
            } else if t0 != PhpType::Float {
                emitter.instruction("scvtf d0, x0");                            // convert the pow() base to float when the first argument is an integer
            }
        }
        Arch::X86_64 => {
            if t0_mixed {
                abi::emit_call_label(emitter, "__rt_mixed_cast_float");
            } else if t0 != PhpType::Float {
                emitter.instruction("cvtsi2sd xmm0, rax");                      // convert the pow() base to float when the first argument is an integer
            }
        }
    }
    abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));          // preserve the floating pow() base while the exponent expression is evaluated
    let t1 = emit_expr(&args[1], emitter, ctx, data);
    let t1_mixed = matches!(t1, PhpType::Mixed | PhpType::Union(_));
    match emitter.target.arch {
        Arch::AArch64 => {
            if t1_mixed {
                // The exponent is a boxed Mixed cell pointer; cast it to a double through
                // the runtime so `scvtf` does not convert the pointer itself.
                abi::emit_call_label(emitter, "__rt_mixed_cast_float");
            } else if t1 != PhpType::Float {
                emitter.instruction("scvtf d0, x0");                            // convert the pow() exponent to float when the second argument is an integer
            }
            emitter.instruction("fmov d1, d0");                                 // move the floating exponent into the second libc pow() argument register
            abi::emit_pop_float_reg(emitter, "d0");                             // restore the floating base into the first libc pow() argument register
            emitter.bl_c("pow");                                                // delegate the exponentiation to libc pow() on AArch64
        }
        Arch::X86_64 => {
            if t1_mixed {
                abi::emit_call_label(emitter, "__rt_mixed_cast_float");
            } else if t1 != PhpType::Float {
                emitter.instruction("cvtsi2sd xmm0, rax");                      // convert the pow() exponent to float when the second argument is an integer
            }
            abi::emit_pop_float_reg(emitter, "xmm1");                           // restore the floating base into a scratch floating-point register before ordering the SysV libc pow() arguments
            emitter.instruction("movapd xmm2, xmm0");                           // preserve the floating exponent while the floating base is moved into the first libc pow() argument register
            emitter.instruction("movapd xmm0, xmm1");                           // move the floating base into the first libc pow() argument register
            emitter.instruction("movapd xmm1, xmm2");                           // move the floating exponent into the second libc pow() argument register
            emitter.instruction("call pow");                                    // delegate the exponentiation to libc pow() on linux-x86_64
        }
    }
    Some(PhpType::Float)
}
