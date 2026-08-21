//! Purpose:
//! Home of the PHP `str_replace` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - `$search` and `$replace` are `array|string`; an array `$search` applies its terms in order,
//!   each to the result of the last, and is lowered over its own runtime helper.
//! - `$subject` decides the RESULT SHAPE, which is why this builtin needs a `check` hook at all:
//!   php answers an array for an array subject and a string for a string one. The contract's
//!   declared `Str` cannot say that, and a blanket `Mixed` would widen every existing call site
//!   whose subject is a plain string.
//! - The declared signature includes an optional `count` param, but `max_args: 3` caps arity so
//!   only three arguments are accepted.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "str_replace",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StrReplace,
    ),
}

/// Answers php's result shape, which follows `$subject` alone.
///
/// An array subject answers an array of strings — php replaces inside every element and keeps the
/// keys. Anything else answers a string, which is what every call site with a plain string subject
/// already relied on, so their inferred types do not move.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let subject = cx.checker.infer_type(&cx.args[2], cx.env)?;
    if matches!(subject, PhpType::AssocArray { .. }) {
        // php keeps the subject's keys, so a keyed subject answers a keyed array. Saying so here
        // rather than letting the string coercion refuse it downstream is the difference between a
        // diagnostic that names the limit and one that leaks a backend type.
        return Err(CompileError::new(
            cx.span,
            "str_replace() subject array with string or sparse keys is not supported yet; \
             a list subject works",
        ));
    }
    if matches!(subject, PhpType::Array(_)) {
        return Ok(PhpType::Array(Box::new(PhpType::Str)));
    }
    Ok(PhpType::Str)
}
