//! Purpose:
//! Emits PHP `strtoupper` string transformation or formatting calls.
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

/// Emits code for the PHP `strtoupper` builtin.
///
/// # Arguments
/// - `_name`: Unused parameter present for dispatcher uniformity.
/// - `args`: Must contain exactly one expression producing a string value.
/// - `emitter`: Target-aware assembly emitter.
/// - `ctx`: Codegen context carrying variable layout and ownership state.
/// - `data`: Data section for relocations and static data.
///
/// # Behavior
/// 1. Emits code to evaluate and materialize the first argument onto the stack.
/// 2. Calls `__rt_strtoupper`, a target-aware runtime helper that converts the
///    string in-place to uppercase using PHP's locale-aware rules.
/// 3. Returns `PhpType::Str` indicating the result is an owned PHP string.
///
/// # ABI Constraints
/// The runtime helper expects the input string in the standard string ABI registers
/// (`x1`=ptr, `x2`=len on ARM64; `rdi`=ptr, `rsi`=len on x86_64) and returns the
/// transformed string pointer/length pair via the same registers.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("strtoupper()");
    // Coerce the operand to a string in x1/x2 (rdi/rsi). Using emit_string_arg
    // (rather than a bare emit_expr) means a Mixed argument — e.g. a `mixed`
    // property/return value or an assoc-array element — is cast to a real string
    // via __rt_mixed_cast_string instead of leaving a boxed cell in x0 with stale
    // string registers (which produced an empty result).
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    // -- convert all characters to uppercase --
    abi::emit_call_label(emitter, "__rt_strtoupper");                           // call the target-aware runtime helper that uppercases the current string into concat storage

    Some(PhpType::Str)
}
