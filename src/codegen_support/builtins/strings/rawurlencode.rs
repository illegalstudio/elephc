//! Purpose:
//! Emits PHP `rawurlencode` string transformation or formatting calls.
//! Marshals string/scalar arguments into runtime helpers that allocate returned PHP strings.
//!
//! Called from:
//! - `crate::codegen_support::builtins::strings::emit()`.
//!
//! Key details:
//! - Returned string pointer/length pairs must be treated as owned runtime values when the helper allocates.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code to call `__rt_rawurlencode`, which encodes a PHP string using RFC 3986
/// percent-encoding (`%XX` for unescaped characters, no `+` for spaces).
///
/// Arguments:
///   - `args[0]`: the expression producing the string to encode
///   - `emitter`: instruction emission context
///   - `ctx`: variable layout and ownership state
///   - `data`: runtime data section for relocations and string constants
///
/// Returns `Some(PhpType::Str)` — the helper allocates and returns the encoded string,
/// which must be treated as an owned runtime value.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("rawurlencode()");
    // Coerce the operand to a string in the string ABI registers via emit_string_arg, so a
    // Mixed argument is cast through __rt_mixed_cast_string instead of leaving a boxed cell in
    // the result register with stale string registers.
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_rawurlencode");                         // call the target-aware runtime helper that percent-encodes the current string with RFC 3986 rules
    Some(PhpType::Str)
}
