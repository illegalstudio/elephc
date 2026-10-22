//! Purpose:
//! Decides whether a method body depends on late static binding, by a typed walk of its AST.
//!
//! Called from:
//! - `crate::ir_lower::expr::dynamic_class_access`, before calling an inherited private static
//!   method on its declaring class (where `static` would bind to the declaring class instead of
//!   the receiver's runtime class).
//!
//! Key details:
//! - Tainted: any `static` (`static::x`, `static::$p`, `static::class`, `new static`,
//!   `instanceof static`), any `self::`/`parent::`/`static::` method call (they forward the
//!   called class), and `get_called_class()`. Closures are entered: they inherit the scope.
//! - Deliberately not transitive: a forwarding call taints by itself, so no callee is analysed.
//! - The matches are exhaustive, so a new AST node must be classified here explicitly.

use crate::parser::ast::{CallableTarget, Expr, ExprKind, InstanceOfTarget, StaticReceiver, Stmt, StmtKind};

/// Returns true when `body` may observe the called class.
pub(super) fn body_uses_late_static_binding(body: &[Stmt]) -> bool {
    body.iter().any(stmt_tainted)
}

/// Returns true for `static` (the late-bound receiver).
fn is_static(receiver: &StaticReceiver) -> bool {
    matches!(receiver, StaticReceiver::Static)
}

/// Returns true for a receiver whose method call forwards the called class.
fn forwards(receiver: &StaticReceiver) -> bool {
    matches!(receiver, StaticReceiver::Static | StaticReceiver::Self_ | StaticReceiver::Parent)
}

/// Returns true when one statement may observe the called class: a `static::` write target,
/// or any tainting expression in its own expressions or nested bodies (blocks, loops, `try`,
/// `switch`). Nested function and class declarations stop the walk, since their bodies run in
/// their own scope.
fn stmt_tainted(stmt: &Stmt) -> bool {
    let expr = expr_tainted;
    let body = body_uses_late_static_binding;
    match &stmt.kind {
        StmtKind::StaticPropertyAssign { receiver, value, .. }
        | StmtKind::StaticPropertyArrayPush { receiver, value, .. } => {
            is_static(receiver) || expr(value)
        }
        StmtKind::StaticPropertyArrayAssign { receiver, index, value, .. } => {
            is_static(receiver) || expr(index) || expr(value)
        }
        StmtKind::Echo(value) | StmtKind::Throw(value) | StmtKind::ExprStmt(value)
        | StmtKind::Assign { value, .. } | StmtKind::TypedAssign { value, .. }
        | StmtKind::ArrayPush { value, .. } | StmtKind::ConstDecl { value, .. }
        | StmtKind::ListUnpack { value, .. } | StmtKind::StaticVar { init: value, .. }
        | StmtKind::RefAssign { source: value, .. } | StmtKind::Include { path: value, .. } => {
            expr(value)
        }
        StmtKind::Return(value) => value.as_ref().is_some_and(expr),
        StmtKind::ArrayAssign { index, value, .. } => expr(index) || expr(value),
        StmtKind::NestedArrayAssign { target, value } => expr(target) || expr(value),
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => expr(object) || expr(value),
        StmtKind::PropertyArrayAssign { object, index, value, .. } => {
            expr(object) || expr(index) || expr(value)
        }
        StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
            expr(condition)
                || body(then_body)
                || elseif_clauses.iter().any(|(condition, statements)| expr(condition) || body(statements))
                || else_body.as_deref().is_some_and(body)
        }
        StmtKind::IfDef { then_body, else_body, .. } => {
            body(then_body) || else_body.as_deref().is_some_and(body)
        }
        StmtKind::While { condition, body: statements }
        | StmtKind::DoWhile { condition, body: statements } => expr(condition) || body(statements),
        StmtKind::For { init, condition, update, body: statements } => {
            init.as_deref().is_some_and(stmt_tainted)
                || condition.as_ref().is_some_and(expr)
                || update.as_deref().is_some_and(stmt_tainted)
                || body(statements)
        }
        StmtKind::Foreach { array, body: statements, .. } => expr(array) || body(statements),
        StmtKind::Switch { subject, cases, default } => {
            expr(subject)
                || cases.iter().any(|(conditions, statements)| conditions.iter().any(expr) || body(statements))
                || default.as_deref().is_some_and(body)
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            body(try_body)
                || catches.iter().any(|catch| body(&catch.body))
                || finally_body.as_deref().is_some_and(body)
        }
        StmtKind::Synthetic(statements)
        | StmtKind::IncludeOnceGuard { body: statements, .. }
        | StmtKind::NamespaceBlock { body: statements, .. } => body(statements),
        StmtKind::Break(_) | StmtKind::Continue(_) | StmtKind::Global { .. }
        | StmtKind::IncludeOnceMark { .. } | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. } | StmtKind::FunctionDecl { .. }
        | StmtKind::FunctionVariantGroup { .. } | StmtKind::FunctionVariantMark { .. }
        | StmtKind::ClassDecl { .. } | StmtKind::EnumDecl { .. }
        | StmtKind::PackedClassDecl { .. } | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. } | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. } | StmtKind::ExternGlobalDecl { .. } => false,
    }
}

