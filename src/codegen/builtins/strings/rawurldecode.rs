//! Purpose:
//! Emits PHP `rawurldecode` string transformation or formatting calls.
//! Marshals string/scalar arguments into runtime helpers that allocate returned PHP strings.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - Returned string pointer/length pairs must be treated as owned runtime values when the helper allocates.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `rawurldecode` builtin call.
///
/// Arguments:
/// - `args[0]`: the string to decode (evaluated and pushed as the runtime argument)
///
/// Behavior:
/// - Emits `args[0]` expression to obtain the source string.
/// - Calls `__rt_urldecode` runtime helper which percent-decodes the string.
/// - The helper allocates and returns a new PHP-owned string; the caller receives
///   ownership of the returned value.
///
/// Returns:
/// - `PhpType::Str` indicating the result is a PHP string type.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("rawurldecode()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_urldecode");                            // rawurldecode() currently reuses the shared target-aware percent-decoder runtime helper
    Some(PhpType::Str)
}
