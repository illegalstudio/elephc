//! Purpose:
//! Home of the PHP `array_is_list` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The return type is always `Bool`, but a check hook is still required (rather than
//!   a pure-data `returns: Bool`) because the legacy arm rejects non-array arguments at
//!   type-check time, and that guard is covered by an error test
//!   (`array_is_list() argument must be array`). The hook reproduces the guard and
//!   then returns `Bool`.
//! - Arity (exactly 1 argument) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "array_is_list",
    area: Array,
    params: [array: Mixed],
    returns: Bool,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayIsList,
    ),
    summary: "Checks whether an array is a list (sequential 0-based integer keys).",
    php_manual: "https://www.php.net/manual/en/function.array-is-list.php",
}

/// Returns `PhpType::Bool` for an `array_is_list` call, rejecting non-array arguments.
///
/// The return type is always `Bool`, but the argument must be an array-like value
/// (`Array`, `AssocArray`, or boxed `Mixed`); other types are a type error. The
/// argument is re-inferred here to enforce that guard; the registry already inferred
/// it once for side effects, and arity (exactly 1) is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(
        ty,
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed
    ) {
        return Err(CompileError::new(
            cx.span,
            "array_is_list() argument must be array",
        ));
    }
    Ok(PhpType::Bool)
}
