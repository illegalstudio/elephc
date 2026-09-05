//! Purpose:
//! Validates Linux-only PCNTL CPU-affinity builtins and refines their PHP result shapes.
//!
//! Called from:
//! - The `pcntl_getcpuaffinity` and `pcntl_setcpuaffinity` builtin checker hooks.
//!
//! Key details:
//! - CPU masks are represented as indexed integer arrays in AOT code.
//! - PHP's optional-looking `$cpu_ids` parameter must still be present; empty masks reach the
//!   runtime so they raise PHP's catchable `ValueError` instead of a compile-time diagnostic.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Checks the optional process identifier and returns PHP's `array<int>|false` storage type.
pub(super) fn check_getcpuaffinity(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for arg in cx.args {
        cx.checker.infer_type(arg, cx.env)?;
    }
    Ok(PhpType::Mixed)
}

/// Requires an indexed integer mask while checking the optional process identifier.
pub(super) fn check_setcpuaffinity(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let mut cpu_ids = None;
    for (index, arg) in cx.args.iter().enumerate() {
        let (name, value) = named_value(arg);
        if name.as_deref() == Some("cpu_ids") || (name.is_none() && index == 1) {
            cpu_ids = Some(value);
            continue;
        }
        cx.checker.infer_type(value, cx.env)?;
    }
    let Some(cpu_ids) = cpu_ids else {
        return Err(CompileError::new(
            cx.span,
            "pcntl_setcpuaffinity() parameter $cpu_ids must not be empty",
        ));
    };
    let ty = cx.checker.infer_type(cpu_ids, cx.env)?;
    match ty.codegen_repr() {
        PhpType::Array(element) if matches!(&*element, PhpType::Int | PhpType::Never) => {}
        other => {
            return Err(CompileError::new(
                cpu_ids.span,
                &format!(
                    "pcntl_setcpuaffinity() parameter $cpu_ids must be an indexed integer array, {other:?} given"
                ),
            ));
        }
    }
    Ok(PhpType::Bool)
}

/// Returns the normalized optional name and value of one source argument.
fn named_value(arg: &Expr) -> (Option<String>, &Expr) {
    match &arg.kind {
        ExprKind::NamedArg { name, value } => (Some(php_symbol_key(name)), value),
        _ => (None, arg),
    }
}
