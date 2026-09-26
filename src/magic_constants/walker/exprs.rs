//! Purpose:
//! Walks expression AST nodes for magic-constant substitution passes.
//! Recurses through calls, literals, access forms, closures, match arms, assignments, and nested statements.
//!
//! Called from:
//! - `crate::magic_constants::walker::stmts` and member walkers.
//!
//! Key details:
//! - Expression traversal must cover every `ExprKind` so raw magic constants cannot reach later passes.

use crate::parser::ast::{CallableTarget, Expr, ExprKind, InstanceOfTarget};

use crate::span::Span;

use super::stmts::{walk_program, walk_stmt};
use super::{walk_static_receiver, Pass};

/// Recursively walks an expression AST, applying `pass` transformations to magic constants and
/// string literals, and recursing into all expression subtrees.
///
/// Returns a new `Expr` with transformed `MagicConstant` and `StringLiteral` nodes, and all
/// child expressions recursively processed. Other leaf variants are returned unchanged.
pub(super) fn walk_expr<P: Pass>(expr: Expr, pass: &mut P) -> Expr {
    let span = expr.span;
    let kind = match expr.kind {
        ExprKind::MagicConstant(mc) => pass.transform_magic(span, mc),

        ExprKind::StringLiteral(value) => pass.transform_string(value),

        // Leaves with no Expr subtrees:
        kind @ (ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::Variable(_)
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::This) => kind,

        // Not a leaf after all: its receiver can name a generic class, which is a type position.
        ExprKind::StaticPropertyAccess { receiver, property } => ExprKind::StaticPropertyAccess {
            receiver: walk_static_receiver(receiver, pass, span),
            property,
        },

        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
            left: Box::new(walk_expr(*left, pass)),
            op,
            right: Box::new(walk_expr(*right, pass)),
        },
        ExprKind::InstanceOf { value, target } => ExprKind::InstanceOf {
            value: Box::new(walk_expr(*value, pass)),
            target: walk_instanceof_target(target, pass, span),
        },
        ExprKind::Negate(inner) => ExprKind::Negate(Box::new(walk_expr(*inner, pass))),
        ExprKind::Not(inner) => ExprKind::Not(Box::new(walk_expr(*inner, pass))),
        ExprKind::BitNot(inner) => ExprKind::BitNot(Box::new(walk_expr(*inner, pass))),
        ExprKind::Throw(inner) => ExprKind::Throw(Box::new(walk_expr(*inner, pass))),
        ExprKind::Clone(inner) => ExprKind::Clone(Box::new(walk_expr(*inner, pass))),
        ExprKind::ErrorSuppress(inner) => ExprKind::ErrorSuppress(Box::new(walk_expr(*inner, pass))),
        ExprKind::Print(inner) => ExprKind::Print(Box::new(walk_expr(*inner, pass))),
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(walk_expr(*value, pass)),
            default: Box::new(walk_expr(*default, pass)),
        },
        ExprKind::Pipe { value, callable } => ExprKind::Pipe {
            value: Box::new(walk_expr(*value, pass)),
            callable: Box::new(walk_expr(*callable, pass)),
        },
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp,
        } => ExprKind::Assignment {
            target: Box::new(walk_expr(*target, pass)),
            value: Box::new(walk_expr(*value, pass)),
            result_target: result_target.map(|target| Box::new(walk_expr(*target, pass))),
            prelude: prelude.into_iter().map(|stmt| walk_stmt(stmt, pass)).collect(),
            conditional_value_temp,
        },
        ExprKind::FunctionCall { name, args } => ExprKind::FunctionCall {
            name,
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::ArrayLiteral(items) => {
            ExprKind::ArrayLiteral(items.into_iter().map(|i| walk_expr(i, pass)).collect())
        }
        ExprKind::ArrayLiteralAssoc(pairs) => ExprKind::ArrayLiteralAssoc(
            pairs
                .into_iter()
                .map(|(k, v)| (walk_expr(k, pass), walk_expr(v, pass)))
                .collect(),
        ),
        ExprKind::ArrayLiteralMixed(entries) => ExprKind::ArrayLiteralMixed(
            entries
                .into_iter()
                .map(|entry| entry.map_exprs(|expr| walk_expr(expr, pass)))
                .collect(),
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => ExprKind::Match {
            subject: Box::new(walk_expr(*subject, pass)),
            arms: arms
                .into_iter()
                .map(|(patterns, value)| {
                    (
                        patterns.into_iter().map(|p| walk_expr(p, pass)).collect(),
                        walk_expr(value, pass),
                    )
                })
                .collect(),
            default: default.map(|d| Box::new(walk_expr(*d, pass))),
        },
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(walk_expr(*array, pass)),
            index: Box::new(walk_expr(*index, pass)),
        },
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ternary {
            condition: Box::new(walk_expr(*condition, pass)),
            then_expr: Box::new(walk_expr(*then_expr, pass)),
            else_expr: Box::new(walk_expr(*else_expr, pass)),
        },
        ExprKind::ShortTernary { value, default } => ExprKind::ShortTernary {
            value: Box::new(walk_expr(*value, pass)),
            default: Box::new(walk_expr(*default, pass)),
        },
        ExprKind::Cast { target, expr: inner } => ExprKind::Cast {
            target,
            expr: Box::new(walk_expr(*inner, pass)),
        },
        ExprKind::Closure {
            params,
            variadic,
            variadic_by_ref,
            variadic_type,
            return_type,
            body,
            is_arrow,
            is_static,
            captures,
            capture_refs,
            by_ref_return,
        } => {
            pass.enter_closure(span);
            let new_params = params
                .into_iter()
                .map(|(n, t, default, by_ref)| {
                    (
                        n,
                        t.map(|ty| pass.transform_type(ty, span)),
                        default.map(|d| walk_expr(d, pass)),
                        by_ref,
                    )
                })
                .collect();
            let new_body = walk_program(body, pass);
            // Before `leave_closure`, for the reason the function arm documents: a struct
            // literal's fields are evaluated after the statements above it.
            let new_variadic_type = variadic_type.map(|ty| pass.transform_type(ty, span));
            let new_return_type = return_type.map(|ty| pass.transform_type(ty, span));
            pass.leave_closure();
            ExprKind::Closure {
                params: new_params,
                variadic,
                variadic_by_ref,
                variadic_type: new_variadic_type,
                return_type: new_return_type,
                body: new_body,
                is_arrow,
                is_static,
                captures,
                capture_refs,
                by_ref_return,
            }
        }
        ExprKind::NamedArg { name, value } => ExprKind::NamedArg {
            name,
            value: Box::new(walk_expr(*value, pass)),
        },
        ExprKind::IncludeValue {
            path,
            once,
            required,
        } => ExprKind::IncludeValue {
            path: Box::new(walk_expr(*path, pass)),
            once,
            required,
        },
        ExprKind::Spread(inner) => ExprKind::Spread(Box::new(walk_expr(*inner, pass))),
        ExprKind::ClosureCall { var, args } => ExprKind::ClosureCall {
            var,
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::ExprCall { callee, args } => ExprKind::ExprCall {
            callee: Box::new(walk_expr(*callee, pass)),
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name: pass.transform_class_reference(class_name, span),
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::NewGeneric { class_type, args } => {
            let class_type = pass.transform_type(class_type, span);
            let args: Vec<Expr> = args.into_iter().map(|a| walk_expr(a, pass)).collect();
            // A construction whose type is no longer generic is an ordinary construction, and
            // collapsing it HERE is what keeps `NewGeneric` out of every later pass: the
            // instantiating pass only has to answer "what does this type become", never "and
            // which expression node should replace it".
            match class_type {
                crate::parser::ast::TypeExpr::Named(class_name) => {
                    ExprKind::NewObject { class_name, args }
                }
                class_type => ExprKind::NewGeneric { class_type, args },
            }
        }
        ExprKind::NewDynamic { name_expr, args } => ExprKind::NewDynamic {
            name_expr: Box::new(walk_expr(*name_expr, pass)),
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class,
            required_parent,
            args,
        } => ExprKind::NewDynamicObject {
            class_name: Box::new(walk_expr(*class_name, pass)),
            fallback_class,
            required_parent,
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(walk_expr(*object, pass)),
            property,
        },
        ExprKind::DynamicPropertyAccess { object, property } => {
            ExprKind::DynamicPropertyAccess {
                object: Box::new(walk_expr(*object, pass)),
                property: Box::new(walk_expr(*property, pass)),
            }
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            ExprKind::NullsafePropertyAccess {
                object: Box::new(walk_expr(*object, pass)),
                property,
            }
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            ExprKind::NullsafeDynamicPropertyAccess {
                object: Box::new(walk_expr(*object, pass)),
                property: Box::new(walk_expr(*property, pass)),
            }
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => ExprKind::MethodCall {
            object: Box::new(walk_expr(*object, pass)),
            method: pass.transform_method_call_name(method, span),
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => ExprKind::NullsafeMethodCall {
            object: Box::new(walk_expr(*object, pass)),
            method,
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => ExprKind::NullsafeDynamicMethodCall {
            object: Box::new(walk_expr(*object, pass)),
            method: Box::new(walk_expr(*method, pass)),
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => ExprKind::StaticMethodCall {
            receiver: walk_static_receiver(receiver, pass, span),
            // Renamed like the instance form: a generic method resolved by the checker is called
            // under its INSTANTIATED name, and leaving the static spelling alone left the backend
            // looking for a template that was stripped from the program. A call the checker
            // recorded nothing for is keyed by nothing and passes through unchanged.
            method: pass.transform_method_call_name(method, span),
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::FirstClassCallable(target) => {
            ExprKind::FirstClassCallable(walk_callable_target(target, pass, span))
        }
        ExprKind::PtrCast { target_type, expr: inner } => ExprKind::PtrCast {
            target_type,
            expr: Box::new(walk_expr(*inner, pass)),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type: pass.transform_type(element_type, span),
            len: Box::new(walk_expr(*len, pass)),
        },
        ExprKind::ClassConstant { receiver } => ExprKind::ClassConstant {
            receiver: walk_static_receiver(receiver, pass, span),
        },
        ExprKind::ObjectClassName { object } => ExprKind::ObjectClassName {
            object: Box::new(walk_expr(*object, pass)),
        },
        ExprKind::ScopedConstantAccess { receiver, name } => ExprKind::ScopedConstantAccess {
            receiver: walk_static_receiver(receiver, pass, span),
            name,
        },
        ExprKind::NewScopedObject { receiver, args } => ExprKind::NewScopedObject {
            receiver: walk_static_receiver(receiver, pass, span),
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::Yield { key, value } => ExprKind::Yield {
            key: key.map(|k| Box::new(walk_expr(*k, pass))),
            value: value.map(|v| Box::new(walk_expr(*v, pass))),
        },
        ExprKind::YieldFrom(inner) => ExprKind::YieldFrom(Box::new(walk_expr(*inner, pass))),
    };
    Expr { kind, span }
}

/// Transforms a `CallableTarget` by recursively walking any boxed expression inside it.
///
/// `CallableTarget::Method` carries a boxed object expression that must be walked;
/// `CallableTarget::Function` carries nothing; `CallableTarget::StaticMethod` carries a receiver
/// that may name a generic class and so is a type position.
fn walk_callable_target<P: Pass>(
    target: CallableTarget,
    pass: &mut P,
    span: Span,
) -> CallableTarget {
    match target {
        CallableTarget::Method { object, method } => CallableTarget::Method {
            object: Box::new(walk_expr(*object, pass)),
            method,
        },
        CallableTarget::Function(name) => CallableTarget::Function(name),
        CallableTarget::StaticMethod { receiver, method } => CallableTarget::StaticMethod {
            receiver: walk_static_receiver(receiver, pass, span),
            method,
        },
    }
}

/// Transforms an `InstanceOfTarget` by recursively walking any boxed expression inside it.
///
/// `InstanceOfTarget::Expr` carries a boxed expression operand that must be walked;
/// `InstanceOfTarget::Name` carries no expression and is returned unchanged.
fn walk_instanceof_target<P: Pass>(
    target: InstanceOfTarget,
    pass: &mut P,
    span: Span,
) -> InstanceOfTarget {
    match target {
        InstanceOfTarget::Name(name) => InstanceOfTarget::Name(name),
        // A target that is no longer generic is an ordinary named target, and collapsing it
        // here keeps `InstanceOfTarget::Generic` out of every pass after instantiation.
        InstanceOfTarget::Generic(class_type) => {
            match pass.transform_type(class_type, span) {
                crate::parser::ast::TypeExpr::Named(name) => InstanceOfTarget::Name(name),
                class_type => InstanceOfTarget::Generic(class_type),
            }
        }
        InstanceOfTarget::Expr(expr) => {
            InstanceOfTarget::Expr(Box::new(walk_expr(*expr, pass)))
        }
    }
}
