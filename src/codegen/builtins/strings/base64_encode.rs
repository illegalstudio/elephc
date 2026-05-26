//! Purpose:
//! Emits PHP `base64_encode` string transformation or formatting calls.
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

/// Emits a `base64_encode($string)` call, which base64-encodes a string or scalar value.
///
/// ## Arguments
/// - `args[0]`: the expression producing the string or scalar to encode
///
/// ## Behavior
/// - Evaluates `args[0]` and leaves its result in the appropriate registers per ABI (x1/x2 on ARM64).
/// - Calls the target-aware runtime helper `__rt_base64_encode`.
/// - Returns `PhpType::Str`; the runtime helper allocates and owns the returned PHP string.
///
/// ## Ownership
/// - The returned string is an owned runtime value; callers must treat it as allocated heap memory.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("base64_encode()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_base64_encode");                        // encode the current string result through the target-aware base64 runtime helper
    Some(PhpType::Str)
}
