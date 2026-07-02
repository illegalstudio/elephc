//! Purpose:
//! Emits PHP `log` numeric builtin calls backed by floating-point/libm-style helpers.
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

/// Emits PHP `log($num)` or `log($num, $base)` as a call to libc `log()`.
///
/// For a single argument, computes the natural logarithm directly. For two
/// arguments, computes `log($num) / log($base)` (change of base formula).
///
/// Integer and boxed `Mixed`/`Union` operands are normalized to floating-point before the
/// libc call via `coerce_to_float`. The result is always `PhpType::Float`.
/// Both the numerator and denominator are preserved across the second `log()`
/// call using a stack push/pop on both architectures.
///
/// ABI: SysV on x86_64 (scalar float args in `xmm0`-`xmm7`), AAPCS on AArch64
/// (scalar float args in `d0`-`d7`). Return value in `xmm0` / `d0`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("log()");
    if args.len() == 1 {
        // -- log($num) — natural logarithm --
        let ty = emit_expr(&args[0], emitter, ctx, data);
        coerce_to_float(emitter, &ty);                                          // normalize int/Mixed log() input to a float in d0/xmm0
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.bl_c("log");                                            // call libc log() with the scalar argument in the native AArch64 floating-point argument register
            }
            Arch::X86_64 => {
                emitter.instruction("call log");                                // call libc log() with the scalar argument in the native SysV floating-point argument register
            }
        }
    } else {
        // -- log($num, $base) — change of base: log($num) / log($base) --
        let ty = emit_expr(&args[0], emitter, ctx, data);
        coerce_to_float(emitter, &ty);                                          // normalize int/Mixed log() value to a float in d0/xmm0
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.bl_c("log");                                            // compute log($num) through libc with the AArch64 floating-point calling convention
            }
            Arch::X86_64 => {
                emitter.instruction("call log");                                // compute log($num) through libc with the SysV x86_64 floating-point calling convention
            }
        }
        abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));      // preserve log($num) while the logarithm base expression is evaluated and converted
        let ty2 = emit_expr(&args[1], emitter, ctx, data);
        coerce_to_float(emitter, &ty2);                                         // normalize int/Mixed log() base to a float in d0/xmm0
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.bl_c("log");                                            // compute log($base) through libc with the AArch64 floating-point calling convention
                emitter.instruction("fmov d1, d0");                             // preserve log($base) in the secondary AArch64 floating-point scratch register
                abi::emit_pop_float_reg(emitter, "d0");                         // restore log($num) into the primary AArch64 floating-point result register
                emitter.instruction("fdiv d0, d0, d1");                         // compute the change-of-base quotient in the standard AArch64 floating-point result register
            }
            Arch::X86_64 => {
                emitter.instruction("call log");                                // compute log($base) through libc with the SysV x86_64 floating-point calling convention
                abi::emit_pop_float_reg(emitter, "xmm1");                       // restore log($num) into a scratch floating-point register before forming the change-of-base quotient
                emitter.instruction("divsd xmm1, xmm0");                        // divide log($num) by log($base) in the scratch floating-point register
                emitter.instruction("movsd xmm0, xmm1");                        // move the change-of-base quotient back into the standard x86_64 floating-point result register
            }
        }
    }
    Some(PhpType::Float)
}
