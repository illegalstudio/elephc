//! Purpose:
//! Folds include and require path expressions into static filesystem paths.
//! Accepts literal and constant-derived path expressions while rejecting dynamic includes.
//!
//! Called from:
//! - `crate::resolver::engine_includes` and include discovery statement walkers.
//!
//! Key details:
//! - Dynamic include errors are reported early because the compiler needs a closed source graph.

use crate::parser::ast::{BinOp, Expr, ExprKind};

use super::state::{resolve_constant_ref, ResolveState};

/// Fold a path expression to a compile-time string. Handles string literals,
/// concat of foldable subexpressions, and references to const/define-d string
/// constants tracked in `state`. Returns the human-readable error message when
/// the expression cannot be folded.
pub(super) fn fold_include_path(expr: &Expr, state: &ResolveState) -> Result<String, String> {
    match &expr.kind {
        ExprKind::StringLiteral(s) => Ok(s.clone()),
        ExprKind::BinaryOp {
            left,
            op: BinOp::Concat,
            right,
        } => {
            let l = fold_include_path(left, state)?;
            let r = fold_include_path(right, state)?;
            Ok(l + &r)
        }
        ExprKind::ConstRef(name) => resolve_constant_ref(name, state).ok_or_else(|| {
            format!(
                "include path references unknown constant '{}'; \
                 the constant must be defined (via `const` or `define()`) \
                 before the include statement",
                name.as_str()
            )
        }),
        _ => Err(include_path_error_message(expr)),
    }
}

/// Formats the user-facing error message for a non-foldable include path expression.
/// Handles both runtime-dynamic expressions (variables, calls, etc.) and other
/// invalid expression types, delegating to `runtime_dynamic_include_path_detail`
/// for dynamic expressions or `invalid_include_path_detail` for static invalid shapes.
fn include_path_error_message(expr: &Expr) -> String {
    if let Some(detail) = runtime_dynamic_include_path_detail(expr) {
        return format!(
            "Runtime-dynamic include/require path expressions are not supported: {}. \
             Include paths must be compile-time-constant strings (string literals, \
             concatenations of foldable strings, or `const`/`define()` string constants)",
            detail
        );
    }

    format!(
        "include path must be a compile-time-constant string \
         (string literal, concatenation thereof, or a `const`/`define()`-d \
         string constant): {}",
        invalid_include_path_detail(expr)
    )
}

/// Classifies runtime-dynamic expression kinds for the include path error message.
/// Returns `Some(description)` for expressions that resolve at runtime (variables,
/// calls, ternaries, property access), `None` for expressions that could theoretically
/// be foldable but are expressed in a dynamic way.
fn runtime_dynamic_include_path_detail(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Variable(name) => {
            Some(format!("variable `${}` is resolved at runtime", name))
        }
        ExprKind::FunctionCall { name, .. } => {
            Some(format!("function call `{}()` is resolved at runtime", name.as_str()))
        }
        ExprKind::ClosureCall { var, .. } => {
            Some(format!("closure call `${}()` is resolved at runtime", var))
        }
        ExprKind::ExprCall { .. } => {
            Some("callable expression call is resolved at runtime".to_string())
        }
        ExprKind::MethodCall { method, .. } | ExprKind::NullsafeMethodCall { method, .. } => {
            Some(format!("method call `->{}` is resolved at runtime", method))
        }
        ExprKind::StaticMethodCall { method, .. } => {
            Some(format!("static method call `::{}` is resolved at runtime", method))
        }
        ExprKind::Ternary { .. } | ExprKind::ShortTernary { .. } => {
            Some("ternary path selection is resolved at runtime".to_string())
        }
        ExprKind::PropertyAccess { property, .. } | ExprKind::NullsafePropertyAccess { property, .. } => {
            Some(format!("property access `->{}` is resolved at runtime", property))
        }
        ExprKind::StaticPropertyAccess { property, .. } => {
            Some(format!("static property access `::${}` is resolved at runtime", property))
        }
        ExprKind::ArrayAccess { .. } => {
            Some("array access is resolved at runtime".to_string())
        }
        _ => None,
    }
}

/// Classifies statically-invalid expression kinds for the include path error message.
/// Returns a description for expressions that are invalid but not runtime-dynamic
/// (e.g., wrong binary operator, non-string literals). Used when the expression
/// shape is known at compile time but is not a valid include path.
fn invalid_include_path_detail(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::BinaryOp { op, .. } if *op != BinOp::Concat => {
            "only string concatenation can be folded for include paths".to_string()
        }
        ExprKind::BinaryOp { .. } => {
            "concatenation contains a runtime-evaluated subexpression".to_string()
        }
        ExprKind::IntLiteral(_) => "integer literals are not valid include paths".to_string(),
        ExprKind::FloatLiteral(_) => "float literals are not valid include paths".to_string(),
        ExprKind::BoolLiteral(_) => "boolean literals are not valid include paths".to_string(),
        ExprKind::Null => "null is not a valid include path".to_string(),
        _ => "this expression cannot be folded to a string at compile time".to_string(),
    }
}
