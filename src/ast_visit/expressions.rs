//! Purpose:
//! Walks expressions and their nested AST positions.
//!
//! Called from:
//! - The shared AST visitor traversal.
//!
//! Key details:
//! - Recursion remains exhaustive and never prunes unreachable source.

use super::*;

/// Visits an argument list.
pub(super) fn walk_exprs(exprs: &[Expr], visitor: &mut impl AstVisitor) {
    for expr in exprs {
        walk_expr(expr, visitor);
    }
}

/// Visits one expression, its call targets, and every nested operand.
pub(super) fn walk_expr(expr: &Expr, visitor: &mut impl AstVisitor) {
    visitor.expression(expr);
    let span = expr.span;
    match &expr.kind {
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
        | ExprKind::This
        | ExprKind::MagicConstant(_) => {}
        // A receiver is a class position, and `Box<int>::L` is not PHP even when the class is
        // declared in another file. Auditing only the DECLARATION would accept a file php-src
        // cannot parse, which is the one thing this flag exists to prevent.
        ExprKind::ClassConstant { receiver }
        | ExprKind::ScopedConstantAccess { receiver, name: _ } => {
            walk_static_receiver(receiver, span, visitor)
        }
        ExprKind::BinaryOp { left, op: _, right } => {
            walk_expr(left, visitor);
            walk_expr(right, visitor);
        }
        ExprKind::InstanceOf { value, target } => {
            walk_expr(value, visitor);
            match target {
                InstanceOfTarget::Name(_) => {}
                // `$x instanceof Box<int>` is not PHP even when `Box` is declared elsewhere.
                InstanceOfTarget::Generic(class_type) => walk_type(class_type, span, visitor),
                InstanceOfTarget::Expr(target_expr) => walk_expr(target_expr, visitor),
            }
        }
        ExprKind::UnaryPlus(inner) | ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::Clone(inner)
        | ExprKind::ObjectClassName { object: inner }
        | ExprKind::YieldFrom(inner) => walk_expr(inner, visitor),
        ExprKind::NullCoalesce { value, default } => {
            walk_expr(value, visitor);
            walk_expr(default, visitor);
        }
        ExprKind::Pipe { value, callable } => {
            walk_expr(value, visitor);
            walk_expr(callable, visitor);
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp: _,
        } => {
            walk_expr(target, visitor);
            walk_expr(value, visitor);
            if let Some(result_target) = result_target {
                walk_expr(result_target, visitor);
            }
            walk_stmts(prelude, visitor);
        }
        ExprKind::FunctionCall { name, args } => {
            visitor.function_name(span, name);
            walk_exprs(args, visitor);
        }
        ExprKind::ArrayLiteral(items) => walk_exprs(items, visitor),
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs {
                walk_expr(key, visitor);
                walk_expr(value, visitor);
            }
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            for entry in entries.iter() {
                for expr in entry.exprs() {
                    walk_expr(expr, visitor);
                }
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            walk_expr(subject, visitor);
            for (conditions, arm_value) in arms {
                walk_exprs(conditions, visitor);
                walk_expr(arm_value, visitor);
            }
            if let Some(default) = default {
                walk_expr(default, visitor);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            walk_expr(array, visitor);
            walk_expr(index, visitor);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            walk_expr(condition, visitor);
            walk_expr(then_expr, visitor);
            walk_expr(else_expr, visitor);
        }
        ExprKind::ShortTernary { value, default } => {
            walk_expr(value, visitor);
            walk_expr(default, visitor);
        }
        ExprKind::Cast { target: _, expr } => walk_expr(expr, visitor),
        ExprKind::Closure {
            params,
            variadic: _,
            variadic_by_ref: _,
            variadic_type,
            return_type,
            body,
            is_arrow: _,
            is_static: _,
            by_ref_return: _,
            captures: _,
            capture_refs: _,
        } => {
            walk_params(params, span, visitor);
            if let Some(variadic_type) = variadic_type {
                walk_type(variadic_type, span, visitor);
            }
            if let Some(return_type) = return_type {
                walk_type(return_type, span, visitor);
            }
            walk_stmts(body, visitor);
        }
        ExprKind::NamedArg { name: _, value } => walk_expr(value, visitor),
        ExprKind::IncludeValue {
            path,
            once: _,
            required: _,
        } => walk_expr(path, visitor),
        ExprKind::ClosureCall { var: _, args } => walk_exprs(args, visitor),
        ExprKind::ExprCall { callee, args } => {
            walk_expr(callee, visitor);
            walk_exprs(args, visitor);
        }
        ExprKind::NewObject {
            class_name: _,
            args,
        } => walk_exprs(args, visitor),
        // `new Box<int>()` is not PHP: php-src reads the `<` as a comparison. The audit rejects
        // the type itself so the message names the extension rather than the parse.
        ExprKind::NewGeneric { class_type, args } => {
            walk_type(class_type, expr.span, visitor);
            walk_exprs(args, visitor);
        }
        ExprKind::NewDynamic { name_expr, args } => {
            walk_expr(name_expr, visitor);
            walk_exprs(args, visitor);
        }
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class: _,
            required_parent: _,
            args,
        } => {
            walk_expr(class_name, visitor);
            walk_exprs(args, visitor);
        }
        ExprKind::PropertyAccess {
            object,
            property: _,
        }
        | ExprKind::NullsafePropertyAccess {
            object,
            property: _,
        } => walk_expr(object, visitor),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            walk_expr(object, visitor);
            walk_expr(property, visitor);
        }
        ExprKind::StaticPropertyAccess {
            receiver,
            property: _,
        } => walk_static_receiver(receiver, span, visitor),
        ExprKind::MethodCall {
            object,
            method: _,
            args,
        }
        | ExprKind::NullsafeMethodCall {
            object,
            method: _,
            args,
        } => {
            walk_expr(object, visitor);
            walk_exprs(args, visitor);
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            walk_expr(object, visitor);
            walk_expr(method, visitor);
            walk_exprs(args, visitor);
        }
        ExprKind::StaticMethodCall {
            receiver,
            method: _,
            args,
        } => {
            walk_static_receiver(receiver, span, visitor);
            walk_exprs(args, visitor);
        }
        ExprKind::FirstClassCallable(target) => match target {
            CallableTarget::Function(name) => visitor.function_name(span, name),
            CallableTarget::StaticMethod {
                receiver,
                method: _,
            } => walk_static_receiver(receiver, span, visitor),
            CallableTarget::Method { object, method: _ } => walk_expr(object, visitor),
        },
        ExprKind::PtrCast {
            target_type: _,
            expr,
        } => {
            extension(
                visitor,
                span,
                "`ptr_cast<T>` is an elephc extension and is not valid PHP",
            );
            walk_expr(expr, visitor);
        }
        ExprKind::BufferNew { element_type, len } => {
            extension(
                visitor,
                span,
                "`buffer_new<T>` is an elephc extension and is not valid PHP",
            );
            walk_type(element_type, span, visitor);
            walk_expr(len, visitor);
        }
        ExprKind::NewScopedObject { receiver, args } => {
            walk_static_receiver(receiver, span, visitor);
            walk_exprs(args, visitor);
        }
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                walk_expr(key, visitor);
            }
            if let Some(value) = value {
                walk_expr(value, visitor);
            }
        }
    }
}

