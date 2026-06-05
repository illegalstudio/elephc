//! Purpose:
//! Emits PHP `sha1` string transformation or formatting calls.
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

/// Emits code for the PHP `sha1()` builtin.
///
/// Arguments:
/// - `args[0]` is evaluated and pushed onto the stack as the input string.
/// - `emitter` accumulates the emitted assembly instructions.
/// - `ctx` provides variable layout and codegen state.
/// - `data` provides the data section for string literals and metadata.
///
/// Returns `PhpType::Str` to indicate the result is a PHP string.
///
/// Effect:
/// - Calls the target-aware runtime helper `__rt_sha1` which computes the SHA-1 digest
///   of the input string and returns it as a lowercase hexadecimal string.
/// - The returned string pointer/length pair must be treated as an owned runtime value
///   when the helper allocates; the caller is responsible for freeing or transferring ownership.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("sha1()");
    // Coerce the operand to a string in the string ABI registers via emit_string_arg, so a
    // Mixed argument is cast through __rt_mixed_cast_string instead of leaving a boxed cell in
    // the result register with stale string registers.
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_sha1");                                 // call the target-aware runtime helper that computes the SHA1 digest and returns it as lowercase hex
    Some(PhpType::Str)
}
