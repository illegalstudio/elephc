//! Purpose:
//! Home of the PHP `iterator_to_array` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - A `check` hook is required because the return type depends on the source type and
//!   the `preserve_keys` argument (static bool narrows to `AssocArray` or `Array`).
//! - The `returns: Mixed` macro field is a conservative fallback; the check hook always
//!   returns the precise array type.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;
use crate::types::checker::builtins::spl as checker_spl;

builtin! {
    contract: "iterator_to_array",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IteratorToArray,
    ),
}

/// Validates the source and computes the precise array return type based on `preserve_keys`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let inferred_source_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    // A scalar is never iterable, and php finds out when the call runs: the branch that holds it
    // may be unreachable (`$value instanceof Traversable` over a parameter every caller passes a
    // string), which Laravel's `Str::replace()` is. The call answers a declared PHP array and
    // lowers to a helper that raises the `TypeError`.
    if matches!(
        inferred_source_ty,
        PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Bool | PhpType::False | PhpType::Void
    ) {
        if let Some(arg) = cx.args.get(1) {
            checker_spl::check_iterator_to_array_preserve_keys(cx.checker, arg, cx.env)?;
        }
        return Ok(PhpType::php_array());
    }
    // A union (`array|Traversable`, `?iterable`, a `@return` of several shapes) holds a value only
    // known at run time, like `mixed`: the Mixed source path walks whatever it carries and raises
    // php's `TypeError` for a member that is not iterable.
    let source_ty = if matches!(inferred_source_ty, PhpType::Mixed | PhpType::Union(_)) {
        PhpType::Mixed
    } else {
        checker_spl::check_iterator_source(
            cx.checker,
            &cx.args[0],
            cx.span,
            cx.env,
            "iterator_to_array()",
        )?
    };
    let preserve_keys = if let Some(arg) = cx.args.get(1) {
        checker_spl::check_iterator_to_array_preserve_keys(cx.checker, arg, cx.env)?
    } else {
        Some(true)
    };
    Ok(checker_spl::iterator_to_array_return_type(
        cx.checker,
        &source_ty,
        preserve_keys,
    ))
}
