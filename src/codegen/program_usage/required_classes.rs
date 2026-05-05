use std::collections::HashSet;

use crate::parser::ast::{Expr, ExprKind, InstanceOfTarget, Program, Stmt, StmtKind};

pub(in crate::codegen) fn collect_required_class_names(program: &Program) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_required_class_names_in_body(program, &mut names);
    names
}

pub(in crate::codegen) fn program_has_dynamic_instanceof(program: &Program) -> bool {
    body_has_dynamic_instanceof(program)
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

fn body_has_dynamic_instanceof(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_has_dynamic_instanceof)
}

fn stmt_has_dynamic_instanceof(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. } => methods
            .iter()
            .any(|method| body_has_dynamic_instanceof(&method.body)),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            body_has_dynamic_instanceof(try_body)
                || catches
                    .iter()
                    .any(|catch_clause| body_has_dynamic_instanceof(&catch_clause.body))
                || finally_body
                    .as_deref()
                    .is_some_and(body_has_dynamic_instanceof)
        }
        StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body) => body_has_dynamic_instanceof(body),
        StmtKind::FunctionDecl { body, .. } => body_has_dynamic_instanceof(body),
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            body_has_dynamic_instanceof(then_body)
                || else_body
                    .as_deref()
                    .is_some_and(body_has_dynamic_instanceof)
        }
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::Assign { value: expr, .. }
        | StmtKind::TypedAssign { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::Return(Some(expr))
        | StmtKind::ArrayPush { value: expr, .. }
        | StmtKind::PropertyAssign { value: expr, .. }
        | StmtKind::PropertyArrayPush { value: expr, .. }
        | StmtKind::StaticPropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyArrayPush { value: expr, .. } => {
            expr_has_dynamic_instanceof(expr)
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_has_dynamic_instanceof(condition)
                || body_has_dynamic_instanceof(then_body)
                || elseif_clauses.iter().any(|(condition, body)| {
                    expr_has_dynamic_instanceof(condition) || body_has_dynamic_instanceof(body)
                })
                || else_body
                    .as_deref()
                    .is_some_and(body_has_dynamic_instanceof)
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            expr_has_dynamic_instanceof(condition) || body_has_dynamic_instanceof(body)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_has_dynamic_instanceof)
                || condition
                    .as_ref()
                    .is_some_and(expr_has_dynamic_instanceof)
                || update.as_deref().is_some_and(stmt_has_dynamic_instanceof)
                || body_has_dynamic_instanceof(body)
        }
        StmtKind::Foreach { array, body, .. } => {
            expr_has_dynamic_instanceof(array) || body_has_dynamic_instanceof(body)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_has_dynamic_instanceof(subject)
                || cases.iter().any(|(patterns, body)| {
                    patterns.iter().any(expr_has_dynamic_instanceof)
                        || body_has_dynamic_instanceof(body)
                })
                || default.as_deref().is_some_and(body_has_dynamic_instanceof)
        }
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            expr_has_dynamic_instanceof(index) || expr_has_dynamic_instanceof(value)
        }
        _ => false,
    }
}

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
        ExprKind::PropertyAccess { object, .. } => {
            collect_required_class_names_in_expr(object, names);
        }
        ExprKind::NullsafePropertyAccess { object, .. } => {
            collect_required_class_names_in_expr(object, names);
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
        | ExprKind::EnumCase { .. }
        | ExprKind::This => {}
        ExprKind::MagicConstant(_) => {
            unreachable!("MagicConstant must be lowered before codegen analysis")
        }
    }
}

fn expr_has_dynamic_instanceof(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::InstanceOf { value, target } => {
            matches!(target, InstanceOfTarget::Expr(_))
                || expr_has_dynamic_instanceof(value)
                || match target {
                    InstanceOfTarget::Name(_) => false,
                    InstanceOfTarget::Expr(expr) => expr_has_dynamic_instanceof(expr),
                }
        }
        ExprKind::BinaryOp { left, right, .. } => {
            expr_has_dynamic_instanceof(left) || expr_has_dynamic_instanceof(right)
        }
        ExprKind::Negate(expr)
        | ExprKind::Not(expr)
        | ExprKind::BitNot(expr)
        | ExprKind::Throw(expr)
        | ExprKind::ErrorSuppress(expr)
        | ExprKind::Print(expr)
        | ExprKind::Spread(expr)
        | ExprKind::Cast { expr, .. }
        | ExprKind::PtrCast { expr, .. }
        | ExprKind::NamedArg { value: expr, .. }
        | ExprKind::BufferNew { len: expr, .. } => expr_has_dynamic_instanceof(expr),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_has_dynamic_instanceof(value) || expr_has_dynamic_instanceof(default)
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            body_has_dynamic_instanceof(prelude)
                || expr_has_dynamic_instanceof(target)
                || expr_has_dynamic_instanceof(value)
                || result_target
                    .as_deref()
                    .is_some_and(expr_has_dynamic_instanceof)
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. } => args.iter().any(expr_has_dynamic_instanceof),
        ExprKind::ExprCall { callee, args } => {
            expr_has_dynamic_instanceof(callee) || args.iter().any(expr_has_dynamic_instanceof)
        }
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_has_dynamic_instanceof),
        ExprKind::ArrayLiteralAssoc(items) => items.iter().any(|(key, value)| {
            expr_has_dynamic_instanceof(key) || expr_has_dynamic_instanceof(value)
        }),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_has_dynamic_instanceof(subject)
                || arms.iter().any(|(patterns, result)| {
                    patterns.iter().any(expr_has_dynamic_instanceof)
                        || expr_has_dynamic_instanceof(result)
                })
                || default
                    .as_deref()
                    .is_some_and(expr_has_dynamic_instanceof)
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_has_dynamic_instanceof(array) || expr_has_dynamic_instanceof(index)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_has_dynamic_instanceof(condition)
                || expr_has_dynamic_instanceof(then_expr)
                || expr_has_dynamic_instanceof(else_expr)
        }
        ExprKind::Closure { body, .. } => body_has_dynamic_instanceof(body),
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_has_dynamic_instanceof(object),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_has_dynamic_instanceof(object) || args.iter().any(expr_has_dynamic_instanceof)
        }
        ExprKind::FirstClassCallable(crate::parser::ast::CallableTarget::Method {
            object,
            ..
        }) => expr_has_dynamic_instanceof(object),
        ExprKind::NewScopedObject { args, .. } => args.iter().any(expr_has_dynamic_instanceof),
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
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::This
        | ExprKind::ClassConstant { .. } => false,
        ExprKind::MagicConstant(_) => {
            unreachable!("MagicConstant must be lowered before codegen analysis")
        }
    }
}
