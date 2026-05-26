//! Purpose:
//! Emits PHP `sqrt` numeric builtin calls backed by floating-point/libm-style helpers.
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

/// Emits a `sqrt($arg)` builtin call as target-native square-root instructions.
///
/// Consumes the first argument expression, promoting integer operands to float before
/// the square-root operation. Emits `fsqrt d0, d0` on AArch64 or `sqrtsd xmm0, xmm0` on
/// x86_64. The floating-point result is left in the ABI return register (`d0`/`xmm0`).
///
/// Returns `Some(PhpType::Float)` since `sqrt` always produces a float in PHP.
///
/// # Arguments
/// * `_name` — unused; present for dispatcher uniformity
/// * `args` — must contain exactly one argument (checked by the type checker)
/// * `emitter` — target assembly emitter
/// * `ctx` — codegen context (variable layout, ownership state)
/// * `data` — data section for any emitted constants
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("sqrt()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    // -- convert int to float if needed, then compute square root --
    if ty != PhpType::Float {
        abi::emit_int_result_to_float_result(emitter);                          // normalize integer sqrt() inputs into the active floating-point result register before the square-root operation
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fsqrt d0, d0");                                // compute the scalar square root in the native AArch64 floating-point result register
        }
        Arch::X86_64 => {
            emitter.instruction("sqrtsd xmm0, xmm0");                           // compute the scalar square root in the native x86_64 floating-point result register
        }
    }
    Some(PhpType::Float)
}
