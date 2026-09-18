//! Purpose:
//! Shared checker contract for PHP's internal-array-pointer builtins
//! (`key`, `current`, `next`, `prev`, `reset`, `end`).
//!
//! Called from:
//! - The `check` hook of each of the six home files in `crate::builtins::array`.
//!
//! Key details:
//! - This module declares no builtin of its own; it only holds the validation the six
//!   home files share, so each of them keeps exactly one `builtin!` declaration.
//! - The receiver-shape rule lives here rather than in EIR lowering so the diagnostic is
//!   a normal type-check error with a source span, next to the argument-type diagnostic.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

/// Validates one internal-array-pointer call and returns its `Mixed` result type.
///
/// Two rules are enforced, in the order PHP itself would notice them:
///
/// 1. The receiver must be a **plain variable**. elephc stores the internal pointer in a
///    hidden cursor slot beside the array local rather than inside the array header, so a
///    property, array element, call result, or any other expression has nowhere to keep a
///    cursor. Accepting those silently would hand back a cursor detached from the value
///    the program actually names, so they are a named compile error instead.
/// 2. The receiver must be array-typed. `Mixed` is allowed because heterogeneous arrays
///    are `Mixed` at compile time; the runtime helpers report `false`/`null` when a Mixed
///    payload turns out not to be a container.
///
/// The registry's `check_arity` has already enforced the single-argument arity, so
/// `cx.args[0]` is present whenever this runs.
pub fn check_array_pointer_call(
    cx: &mut BuiltinCheckCtx,
    name: &str,
) -> Result<PhpType, CompileError> {
    if !matches!(cx.args[0].kind, ExprKind::Variable(_)) {
        return Err(CompileError::new(
            cx.span,
            &format!(
                "{}() argument must be an array variable: elephc keeps the internal array \
                 pointer in a hidden slot beside the variable, so properties, array \
                 elements, and call results have no pointer to move",
                name,
            ),
        ));
    }
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !ty.is_php_array() && !matches!(
        ty,
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed
    ) {
        return Err(CompileError::new(
            cx.span,
            &format!("{}() argument must be array", name),
        ));
    }
    Ok(PhpType::Mixed)
}
