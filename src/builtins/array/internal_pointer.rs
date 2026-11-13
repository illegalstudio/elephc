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
/// 1. The receiver must be a **plain variable or a temporary call result**. elephc stores
///    the internal pointer in a hidden cursor slot beside the array local rather than inside
///    the array header, so a property or array element has nowhere to keep a cursor the
///    caller can observe. Accepting those silently would hand back a cursor detached from
///    the value the program actually names, so they are a named compile error instead. A
///    temporary result (a call, `match`, or ternary) has no caller-visible pointer after the
///    expression, so lowering materializes it into a hidden local and the cursor is sound.
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
    if !matches!(&cx.args[0].kind, ExprKind::Variable(_))
        && !is_temporary_array_receiver(&cx.args[0].kind)
    {
        return Err(CompileError::new(
            cx.span,
            &format!(
                "{}() argument must be an array variable or a temporary call result: elephc \
                 keeps the internal array pointer in a hidden slot beside the variable, so \
                 properties and array elements have no pointer to move",
                name,
            ),
        ));
    }
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let contains_array = |ty: &PhpType| match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed => true,
        PhpType::Union(members) => members.iter().any(|member| matches!(
            member,
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed
        )),
        _ => false,
    };
    if !ty.is_php_array() && !contains_array(&ty) {
        return Err(CompileError::new(
            cx.span,
            &format!("{}() argument must be array", name),
        ));
    }
    Ok(PhpType::Mixed)
}

/// Returns whether an expression produces a temporary array whose pointer is unobservable
/// after the call, so the hidden cursor can live on the materialized copy.
fn is_temporary_array_receiver(kind: &ExprKind) -> bool {
    matches!(
        kind,
        ExprKind::FunctionCall { .. }
            | ExprKind::MethodCall { .. }
            | ExprKind::NullsafeMethodCall { .. }
            | ExprKind::NullsafeDynamicMethodCall { .. }
            | ExprKind::StaticMethodCall { .. }
            | ExprKind::ClosureCall { .. }
            | ExprKind::ExprCall { .. }
            | ExprKind::Match { .. }
            | ExprKind::Ternary { .. }
            | ExprKind::ShortTernary { .. }
    )
}
