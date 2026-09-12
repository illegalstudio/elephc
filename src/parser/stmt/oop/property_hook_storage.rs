//! Purpose:
//! Detects backing-value references in the original bodies of PHP property hooks.
//!
//! Called from:
//! - `super::body::parse_property_hooks()` before optimizer rewrites can remove syntax.
//!
//! Key details:
//! - Only direct `$this->property` accesses establish backing storage, not variable names.
//! - Reads, writes, and branches are inspected without crossing function or class boundaries.

use crate::parser::ast::{CallableTarget, Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind};

/// Checks original statements for a syntactic reference to one hook's own property.
pub(super) fn body_uses_backing_slot(body: &[Stmt], property: &str) -> bool {
    body.iter().any(|stmt| stmt_uses_backing_slot(stmt, property))
}

/// Recognizes a direct access to the backing property on the hook's bound receiver.
fn is_backing_property(object: &Expr, name: &str, property: &str) -> bool {
    matches!(object.kind, ExprKind::This) && name == property
}

/// Visits executable statements without crossing into a separately declared class or function.
fn stmt_uses_backing_slot(stmt: &Stmt, property: &str) -> bool {
    let expr = |value: &Expr| expr_uses_backing_slot(value, property);
    let body = |statements: &[Stmt]| body_uses_backing_slot(statements, property);
    match &stmt.kind {
        StmtKind::Echo(value) | StmtKind::Throw(value) | StmtKind::ExprStmt(value)
        | StmtKind::Assign { value, .. } | StmtKind::TypedAssign { value, .. }
        | StmtKind::ArrayPush { value, .. } | StmtKind::ConstDecl { value, .. }
        | StmtKind::ListUnpack { value, .. } | StmtKind::StaticVar { init: value, .. }
        | StmtKind::RefAssign { source: value, .. } | StmtKind::Include { path: value, .. }
        | StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => expr(value),
        StmtKind::Return(value) => value.as_ref().is_some_and(expr),
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. } => expr(index) || expr(value),
        StmtKind::NestedArrayAssign { target, value } => expr(target) || expr(value),
        StmtKind::PropertyAssign { object, property: name, value }
        | StmtKind::PropertyArrayPush { object, property: name, value } => {
            is_backing_property(object, name, property) || expr(object) || expr(value)
        }
        StmtKind::PropertyArrayAssign { object, property: name, index, value } => {
            is_backing_property(object, name, property)
                || expr(object) || expr(index) || expr(value)
        }
        StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
            expr(condition) || body(then_body)
                || elseif_clauses.iter().any(|(condition, statements)| expr(condition) || body(statements))
                || else_body.as_deref().is_some_and(body)
        }
        StmtKind::IfDef { then_body, else_body, .. } => {
            body(then_body) || else_body.as_deref().is_some_and(body)
        }
        StmtKind::While { condition, body: statements }
        | StmtKind::DoWhile { condition, body: statements } => expr(condition) || body(statements),
        StmtKind::For { init, condition, update, body: statements } => {
            init.as_deref().is_some_and(|stmt| stmt_uses_backing_slot(stmt, property))
                || condition.as_ref().is_some_and(expr)
                || update.as_deref().is_some_and(|stmt| stmt_uses_backing_slot(stmt, property))
                || body(statements)
        }
        StmtKind::Foreach { array, body: statements, .. } => expr(array) || body(statements),
        StmtKind::Switch { subject, cases, default } => {
            expr(subject)
                || cases.iter().any(|(conditions, statements)| conditions.iter().any(expr) || body(statements))
                || default.as_deref().is_some_and(body)
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            body(try_body) || catches.iter().any(|catch| body(&catch.body))
                || finally_body.as_deref().is_some_and(body)
        }
        StmtKind::Synthetic(statements) | StmtKind::IncludeOnceGuard { body: statements, .. }
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

