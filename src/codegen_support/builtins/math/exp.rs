//! Purpose:
//! Emits PHP `exp` numeric builtin calls backed by floating-point/libm-style helpers.
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

/// Emits a PHP `exp(x)` call to target assembly via the libc `exp` function.
///
/// Arguments:
/// - `_name`: unused (保留 for API compatibility with the builtin dispatcher)
/// - `args`: single expression producing the exponent; must be int or float per signature
/// - `emitter`: target assembly emitter
/// - `ctx`: codegen context (variable layout, class metadata)
/// - `data`: data section for embedded constants
///
/// Returns `Some(PhpType::Float)` — `exp` always returns float in PHP.
///
/// Side effects:
/// - Emits the argument expression; if its type is `PhpType::Int`, emits an int-to-float
///   normalization step before the call so the libc function receives a float register value.
/// - Calls the platform's libc `exp()` using the native floating-point argument registers
///   (`x0`-`x7` on AArch64, ` xmm0`-`xmm7` on x86_64 SysV).
///
/// ABI constraints:
/// - AArch64: scalar float arg in `d0`, result returned in `d0`.
/// - x86_64: scalar float arg in `xmm0`, result returned in `xmm0`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("exp()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    if ty != PhpType::Float {
        abi::emit_int_result_to_float_result(emitter);                          // normalize integer exp() inputs into the active floating-point result register before the libc call
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.bl_c("exp");                                                // call libc exp() with the scalar argument in the native AArch64 floating-point argument register
        }
        Arch::X86_64 => {
            emitter.instruction("call exp");                                    // call libc exp() with the scalar argument in the native SysV floating-point argument register
        }
    }
    Some(PhpType::Float)
}
