//! Purpose:
//! Normalizes class-constant value expressions while class schema metadata is built.
//! Resolves lexical `self::` and `parent::` constant receivers to concrete class names.
//!
//! Called from:
//! - `crate::types::checker::schema::classes::state::ClassBuildState::into_class_info()`
//!
//! Key details:
//! - Class constant values are later re-inferred and emitted outside the declaring class scope.
//! - `static::` is rejected in compile-time constants to match PHP's early-bound rules.

use crate::errors::CompileError;
use crate::names::{Name, NameKind};
use crate::parser::ast::{
    CallableTarget, Expr, ExprKind, InstanceOfTarget, StaticReceiver,
};
use crate::span::Span;
use crate::types::traits::FlattenedClass;

/// Normalizes a class-constant value expression by resolving lexical `self::` and
/// `parent::` constant receivers to their concrete class names. Rejects `static::`
/// since class constants are early-bound in PHP.
///
/// - `self::CONST` is rewritten to `ClassName::CONST`
/// - `parent::CONST` is rewritten to `ParentClassName::CONST`
/// - `static::CONST` produces a compile error
pub(super) fn resolve_lexical_class_constant_value(
    value: &Expr,
    class: &FlattenedClass,
) -> Result<Expr, CompileError> {
    rewrite_expr(value, &class.name, class.extends.as_deref())
}

