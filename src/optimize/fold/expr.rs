//! Purpose:
//! Implements constant-folding support for expr expressions.
//! Evaluates compile-time scalar cases that are safe to replace with literal AST nodes.
//!
//! Called from:
//! - `crate::optimize::fold`
//!
//! Key details:
//! - Folding must respect PHP coercions, truthiness, numeric edge cases, and runtime error boundaries.

use super::super::{fold_block, try_prune_match_expr};
use super::super::*;
use super::casts::try_fold_cast;
use super::ops::{
    try_fold_array_access, try_fold_binary_op, try_fold_bit_not, try_fold_negate,
    try_fold_not, try_fold_null_coalesce, try_fold_short_ternary, try_fold_ternary,
};

/// Folds default expressions in function parameters.
/// Returns a new parameter list with each parameter's default expression folded.
pub(in crate::optimize) fn fold_params(
    params: Vec<(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)>,
) -> Vec<(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)> {
    params
        .into_iter()
        .map(|(name, type_expr, default, is_ref)| {
            (name, type_expr, default.map(fold_expr), is_ref)
        })
        .collect()
}

/// Folds default expressions in a class property declaration.
pub(in crate::optimize) fn fold_property(property: ClassProperty) -> ClassProperty {
    ClassProperty {
        name: property.name,
        visibility: property.visibility,
        type_expr: property.type_expr,
        hooks: property.hooks,
        readonly: property.readonly,
        is_final: property.is_final,
        is_static: property.is_static,
        is_abstract: property.is_abstract,
        by_ref: property.by_ref,
        default: property.default.map(fold_expr),
        span: property.span,
        attributes: property.attributes,
    }
}

/// Folds default expressions and block body in a class method declaration.
pub(in crate::optimize) fn fold_method(method: ClassMethod) -> ClassMethod {
    ClassMethod {
        name: method.name,
        visibility: method.visibility,
        is_static: method.is_static,
        is_abstract: method.is_abstract,
        is_final: method.is_final,
        has_body: method.has_body,
        params: fold_params(method.params),
        variadic: method.variadic,
        return_type: method.return_type,
        body: fold_block(method.body),
        span: method.span,
        attributes: method.attributes,
    }
}

/// Folds the optional value expression in an enum case declaration.
pub(in crate::optimize) fn fold_enum_case(case: EnumCaseDecl) -> EnumCaseDecl {
    EnumCaseDecl {
        name: case.name,
        value: case.value.map(fold_expr),
        span: case.span,
        attributes: case.attributes,
    }
}