/// Returns true when an expression may observe the called class: a `static` receiver
/// (`static::X`, `static::$p`, `static::class`, `new static`, `instanceof static`), a forwarding
/// `self::`/`parent::`/`static::` method call, or `get_called_class()`, found anywhere in its
/// operands and in the bodies of closures it creates.
fn expr_tainted(value: &Expr) -> bool {
    let expr = expr_tainted;
    let args = |values: &[Expr]| values.iter().any(expr_tainted);
    match &value.kind {
        ExprKind::StaticPropertyAccess { receiver, .. }
        | ExprKind::ScopedConstantAccess { receiver, .. }
        | ExprKind::ClassConstant { receiver } => is_static(receiver),
        ExprKind::NewScopedObject { receiver, args: values } => is_static(receiver) || args(values),
        ExprKind::StaticMethodCall { receiver, args: values, .. } => forwards(receiver) || args(values),
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, .. }) => forwards(receiver),
        ExprKind::FunctionCall { name, args: values } => {
            crate::names::php_symbol_key(name.as_str().trim_start_matches('\\')) == "get_called_class"
                || args(values)
        }
        ExprKind::InstanceOf { value, target } => {
            expr(value)
                || match target {
                    InstanceOfTarget::Name(name) => name.as_str().eq_ignore_ascii_case("static"),
                    InstanceOfTarget::Generic(_) => false,
                    InstanceOfTarget::Expr(target) => expr(target),
                }
        }
        // A closure inherits the method's called class.
        ExprKind::Closure { body, .. } => body_uses_late_static_binding(body),
        ExprKind::NewGeneric { args: values, .. } => args(values),
        ExprKind::PropertyAccess { object, .. } | ExprKind::NullsafePropertyAccess { object, .. } => {
            expr(object)
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => expr(object) || expr(property),
        ExprKind::BinaryOp { left, right, .. } => expr(left) || expr(right),
        ExprKind::ArrayAccess { array, index } => expr(array) || expr(index),
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            expr(value) || expr(default)
        }
        ExprKind::Pipe { value, callable } => expr(value) || expr(callable),
        ExprKind::Assignment { target, value, result_target, prelude, .. } => {
            expr(target)
                || expr(value)
                || result_target.as_deref().is_some_and(expr)
                || body_uses_late_static_binding(prelude)
        }
        ExprKind::Negate(value) | ExprKind::Not(value) | ExprKind::BitNot(value)
        | ExprKind::Throw(value) | ExprKind::ErrorSuppress(value) | ExprKind::Print(value)
        | ExprKind::Spread(value) | ExprKind::Clone(value) | ExprKind::YieldFrom(value)
        | ExprKind::Cast { expr: value, .. } | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::NamedArg { value, .. } | ExprKind::IncludeValue { path: value, .. }
        | ExprKind::BufferNew { len: value, .. } | ExprKind::ObjectClassName { object: value } => expr(value),
        ExprKind::ArrayLiteral(values) | ExprKind::ClosureCall { args: values, .. }
        | ExprKind::NewObject { args: values, .. } => args(values),
        ExprKind::ExprCall { callee, args: values }
        | ExprKind::NewDynamic { name_expr: callee, args: values }
        | ExprKind::NewDynamicObject { class_name: callee, args: values, .. }
        | ExprKind::MethodCall { object: callee, args: values, .. }
        | ExprKind::NullsafeMethodCall { object: callee, args: values, .. } => expr(callee) || args(values),
        ExprKind::NullsafeDynamicMethodCall { object, method, args: values } => {
            expr(object) || expr(method) || args(values)
        }
        ExprKind::ArrayLiteralAssoc(entries) => entries.iter().any(|(key, value)| expr(key) || expr(value)),
        ExprKind::ArrayLiteralMixed(entries) => entries.iter().flat_map(|entry| entry.exprs()).any(expr),
        ExprKind::Match { subject, arms, default } => {
            expr(subject)
                || arms.iter().any(|(conditions, value)| args(conditions) || expr(value))
                || default.as_deref().is_some_and(expr)
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            expr(condition) || expr(then_expr) || expr(else_expr)
        }
        ExprKind::FirstClassCallable(CallableTarget::Method { object, .. }) => expr(object),
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(expr) || value.as_deref().is_some_and(expr)
        }
        ExprKind::StringLiteral(_) | ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_) | ExprKind::BoolLiteral(_) | ExprKind::Null | ExprKind::This
        | ExprKind::PreIncrement(_) | ExprKind::PostIncrement(_) | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_) | ExprKind::ConstRef(_) | ExprKind::MagicConstant(_)
        | ExprKind::FirstClassCallable(CallableTarget::Function(_)) => false,
    }
}
