//! Purpose:
//! Emits PHP `fmod` numeric builtin calls backed by floating-point/libm-style helpers.
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

/// Emits the `fmod(x, y)` builtin call, returning the floating-point remainder of x / y.
///
/// # Arguments
/// - `_name`: unused, matches the dispatcher contract.
/// - `args`: two expressions — the dividend `x` and the divisor `y`.
/// - `emitter`: target instruction emission.
/// - `ctx`: variable layout, ownership state, class/FFI metadata.
/// - `data`: read-only data section for relocations.
///
/// # Returns
/// `Some(PhpType::Float)` — `fmod` always returns a float.
///
/// # Behavior
/// Both operands are evaluated in source order. The dividend is preserved on the stack while
/// the divisor is evaluated so the ABI argument order can be satisfied.
/// - ARM64: converts integers to double (`scvtf`), then computes `dividend - trunc(dividend/divisor) * divisor`
///   using `frintz` + `fmsub` to match PHP/C `fmod` truncation-toward-zero semantics.
/// - x86_64: converts integers to double (`cvtsi2sd`), then delegates to `libc::fmod` via `call fmod`.
/// Division-by-zero and NaN inputs produce PHP-expected results via the underlying libm routines.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fmod()");
    let t0 = emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            if t0 != PhpType::Float {
                emitter.instruction("scvtf d0, x0");                            // convert the dividend to float when the first fmod() argument is an integer
            }
        }
        Arch::X86_64 => {
            if t0 != PhpType::Float {
                emitter.instruction("cvtsi2sd xmm0, rax");                      // convert the dividend to float when the first fmod() argument is an integer
            }
        }
    }
    abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));          // preserve the dividend while the divisor expression is evaluated
    let t1 = emit_expr(&args[1], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            if t1 != PhpType::Float {
                emitter.instruction("scvtf d0, x0");                            // convert the divisor to float when the second fmod() argument is an integer
            }
            abi::emit_pop_float_reg(emitter, "d1");                             // restore the dividend into the left-hand floating-point scratch register
            emitter.instruction("fdiv d2, d1, d0");                             // compute dividend / divisor in a temporary floating-point register
            emitter.instruction("frintz d2, d2");                               // truncate the quotient toward zero to match PHP/C fmod semantics
            emitter.instruction("fmsub d0, d2, d0, d1");                        // compute dividend - trunc(dividend/divisor) * divisor as the floating remainder
        }
        Arch::X86_64 => {
            if t1 != PhpType::Float {
                emitter.instruction("cvtsi2sd xmm0, rax");                      // convert the divisor to float when the second fmod() argument is an integer
            }
            abi::emit_pop_float_reg(emitter, "xmm1");                           // restore the dividend into the left-hand floating-point argument register
            emitter.instruction("movapd xmm2, xmm0");                           // preserve the divisor while rearranging the floating-point libc fmod() arguments
            emitter.instruction("movapd xmm0, xmm1");                           // move the dividend into the first libc fmod() floating-point argument register
            emitter.instruction("movapd xmm1, xmm2");                           // move the divisor into the second libc fmod() floating-point argument register
            emitter.instruction("call fmod");                                   // delegate the floating remainder semantics to libc fmod() on linux-x86_64
        }
    }
    Some(PhpType::Float)
}
