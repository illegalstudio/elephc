//! Purpose:
//! Home of the PHP `class_alias` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The check hook always errors: `class_alias()` is only supported as a top-level
//!   statement with literal class names (handled by the AST-level resolver before
//!   reaching the type checker). Any direct call that reaches this hook is rejected.
//! - Arguments are pre-inferred by the registry common path before the hook runs.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "class_alias",
    area: Callables,
    params: [class: Str, alias: Str, autoload: Bool = DefaultSpec::Bool(true)],
    returns: Bool,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ClassAlias,
    ),
    summary: "Creates an alias for a class.",
    php_manual: "function.class-alias",
}

/// Rejects any direct `class_alias()` call that reaches the type checker.
///
/// AOT compilation resolves `class_alias()` at the top-level statement stage only.
/// Direct calls in other contexts are not supported and must be rejected here.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Err(CompileError::new(
        cx.span,
        "class_alias() is only supported as a top-level statement with literal class names",
    ))
}
