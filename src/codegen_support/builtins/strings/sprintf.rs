//! Purpose:
//! Emits PHP `sprintf` string formatting calls.
//! Marshals string/scalar arguments into the shared sprintf runtime helper that allocates the
//! returned PHP string.
//!
//! Called from:
//! - `crate::codegen_support::builtins::strings::emit()`.
//!
//! Key details:
//! - Returned string pointer/length pairs must be treated as owned runtime values when the helper allocates.
//! - Argument marshalling (including coercing each argument to its conversion specifier's type for
//!   literal formats) lives in `super::format_args`.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `sprintf` builtin call.
///
/// Delegates argument marshalling and the `__rt_sprintf` call to
/// [`super::format_args::emit_format_and_call`], which pushes each value argument as a 16-byte
/// tagged record (coercing to the conversion specifier's type for literal formats), evaluates the
/// format string, and invokes the runtime helper.
///
/// # Arguments
/// * `_name` - Unused; matches the builtin dispatch signature.
/// * `args[0]` - Format string expression.
/// * `args[1..]` - Values to substitute into the format string.
///
/// # Returns
///   `Some(PhpType::Str)` — caller owns the returned string pointer/length.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("sprintf()");
    super::format_args::emit_format_and_call(args, emitter, ctx, data);
    Some(PhpType::Str)
}
