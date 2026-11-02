//! Purpose:
//! Declares PHP's `headers_sent` builtin and its optional by-reference output parameters.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The runtime flag changes only when bytes leave the output-buffering stack.
//! - Optional `$filename` and `$line` outputs are write-only and therefore checked lazily.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "headers_sent",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::HeadersSent,
    ),
}

/// Validates that supplied output parameters are writable variables without reading them first.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for argument in cx.args {
        let argument = match &argument.kind {
            ExprKind::NamedArg { value, .. } => value.as_ref(),
            _ => argument,
        };
        if !matches!(argument.kind, ExprKind::Variable(_)) {
            return Err(CompileError::new(
                argument.span,
                "headers_sent() output parameters must be passed as variables",
            ));
        }
    }
    Ok(PhpType::Bool)
}
