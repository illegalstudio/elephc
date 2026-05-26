//! Purpose:
//! Emits PHP `htmlentities` string transformation or formatting calls.
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

/// Emits the `htmlentities` PHP builtin call.
///
/// Loads the string argument (first element of `args`) and calls the shared
/// `__rt_htmlspecialchars` runtime helper, which performs the HTML entity
/// encoding. The runtime allocates and returns a new PHP string.
///
/// # Arguments
/// * `args` - Must contain at least one expression producing a string value.
/// * `emitter` - Target-aware instruction emitter.
/// * `ctx` - Codegen context carrying variable layout and ownership state.
/// * `data` - Data section for relocations and static data.
///
/// # Returns
/// `Some(PhpType::Str)` indicating the result is a PHP string. `None` is
/// returned only if the callee reports a type error (not applicable here).
///
/// # Notes
/// `htmlentities()` currently delegates to the `htmlspecialchars` runtime
/// helper. Both share the same encoding logic and runtime routine.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("htmlentities()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_htmlspecialchars");                    // call the shared target-aware runtime helper because htmlentities() currently aliases htmlspecialchars()
    Some(PhpType::Str)
}
