//! Purpose:
//! Binds `pcntl_exec` to typed EIR lowering and validates its PHP array operands.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - Argument and environment values accept PHP scalar-to-string coercion.
//! - Successful execution replaces the process; false is the only returning result.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Validates the executable path plus the optional argument and environment arrays.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for (index, argument) in cx.args.iter().enumerate() {
        let value = argument_value(argument);
        let ty = cx.checker.infer_type(value, cx.env)?;
        let parameter = argument_name(argument).unwrap_or_else(|| match index {
            0 => "path".to_string(),
            1 => "args".to_string(),
            _ => "env_vars".to_string(),
        });
        if parameter == "path" {
            if !matches!(ty.codegen_repr(), PhpType::Str | PhpType::Mixed) {
                return Err(CompileError::new(
                    value.span,
                    &format!("pcntl_exec() parameter $path must be a string, {ty:?} given"),
                ));
            }
            continue;
        }
        let value_ty = match ty.codegen_repr() {
            PhpType::Array(value) => *value,
            PhpType::AssocArray { value, .. } => *value,
            other => {
                return Err(CompileError::new(
                    value.span,
                    &format!("pcntl_exec() parameter ${parameter} must be an array, {other:?} given"),
                ));
            }
        };
        if !matches!(
            value_ty,
            PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Iterable
                | PhpType::Resource(_)
                | PhpType::Str
                | PhpType::Int
                | PhpType::Float
                | PhpType::Bool
                | PhpType::False
                | PhpType::Void
                | PhpType::Never
                | PhpType::TaggedScalar
                | PhpType::Object(_)
                | PhpType::Mixed
                | PhpType::Union(_)
        ) {
            return Err(CompileError::new(
                value.span,
                &format!(
                    "pcntl_exec() array values must be coercible to string, {value_ty:?} given"
                ),
            ));
        }
    }
    Ok(PhpType::Bool)
}

/// Returns the normalized name attached to one named call argument.
fn argument_name(argument: &Expr) -> Option<String> {
    match &argument.kind {
        ExprKind::NamedArg { name, .. } => Some(php_symbol_key(name)),
        _ => None,
    }
}

/// Unwraps a named call argument to its PHP value expression.
fn argument_value(argument: &Expr) -> &Expr {
    match &argument.kind {
        ExprKind::NamedArg { value, .. } => value,
        _ => argument,
    }
}

builtin! {
    contract: "pcntl_exec",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::with_argument_lowering(
        crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::Exec),
        crate::builtins::semantics::BuiltinArgumentLowering::PcntlPreserveOmitted,
    ),
}
