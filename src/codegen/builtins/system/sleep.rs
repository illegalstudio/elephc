//! Purpose:
//! Emits PHP `sleep` time/date builtin calls.
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

/// Emits code for the PHP `sleep($seconds)` builtin.
///
/// Evaluates the seconds argument into `x0` then calls the libc `sleep` helper.
/// Returns an integer indicating whether sleep completed normally (0) or was
/// interrupted (non-zero), following PHP's `sleep` semantics.
///
/// Inputs:
/// - `args[0]`: seconds to sleep, must evaluate to an integer
/// - `emitter`: assembly emitter for writing instructions
/// - `ctx`: current.codegen context (scope, locals, etc.)
/// - `data`: data section for relocations and constants
///
/// ABI: `x0` holds the argument (seconds); return value in `x0`
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("sleep()");
    // -- evaluate seconds argument --
    let seconds_ty = emit_expr(&args[0], emitter, ctx, data);
    coerce_to_int(emitter, &seconds_ty);                                        // unbox a Mixed/Union seconds argument into a raw integer
    // -- call libc sleep (x0 = seconds) --
    emitter.bl_c("sleep");                                           // sleep for x0 seconds, returns 0 on success
    Some(PhpType::Int)
}
