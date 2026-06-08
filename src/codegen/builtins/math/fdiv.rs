//! Purpose:
//! Emits PHP `fdiv` numeric builtin calls backed by floating-point/libm-style helpers.
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
use crate::codegen::expr::{coerce_to_float, emit_expr};
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `fdiv(dividend, divisor)` builtin call.
///
/// Converts the dividend to a double-precision float if it is an integer, then
/// preserves it in a float register while evaluating the divisor expression. The
/// divisor is similarly converted to float if needed before the division is
/// performed. On AArch64 the quotient is written directly to `d0`; on x86_64 the
/// result is moved from the left-hand scratch register to the standard result
/// register (`xmm0`). Returns `PhpType::Float`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fdiv()");
    let t0 = emit_expr(&args[0], emitter, ctx, data);
    coerce_to_float(emitter, &t0);                                              // normalize the dividend to a float (handles int and boxed Mixed/Union)
    abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));          // preserve the dividend while the divisor expression is evaluated
    let t1 = emit_expr(&args[1], emitter, ctx, data);
    coerce_to_float(emitter, &t1);                                              // normalize the divisor to a float (handles int and boxed Mixed/Union)
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_float_reg(emitter, "d1");                             // restore the dividend into the left-hand floating-point scratch register
            emitter.instruction("fdiv d0, d1, d0");                             // compute dividend / divisor in the standard floating-point result register
        }
        Arch::X86_64 => {
            abi::emit_pop_float_reg(emitter, "xmm1");                           // restore the dividend into the left-hand floating-point scratch register
            emitter.instruction("divsd xmm1, xmm0");                            // compute dividend / divisor in the left-hand floating-point scratch register
            emitter.instruction("movsd xmm0, xmm1");                            // move the quotient back into the standard floating-point result register
        }
    }
    Some(PhpType::Float)
}
