//! Purpose:
//! Home of the PHP `implode` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `implode` is the one builtin whose supported compiler contract (exactly 2 arguments)
//!   is stricter than its golden signature's minimum. The golden marks `array` optional
//!   (required count 1), which the parity gate compares against, so `array` must keep a
//!   default here. `max_args` caps only the maximum, so it cannot raise the minimum;
//!   the exact-2 requirement is therefore re-enforced inside the `check` hook to keep the
//!   established `"implode() takes exactly 2 arguments"` diagnostic for the tested 1-arg call.
//! - `check` returns `PhpType::Str`.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "implode",
    area: String,
    params: [separator: Str, array: Mixed = DefaultSpec::Null],
    max_args: 2,
    returns: Str,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Implode,
    ),
    summary: "Joins array elements into a single string using a separator.",
    php_manual: "https://www.php.net/manual/en/function.implode.php",
}

/// Returns `PhpType::Str` for an `implode` call, enforcing the supported exactly-2 arity.
///
/// The golden signature marks `array` optional (so the parity gate sees one required
/// param), but the compiler contract requires exactly two arguments. `check_arity`'s
/// `max_args` override caps the maximum only and cannot raise the minimum, so the
/// exact-2 requirement is re-enforced here to preserve the established diagnostic. Argument
/// types are inferred by the common registry dispatch path before this hook fires.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if cx.args.len() != 2 {
        return Err(CompileError::new(
            cx.span,
            "implode() takes exactly 2 arguments",
        ));
    }
    Ok(PhpType::Str)
}
