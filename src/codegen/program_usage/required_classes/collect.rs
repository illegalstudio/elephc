//! Purpose:
//! Walks statements and expressions to find class names that require emitted metadata.
//! Covers allocations, static access, catch clauses, instanceof, and type-driven references.
//!
//! Called from:
//! - `crate::codegen::program_usage::required_classes`
//!
//! Key details:
//! - Missing a recursive AST case can omit class tables and break later object dispatch.

use std::collections::HashSet;

use crate::parser::ast::{Expr, ExprKind, InstanceOfTarget, Program, Stmt, StmtKind};

/// Entry point: walks a complete `Program` and returns all class names that require
/// emitted metadata (class tables, vtables, dispatch helpers, etc.).
///
/// The returned `HashSet` contains fully-qualified or local class names gathered from:
/// - Class declarations (the class itself, its `extends` parent, and `implements` interfaces)
/// - `new` expressions and `instanceof` checks
/// - Catch clause exception types
/// - Static property/method access on named receivers
/// - `Generator` for `yield`/`yield from` expressions
///
/// # Arguments
/// - `program` — the parsed program to analyze
///
/// # Returns
/// - `HashSet<String>` of class names referenced anywhere in the program
pub(in crate::codegen) fn collect_required_class_names(program: &Program) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_required_class_names_in_body(program, &mut names);
    names
}

/// Collects required class names in stmts for the surrounding analysis or metadata result.
pub(in crate::codegen) fn collect_required_class_names_in_stmts(
    stmts: &[Stmt],
    names: &mut HashSet<String>,
) {
    collect_required_class_names_in_body(stmts, names);
}

/// Recursively walks a list of statements, collecting class names into `names`.
///
/// Dispatches on `StmtKind` variants that can introduce or reference class names:
/// - `ClassDecl`: inserts the class name, its `extends` parent, and all `implements` interfaces,
///   then recurses into method bodies.
/// - `Try`: recurses into `try_body`, each `catches` clause body, and optionally `finally_body`.
///   Each catch's `exception_types` are inserted.
/// - Control-flow statements (`If`, `While`, `Foreach`, `Switch`, etc.) recurse into their bodies
///   and also process any embedded expressions that may contain class references.
/// - Property/static assignments with named `StaticReceiver`: inserts the receiver class name.
/// - Statement variants that are purely expression-based delegate to
///   `collect_required_class_names_in_expr` for their expression(s).
///
/// # Arguments
/// - `stmts` — statement list to walk
/// - `names` — mutable accumulator into which class names are inserted
fn collect_required_class_names_in_body(stmts: &[Stmt], names: &mut HashSet<String>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::ClassDecl {
                name,
                extends,
                implements,
                methods,
                ..
            } => {
                names.insert(name.clone());
                if let Some(parent) = extends {
                    names.insert(parent.as_str().to_string());
                }
                for interface in implements {
                    names.insert(interface.as_str().to_string());
                }
                for method in methods {
                    collect_required_class_names_in_body(&method.body, names);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                collect_required_class_names_in_body(try_body, names);
                for catch_clause in catches {
                    for exception_type in &catch_clause.exception_types {
                        names.insert(exception_type.as_str().to_string());
                    }
                    collect_required_class_names_in_body(&catch_clause.body, names);
                }
                if let Some(body) = finally_body {
                    collect_required_class_names_in_body(body, names);
                }
            }
            StmtKind::NamespaceBlock { body, .. } => {
                collect_required_class_names_in_body(body, names);
            }
            StmtKind::IncludeOnceGuard { body, .. } => {
                collect_required_class_names_in_body(body, names);
            }
            StmtKind::IncludeOnceMark { .. } => {}
            StmtKind::FunctionDecl { body, .. } => {
                collect_required_class_names_in_body(body, names);
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                collect_required_class_names_in_body(then_body, names);
                if let Some(body) = else_body {
                    collect_required_class_names_in_body(body, names);
                }
            }
            StmtKind::Echo(expr)
            | StmtKind::Throw(expr)
            | StmtKind::ExprStmt(expr)
            | StmtKind::ConstDecl { value: expr, .. }
            | StmtKind::Assign { value: expr, .. }
            | StmtKind::TypedAssign { value: expr, .. }
            | StmtKind::StaticVar { init: expr, .. } => {
                collect_required_class_names_in_expr(expr, names);
            }
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                collect_required_class_names_in_expr(condition, names);
                collect_required_class_names_in_body(then_body, names);
                for (elseif_condition, body) in elseif_clauses {
                    collect_required_class_names_in_expr(elseif_condition, names);
                    collect_required_class_names_in_body(body, names);
                }
                if let Some(body) = else_body {
                    collect_required_class_names_in_body(body, names);
                }
            }
            StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
                collect_required_class_names_in_expr(condition, names);
                collect_required_class_names_in_body(body, names);
            }
            StmtKind::For {
                init,
                condition,
                update,
                body,
            } => {
                if let Some(init) = init {
                    collect_required_class_names_in_body(std::slice::from_ref(init.as_ref()), names);
                }
                if let Some(condition) = condition {
                    collect_required_class_names_in_expr(condition, names);
                }
                if let Some(update) = update {
                    collect_required_class_names_in_body(
                        std::slice::from_ref(update.as_ref()),
                        names,
                    );
                }
                collect_required_class_names_in_body(body, names);
            }
            StmtKind::Foreach { array, body, .. } => {
                collect_required_class_names_in_expr(array, names);
                collect_required_class_names_in_body(body, names);
            }
            StmtKind::Switch {
                subject,
                cases,
                default,
            } => {
                collect_required_class_names_in_expr(subject, names);
                for (patterns, body) in cases {
                    for pattern in patterns {
                        collect_required_class_names_in_expr(pattern, names);
                    }
                    collect_required_class_names_in_body(body, names);
                }
                if let Some(body) = default {
                    collect_required_class_names_in_body(body, names);
                }
            }
            StmtKind::ArrayAssign { index, value, .. } => {
                collect_required_class_names_in_expr(index, names);
                collect_required_class_names_in_expr(value, names);
            }
            StmtKind::NestedArrayAssign { target, value } => {
                collect_required_class_names_in_expr(target, names);
                collect_required_class_names_in_expr(value, names);
            }
            StmtKind::ArrayPush { value, .. }
            | StmtKind::Return(Some(value))
            | StmtKind::ListUnpack { value, .. }
            | StmtKind::PropertyAssign { value, .. }
            | StmtKind::PropertyArrayPush { value, .. } => {
                collect_required_class_names_in_expr(value, names);
            }
            StmtKind::StaticPropertyAssign {
                receiver, value, ..
            }
            | StmtKind::StaticPropertyArrayPush {
                receiver, value, ..
            } => {
                if let crate::parser::ast::StaticReceiver::Named(name) = receiver {
                    names.insert(name.as_str().to_string());
                }
                collect_required_class_names_in_expr(value, names);
            }
            StmtKind::PropertyArrayAssign { index, value, .. } => {
                collect_required_class_names_in_expr(index, names);
                collect_required_class_names_in_expr(value, names);
            }
            StmtKind::StaticPropertyArrayAssign {
                receiver,
                index,
                value,
                ..
            } => {
                if let crate::parser::ast::StaticReceiver::Named(name) = receiver {
                    names.insert(name.as_str().to_string());
                }
                collect_required_class_names_in_expr(index, names);
                collect_required_class_names_in_expr(value, names);
            }
            _ => {}
        }
    }
}

