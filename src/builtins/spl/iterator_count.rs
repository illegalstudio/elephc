//! Purpose:
//! Home of the PHP `iterator_count` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - A `check` hook is required to validate that the argument is a statically known
//!   array or Traversable (not an arbitrary value); returns `Int`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;
use crate::types::checker::builtins::spl as checker_spl;

builtin! {
    contract: "iterator_count",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IteratorCount,
    ),
}

/// Validates the iterator source type and returns `Int`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    // A value only known at run time (`mixed`, or a union such as `int|iterable` narrowed by an
    // `is_iterable()` arm) is counted by its runtime tag, which raises php's `TypeError` for a
    // payload that is not iterable -- the rule `iterator_to_array()` follows.
    if matches!(
        cx.checker.infer_type(&cx.args[0], cx.env)?,
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return Ok(PhpType::Int);
    }
    checker_spl::check_iterator_source(
        cx.checker,
        &cx.args[0],
        cx.span,
        cx.env,
        "iterator_count()",
    )?;
    Ok(PhpType::Int)
}
