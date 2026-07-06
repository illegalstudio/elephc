//! Purpose:
//! Emits PHP `asin` numeric builtin calls backed by floating-point/libm-style helpers.
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

/// Emits the `asin` builtin call with a single scalar argument, delegating to the
/// platform's libc `asin` function. Integer operands are normalized into the floating-point
/// result register before the call; floats are passed directly. Returns `PhpType::Float`.
///
/// # Arguments
/// * `_name` - unused; kept for interface parity with other builtin emitters
/// * `args` - must contain exactly one expression (the angle in radians)
/// * `emitter` - drives instruction emission and exposes `target`
/// * `ctx` - carries variable layout and ownership state
/// * `data` - target data section for constants/literals
///
/// # Aborts
/// Panics if `args` is empty or if `emitter.target` is an unsupported architecture.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("asin()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    if ty != PhpType::Float {
        abi::emit_int_result_to_float_result(emitter);                          // normalize integer asin() inputs into the active floating-point result register before the libc call
    }
    match emitter.target.arch {
        Arch::AArch64 => emitter.bl_c("asin"),                                  // call libc asin() with the scalar argument in the native AArch64 floating-point argument register
        Arch::X86_64 => emitter.instruction("call asin"),                       // call libc asin() with the scalar argument in the native SysV floating-point argument register
    }
    Some(PhpType::Float)
}
