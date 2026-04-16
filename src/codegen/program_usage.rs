use std::collections::HashSet;

use crate::parser::ast::{Expr, ExprKind, Program, Stmt, StmtKind};

pub(super) fn collect_required_class_names(program: &Program) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_required_class_names_in_body(program, &mut names);
    names
}

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
            StmtKind::ArrayPush { value, .. }
            | StmtKind::Return(Some(value))
            | StmtKind::ListUnpack { value, .. }
            | StmtKind::PropertyAssign { value, .. } => {
                collect_required_class_names_in_expr(value, names);
            }
            _ => {}
        }
    }
}

fn collect_required_class_names_in_expr(expr: &Expr, names: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::BinaryOp { left, right, .. } => {
            collect_required_class_names_in_expr(left, names);
            collect_required_class_names_in_expr(right, names);
        }
        ExprKind::Negate(expr)
        | ExprKind::Not(expr)
        | ExprKind::BitNot(expr)
        | ExprKind::Throw(expr)
        | ExprKind::Spread(expr)
        | ExprKind::Cast { expr, .. }
        | ExprKind::PtrCast { expr, .. } => collect_required_class_names_in_expr(expr, names),
        ExprKind::NullCoalesce { value, default } => {
            collect_required_class_names_in_expr(value, names);
            collect_required_class_names_in_expr(default, names);
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
        ExprKind::PropertyAccess { object, .. } => {
            collect_required_class_names_in_expr(object, names);
        }
        ExprKind::MethodCall { object, args, .. } => {
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
        | ExprKind::EnumCase { .. }
        | ExprKind::This => {}
    }
}

pub(super) fn program_uses_variable(program: &Program, needle: &str) -> bool {
    program.iter().any(|stmt| stmt_uses_variable(stmt, needle))
}

fn stmt_uses_variable(stmt: &Stmt, needle: &str) -> bool {
    match &stmt.kind {
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::Echo(value)
        | StmtKind::Throw(value)
        | StmtKind::ExprStmt(value)
        | StmtKind::ConstDecl { value, .. } => expr_uses_variable(value, needle),
        StmtKind::Return(Some(value)) => expr_uses_variable(value, needle),
        StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => false,
        StmtKind::ArrayAssign { array, index, value } => {
            array == needle
                || expr_uses_variable(index, needle)
                || expr_uses_variable(value, needle)
        }
        StmtKind::ArrayPush { array, value } => {
            array == needle || expr_uses_variable(value, needle)
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_uses_variable(condition, needle)
                || then_body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
                || elseif_clauses.iter().any(|(cond, body)| {
                    expr_uses_variable(cond, needle)
                        || body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
                })
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(|stmt| stmt_uses_variable(stmt, needle)))
        }
        StmtKind::IfDef {
            then_body, else_body, ..
        } => {
            then_body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(|stmt| stmt_uses_variable(stmt, needle)))
        }
        StmtKind::While { condition, body } => {
            expr_uses_variable(condition, needle)
                || body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
        }
        StmtKind::DoWhile { body, condition } => {
            body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
                || expr_uses_variable(condition, needle)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_ref()
                .is_some_and(|stmt| stmt_uses_variable(stmt, needle))
                || condition
                    .as_ref()
                    .is_some_and(|expr| expr_uses_variable(expr, needle))
                || update
                    .as_ref()
                    .is_some_and(|stmt| stmt_uses_variable(stmt, needle))
                || body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
        }
        StmtKind::Foreach { array, body, .. } => {
            expr_uses_variable(array, needle)
                || body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_uses_variable(subject, needle)
                || cases.iter().any(|(values, body)| {
                    values.iter().any(|expr| expr_uses_variable(expr, needle))
                        || body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
                })
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(|stmt| stmt_uses_variable(stmt, needle)))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
                || catches.iter().any(|catch_clause| {
                    catch_clause
                        .body
                        .iter()
                        .any(|stmt| stmt_uses_variable(stmt, needle))
                })
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(|stmt| stmt_uses_variable(stmt, needle)))
        }
        StmtKind::ListUnpack { value, .. } => expr_uses_variable(value, needle),
        StmtKind::StaticVar { init, .. } => expr_uses_variable(init, needle),
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_uses_variable(object, needle) || expr_uses_variable(value, needle)
        }
        StmtKind::FunctionDecl { body, .. } | StmtKind::NamespaceBlock { body, .. } => {
            body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
        }
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. } => methods.iter().any(|method| {
            method
                .body
                .iter()
                .any(|stmt| stmt_uses_variable(stmt, needle))
        }),
        StmtKind::EnumDecl { cases, .. } => cases.iter().any(|case| {
            case.value
                .as_ref()
                .is_some_and(|expr| expr_uses_variable(expr, needle))
        }),
        StmtKind::Global { vars } => vars.iter().any(|name| name == needle),
        StmtKind::PackedClassDecl { .. }
        | StmtKind::Include { .. }
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => false,
    }
}

fn expr_uses_variable(expr: &Expr, needle: &str) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => name == needle,
        ExprKind::BinaryOp { left, right, .. } => {
            expr_uses_variable(left, needle) || expr_uses_variable(right, needle)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Spread(inner)
        | ExprKind::PtrCast { expr: inner, .. } => expr_uses_variable(inner, needle),
        ExprKind::NullCoalesce { value, default } => {
            expr_uses_variable(value, needle) || expr_uses_variable(default, needle)
        }
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => name == needle,
        ExprKind::FunctionCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. } => {
            args.iter().any(|arg| expr_uses_variable(arg, needle))
        }
        ExprKind::ExprCall { callee, args } => {
            expr_uses_variable(callee, needle)
                || args.iter().any(|arg| expr_uses_variable(arg, needle))
        }
        ExprKind::ArrayLiteral(items) => items.iter().any(|item| expr_uses_variable(item, needle)),
        ExprKind::ArrayLiteralAssoc(items) => items.iter().any(|(key, value)| {
            expr_uses_variable(key, needle) || expr_uses_variable(value, needle)
        }),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_uses_variable(subject, needle)
                || arms.iter().any(|(values, value)| {
                    values.iter().any(|expr| expr_uses_variable(expr, needle))
                        || expr_uses_variable(value, needle)
                })
                || default
                    .as_ref()
                    .is_some_and(|expr| expr_uses_variable(expr, needle))
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_uses_variable(array, needle) || expr_uses_variable(index, needle)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_uses_variable(condition, needle)
                || expr_uses_variable(then_expr, needle)
                || expr_uses_variable(else_expr, needle)
        }
        ExprKind::Cast { expr, .. }
        | ExprKind::NamedArg { value: expr, .. }
        | ExprKind::BufferNew { len: expr, .. } => expr_uses_variable(expr, needle),
        ExprKind::Closure { body, .. } => {
            body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
        }
        ExprKind::PropertyAccess { object, .. } => expr_uses_variable(object, needle),
        ExprKind::MethodCall { object, args, .. } => {
            expr_uses_variable(object, needle)
                || args.iter().any(|arg| expr_uses_variable(arg, needle))
        }
        ExprKind::FirstClassCallable(callable) => match callable {
            crate::parser::ast::CallableTarget::Function(_)
            | crate::parser::ast::CallableTarget::StaticMethod { .. } => false,
            crate::parser::ast::CallableTarget::Method { object, .. } => {
                expr_uses_variable(object, needle)
            }
        },
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::EnumCase { .. }
        | ExprKind::This => false,
    }
}
