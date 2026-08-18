//! Purpose:
//! Validates PCNTL wait output parameters without reading their pre-call values.
//!
//! Called from:
//! - The `pcntl_wait` and `pcntl_waitpid` builtin checker hooks.
//!
//! Key details:
//! - `$status` and `$resource_usage` are write-only by-reference outputs and may name
//!   variables that do not exist before the call.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Checks `pcntl_wait()` inputs while leaving its two write-only outputs unread.
pub(super) fn check_wait(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    check_wait_outputs(cx, false)
}

/// Checks `pcntl_waitpid()` inputs while leaving its two write-only outputs unread.
pub(super) fn check_waitpid(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    check_wait_outputs(cx, true)
}

/// Validates the shared wait-family argument shape and infers only input operands.
fn check_wait_outputs(
    cx: &mut BuiltinCheckCtx,
    selected_child: bool,
) -> Result<PhpType, CompileError> {
    for (index, arg) in cx.args.iter().enumerate() {
        let parameter = wait_parameter_name(arg, index, selected_child);
        if matches!(parameter.as_deref(), Some("status" | "resource_usage")) {
            let value = named_argument_value(arg);
            if !matches!(value.kind, ExprKind::Variable(_)) {
                return Err(CompileError::new(
                    value.span,
                    &format!(
                        "{}() parameter ${} must be passed a variable",
                        cx.name,
                        parameter.expect("output parameter name must be present"),
                    ),
                ));
            }
        } else {
            cx.checker.infer_type(arg, cx.env)?;
        }
    }
    Ok(PhpType::Int)
}

/// Resolves one source argument to the wait-family parameter it binds without evaluating it.
fn wait_parameter_name(arg: &Expr, index: usize, selected_child: bool) -> Option<String> {
    if let ExprKind::NamedArg { name, .. } = &arg.kind {
        return Some(php_symbol_key(name));
    }
    let parameters: &[&str] = if selected_child {
        &["process_id", "status", "flags", "resource_usage"]
    } else {
        &["status", "flags", "resource_usage"]
    };
    parameters.get(index).map(|name| (*name).to_string())
}

/// Unwraps a named argument to the value PHP passes to the selected parameter.
fn named_argument_value(arg: &Expr) -> &Expr {
    match &arg.kind {
        ExprKind::NamedArg { value, .. } => value,
        _ => arg,
    }
}
