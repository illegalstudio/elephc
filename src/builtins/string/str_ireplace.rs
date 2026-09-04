//! Purpose:
//! Home of the PHP `str_ireplace` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - `$subject` decides the RESULT SHAPE, exactly as it does for `str_replace`, which is why this
//!   builtin needs a `check` hook at all.
//! - The declared signature includes an optional `count` param, but `max_args: 3`
//!   caps arity so only three arguments are accepted, matching PHP's practical use.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "str_ireplace",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics_with_effects(
        crate::ir::RuntimeFnId::StrIreplace,
        effects,
    ),
}

/// php's optional fourth argument is BY REFERENCE, so a call that passes it writes its caller's
/// variable and cannot be deleted for having an unused result.
///
/// This is per CALL SITE: the three-argument spelling really is pure, and saying otherwise would
/// keep every discarded `str_replace()` alive. MEASURED: with the static `empty()` summary,
/// `str_replace("a", "b", $s, $n);` in statement position left `$n` untouched, because the whole
/// call had already been eliminated before it could write.
fn effects(input: &crate::builtins::semantics::BuiltinSemanticInput<'_>) -> crate::ir::Effects {
    if input.arg_types.len() >= 4 {
        crate::ir::Effects::WRITES_LOCAL
    } else {
        crate::ir::Effects::empty()
    }
}

/// Answers php's result shape, which follows `$subject` alone.
///
/// Without a hook this builtin took the contract's declared `Str` for every call, so
/// `str_ireplace("A", "x", ["banana", "Apple"])` answered the STRING `"xpple"` where php answers
/// a two-element array — MEASURED on `php -n` 8.5.6. Its case-sensitive sibling has carried this
/// hook since the array subject was added; only this one was left behind.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let subject = cx.checker.infer_type(&cx.args[2], cx.env)?;
    if matches!(subject, PhpType::AssocArray { .. }) {
        // php keeps the subject's keys, so a keyed subject answers a keyed array. Saying so here
        // rather than letting the string coercion refuse it downstream is the difference between a
        // diagnostic that names the limit and one that leaks a backend type.
        return Err(CompileError::new(
            cx.span,
            "str_ireplace() subject array with string or sparse keys is not supported yet; \
             a list subject works",
        ));
    }
    if matches!(subject, PhpType::Array(_)) {
        return Ok(PhpType::Array(Box::new(PhpType::Str)));
    }
    Ok(PhpType::Str)
}
