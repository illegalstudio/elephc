//! Purpose:
//! Emits PHP `pi` numeric builtin calls.
//! Handles scalar argument lowering and returns the PHP numeric type promised by signature checking.
//!
//! Called from:
//! - `crate::codegen::builtins::math::emit()`.
//!
//! Key details:
//! - Integer-vs-float result selection must stay aligned with PHP semantics and local type inference.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `pi()` builtin as a compile-time float constant loaded into the ABI return register.
///
/// `_name` is unused—signature checking has already validated the call.
/// `_args` is empty and not accessed—signature checking enforces arity.
/// Returns `Some(PhpType::Float)` since `pi()` always yields a float.
/// Loads the `std::f64::consts::PI` constant into `d0` (ARM64) or `xmm0` (x86_64) via
/// `DataSection` to avoid hardcoding relocatable assembly constants in the emitter.
pub fn emit(
    _name: &str,
    _args: &[Expr],
    emitter: &mut Emitter,
    _ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("pi()");
    let label = data.add_float(std::f64::consts::PI);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.adrp("x9", &format!("{}", label));                           // load the page address that contains the M_PI floating constant
            emitter.ldr_lo12("d0", "x9", &format!("{}", label));                // load the M_PI floating constant into the standard AArch64 floating-point result register
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("movsd xmm0, QWORD PTR [rip + {}]", label)); // load the M_PI floating constant into the standard x86_64 floating-point result register
        }
    }
    Some(PhpType::Float)
}
