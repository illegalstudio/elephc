//! Purpose:
//! Home of the PHP `random_bytes` builtin: its declaration and compile-time length guard.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - A `check` hook is required only to reject a statically-known length below 1
//!   at compile time. PHP throws a `ValueError` for such a length; elephc has no
//!   catchable path out of the runtime helper, so a constant literal below 1
//!   (folded `0` or a negative) is rejected here. Runtime-unknown lengths are
//!   guarded in the `__rt_random_bytes` runtime helper instead.
//! - Arity (exactly 1 argument) is enforced by the registry from `params`, so the
//!   check hook does not re-check it; the return type is always `Str`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "random_bytes",
    area: Math,
    params: [length: Int],
    returns: Str,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::RandomBytes,
    ),
    summary: "Get a cryptographically secure random string of the given length.",
    php_manual: "https://www.php.net/manual/en/function.random-bytes.php",
}

/// Rejects a statically-known `length` below 1 at compile time and returns `Str`.
///
/// A constant integer literal argument that folds to `0` or a negative value is a
/// guaranteed PHP `ValueError`; since the runtime helper cannot surface a catchable
/// exception, that case is rejected here. Runtime-unknown lengths pass through and
/// are guarded by the `__rt_random_bytes` runtime helper. Arity and per-argument
/// inference are handled by the registry common path before this hook runs.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if let ExprKind::IntLiteral(length) = cx.args[0].kind {
        if length < 1 {
            return Err(CompileError::new(
                cx.span,
                "random_bytes(): Argument #1 ($length) must be greater than 0",
            ));
        }
    }
    Ok(PhpType::Str)
}
