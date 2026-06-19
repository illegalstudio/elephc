//! Purpose:
//! Emits PHP `date_default_timezone_get()` calls.
//! Delegates to the runtime helper that returns the stored timezone identifier (or `"UTC"`).
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - Reads process-global timezone state; the returned pointer/length is an owned PHP string in the
//!   string-result registers (`x1`/`x2` on ARM64, `rax`/`rdx` on x86_64).

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for PHP's `date_default_timezone_get()` builtin.
///
/// Calls `__rt_date_default_timezone_get`, which returns the identifier set by
/// `date_default_timezone_set` (or the literal `"UTC"` when none was set) in the string-result
/// registers.
pub fn emit(
    _name: &str,
    _args: &[Expr],
    emitter: &mut Emitter,
    _ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("date_default_timezone_get()");
    abi::emit_call_label(emitter, "__rt_date_default_timezone_get");            // return the stored timezone identifier (or "UTC") in the string-result registers
    Some(PhpType::Str)
}
