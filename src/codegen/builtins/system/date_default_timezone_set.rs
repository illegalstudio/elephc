//! Purpose:
//! Emits PHP `date_default_timezone_set()` calls.
//! Materializes the timezone-identifier string and hands it to the runtime helper that applies it
//! via libc `putenv`/`tzset`.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - The runtime sets process-global timezone state, so this call has observable side effects and
//!   must not be folded away. Returns PHP `true`.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_result_to_type, emit_expr};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for PHP's `date_default_timezone_set($timezoneId)` builtin.
///
/// Evaluates the argument, coerces it to a string in the string-result registers
/// (`x1`/`x2` on ARM64, `rax`/`rdx` on x86_64), then calls `__rt_date_default_timezone_set`
/// which writes `"TZ=<id>"` to the static env buffer, applies it through libc `putenv`+`tzset`,
/// and returns the PHP boolean `true`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("date_default_timezone_set()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    // Coerce the identifier to a string so the runtime helper receives it in the string ABI
    // registers (a Str is a no-op; a Mixed/Int is unboxed/stringified).
    coerce_result_to_type(emitter, ctx, data, &ty, &PhpType::Str);
    abi::emit_call_label(emitter, "__rt_date_default_timezone_set");            // apply TZ via libc; returns PHP true in the integer-result register
    Some(PhpType::Bool)
}
