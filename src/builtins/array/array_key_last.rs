//! Purpose:
//! Home of the PHP `array_key_last` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is an array (or Mixed) and returns `Mixed`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "array_key_last",
    area: Array,
    params: [array: Mixed],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayKeyLast,
    ),
    summary: "Gets the last key of an array.",
    php_manual: "https://www.php.net/manual/en/function.array-key-last.php",
}

/// Validates that the argument is an array or Mixed and returns `Mixed`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 1 argument).
/// Mixed is permitted because heterogeneous arrays are represented as Mixed at compile time.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed) {
        return Err(CompileError::new(
            cx.span,
            "array_key_last() argument must be array",
        ));
    }
    Ok(PhpType::Mixed)
}
