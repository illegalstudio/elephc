//! Purpose:
//! Emits PHP `usleep` time/date builtin calls.
//! Marshals timestamp and format arguments into runtime helpers that consult wall-clock state.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - Time calls are effectful/non-deterministic and must preserve PHP scalar return conventions.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_to_int, emit_expr};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a call to the libc `usleep` function, suspending execution for the given number of microseconds.
///
/// # Arguments
/// - `_name`: unused, always `"usleep"`
/// - `args[0]`: evaluated and placed in `x0` (ARM64 integer return register) as the sleep duration in microseconds
/// - `emitter`: controls output assembly
/// - `ctx`: carries codegen state
/// - `data`: data section for literals/constants
///
/// # Returns
/// `Some(PhpType::Void)` — PHP `usleep` has no return value
///
/// # Side effects
/// Invokes the `usleep` libc routine, which blocks the calling thread for the specified duration.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("usleep()");
    // -- evaluate microseconds argument --
    let micros_ty = emit_expr(&args[0], emitter, ctx, data);
    coerce_to_int(emitter, &micros_ty);                                         // unbox a Mixed/Union microseconds argument into a raw integer
    // -- call libc usleep (x0 = microseconds) --
    emitter.bl_c("usleep");                                          // sleep for x0 microseconds
    Some(PhpType::Void)
}
