//! Purpose:
//! Emits PHP `strrev` string transformation or formatting calls.
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

/// Emits code for the PHP `strrev` builtin.
///
/// Arguments:
/// - `args[0]`: the input string (emitted via `emit_string_arg`)
/// - The runtime helper `__rt_strrev` reverses the string and returns an owned result slice.
///
/// Outputs:
/// - Calls `__rt_strrev` via `abi::emit_call_label`
/// - Returns `PhpType::Str` (caller receives ownership of the returned string)
///
/// ABI constraints:
/// - Input string passed as pointer/length pair following standard string ABI
/// - Returned string pointer in `x1`, length in `x2` (ARM64 string return convention)
/// - Caller owns the returned string; no lifetime aliasing
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("strrev()");
    // Coerce the operand to a string in the string ABI registers via emit_string_arg, so a
    // Mixed argument is cast through __rt_mixed_cast_string instead of leaving a boxed cell in
    // the result register with stale string registers.
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_strrev");                               // reverse the input string through the target-aware runtime helper and return an owned result slice

    Some(PhpType::Str)
}
