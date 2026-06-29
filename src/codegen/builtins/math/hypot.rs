//! Purpose:
//! Emits PHP `hypot` numeric builtin calls backed by floating-point/libm-style helpers.
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

/// Emits a `hypot($x, $y)` builtin call, delegating to the target libc.
///
/// # Arguments
/// - `args`: Two expressions for the x and y operands.
/// - `emitter`: Target instruction emitter.
/// - `ctx`: Codegen context carrying variable layout and metadata.
/// - `data`: Read-only data section for constants.
///
/// # Returns
/// Always returns `Some(PhpType::Float)`. The `Option` satisfies the
/// builtin-emitter interface even though `hypot` always produces a float.
///
/// # Implementation notes
/// - Each argument is evaluated and normalized to a floating-point value.
/// - The x operand is saved to the stack while y is evaluated to avoid
///   clobbering the first argument register.
/// - AArch64: passes x in `d0`, y in `d1`, calls `hypot` via `bl`.
/// - X86_64 (SysV): passes x in `xmm0`, y in `xmm1`, calls `hypot` via `call`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("hypot()");
    // -- evaluate x (first arg) --
    let t0 = emit_expr(&args[0], emitter, ctx, data);
    coerce_to_float(emitter, &t0);                                              // normalize the hypot() x operand (handles int and boxed Mixed/Union)
    abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));          // preserve the floating hypot() x operand while the y operand expression is evaluated
    // -- evaluate y (second arg) --
    let t1 = emit_expr(&args[1], emitter, ctx, data);
    coerce_to_float(emitter, &t1);                                              // normalize the hypot() y operand (handles int and boxed Mixed/Union)
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fmov d1, d0");                                 // move the floating hypot() y operand into the second AArch64 floating-point argument register
            abi::emit_pop_float_reg(emitter, "d0");                             // restore the floating hypot() x operand into the first AArch64 floating-point argument register
            emitter.bl_c("hypot");                                              // delegate hypot(x, y) to libc on AArch64
        }
        Arch::X86_64 => {
            abi::emit_pop_float_reg(emitter, "xmm1");                           // restore the floating hypot() x operand into a scratch floating-point register before ordering the SysV libc arguments
            emitter.instruction("movapd xmm2, xmm0");                           // preserve the floating hypot() y operand while the x operand is moved into the first SysV floating-point argument register
            emitter.instruction("movapd xmm0, xmm1");                           // move the floating hypot() x operand into the first SysV floating-point argument register
            emitter.instruction("movapd xmm1, xmm2");                           // move the floating hypot() y operand into the second SysV floating-point argument register
            emitter.instruction("call hypot");                                  // delegate hypot(x, y) to libc on linux-x86_64
        }
    }
    Some(PhpType::Float)
}