/// Recursively folds constant expressions in an AST expression.
///
/// Dispatches each `ExprKind` variant to the appropriate helper under `ops`,
/// `casts`, `pipes`, or `inline_closure`. Returns the folded expression, or
/// falls back to the original node when no fold is applicable. Division by
/// zero, overflow, and other PHP-runtime behaviors are intentionally left as
/// runtime decisions — this function only folds when the result is an
/// unambiguous PHP equivalent.
pub(in crate::optimize) fn fold_expr(expr: Expr) -> Expr {
    let span = expr.span;
    let kind = match expr.kind {
        ExprKind::StringLiteral(value) => ExprKind::StringLiteral(value),
        ExprKind::IntLiteral(value) => ExprKind::IntLiteral(value),
        ExprKind::FloatLiteral(value) => ExprKind::FloatLiteral(value),
        ExprKind::Variable(name) => ExprKind::Variable(name),
        ExprKind::BinaryOp { left, op, right } => {
            let left = fold_expr(*left);
            let right = fold_expr(*right);
            try_fold_binary_op(&op, &left, &right).unwrap_or_else(|| ExprKind::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            })
        }
        ExprKind::InstanceOf { value, target } => ExprKind::InstanceOf {
            value: Box::new(fold_expr(*value)),
            target: fold_instanceof_target(target),
        },
        ExprKind::BoolLiteral(value) => ExprKind::BoolLiteral(value),
        ExprKind::Null => ExprKind::Null,
        ExprKind::Negate(inner) => {
            let inner = fold_expr(*inner);
            try_fold_negate(&inner).unwrap_or_else(|| ExprKind::Negate(Box::new(inner)))
        }
        ExprKind::Not(inner) => {
            let inner = fold_expr(*inner);
            try_fold_not(&inner).unwrap_or_else(|| ExprKind::Not(Box::new(inner)))
        }
        ExprKind::BitNot(inner) => {
            let inner = fold_expr(*inner);
            try_fold_bit_not(&inner).unwrap_or_else(|| ExprKind::BitNot(Box::new(inner)))
        }
        ExprKind::Throw(inner) => ExprKind::Throw(Box::new(fold_expr(*inner))),
        ExprKind::ErrorSuppress(inner) => ExprKind::ErrorSuppress(Box::new(fold_expr(*inner))),
        ExprKind::Print(inner) => ExprKind::Print(Box::new(fold_expr(*inner))),
        ExprKind::NullCoalesce { value, default } => {
            let value = fold_expr(*value);
            let default = fold_expr(*default);
            try_fold_null_coalesce(&value, &default).unwrap_or_else(|| ExprKind::NullCoalesce {
                value: Box::new(value),
                default: Box::new(default),
            })
        }
        ExprKind::Pipe { value, callable } => {
            let value = fold_expr(*value);
            let callable = fold_expr(*callable);
            super::pipes::try_fold_pure_pipe(&value, &callable)
                .or_else(|| super::inline_closure::try_inline_closure_pipe(&value, &callable))
                .unwrap_or_else(|| ExprKind::Pipe {
                    value: Box::new(value),
                    callable: Box::new(callable),
                })
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp,
        } => {
            let target = Box::new(fold_expr(*target));
            let value = Box::new(fold_expr(*value));
            let result_target = result_target
                .map(|inner| Box::new(fold_expr(*inner)))
                .filter(|inner| inner.kind != target.kind);
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude: fold_block(prelude),
                conditional_value_temp,
            }
        }
        ExprKind::PreIncrement(name) => ExprKind::PreIncrement(name),
        ExprKind::PostIncrement(name) => ExprKind::PostIncrement(name),
        ExprKind::PreDecrement(name) => ExprKind::PreDecrement(name),
        ExprKind::PostDecrement(name) => ExprKind::PostDecrement(name),
        ExprKind::FunctionCall { name, args } => ExprKind::FunctionCall {
            name,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::ArrayLiteral(items) => {
            ExprKind::ArrayLiteral(items.into_iter().map(fold_expr).collect())
        }
        ExprKind::ArrayLiteralAssoc(items) => ExprKind::ArrayLiteralAssoc(
            items
                .into_iter()
                .map(|(key, value)| (fold_expr(key), fold_expr(value)))
                .collect(),
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            let subject = fold_expr(*subject);
            let arms = arms
                .into_iter()
                .map(|(patterns, value)| {
                    (
                        patterns.into_iter().map(fold_expr).collect(),
                        fold_expr(value),
                    )
                })
                .collect();
            let default = default.map(|expr| Box::new(fold_expr(*expr)));
            try_prune_match_expr(subject, arms, default)
        }
        ExprKind::ArrayAccess { array, index } => {
            let array = fold_expr(*array);
            let index = fold_expr(*index);
            try_fold_array_access(&array, &index).unwrap_or_else(|| ExprKind::ArrayAccess {
                array: Box::new(array),
                index: Box::new(index),
            })
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            let condition = fold_expr(*condition);
            let then_expr = fold_expr(*then_expr);
            let else_expr = fold_expr(*else_expr);
            try_fold_ternary(&condition, &then_expr, &else_expr).unwrap_or_else(|| {
                ExprKind::Ternary {
                    condition: Box::new(condition),
                    then_expr: Box::new(then_expr),
                    else_expr: Box::new(else_expr),
                }
            })
        }
        ExprKind::ShortTernary { value, default } => {
            let value = fold_expr(*value);
            let default = fold_expr(*default);
            try_fold_short_ternary(&value, &default).unwrap_or_else(|| ExprKind::ShortTernary {
                value: Box::new(value),
                default: Box::new(default),
            })
        }
        ExprKind::Cast { target, expr } => {
            let expr = fold_expr(*expr);
            try_fold_cast(&target, &expr).unwrap_or_else(|| ExprKind::Cast {
                target,
                expr: Box::new(expr),
            })
        }
        ExprKind::Closure {
            params,
            variadic,
            return_type,
            body,
            is_arrow,
            is_static,
            captures,
            capture_refs,
        } => ExprKind::Closure {
            params: fold_params(params),
            variadic,
            return_type,
            body: fold_block(body),
            is_arrow,
            is_static,
            captures,
            capture_refs,
        },
        ExprKind::NamedArg { name, value } => ExprKind::NamedArg {
            name,
            value: Box::new(fold_expr(*value)),
        },
        ExprKind::Spread(inner) => ExprKind::Spread(Box::new(fold_expr(*inner))),
        ExprKind::ClosureCall { var, args } => ExprKind::ClosureCall {
            var,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::ExprCall { callee, args } => ExprKind::ExprCall {
            callee: Box::new(fold_expr(*callee)),
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::ConstRef(name) => ExprKind::ConstRef(name),
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(fold_expr(*object)),
            property,
        },
        ExprKind::DynamicPropertyAccess { object, property } => {
            ExprKind::DynamicPropertyAccess {
                object: Box::new(fold_expr(*object)),
                property: Box::new(fold_expr(*property)),
            }
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            ExprKind::NullsafePropertyAccess {
                object: Box::new(fold_expr(*object)),
                property,
            }
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            ExprKind::NullsafeDynamicPropertyAccess {
                object: Box::new(fold_expr(*object)),
                property: Box::new(fold_expr(*property)),
            }
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            ExprKind::StaticPropertyAccess { receiver, property }
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => ExprKind::MethodCall {
            object: Box::new(fold_expr(*object)),
            method,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => ExprKind::NullsafeMethodCall {
            object: Box::new(fold_expr(*object)),
            method,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => ExprKind::StaticMethodCall {
            receiver,
            method,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::FirstClassCallable(target) => {
            ExprKind::FirstClassCallable(fold_callable_target(target))
        }
        ExprKind::This => ExprKind::This,
        ExprKind::PtrCast { target_type, expr } => ExprKind::PtrCast {
            target_type,
            expr: Box::new(fold_expr(*expr)),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type,
            len: Box::new(fold_expr(*len)),
        },
        ExprKind::ClassConstant { receiver } => ExprKind::ClassConstant { receiver },
        ExprKind::ScopedConstantAccess { receiver, name } => {
            ExprKind::ScopedConstantAccess { receiver, name }
        }
        ExprKind::NewScopedObject { receiver, args } => ExprKind::NewScopedObject {
            receiver,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::Yield { key, value } => ExprKind::Yield {
            key: key.map(|k| Box::new(fold_expr(*k))),
            value: value.map(|v| Box::new(fold_expr(*v))),
        },
        ExprKind::YieldFrom(inner) => ExprKind::YieldFrom(Box::new(fold_expr(*inner))),
        ExprKind::MagicConstant(_) => {
            unreachable!("MagicConstant must be lowered before optimizer passes")
        }
    };
    Expr { kind, span }
}

/// Folds the target of an instanceof expression, recursing into the expression form.
fn fold_instanceof_target(target: InstanceOfTarget) -> InstanceOfTarget {
    match target {
        InstanceOfTarget::Name(name) => InstanceOfTarget::Name(name),
        InstanceOfTarget::Expr(expr) => InstanceOfTarget::Expr(Box::new(fold_expr(*expr))),
    }
}

/// Folds the target of a first-class callable, recursing into object expressions.
fn fold_callable_target(target: CallableTarget) -> CallableTarget {
    match target {
        CallableTarget::Function(name) => CallableTarget::Function(name),
        CallableTarget::StaticMethod { receiver, method } => {
            CallableTarget::StaticMethod { receiver, method }
        }
        CallableTarget::Method { object, method } => CallableTarget::Method {
            object: Box::new(fold_expr(*object)),
            method,
        },
    }
}