/// Recursively rewrites all expressions in a class-constant value, resolving lexical
/// constant receivers found in `ClassConstant` and `ScopedConstantAccess` nodes.
fn rewrite_expr(
    expr: &Expr,
    class_name: &str,
    parent_name: Option<&str>,
) -> Result<Expr, CompileError> {
    let kind = match &expr.kind {
        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
            left: Box::new(rewrite_expr(left, class_name, parent_name)?),
            op: op.clone(),
            right: Box::new(rewrite_expr(right, class_name, parent_name)?),
        },
        ExprKind::InstanceOf { value, target } => ExprKind::InstanceOf {
            value: Box::new(rewrite_expr(value, class_name, parent_name)?),
            target: rewrite_instanceof_target(target, class_name, parent_name)?,
        },
        ExprKind::Negate(inner) => {
            ExprKind::Negate(Box::new(rewrite_expr(inner, class_name, parent_name)?))
        }
        ExprKind::Not(inner) => {
            ExprKind::Not(Box::new(rewrite_expr(inner, class_name, parent_name)?))
        }
        ExprKind::BitNot(inner) => {
            ExprKind::BitNot(Box::new(rewrite_expr(inner, class_name, parent_name)?))
        }
        ExprKind::Throw(inner) => {
            ExprKind::Throw(Box::new(rewrite_expr(inner, class_name, parent_name)?))
        }
        ExprKind::ErrorSuppress(inner) => ExprKind::ErrorSuppress(Box::new(rewrite_expr(
            inner,
            class_name,
            parent_name,
        )?)),
        ExprKind::Print(inner) => {
            ExprKind::Print(Box::new(rewrite_expr(inner, class_name, parent_name)?))
        }
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(rewrite_expr(value, class_name, parent_name)?),
            default: Box::new(rewrite_expr(default, class_name, parent_name)?),
        },
        ExprKind::Pipe { value, callable } => ExprKind::Pipe {
            value: Box::new(rewrite_expr(value, class_name, parent_name)?),
            callable: Box::new(rewrite_expr(callable, class_name, parent_name)?),
        },
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp,
        } => ExprKind::Assignment {
            target: Box::new(rewrite_expr(target, class_name, parent_name)?),
            value: Box::new(rewrite_expr(value, class_name, parent_name)?),
            result_target: result_target
                .as_ref()
                .map(|expr| rewrite_expr(expr, class_name, parent_name).map(Box::new))
                .transpose()?,
            prelude: prelude.clone(),
            conditional_value_temp: conditional_value_temp.clone(),
        },
        ExprKind::FunctionCall { name, args } => ExprKind::FunctionCall {
            name: name.clone(),
            args: rewrite_expr_list(args, class_name, parent_name)?,
        },
        ExprKind::ArrayLiteral(values) => {
            ExprKind::ArrayLiteral(rewrite_expr_list(values, class_name, parent_name)?)
        }
        ExprKind::ArrayLiteralAssoc(values) => ExprKind::ArrayLiteralAssoc(
            values
                .iter()
                .map(|(key, value)| {
                    Ok((
                        rewrite_expr(key, class_name, parent_name)?,
                        rewrite_expr(value, class_name, parent_name)?,
                    ))
                })
                .collect::<Result<Vec<_>, CompileError>>()?,
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => ExprKind::Match {
            subject: Box::new(rewrite_expr(subject, class_name, parent_name)?),
            arms: arms
                .iter()
                .map(|(conditions, result)| {
                    Ok((
                        rewrite_expr_list(conditions, class_name, parent_name)?,
                        rewrite_expr(result, class_name, parent_name)?,
                    ))
                })
                .collect::<Result<Vec<_>, CompileError>>()?,
            default: default
                .as_ref()
                .map(|expr| rewrite_expr(expr, class_name, parent_name).map(Box::new))
                .transpose()?,
        },
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(rewrite_expr(array, class_name, parent_name)?),
            index: Box::new(rewrite_expr(index, class_name, parent_name)?),
        },
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ternary {
            condition: Box::new(rewrite_expr(condition, class_name, parent_name)?),
            then_expr: Box::new(rewrite_expr(then_expr, class_name, parent_name)?),
            else_expr: Box::new(rewrite_expr(else_expr, class_name, parent_name)?),
        },
        ExprKind::ShortTernary { value, default } => ExprKind::ShortTernary {
            value: Box::new(rewrite_expr(value, class_name, parent_name)?),
            default: Box::new(rewrite_expr(default, class_name, parent_name)?),
        },
        ExprKind::Cast { target, expr } => ExprKind::Cast {
            target: target.clone(),
            expr: Box::new(rewrite_expr(expr, class_name, parent_name)?),
        },
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
            params: params
                .iter()
                .map(|(name, type_expr, default, is_ref)| {
                    Ok((
                        name.clone(),
                        type_expr.clone(),
                        default
                            .as_ref()
                            .map(|expr| rewrite_expr(expr, class_name, parent_name))
                            .transpose()?,
                        *is_ref,
                    ))
                })
                .collect::<Result<Vec<_>, CompileError>>()?,
            variadic: variadic.clone(),
            return_type: return_type.clone(),
            body: body.clone(),
            is_arrow: *is_arrow,
            is_static: *is_static,
            captures: captures.clone(),
            capture_refs: capture_refs.clone(),
        },
        ExprKind::NamedArg { name, value } => ExprKind::NamedArg {
            name: name.clone(),
            value: Box::new(rewrite_expr(value, class_name, parent_name)?),
        },
        ExprKind::Spread(inner) => {
            ExprKind::Spread(Box::new(rewrite_expr(inner, class_name, parent_name)?))
        }
        ExprKind::ClosureCall { var, args } => ExprKind::ClosureCall {
            var: var.clone(),
            args: rewrite_expr_list(args, class_name, parent_name)?,
        },
        ExprKind::ExprCall { callee, args } => ExprKind::ExprCall {
            callee: Box::new(rewrite_expr(callee, class_name, parent_name)?),
            args: rewrite_expr_list(args, class_name, parent_name)?,
        },
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name: class_name.clone(),
            args: rewrite_expr_list(args, class_name, parent_name)?,
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(rewrite_expr(object, class_name, parent_name)?),
            property: property.clone(),
        },
        ExprKind::DynamicPropertyAccess { object, property } => {
            ExprKind::DynamicPropertyAccess {
                object: Box::new(rewrite_expr(object, class_name, parent_name)?),
                property: Box::new(rewrite_expr(property, class_name, parent_name)?),
            }
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            ExprKind::NullsafePropertyAccess {
                object: Box::new(rewrite_expr(object, class_name, parent_name)?),
                property: property.clone(),
            }
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            ExprKind::NullsafeDynamicPropertyAccess {
                object: Box::new(rewrite_expr(object, class_name, parent_name)?),
                property: Box::new(rewrite_expr(property, class_name, parent_name)?),
            }
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => ExprKind::MethodCall {
            object: Box::new(rewrite_expr(object, class_name, parent_name)?),
            method: method.clone(),
            args: rewrite_expr_list(args, class_name, parent_name)?,
        },
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => ExprKind::NullsafeMethodCall {
            object: Box::new(rewrite_expr(object, class_name, parent_name)?),
            method: method.clone(),
            args: rewrite_expr_list(args, class_name, parent_name)?,
        },
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => ExprKind::StaticMethodCall {
            receiver: receiver.clone(),
            method: method.clone(),
            args: rewrite_expr_list(args, class_name, parent_name)?,
        },
        ExprKind::FirstClassCallable(target) => ExprKind::FirstClassCallable(
            rewrite_callable_target(target, class_name, parent_name)?,
        ),
        ExprKind::PtrCast { target_type, expr } => ExprKind::PtrCast {
            target_type: target_type.clone(),
            expr: Box::new(rewrite_expr(expr, class_name, parent_name)?),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type: element_type.clone(),
            len: Box::new(rewrite_expr(len, class_name, parent_name)?),
        },
        ExprKind::ClassConstant { receiver } => ExprKind::ClassConstant {
            receiver: rewrite_constant_receiver(receiver, class_name, parent_name, expr.span)?,
        },
        ExprKind::ScopedConstantAccess { receiver, name } => {
            ExprKind::ScopedConstantAccess {
                receiver: rewrite_constant_receiver(receiver, class_name, parent_name, expr.span)?,
                name: name.clone(),
            }
        }
        ExprKind::NewScopedObject { receiver, args } => ExprKind::NewScopedObject {
            receiver: receiver.clone(),
            args: rewrite_expr_list(args, class_name, parent_name)?,
        },
        ExprKind::Yield { key, value } => ExprKind::Yield {
            key: key
                .as_ref()
                .map(|expr| rewrite_expr(expr, class_name, parent_name).map(Box::new))
                .transpose()?,
            value: value
                .as_ref()
                .map(|expr| rewrite_expr(expr, class_name, parent_name).map(Box::new))
                .transpose()?,
        },
        ExprKind::YieldFrom(inner) => {
            ExprKind::YieldFrom(Box::new(rewrite_expr(inner, class_name, parent_name)?))
        }
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::This
        | ExprKind::MagicConstant(_) => expr.kind.clone(),
    };
    Ok(Expr::new(kind, expr.span))
}

