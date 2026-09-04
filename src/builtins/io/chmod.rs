//! Purpose:
//! Home of the PHP `chmod` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Bool` and accepts any `permissions` argument php would COERCE to int,
//!   which is every scalar. An array or object is still refused, at the mode argument's span.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "chmod",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Chmod,
    ),
}

/// Returns `Bool`, refusing only a `permissions` argument php itself cannot coerce.
///
/// `$permissions` is an `int` parameter, and php's non-strict coercion fills it from any scalar.
/// MEASURED on `php -n` 8.5.6: `chmod($f, "0644")` answers `true` and leaves the file at `0204`,
/// because the string is read as DECIMAL 644 — which is a trap worth being able to HIT, since
/// `"0644"` is written by people who believe it is octal. `chmod($f, true)` gives `01`.
///
/// Requiring `Int` here refused the program outright, so a php script that runs could not be
/// built. The lowering coerces through the same `load_as_int` every other int-taking builtin
/// uses, so the two halves cannot disagree about what is accepted.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    let mode_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    // A LITERAL string php would throw for is refused here, which is that TypeError one stage
    // earlier. A string that only exists at run time is coerced, the way php coerces a numeric
    // one — elephc has no run-time TypeError for this parameter, and refusing every dynamic
    // string would refuse the numeric ones with it.
    if let ExprKind::StringLiteral(literal) = &cx.args[1].kind {
        if !php_numeric_string(literal) {
            return Err(CompileError::new(
                cx.args[1].span,
                "chmod() mode must be int",
            ));
        }
    }
    if !coercible_to_int(&mode_ty) {
        return Err(CompileError::new(
            cx.args[1].span,
            "chmod() mode must be int",
        ));
    }
    Ok(PhpType::Bool)
}

/// Returns true for a string php's `int` parameter coercion accepts.
///
/// php's own "numeric string": optional leading whitespace, a sign, digits with an optional
/// fraction and exponent, and nothing after it but trailing whitespace. A LEADING-numeric string
/// like `"12abc"` is not one — MEASURED, php throws for it exactly as it does for `"abc"`.
fn php_numeric_string(text: &str) -> bool {
    let body = text.trim();
    if body.is_empty() {
        return false;
    }
    let mut chars = body.chars().peekable();
    if matches!(chars.peek(), Some('+' | '-')) {
        chars.next();
    }
    let mut digits = false;
    while matches!(chars.peek(), Some(c) if c.is_ascii_digit()) {
        chars.next();
        digits = true;
    }
    if matches!(chars.peek(), Some('.')) {
        chars.next();
        while matches!(chars.peek(), Some(c) if c.is_ascii_digit()) {
            chars.next();
            digits = true;
        }
    }
    if !digits {
        return false;
    }
    if matches!(chars.peek(), Some('e' | 'E')) {
        chars.next();
        if matches!(chars.peek(), Some('+' | '-')) {
            chars.next();
        }
        let mut exponent = false;
        while matches!(chars.peek(), Some(c) if c.is_ascii_digit()) {
            chars.next();
            exponent = true;
        }
        if !exponent {
            return false;
        }
    }
    chars.next().is_none()
}

/// Returns true for a type php's `int` parameter coercion accepts.
///
/// Deliberately the set `load_as_int` can lower — every scalar plus a boxed `Mixed`, whose
/// run-time value php judges at run time too. An array or an object is what php rejects, and
/// what stays rejected here.
fn coercible_to_int(ty: &PhpType) -> bool {
    match ty {
        PhpType::Int
        | PhpType::Bool
        | PhpType::False
        | PhpType::Float
        | PhpType::Str
        | PhpType::Void
        | PhpType::Never
        | PhpType::TaggedScalar
        | PhpType::Mixed => true,
        PhpType::Union(members) => members.iter().all(coercible_to_int),
        _ => false,
    }
}
