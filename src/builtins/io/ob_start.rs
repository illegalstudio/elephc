//! Purpose:
//! Home of the PHP `ob_start` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Output handlers are supported: closures, first-class callables, function
//!   name strings, and boxed Mixed callables run on flush/clean with PHP's
//!   phase bits; array-pair callables are rejected at compile time.
//! - `chunk_size` arms PHP's auto-flush threshold; `flags` gate
//!   cleanable/flushable/removable behavior exactly like PHP.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "ob_start",
    area: Io,
    params: [
        callback: Mixed = DefaultSpec::Null,
        chunk_size: Int = DefaultSpec::Int(0),
        flags: Int = DefaultSpec::Int(112)
    ],
    returns: Bool,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ObStart,
    ),
    summary: "Turns on output buffering.",
    php_manual: "function.ob-start",
}

/// Returns `Bool`. Output handlers are supported for `null`, closures,
/// first-class callables, function-name strings, and boxed `Mixed` callables;
/// array-pair callables (`[$obj, 'method']`) are rejected at compile time.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if let Some(callback) = cx.args.first() {
        if matches!(
            callback.kind,
            ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)
        ) {
            return Err(CompileError::new(
                cx.span,
                "ob_start() array output-handler callbacks are not supported; use a closure or function name",
            ));
        }
    }
    Ok(PhpType::Bool)
}