/// Applies `rewrite_expr` over a list of expressions, returning the transformed list.
fn rewrite_expr_list(
    exprs: &[Expr],
    class_name: &str,
    parent_name: Option<&str>,
) -> Result<Vec<Expr>, CompileError> {
    exprs
        .iter()
        .map(|expr| rewrite_expr(expr, class_name, parent_name))
        .collect()
}

/// Rewrites the target of an `instanceof` expression. If the target is a bare name,
/// it is returned unchanged; if it is an expression, `rewrite_expr` is applied.
fn rewrite_instanceof_target(
    target: &InstanceOfTarget,
    class_name: &str,
    parent_name: Option<&str>,
) -> Result<InstanceOfTarget, CompileError> {
    match target {
        InstanceOfTarget::Name(name) => Ok(InstanceOfTarget::Name(name.clone())),
        InstanceOfTarget::Expr(expr) => Ok(InstanceOfTarget::Expr(Box::new(rewrite_expr(
            expr,
            class_name,
            parent_name,
        )?))),
    }
}

/// Rewrites the target of a first-class callable (e.g. `ClassName::method(...)`).
/// Static method receivers are cloned unchanged; instance method targets have their
/// object expression rewritten.
fn rewrite_callable_target(
    target: &CallableTarget,
    class_name: &str,
    parent_name: Option<&str>,
) -> Result<CallableTarget, CompileError> {
    match target {
        CallableTarget::Function(name) => Ok(CallableTarget::Function(name.clone())),
        CallableTarget::StaticMethod { receiver, method } => Ok(CallableTarget::StaticMethod {
            receiver: receiver.clone(),
            method: method.clone(),
        }),
        CallableTarget::Method { object, method } => Ok(CallableTarget::Method {
            object: Box::new(rewrite_expr(object, class_name, parent_name)?),
            method: method.clone(),
        }),
    }
}

/// Resolves a constant receiver (`self::`, `parent::`, `static::`, or a bare name).
/// Returns a `StaticReceiver::Named` with the fully-qualified name, or an error for
/// `static::` or an unresolvable `parent::`.
fn rewrite_constant_receiver(
    receiver: &StaticReceiver,
    class_name: &str,
    parent_name: Option<&str>,
    span: Span,
) -> Result<StaticReceiver, CompileError> {
    match receiver {
        StaticReceiver::Named(name) => Ok(StaticReceiver::Named(name.clone())),
        StaticReceiver::Self_ => Ok(StaticReceiver::Named(fqn_name(class_name))),
        StaticReceiver::Parent => parent_name
            .map(fqn_name)
            .map(StaticReceiver::Named)
            .ok_or_else(|| {
                CompileError::new(span, &format!("Class '{}' has no parent class", class_name))
            }),
        StaticReceiver::Static => Err(CompileError::new(
            span,
            "Cannot use static:: in class constant expression",
        )),
    }
}

/// Builds a fully-qualified `Name` from a class name string, splitting on `\\` and
/// marking the resulting parts as fully-qualified.
fn fqn_name(name: &str) -> Name {
    Name::from_parts(
        NameKind::FullyQualified,
        name.split('\\').map(str::to_string).collect(),
    )
}