/// Recursively walks an expression, collecting class names into `names`.
///
/// Dispatches on `ExprKind` variants that can reference classes:
/// - `NewObject`: inserts the constructed class name, then recurses into constructor arguments.
/// - `InstanceOf`: recurses into the value; for `InstanceOfTarget::Name` (excluding `self`,
///   `parent`, `static`) inserts the class name.
/// - `StaticPropertyAccess`, `StaticMethodCall`, `ClassConstant`, `ScopedConstantAccess`,
///   `NewScopedObject`: for named `StaticReceiver`, inserts the class name; recurses into
///   arguments/expressions as needed.
/// - `FirstClassCallable` when targeting a static method: inserts the receiver class name;
///   when targeting an instance method: recurses into the object expression.
/// - `Yield` / `YieldFrom`: unconditionally inserts `"Generator"` (the builtin iterator class),
///   then recurses into optional key/value expressions.
/// - Complex expressions that nest other expressions (`BinaryOp`, `Assignment`, `Ternary`,
///   `Match`, `ArrayAccess`, etc.) recurse into their sub-expressions.
/// - Literal and variable expressions (`IntLiteral`, `Variable`, `This`, etc.) are no-ops.
///
/// # Arguments
/// - `expr` — the expression to walk
/// - `names` — mutable accumulator into which class names are inserted
///
/// # Panics
/// - `unreachable!` if a `MagicConstant` is encountered (must be lowered before this pass).
fn collect_required_class_names_in_expr(expr: &Expr, names: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::BinaryOp { left, right, .. } => {
            collect_required_class_names_in_expr(left, names);
            collect_required_class_names_in_expr(right, names);
        }
        ExprKind::InstanceOf { value, target } => {
            collect_required_class_names_in_expr(value, names);
            match target {
                InstanceOfTarget::Name(name) if !matches!(name.as_str(), "self" | "parent" | "static") => {
                    names.insert(name.as_str().to_string());
                }
                InstanceOfTarget::Expr(expr) => collect_required_class_names_in_expr(expr, names),
                _ => {}
            }
        }
        ExprKind::Negate(expr)
        | ExprKind::Not(expr)
        | ExprKind::BitNot(expr)
        | ExprKind::Throw(expr)
        | ExprKind::ErrorSuppress(expr)
        | ExprKind::Print(expr)
        | ExprKind::Spread(expr)
        | ExprKind::Cast { expr, .. }
        | ExprKind::PtrCast { expr, .. } => collect_required_class_names_in_expr(expr, names),
        ExprKind::NullCoalesce { value, default } => {
            collect_required_class_names_in_expr(value, names);
            collect_required_class_names_in_expr(default, names);
        }
        ExprKind::Pipe { value, callable } => {
            collect_required_class_names_in_expr(value, names);
            collect_required_class_names_in_expr(callable, names);
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            collect_required_class_names_in_body(prelude, names);
            collect_required_class_names_in_expr(target, names);
            collect_required_class_names_in_expr(value, names);
            if let Some(result_target) = result_target {
                collect_required_class_names_in_expr(result_target, names);
            }
        }
        ExprKind::FunctionCall { args, .. } | ExprKind::ClosureCall { args, .. } => {
            for arg in args {
                collect_required_class_names_in_expr(arg, names);
            }
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_required_class_names_in_expr(item, names);
            }
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                collect_required_class_names_in_expr(key, names);
                collect_required_class_names_in_expr(value, names);
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_required_class_names_in_expr(subject, names);
            for (patterns, result) in arms {
                for pattern in patterns {
                    collect_required_class_names_in_expr(pattern, names);
                }
                collect_required_class_names_in_expr(result, names);
            }
            if let Some(default) = default {
                collect_required_class_names_in_expr(default, names);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_required_class_names_in_expr(array, names);
            collect_required_class_names_in_expr(index, names);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_required_class_names_in_expr(condition, names);
            collect_required_class_names_in_expr(then_expr, names);
            collect_required_class_names_in_expr(else_expr, names);
        }
        ExprKind::ShortTernary { value, default } => {
            collect_required_class_names_in_expr(value, names);
            collect_required_class_names_in_expr(default, names);
        }
        ExprKind::Closure { body, .. } => {
            collect_required_class_names_in_body(body, names);
        }
        ExprKind::NamedArg { value, .. } => collect_required_class_names_in_expr(value, names),
        ExprKind::ExprCall { callee, args } => {
            collect_required_class_names_in_expr(callee, names);
            for arg in args {
                collect_required_class_names_in_expr(arg, names);
            }
        }
        ExprKind::NewObject { class_name, args } => {
            names.insert(class_name.as_str().to_string());
            for arg in args {
                collect_required_class_names_in_expr(arg, names);
            }
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            collect_required_class_names_in_expr(object, names);
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            collect_required_class_names_in_expr(object, names);
            collect_required_class_names_in_expr(property, names);
        }
        ExprKind::StaticPropertyAccess { receiver, .. } => {
            if let crate::parser::ast::StaticReceiver::Named(name) = receiver {
                names.insert(name.as_str().to_string());
            }
        }
        ExprKind::MethodCall { object, args, .. } => {
            collect_required_class_names_in_expr(object, names);
            for arg in args {
                collect_required_class_names_in_expr(arg, names);
            }
        }
        ExprKind::NullsafeMethodCall { object, args, .. } => {
            collect_required_class_names_in_expr(object, names);
            for arg in args {
                collect_required_class_names_in_expr(arg, names);
            }
        }
        ExprKind::StaticMethodCall { receiver, args, .. } => {
            if let crate::parser::ast::StaticReceiver::Named(name) = receiver {
                names.insert(name.as_str().to_string());
            }
            for arg in args {
                collect_required_class_names_in_expr(arg, names);
            }
        }
        ExprKind::FirstClassCallable(target) => match target {
            crate::parser::ast::CallableTarget::StaticMethod { receiver, .. } => {
                if let crate::parser::ast::StaticReceiver::Named(name) = receiver {
                    names.insert(name.as_str().to_string());
                }
            }
            crate::parser::ast::CallableTarget::Method { object, .. } => {
                collect_required_class_names_in_expr(object, names);
            }
            _ => {}
        },
        ExprKind::BufferNew { len, .. } => collect_required_class_names_in_expr(len, names),
        ExprKind::ClassConstant { receiver } => {
            if let crate::parser::ast::StaticReceiver::Named(name) = receiver {
                names.insert(name.as_str().to_string());
            }
        }
        ExprKind::ScopedConstantAccess { receiver, .. } => {
            if let crate::parser::ast::StaticReceiver::Named(name) = receiver {
                names.insert(name.as_str().to_string());
            }
        }
        ExprKind::NewScopedObject { receiver, args } => {
            if let crate::parser::ast::StaticReceiver::Named(name) = receiver {
                names.insert(name.as_str().to_string());
            }
            for arg in args {
                collect_required_class_names_in_expr(arg, names);
            }
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
        | ExprKind::This => {}
        ExprKind::Yield { key, value } => {
            names.insert("Generator".to_string());
            if let Some(k) = key {
                collect_required_class_names_in_expr(k, names);
            }
            if let Some(v) = value {
                collect_required_class_names_in_expr(v, names);
            }
        }
        ExprKind::YieldFrom(inner) => {
            names.insert("Generator".to_string());
            collect_required_class_names_in_expr(inner, names);
        }
        ExprKind::MagicConstant(_) => {
            unreachable!("MagicConstant must be lowered before codegen analysis")
        }
    }
}
