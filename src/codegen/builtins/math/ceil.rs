//! Purpose:
//! Emits PHP `ceil` numeric builtin calls backed by floating-point/libm-style helpers.
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

/// Emits the `ceil(number)` builtin call, rounding its operand toward positive infinity.
///
/// # Arguments
/// - `_name`: Unused; the builtin name is hardcoded as `ceil`.
/// - `args`: Single expression giving the number to round.
/// - `emitter`: Target-specific instruction emitter.
/// - `ctx`: Codegen context carrying variable layout and arch info.
/// - `data`: Data section for relocations and constant storage.
///
/// # Returns
/// `Some(PhpType::Float)` — `ceil` always returns a float in PHP.
///
/// # Codegen behavior
/// - Converts integer operands to float before rounding (SCVTF on ARM64, CVTSI2SD on x86_64).
/// - Uses `frintp` (ARM64) or `roundsd` with mode 2 (x86_64) to round toward +infinity.
/// - NaN and infinity inputs follow IEEE-754 rounding semantics.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("ceil()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            if ty != PhpType::Float {
                emitter.instruction("scvtf d0, x0");                            // convert the ceil() input to float when it is an integer
            }
            emitter.instruction("frintp d0, d0");                               // round toward plus infinity on AArch64
        }
        Arch::X86_64 => {
            if ty != PhpType::Float {
                emitter.instruction("cvtsi2sd xmm0, rax");                      // convert the ceil() input to float when it is an integer
            }
            emitter.instruction("roundsd xmm0, xmm0, 2");                       // round toward plus infinity on x86_64 using SSE4.1 roundsd
        }
    }
    Some(PhpType::Float)
}
