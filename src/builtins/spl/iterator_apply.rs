//! Purpose:
//! Home of the PHP `iterator_apply` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - A `check` hook is required to validate the Traversable source, resolve the callback
//!   signature, and validate the optional args array. Returns `Int` (the iteration count).
//! - The `lazy_check: true` flag skips pre-inference so the hook can control inference
//!   order when the callback signature drives argument type narrowing.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::errors::CompileError;
use crate::types::PhpType;
use crate::types::checker::builtins::spl as checker_spl;

builtin! {
    name: "iterator_apply",
    area: Spl,
    params: [iterator: Mixed, callback: Mixed, args: Mixed = DefaultSpec::Null],
    returns: Int,
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IteratorApply,
    ),
    summary: "Call a function for every element in an iterator.",
    php_manual: "https://www.php.net/manual/en/function.iterator-apply.php",
}

/// Validates the source, resolves callback arity from the args array, and returns `Int`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    checker_spl::check_iterator_apply_source(
        cx.checker,
        &cx.args[0],
        cx.span,
        cx.env,
    )?;
    match checker_spl::iterator_apply_callback_args(
        cx.checker,
        cx.args.get(2),
        cx.span,
        cx.env,
    )? {
        checker_spl::IteratorApplyArgs::Static(callback_args) => {
            checker_spl::check_iterator_apply_static_callback(
                cx.checker,
                &cx.args[1],
                callback_args,
                cx.span,
                cx.env,
            )?;
        }
        checker_spl::IteratorApplyArgs::Dynamic { associative } => {
            checker_spl::check_iterator_apply_dynamic_callback(
                cx.checker,
                &cx.args[1],
                associative,
                cx.span,
                cx.env,
            )?;
        }
    }
    Ok(PhpType::Int)
}
