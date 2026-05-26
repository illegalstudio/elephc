//! Purpose:
//! Emits PHP `atan2` numeric builtin calls backed by floating-point/libm-style helpers.
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

/// Emits the PHP `atan2(y, x)` builtin call.
///
/// Evaluates `y` (first arg) and preserves it while `x` (second arg) is evaluated,
/// then calls the target libc `atan2` function. Both operands are normalized to
/// floating-point before the call. The return type is always `PhpType::Float`.
/// Target ABI: AArch64 passes `y` in `d0` and `x` in `d1`; x86_64 SysV passes
/// `y` in `xmm0` and `x` in `xmm1`.
///
/// # Arguments
/// * `_name` – unused (builtin dispatch is by signature)
/// * `args` – exactly two expressions: `y` then `x`
/// * `emitter` – target assembly emitter
/// * `ctx` – compilation context (variables, types)
/// * `data` – data section for constants/literals
///
/// # Returns
/// `Some(PhpType::Float)` – always a float result per PHP spec
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("atan2()");
    // -- evaluate y (first arg) --
    let t0 = emit_expr(&args[0], emitter, ctx, data);
    if t0 != PhpType::Float {
        abi::emit_int_result_to_float_result(emitter);                          // normalize the atan2() y operand into the active floating-point result register before it is preserved
    }
    abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));          // preserve the floating atan2() y operand while the x operand expression is evaluated
    // -- evaluate x (second arg) --
    let t1 = emit_expr(&args[1], emitter, ctx, data);
    if t1 != PhpType::Float {
        abi::emit_int_result_to_float_result(emitter);                          // normalize the atan2() x operand into the active floating-point result register before the libc call
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fmov d1, d0");                                 // move the floating atan2() x operand into the second AArch64 floating-point argument register
            abi::emit_pop_float_reg(emitter, "d0");                             // restore the floating atan2() y operand into the first AArch64 floating-point argument register
            emitter.bl_c("atan2");                                              // delegate atan2(y, x) to libc on AArch64
        }
        Arch::X86_64 => {
            abi::emit_pop_float_reg(emitter, "xmm1");                           // restore the floating atan2() y operand into a scratch floating-point register before ordering the SysV libc arguments
            emitter.instruction("movapd xmm2, xmm0");                           // preserve the floating atan2() x operand while the y operand is moved into the first SysV floating-point argument register
            emitter.instruction("movapd xmm0, xmm1");                           // move the floating atan2() y operand into the first SysV floating-point argument register
            emitter.instruction("movapd xmm1, xmm2");                           // move the floating atan2() x operand into the second SysV floating-point argument register
            emitter.instruction("call atan2");                                  // delegate atan2(y, x) to libc on linux-x86_64
        }
    }
    Some(PhpType::Float)
}
