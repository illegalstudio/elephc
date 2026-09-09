//! Purpose:
//! Shared type-checking contract for PHP key-ordering builtins.
//!
//! Called from:
//! - `crate::builtins::array::ksort` and `crate::builtins::array::krsort`.
//!
//! Key details:
//! - Concrete arrays and PHP array declarations are accepted directly; boxed cells of array places defer
//!   runtime tag validation to the shared nested key-sort lowering path.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Validates the common array receiver contract for `ksort()` and `krsort()`.
///
/// Concrete arrays and boxed PHP array declarations are accepted statically. A boxed element of a packed
/// or associative heterogeneous array place is also accepted because the nested lowering path
/// checks its runtime tag before mutation and raises the builtin-specific PHP `TypeError` for
/// invalid cells.
pub(super) fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let accepts_mixed_nested_element = ty == PhpType::Mixed
        && is_mixed_array_element_lvalue(cx.checker, cx.env, &cx.args[0])?;
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. })
        && !ty.is_php_array()
        && !accepts_mixed_nested_element
    {
        return Err(CompileError::new(cx.span, &format!("{}() argument must be array", cx.name)));
    }
    Ok(PhpType::Bool)
}

/// Reports whether `arg` is a boxed element lvalue in a supported heterogeneous array place.
///
/// This is deliberately narrower than accepting arbitrary `Mixed`: the EIR nested-place path can
/// recover one stable boxed cell from these receiver shapes and guards every non-array payload.
fn is_mixed_array_element_lvalue(
    checker: &mut crate::types::checker::Checker,
    env: &crate::types::TypeEnv,
    arg: &Expr,
) -> Result<bool, CompileError> {
    let arg = match &arg.kind {
        ExprKind::NamedArg { value, .. } => value.as_ref(),
        _ => arg,
    };
    let ExprKind::ArrayAccess { array, index } = &arg.kind else {
        return Ok(false);
    };
    if !is_supported_parent_place(array) {
        return Ok(false);
    }
    let parent_ty = checker.infer_type(array, env)?;
    let index_ty = checker.infer_type(index, env)?;
    let normalized_key = crate::types::normalized_array_key_type(index, index_ty);
    Ok(match parent_ty.codegen_repr() {
        PhpType::Array(element_ty) => {
            element_ty.codegen_repr() == PhpType::Mixed && normalized_key == PhpType::Int
        }
        PhpType::AssocArray { value, .. } => {
            value.codegen_repr() == PhpType::Mixed
                && matches!(normalized_key, PhpType::Int | PhpType::Str | PhpType::Mixed)
        }
        _ => false,
    })
}

/// Reports whether the nested receiver can be read and written back by EIR place lowering.
fn is_supported_parent_place(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_) | ExprKind::This | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::PropertyAccess { object, .. } => is_supported_parent_place(object),
        ExprKind::ArrayAccess { array, .. } => is_supported_parent_place(array),
        _ => false,
    }
}
