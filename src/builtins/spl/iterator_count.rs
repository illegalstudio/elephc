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
    name: "iterator_count",
    area: Spl,
    params: [iterator: Mixed],
    returns: Int,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IteratorCount,
    ),
    summary: "Count the elements in an iterator.",
    php_manual: "https://www.php.net/manual/en/function.iterator-count.php",
}

/// Validates the iterator source type and returns `Int`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    checker_spl::check_iterator_source(
        cx.checker,
        &cx.args[0],
        cx.span,
        cx.env,
        "iterator_count()",
    )?;
    Ok(PhpType::Int)
}