/// Visits expression operands and assignment preludes without entering nested closures.
fn expr_uses_backing_slot(value: &Expr, property: &str) -> bool {
    let expr = |value: &Expr| expr_uses_backing_slot(value, property);
    let args = |values: &[Expr]| values.iter().any(expr);
    match &value.kind {
        ExprKind::PropertyAccess { object, property: name }
        | ExprKind::NullsafePropertyAccess { object, property: name } => {
            is_backing_property(object, name, property) || expr(object)
        }
        ExprKind::DynamicPropertyAccess { object, property: name }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property: name } => {
            (matches!(&name.kind, ExprKind::StringLiteral(name) if is_backing_property(object, name, property)))
                || expr(object) || expr(name)
        }
        ExprKind::BinaryOp { left, right, .. } => expr(left) || expr(right),
        ExprKind::ArrayAccess { array, index } => expr(array) || expr(index),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => expr(value) || expr(default),
        ExprKind::Pipe { value, callable } => expr(value) || expr(callable),
        ExprKind::Assignment { target, value, result_target, prelude, .. } => {
            expr(target) || expr(value) || result_target.as_deref().is_some_and(expr)
                || body_uses_backing_slot(prelude, property)
        }
        ExprKind::InstanceOf { value, target } => {
            expr(value) || match target {
                InstanceOfTarget::Name(_) => false,
                InstanceOfTarget::Expr(target) => expr(target),
            }
        }
        ExprKind::Negate(value) | ExprKind::Not(value) | ExprKind::BitNot(value)
        | ExprKind::Throw(value) | ExprKind::ErrorSuppress(value) | ExprKind::Print(value)
        | ExprKind::Spread(value) | ExprKind::Clone(value) | ExprKind::YieldFrom(value)
        | ExprKind::Cast { expr: value, .. } | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::NamedArg { value, .. } | ExprKind::IncludeValue { path: value, .. }
        | ExprKind::BufferNew { len: value, .. } | ExprKind::ObjectClassName { object: value } => expr(value),
        ExprKind::FunctionCall { args: values, .. } | ExprKind::ArrayLiteral(values)
        | ExprKind::ClosureCall { args: values, .. } | ExprKind::NewObject { args: values, .. }
        | ExprKind::StaticMethodCall { args: values, .. }
        | ExprKind::NewScopedObject { args: values, .. } => args(values),
        ExprKind::ExprCall { callee, args: values }
        | ExprKind::NewDynamic { name_expr: callee, args: values }
        | ExprKind::NewDynamicObject { class_name: callee, args: values, .. }
        | ExprKind::MethodCall { object: callee, args: values, .. }
        | ExprKind::NullsafeMethodCall { object: callee, args: values, .. } => expr(callee) || args(values),
        ExprKind::NullsafeDynamicMethodCall { object, method, args: values } => {
            expr(object) || expr(method) || args(values)
        }
        ExprKind::ArrayLiteralAssoc(entries) => entries.iter().any(|(key, value)| expr(key) || expr(value)),
        ExprKind::Match { subject, arms, default } => {
            expr(subject) || arms.iter().any(|(conditions, value)| args(conditions) || expr(value))
                || default.as_deref().is_some_and(expr)
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            expr(condition) || expr(then_expr) || expr(else_expr)
        }
        ExprKind::Closure { .. } => false,
        ExprKind::FirstClassCallable(CallableTarget::Method { object, .. }) => expr(object),
        ExprKind::Yield { key, value } => key.as_deref().is_some_and(expr) || value.as_deref().is_some_and(expr),
        ExprKind::StringLiteral(_) | ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_) | ExprKind::BoolLiteral(_) | ExprKind::Null | ExprKind::This
        | ExprKind::PreIncrement(_) | ExprKind::PostIncrement(_) | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_) | ExprKind::ConstRef(_) | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::ClassConstant { .. } | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) | ExprKind::FirstClassCallable(CallableTarget::Function(_))
        | ExprKind::FirstClassCallable(CallableTarget::StaticMethod { .. }) => false,
    }
}
