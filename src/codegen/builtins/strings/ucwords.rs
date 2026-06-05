//! Purpose:
//! Emits PHP `ucwords` string transformation or formatting calls.
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

/// Emits the `ucwords` builtin, which uppercases the first character of each
/// whitespace-delimited word in a string.
///
/// # Arguments
/// - `_name`: Unused; PHP builtins are dispatched by name.
/// - `args`: Single argument — the string to transform.
///
/// # Outputs
/// - Pushes a string pointer/length pair onto the call stack.
/// - Returns `PhpType::Str` indicating the result is a PHP string.
///
/// # Runtime behavior
/// - Calls `__rt_ucwords` via `abi::emit_call_label`.
/// - The runtime helper allocates a new owned PHP string; the caller must
///   treat the returned pointer/length as an owned value.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("ucwords()");
    // Coerce the operand to a string in the string ABI registers via emit_string_arg, so a
    // Mixed argument is cast through __rt_mixed_cast_string instead of leaving a boxed cell in
    // the result register with stale string registers.
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_ucwords");                              // call the target-aware runtime helper that uppercases the first letter of each whitespace-delimited word
    Some(PhpType::Str)
}
