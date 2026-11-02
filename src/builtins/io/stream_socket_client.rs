//! Purpose:
//! Home of the PHP `stream_socket_client` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(stream_resource, Bool)` reflecting PHP's false-on-failure return.
//! - `returns: Mixed` is used because the union cannot be expressed through the scalar field.
//! - `check` also validates that `error_code` (arg[1]) and `error_message` (arg[2]), when
//!   supplied, are plain variables, because the contract declares them by-reference. The
//!   shared by-reference machinery already REJECTS a non-lvalue with PHP's own wording
//!   ("Argument #2 ($error_code) could not be passed by reference"); this hook adds the same
//!   parameter-shaped diagnostic `fsockopen` gives, so the two sibling builtins answer alike.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "stream_socket_client",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamSocketClient,
    ),
}

/// Validates ref output params are plain variables, then returns `Union(stream_resource, Bool)`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if let Some(error_code) = cx.args.get(1) {
        if !is_writable_place(error_code) {
            return Err(CompileError::new(
                error_code.span,
                &format!("{}() parameter $error_code must be passed a variable", cx.name),
            ));
        }
    }
    if let Some(error_message) = cx.args.get(2) {
        if !is_writable_place(error_message) {
            return Err(CompileError::new(
                error_message.span,
                &format!("{}() parameter $error_message must be passed a variable", cx.name),
            ));
        }
    }
    Ok(cx.checker.normalize_union_type(vec![PhpType::stream_resource(), PhpType::False]))
}

/// A by-reference output the call writes back: a variable, or a property, static property or
/// array element (`@pfsockopen($host, -1, $this->errno, $this->errstr)` in Monolog's
/// `SocketHandler`), which take the by-reference builtin write-back.
fn is_writable_place(arg: &crate::parser::ast::Expr) -> bool {
    matches!(
        arg.kind,
        ExprKind::Variable(_)
            | ExprKind::PropertyAccess { .. }
            | ExprKind::StaticPropertyAccess { .. }
            | ExprKind::ArrayAccess { .. }
    )
}
