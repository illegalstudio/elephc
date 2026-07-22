//! Purpose:
//! Home of the PHP `mb_ereg_match` builtin: declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - `returns: Bool` expresses the return type inline; `check` enforces the string/null argument
//!   surface because the registry's common path does not enforce parameter types automatically.
//! - `mb_ereg_match($pattern, $string, $options = null)` is anchored at the START of the subject (verified vs
//!   PHP 8.5). The lowering dispatches to the shared `__rt_mb_ereg_match` regex helper, which
//!   reuses the PCRE2 engine and enforces the start-anchor via `rm_so == 0`. UTF-8/ASCII
//!   patterns are supported; the optional `$options` string currently honors `i` for case-insensitive
//!   matching and leaves other recognized mbregex options without additional runtime effect.

use crate::{
    builtins::spec::{BuiltinCheckCtx, DefaultSpec},
    errors::CompileError,
    types::PhpType,
};

builtin! {
    name: "mb_ereg_match",
    area: String,
    params: [pattern: Str, subject: Str, options: Str = DefaultSpec::Null],
    returns: Bool,
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::MbEregMatch,
    ),
    summary: "Tests whether a regex pattern matches the beginning of a string (multibyte).",
    php_manual: "https://www.php.net/manual/en/function.mb-ereg-match.php",
}

/// Validates the `mb_ereg_match()` argument types and returns `Bool`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let pattern_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if pattern_ty != PhpType::Str {
        return Err(CompileError::new(
            cx.args[0].span,
            "mb_ereg_match() pattern argument must be string",
        ));
    }

    let subject_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if subject_ty != PhpType::Str {
        return Err(CompileError::new(
            cx.args[1].span,
            "mb_ereg_match() subject argument must be string",
        ));
    }

    if let Some(options) = cx.args.get(2) {
        let options_ty = cx.checker.infer_type(options, cx.env)?;
        if !matches!(options_ty, PhpType::Str | PhpType::Void) {
            return Err(CompileError::new(
                options.span,
                "mb_ereg_match() options argument must be string or null",
            ));
        }
    }

    Ok(PhpType::Bool)
}
