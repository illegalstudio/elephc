//! Purpose:
//! Emits PHP `urldecode` string transformation or formatting calls.
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
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a PHP `urldecode()` call, decoding a percent-encoded query string argument.
///
/// Unused `name` parameter supports PHP case-insensitive builtin dispatch.
/// Arguments: args[0] must be a PHP string to decode.
/// Emits: expression evaluation for args[0], then a target-aware call to `__rt_urldecode`.
/// Returns: `Some(PhpType::Str)` — the result is an owned runtime string allocation.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("urldecode()");
    // Coerce the operand to a string in the string ABI registers via emit_string_arg, so a
    // Mixed argument is cast through __rt_mixed_cast_string instead of leaving a boxed cell in
    // the result register with stale string registers.
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_urldecode");                            // call the target-aware runtime helper that decodes query-style percent-encoded strings
    Some(PhpType::Str)
}
