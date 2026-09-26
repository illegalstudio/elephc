//! Purpose:
//! Home of the PHP 8.4 `array_last` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Returns the last VALUE in insertion order, boxed as `Mixed`, or `null` for an empty array.
//! - The runtime helper reads the edge slot directly (the hash tail, or the last indexed slot),
//!   so the array operand is evaluated exactly once and no key lookup or chain walk happens.
//! - PHP 8.4 introduced the function; `crate::php_profile::floor` rejects a call to it under an
//!   older `--php-version`, exactly as it does for `array_find`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_last",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayLast,
    ),
}

/// Validates that the argument is an array or Mixed and returns `Mixed`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 1 argument).
/// The result is `Mixed` because it is either an element of any type or `null`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !ty.is_php_array()
        && !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed)
    {
        return Err(CompileError::new(
            cx.span,
            "array_last() argument must be array",
        ));
    }
    Ok(PhpType::Mixed)
}
