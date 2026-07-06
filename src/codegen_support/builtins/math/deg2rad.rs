//! Purpose:
//! Emits PHP `deg2rad` numeric builtin calls backed by floating-point/libm-style helpers.
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

/// Emits the `deg2rad` builtin call.
///
/// Converts a degree value to radians by multiplying with `M_PI / 180.0`.
///
/// # Arguments
/// - `_name`: Unused; the builtin name is fixed (`deg2rad`).
/// - `args`: Single expression providing the degree value.
/// - `emitter`: Target-specific instruction emission.
/// - `ctx`: Codegen context (target architecture, current function frame).
/// - `data`: Data section for embedding floating-point constants.
///
/// # Behavior
/// - Integer operands are normalized into the floating-point result register before conversion.
/// - Returns `Some(PhpType::Float)` since the result is always floating-point.
/// - Uses architecture-specific multiplication (`fmul` on AArch64, `mulsd` on x86_64).
/// - The conversion constant (`M_PI / 180.0`) is embedded in the data section.
///
/// # ABI constraints
/// - AArch64: degree in `d0`, constant in `d1`, result in `d0`.
/// - x86_64: degree in `xmm0`, constant in `xmm1`, result in `xmm0`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("deg2rad()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    if ty != PhpType::Float {
        abi::emit_int_result_to_float_result(emitter);                          // normalize the degree input into the active floating-point result register before applying the conversion factor
    }
    // -- multiply by M_PI / 180.0 to convert degrees to radians --
    let label = data.add_float(std::f64::consts::PI / 180.0);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg_via_page(emitter, "d1", "x9", &label); // load the degree-to-radian conversion constant into the secondary AArch64 floating-point register
            emitter.instruction("fmul d0, d0, d1");                             // multiply the degree input by the conversion constant in the standard AArch64 floating-point result register
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "xmm1", &label, 0);           // load the degree-to-radian conversion constant into the secondary x86_64 floating-point register
            emitter.instruction("mulsd xmm0, xmm1");                            // multiply the degree input by the conversion constant in the standard x86_64 floating-point result register
        }
    }
    Some(PhpType::Float)
}
