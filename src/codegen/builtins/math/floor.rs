//! Purpose:
//! Emits PHP `floor` numeric builtin calls backed by floating-point/libm-style helpers.
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
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a `floor()` call, which rounds a value down to the nearest integer (toward minus infinity).
///
/// # Arguments
/// - `_name`: Ignored; present for dispatcher consistency.
/// - `args`: Single operand to floor. May be a float or integer type.
/// - `emitter`: Target-aware instruction emitter.
/// - `ctx`: Codegen context carrying type and variable information.
/// - `data`: Data section for constants/literals.
///
/// # Returns
/// Always returns `Some(PhpType::Float)` since floor always produces a floating-point result.
///
/// # ABI & Instruction Details
/// - **AArch64**: Converts integer to double via `scvtf` if needed, then `frintm` (round toward -∞).
/// - **x86_64**: Converts integer to SSE2 double via `cvtsi2sd` if needed, then `roundsd` with mode 1 (round toward -∞).
///
/// # Notes
/// PHP's `floor()` always returns a float, even for integer inputs.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("floor()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            if ty != PhpType::Float {
                emitter.instruction("scvtf d0, x0");                            // convert the floor() input to float when it is an integer
            }
            emitter.instruction("frintm d0, d0");                               // round toward minus infinity on AArch64
        }
        Arch::X86_64 => {
            if ty != PhpType::Float {
                emitter.instruction("cvtsi2sd xmm0, rax");                      // convert the floor() input to float when it is an integer
            }
            emitter.instruction("roundsd xmm0, xmm0, 1");                       // round toward minus infinity on x86_64 using SSE4.1 roundsd
        }
    }
    Some(PhpType::Float)
}
