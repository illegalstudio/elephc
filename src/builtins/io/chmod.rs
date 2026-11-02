//! Purpose:
//! Home of the PHP `chmod` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Bool` and requires the `permissions` argument to possibly be an `Int`,
//!   emitting the diagnostic at the mode argument's span.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "chmod",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Chmod,
    ),
}

/// Returns `Bool`, rejecting a `permissions` argument that can never be an `Int` at its own span.
///
/// A value that MAY be an int is coerced at run time, as php coerces it. Property reads are not
/// narrowed, so Monolog's `if ($this->filePermission !== null) { @chmod($url,
/// $this->filePermission); }` reaches here as `?int`, and refusing it left `StreamHandler` -- the
/// handler behind every Laravel log channel -- to the interpreter.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    let mode_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    let may_be_int = match &mode_ty {
        PhpType::Int | PhpType::Mixed => true,
        PhpType::Union(members) => members.iter().any(|member| *member == PhpType::Int),
        _ => false,
    };
    if !may_be_int {
        return Err(CompileError::new(cx.args[1].span, "chmod() mode must be int"));
    }
    Ok(PhpType::Bool)
}
