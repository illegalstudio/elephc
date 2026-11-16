//! Purpose:
//! Function-like call result-type normalization.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Returns the best available return type for a function-like call.
pub(in crate::ir_lower) fn call_return_type(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    _operands: &[crate::ir::ValueId],
) -> PhpType {
    let php_type = if let Some(sig) = ctx.functions.get(name) {
        eir_user_function_return_type(sig)
    } else if let Some(sig) = ctx.extern_functions.get(name) {
        sig.return_type.clone()
    } else if let Some(sig) = builtin_call_signature(name) {
        sig.return_type
    } else {
        PhpType::Mixed
    };
    normalize_value_php_type(php_type)
}

/// Returns the caller-visible EIR return type for a user function signature.
///
/// Delegates to `crate::types::dynamic_params::caller_visible_return_type`. The checker and the
/// stamp of a closure that returns a user call share its view before the boxed storage of an
/// all-null return (`caller_visible_checker_return_type`), so they agree on the element type.
pub(in crate::ir_lower) fn eir_user_function_return_type(signature: &FunctionSig) -> PhpType {
    crate::types::dynamic_params::caller_visible_return_type(signature)
}
