//! Purpose:
//! Home of the PHP `ptr` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is a variable (not an arbitrary expression)
//!   and returns `PhpType::Pointer(None)`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "ptr",
    area: Pointers,
    params: [value: Mixed],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Ptr,
    ),
    summary: "Returns a raw pointer to the given variable.",
    extension: true,
}

/// Validates that the argument is a variable and returns `PhpType::Pointer(None)`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 1 argument).
/// `ptr()` requires a variable as its argument because taking the address of an
/// arbitrary expression has no well-defined meaning in the pointer model.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    match &cx.args[0].kind {
        ExprKind::Variable(_) => {
            cx.checker.infer_type(&cx.args[0], cx.env)?;
        }
        _ => {
            return Err(CompileError::new(
                cx.span,
                "ptr() argument must be a variable",
            ));
        }
    }
    Ok(PhpType::Pointer(None))
}
