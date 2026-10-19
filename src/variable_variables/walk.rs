//! Purpose:
//! Exhaustive child traversal of statements and expressions for the variable-variable passes.
//!
//! Called from:
//! - `super::scope` (frame-name collection) and `super::rewrite` (the desugar itself).
//!
//! Key details:
//! - The matches are exhaustive (no wildcard arm): a new AST node must be classified here, or a
//!   variable variable nested inside it would silently survive to the checker.
//! - Scope boundaries are NOT descended: function, class-like and closure bodies are their own
//!   frames, and each visitor handles them explicitly before delegating here.

use crate::parser::ast::{CallableTarget, Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind};

/// Callbacks a traversal applies to the direct children of a node.
pub(super) trait Visitor {
    /// Visits one nested statement.
    fn stmt(&mut self, stmt: &mut Stmt);
    /// Visits one child expression.
    fn expr(&mut self, expr: &mut Expr);
    /// Visits a nested statement block.
    fn block(&mut self, stmts: &mut Vec<Stmt>) {
        for stmt in stmts.iter_mut() {
            self.stmt(stmt);
        }
    }
}

/// Visits the direct child expressions and statement blocks of `stmt`.
pub(super) fn walk_stmt_children<V: Visitor + ?Sized>(v: &mut V, stmt: &mut Stmt) {
    match &mut stmt.kind {
        StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::Global { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => {}

        // Scope boundaries: their bodies are separate frames.
        StmtKind::FunctionDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::EnumDecl { .. } => {}

        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::Assign { value: expr, .. }
        | StmtKind::RefAssign { source: expr, .. }
        | StmtKind::TypedAssign { value: expr, .. }
        | StmtKind::ArrayPush { value: expr, .. }
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::Include { path: expr, .. } => v.expr(expr),

        StmtKind::Return(value) => {
            if let Some(value) = value {
                v.expr(value);
            }
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            v.expr(index);
            v.expr(value);
        }
        StmtKind::RefAssignPlace { target, source } => {
            v.expr(target);
            v.expr(source);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            v.expr(target);
            v.expr(value);
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            v.expr(object);
            v.expr(value);
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            v.expr(object);
            v.expr(index);
            v.expr(value);
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => v.expr(value),
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            v.expr(index);
            v.expr(value);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            v.expr(condition);
            v.block(then_body);
            for (condition, body) in elseif_clauses.iter_mut() {
                v.expr(condition);
                v.block(body);
            }
            if let Some(body) = else_body {
                v.block(body);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            v.block(then_body);
            if let Some(body) = else_body {
                v.block(body);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            v.expr(condition);
            v.block(body);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                v.stmt(init);
            }
            if let Some(condition) = condition {
                v.expr(condition);
            }
            if let Some(update) = update {
                v.stmt(update);
            }
            v.block(body);
        }
        StmtKind::Foreach { array, body, .. } => {
            v.expr(array);
            v.block(body);
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            v.expr(subject);
            for (conditions, body) in cases.iter_mut() {
                for condition in conditions.iter_mut() {
                    v.expr(condition);
                }
                v.block(body);
            }
            if let Some(body) = default {
                v.block(body);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            v.block(try_body);
            for catch in catches.iter_mut() {
                v.block(&mut catch.body);
            }
            if let Some(body) = finally_body {
                v.block(body);
            }
        }
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => v.block(body),
    }
}

/// Visits the direct child expressions of `expr`. Closure bodies are not descended.
pub(super) fn walk_expr_children<V: Visitor + ?Sized>(v: &mut V, expr: &mut Expr) {
    match &mut expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::This
        | ExprKind::Variable(_)
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::MagicConstant(_)
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. } => {}

        // Scope boundary: the closure body is its own frame.
        ExprKind::Closure { .. } => {}

        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Clone(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::YieldFrom(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::ObjectClassName { object: inner }
        | ExprKind::PropertyAccess { object: inner, .. }
        | ExprKind::NullsafePropertyAccess { object: inner, .. }
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::IncludeValue { path: inner, .. }
        | ExprKind::BufferNew { len: inner, .. } => v.expr(inner),

        ExprKind::BinaryOp { left, right, .. } => {
            v.expr(left);
            v.expr(right);
        }
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            v.expr(value);
            v.expr(default);
        }
        ExprKind::Pipe { value, callable } => {
            v.expr(value);
            v.expr(callable);
        }
        ExprKind::InstanceOf { value, target } => {
            v.expr(value);
            if let InstanceOfTarget::Expr(target) = target {
                v.expr(target);
            }
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            v.block(prelude);
            v.expr(target);
            v.expr(value);
            if let Some(result_target) = result_target {
                v.expr(result_target);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            v.expr(array);
            v.expr(index);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            v.expr(condition);
            v.expr(then_expr);
            v.expr(else_expr);
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items.iter_mut() {
                v.expr(item);
            }
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs.iter_mut() {
                v.expr(key);
                v.expr(value);
            }
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            for entry in entries.iter_mut() {
                for expr in entry.exprs_mut() {
                    v.expr(expr);
                }
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            v.expr(subject);
            for (conditions, body) in arms.iter_mut() {
                for condition in conditions.iter_mut() {
                    v.expr(condition);
                }
                v.expr(body);
            }
            if let Some(default) = default {
                v.expr(default);
            }
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            v.expr(object);
            v.expr(property);
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            v.expr(object);
            for arg in args.iter_mut() {
                v.expr(arg);
            }
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            v.expr(object);
            v.expr(method);
            for arg in args.iter_mut() {
                v.expr(arg);
            }
        }
        ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewScopedObject { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewGeneric { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::FunctionCall { args, .. } => {
            for arg in args.iter_mut() {
                v.expr(arg);
            }
        }
        ExprKind::NewDynamic { name_expr, args } => {
            v.expr(name_expr);
            for arg in args.iter_mut() {
                v.expr(arg);
            }
        }
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => {
            v.expr(class_name);
            for arg in args.iter_mut() {
                v.expr(arg);
            }
        }
        ExprKind::ExprCall { callee, args } => {
            v.expr(callee);
            for arg in args.iter_mut() {
                v.expr(arg);
            }
        }
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                v.expr(key);
            }
            if let Some(value) = value {
                v.expr(value);
            }
        }
        ExprKind::FirstClassCallable(target) => match target {
            CallableTarget::Function(_) | CallableTarget::StaticMethod { .. } => {}
            CallableTarget::Method { object, .. } => v.expr(object),
        },
    }
}
