//! Purpose:
//! Read-only traversal that reaches every statement of a program at any depth, including the
//! bodies of closures and arrow functions nested inside expressions.
//!
//! Called from:
//! - Whole-program scans that look for a statement kind wherever it occurs (include markers,
//!   class-like activation events).
//!
//! Key details:
//! - Statement-only walks miss what an expression holds: a closure literal carries a statement
//!   body, and an assignment can carry a lowered `prelude`. Symfony's compiled container keeps
//!   `static function ($container) { include_once ...; }` factories in property arrays, and
//!   scans that stopped at statements neither registered those sources nor saw their
//!   activation events.
//! - The expression and statement matches are exhaustive (no wildcard arm): a new AST node must
//!   say here what it contains, or a scan built on this would silently miss it again.
//! - Parameter defaults, property defaults and class constants are constant expressions and are
//!   not walked.

use super::{ArrayEntry, CallableTarget, ClassMethod, Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind};

/// One node a walk reaches.
#[derive(Clone, Copy)]
pub(crate) enum Node<'a> {
    Stmt(&'a Stmt),
    Expr(&'a Expr),
}

/// Calls `visit` for every statement in `stmts`, at any depth, in source order (pre-order).
pub(crate) fn walk_stmts(stmts: &[Stmt], visit: &mut dyn FnMut(&Stmt)) {
    walk_nodes(stmts, &mut |node| {
        if let Node::Stmt(stmt) = node {
            visit(stmt);
        }
    });
}

/// Calls `visit` for every statement AND every expression in `stmts`, at any depth, in source
/// order (pre-order: a node before the nodes it contains).
pub(crate) fn walk_nodes<'a>(stmts: &'a [Stmt], visit: &mut dyn FnMut(Node<'a>)) {
    for stmt in stmts {
        walk_stmt(stmt, visit);
    }
}

fn walk_methods<'a>(methods: &'a [ClassMethod], visit: &mut dyn FnMut(Node<'a>)) {
    for method in methods {
        walk_nodes(&method.body, visit);
    }
}

fn walk_stmt<'a>(stmt: &'a Stmt, visit: &mut dyn FnMut(Node<'a>)) {
    visit(Node::Stmt(stmt));
    match &stmt.kind {
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::Assign { value: expr, .. }
        | StmtKind::RefAssign { source: expr, .. }
        | StmtKind::ArrayPush { value: expr, .. }
        | StmtKind::TypedAssign { value: expr, .. }
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::StaticPropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyArrayPush { value: expr, .. }
        | StmtKind::Include { path: expr, .. } => walk_expr(expr, visit),
        StmtKind::Return(expr) => {
            if let Some(expr) = expr {
                walk_expr(expr, visit);
            }
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            walk_expr(condition, visit);
            walk_nodes(then_body, visit);
            for (condition, body) in elseif_clauses {
                walk_expr(condition, visit);
                walk_nodes(body, visit);
            }
            if let Some(body) = else_body {
                walk_nodes(body, visit);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            walk_nodes(then_body, visit);
            if let Some(body) = else_body {
                walk_nodes(body, visit);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            walk_expr(condition, visit);
            walk_nodes(body, visit);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                walk_stmt(init, visit);
            }
            if let Some(condition) = condition {
                walk_expr(condition, visit);
            }
            if let Some(update) = update {
                walk_stmt(update, visit);
            }
            walk_nodes(body, visit);
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            walk_expr(index, visit);
            walk_expr(value, visit);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            walk_expr(target, visit);
            walk_expr(value, visit);
        }
        StmtKind::Foreach { array, body, .. } => {
            walk_expr(array, visit);
            walk_nodes(body, visit);
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            walk_expr(subject, visit);
            for (values, body) in cases {
                for value in values {
                    walk_expr(value, visit);
                }
                walk_nodes(body, visit);
            }
            if let Some(body) = default {
                walk_nodes(body, visit);
            }
        }
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::FunctionDecl { body, .. } => walk_nodes(body, visit),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            walk_nodes(try_body, visit);
            for catch in catches {
                walk_nodes(&catch.body, visit);
            }
            if let Some(body) = finally_body {
                walk_nodes(body, visit);
            }
        }
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::EnumDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. } => walk_methods(methods, visit),
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            walk_expr(object, visit);
            walk_expr(value, visit);
        }
        StmtKind::PropertyRefAssign { object, source, .. } => {
            walk_expr(object, visit);
            walk_expr(source, visit);
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            walk_expr(object, visit);
            walk_expr(index, visit);
            walk_expr(value, visit);
        }
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            walk_expr(index, visit);
            walk_expr(value, visit);
        }
        StmtKind::StaticPropertyElementRefAssign { index, source, .. } => {
            walk_expr(index, visit);
            walk_expr(source, visit);
        }
        StmtKind::DynamicStaticPropertyWrite {
            property,
            index,
            value,
            ..
        } => {
            walk_expr(property, visit);
            if let Some(index) = index {
                walk_expr(index, visit);
            }
            walk_expr(value, visit);
        }
        StmtKind::IncludeOnceMark { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::Global { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ClassLikeActivate { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => {}
    }
}

fn walk_exprs<'a>(exprs: &'a [Expr], visit: &mut dyn FnMut(Node<'a>)) {
    for expr in exprs {
        walk_expr(expr, visit);
    }
}

fn walk_expr<'a>(expr: &'a Expr, visit: &mut dyn FnMut(Node<'a>)) {
    visit(Node::Expr(expr));
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
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::This
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => {}
        ExprKind::BinaryOp { left, right, .. } => {
            walk_expr(left, visit);
            walk_expr(right, visit);
        }
        ExprKind::InstanceOf { value, target } => {
            walk_expr(value, visit);
            if let InstanceOfTarget::Expr(target) = target {
                walk_expr(target, visit);
            }
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::ArrayReference(inner)
        | ExprKind::Spread(inner)
        | ExprKind::Clone(inner)
        | ExprKind::YieldFrom(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::IncludeValue { path: inner, .. }
        | ExprKind::PropertyAccess { object: inner, .. }
        | ExprKind::NullsafePropertyAccess { object: inner, .. }
        | ExprKind::DynamicStaticPropertyAccess { property: inner, .. }
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::BufferNew { len: inner, .. }
        | ExprKind::ObjectClassName { object: inner }
        | ExprKind::DynamicScopedConstantAccess { receiver: inner, .. } => walk_expr(inner, visit),
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            walk_expr(value, visit);
            walk_expr(default, visit);
        }
        ExprKind::Pipe { value, callable } => {
            walk_expr(value, visit);
            walk_expr(callable, visit);
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            walk_nodes(prelude, visit);
            walk_expr(target, visit);
            walk_expr(value, visit);
            if let Some(result_target) = result_target {
                walk_expr(result_target, visit);
            }
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ArrayLiteral(args)
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewGeneric { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewScopedObject { args, .. } => walk_exprs(args, visit),
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs {
                walk_expr(key, visit);
                walk_expr(value, visit);
            }
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            for entry in entries {
                match entry {
                    ArrayEntry::Spread(value) | ArrayEntry::Value(value) => walk_expr(value, visit),
                    ArrayEntry::Keyed(key, value) => {
                        walk_expr(key, visit);
                        walk_expr(value, visit);
                    }
                }
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            walk_expr(subject, visit);
            for (conditions, value) in arms {
                walk_exprs(conditions, visit);
                walk_expr(value, visit);
            }
            if let Some(default) = default {
                walk_expr(default, visit);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            walk_expr(array, visit);
            walk_expr(index, visit);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            walk_expr(condition, visit);
            walk_expr(then_expr, visit);
            walk_expr(else_expr, visit);
        }
        ExprKind::Closure { body, .. } => walk_nodes(body, visit),
        ExprKind::ExprCall { callee, args } => {
            walk_expr(callee, visit);
            walk_exprs(args, visit);
        }
        ExprKind::NewDynamic { name_expr, args } => {
            walk_expr(name_expr, visit);
            walk_exprs(args, visit);
        }
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => {
            walk_expr(class_name, visit);
            walk_exprs(args, visit);
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            walk_expr(object, visit);
            walk_expr(property, visit);
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            walk_expr(object, visit);
            walk_exprs(args, visit);
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            walk_expr(object, visit);
            walk_expr(method, visit);
            walk_exprs(args, visit);
        }
        ExprKind::FirstClassCallable(target) => {
            if let CallableTarget::Method { object, .. } = target {
                walk_expr(object, visit);
            }
        }
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                walk_expr(key, visit);
            }
            if let Some(value) = value {
                walk_expr(value, visit);
            }
        }
    }
}

/// One node a mutable walk reaches.
pub(crate) enum NodeMut<'a> {
    Stmt(&'a mut Stmt),
    Expr(&'a mut Expr),
}

/// Mutable twin of [`walk_nodes`]: calls `visit` for every statement AND every expression in
/// `stmts`, at any depth, in pre-order, handing each one out mutably.
///
/// A visitor may replace the node wholesale (`*expr = …`); the walk then descends into the
/// REPLACEMENT's children, so a replacement must not contain the node it replaced. The match
/// arms below are the read-only walker's, in the same order, so the two cannot drift apart
/// silently: a new AST node added to one without the other fails to compile.
pub(crate) fn walk_nodes_mut(stmts: &mut [Stmt], visit: &mut dyn FnMut(NodeMut<'_>)) {
    for stmt in stmts {
        walk_stmt_mut(stmt, visit);
    }
}

fn walk_methods_mut(methods: &mut [ClassMethod], visit: &mut dyn FnMut(NodeMut<'_>)) {
    for method in methods {
        walk_nodes_mut(&mut method.body, visit);
    }
}

fn walk_stmt_mut(stmt: &mut Stmt, visit: &mut dyn FnMut(NodeMut<'_>)) {
    visit(NodeMut::Stmt(stmt));
    match &mut stmt.kind {
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::Assign { value: expr, .. }
        | StmtKind::RefAssign { source: expr, .. }
        | StmtKind::ArrayPush { value: expr, .. }
        | StmtKind::TypedAssign { value: expr, .. }
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::StaticPropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyArrayPush { value: expr, .. }
        | StmtKind::Include { path: expr, .. } => walk_expr_mut(expr, visit),
        StmtKind::Return(expr) => {
            if let Some(expr) = expr {
                walk_expr_mut(expr, visit);
            }
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            walk_expr_mut(condition, visit);
            walk_nodes_mut(then_body, visit);
            for (condition, body) in elseif_clauses {
                walk_expr_mut(condition, visit);
                walk_nodes_mut(body, visit);
            }
            if let Some(body) = else_body {
                walk_nodes_mut(body, visit);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            walk_nodes_mut(then_body, visit);
            if let Some(body) = else_body {
                walk_nodes_mut(body, visit);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            walk_expr_mut(condition, visit);
            walk_nodes_mut(body, visit);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                walk_stmt_mut(init, visit);
            }
            if let Some(condition) = condition {
                walk_expr_mut(condition, visit);
            }
            if let Some(update) = update {
                walk_stmt_mut(update, visit);
            }
            walk_nodes_mut(body, visit);
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            walk_expr_mut(index, visit);
            walk_expr_mut(value, visit);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            walk_expr_mut(target, visit);
            walk_expr_mut(value, visit);
        }
        StmtKind::Foreach { array, body, .. } => {
            walk_expr_mut(array, visit);
            walk_nodes_mut(body, visit);
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            walk_expr_mut(subject, visit);
            for (values, body) in cases {
                for value in values {
                    walk_expr_mut(value, visit);
                }
                walk_nodes_mut(body, visit);
            }
            if let Some(body) = default {
                walk_nodes_mut(body, visit);
            }
        }
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::FunctionDecl { body, .. } => walk_nodes_mut(body, visit),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            walk_nodes_mut(try_body, visit);
            for catch in catches {
                walk_nodes_mut(&mut catch.body, visit);
            }
            if let Some(body) = finally_body {
                walk_nodes_mut(body, visit);
            }
        }
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::EnumDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. } => walk_methods_mut(methods, visit),
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            walk_expr_mut(object, visit);
            walk_expr_mut(value, visit);
        }
        StmtKind::PropertyRefAssign { object, source, .. } => {
            walk_expr_mut(object, visit);
            walk_expr_mut(source, visit);
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            walk_expr_mut(object, visit);
            walk_expr_mut(index, visit);
            walk_expr_mut(value, visit);
        }
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            walk_expr_mut(index, visit);
            walk_expr_mut(value, visit);
        }
        StmtKind::StaticPropertyElementRefAssign { index, source, .. } => {
            walk_expr_mut(index, visit);
            walk_expr_mut(source, visit);
        }
        StmtKind::DynamicStaticPropertyWrite {
            property,
            index,
            value,
            ..
        } => {
            walk_expr_mut(property, visit);
            if let Some(index) = index {
                walk_expr_mut(index, visit);
            }
            walk_expr_mut(value, visit);
        }
        StmtKind::IncludeOnceMark { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::Global { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ClassLikeActivate { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => {}
    }
}

fn walk_exprs_mut(exprs: &mut [Expr], visit: &mut dyn FnMut(NodeMut<'_>)) {
    for expr in exprs {
        walk_expr_mut(expr, visit);
    }
}

fn walk_expr_mut(expr: &mut Expr, visit: &mut dyn FnMut(NodeMut<'_>)) {
    visit(NodeMut::Expr(expr));
    match &mut expr.kind {
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
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => {}
        ExprKind::BinaryOp { left, right, .. } => {
            walk_expr_mut(left, visit);
            walk_expr_mut(right, visit);
        }
        ExprKind::InstanceOf { value, target } => {
            walk_expr_mut(value, visit);
            if let InstanceOfTarget::Expr(target) = target {
                walk_expr_mut(target, visit);
            }
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::ArrayReference(inner)
        | ExprKind::Spread(inner)
        | ExprKind::Clone(inner)
        | ExprKind::YieldFrom(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::IncludeValue { path: inner, .. }
        | ExprKind::PropertyAccess { object: inner, .. }
        | ExprKind::NullsafePropertyAccess { object: inner, .. }
        | ExprKind::DynamicStaticPropertyAccess { property: inner, .. }
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::BufferNew { len: inner, .. }
        | ExprKind::ObjectClassName { object: inner }
        | ExprKind::DynamicScopedConstantAccess { receiver: inner, .. } => {
            walk_expr_mut(inner, visit)
        }
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            walk_expr_mut(value, visit);
            walk_expr_mut(default, visit);
        }
        ExprKind::Pipe { value, callable } => {
            walk_expr_mut(value, visit);
            walk_expr_mut(callable, visit);
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            walk_nodes_mut(prelude, visit);
            walk_expr_mut(target, visit);
            walk_expr_mut(value, visit);
            if let Some(result_target) = result_target {
                walk_expr_mut(result_target, visit);
            }
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ArrayLiteral(args)
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewGeneric { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewScopedObject { args, .. } => walk_exprs_mut(args, visit),
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs {
                walk_expr_mut(key, visit);
                walk_expr_mut(value, visit);
            }
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            for entry in entries {
                for value in entry.exprs_mut() {
                    walk_expr_mut(value, visit);
                }
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            walk_expr_mut(subject, visit);
            for (conditions, value) in arms {
                walk_exprs_mut(conditions, visit);
                walk_expr_mut(value, visit);
            }
            if let Some(default) = default {
                walk_expr_mut(default, visit);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            walk_expr_mut(array, visit);
            walk_expr_mut(index, visit);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            walk_expr_mut(condition, visit);
            walk_expr_mut(then_expr, visit);
            walk_expr_mut(else_expr, visit);
        }
        ExprKind::Closure { body, .. } => walk_nodes_mut(body, visit),
        ExprKind::ExprCall { callee, args } => {
            walk_expr_mut(callee, visit);
            walk_exprs_mut(args, visit);
        }
        ExprKind::NewDynamic { name_expr, args } => {
            walk_expr_mut(name_expr, visit);
            walk_exprs_mut(args, visit);
        }
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => {
            walk_expr_mut(class_name, visit);
            walk_exprs_mut(args, visit);
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            walk_expr_mut(object, visit);
            walk_expr_mut(property, visit);
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            walk_expr_mut(object, visit);
            walk_exprs_mut(args, visit);
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            walk_expr_mut(object, visit);
            walk_expr_mut(method, visit);
            walk_exprs_mut(args, visit);
        }
        ExprKind::FirstClassCallable(target) => {
            if let CallableTarget::Method { object, .. } = target {
                walk_expr_mut(object, visit);
            }
        }
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                walk_expr_mut(key, visit);
            }
            if let Some(value) = value {
                walk_expr_mut(value, visit);
            }
        }
    }
}

/// Recognizes the parser's `$obj->prop = &$name` pair and returns `(object, property, name)`.
pub(crate) fn param_property_reference_pair(stmt: &Stmt) -> Option<(&Expr, &str, &str)> {
    let StmtKind::Synthetic(pair) = &stmt.kind else {
        return None;
    };
    let [assign, bind] = pair.as_slice() else {
        return None;
    };
    let StmtKind::PropertyAssign { object, property, value } = &assign.kind else {
        return None;
    };
    let ExprKind::Variable(name) = &value.kind else {
        return None;
    };
    let StmtKind::RefAssign { target, source } = &bind.kind else {
        return None;
    };
    let ExprKind::PropertyAccess { object: bound_object, property: bound_property } = &source.kind
    else {
        return None;
    };
    (target == name && bound_property == property && bound_object.kind == object.kind)
        .then_some((object.as_ref(), property.as_str(), name.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A statement inside a closure literal stored in an array is reached, as are class methods.
    #[test]
    fn walk_reaches_statements_inside_closures_and_methods() {
        let source = "<?php class A { public function m() { $this->p['x'] = static function () { include_once 'f.php'; return 1; }; } } $g = fn() => 2;";
        let tokens = crate::lexer::tokenize(source).unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let mut includes = 0;
        let mut returns = 0;
        walk_stmts(&program, &mut |stmt| match &stmt.kind {
            StmtKind::Include { .. } => includes += 1,
            StmtKind::Return(_) => returns += 1,
            _ => {}
        });
        assert_eq!(includes, 1);
        assert_eq!(returns, 2);
    }
}
