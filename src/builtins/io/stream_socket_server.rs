//! Purpose:
//! Home of the PHP `stream_socket_server` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(stream_resource, Bool)` reflecting PHP's false-on-failure return.
//! - `returns: Mixed` is used because the union cannot be expressed through the scalar field.
//! - TLS listener schemes select the same bridge requirement as TLS client transports.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "stream_socket_server",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamSocketServer,
    ),
    requirements: crate::builtins::semantics::stream_socket_client_requirements,
}

/// Returns `Union(stream_resource, Bool)` reflecting PHP's false-on-failure return.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for index in [1, 2] {
        if let Some(argument) = cx.args.get(index) {
            if !matches!(argument.kind, ExprKind::Variable(_)) {
                let name = if index == 1 { "error_code" } else { "error_message" };
                return Err(CompileError::new(
                    argument.span,
                    &format!(
                        "{}() parameter ${name} must be passed a variable",
                        cx.name
                    ),
                ));
            }
        }
    }
    Ok(cx.checker.normalize_union_type(vec![PhpType::stream_resource(), PhpType::False]))
}
