use std::collections::{HashMap, HashSet};

use crate::errors::CompileWarning;
use crate::parser::ast::{ClassMethod, Expr, ExprKind, Program, Stmt, StmtKind};
use crate::span::Span;

#[derive(Default)]
struct ScopeUsage {
    declared: HashMap<String, Span>,
    reads: HashSet<String>,
}

impl ScopeUsage {
    fn declare(&mut self, name: &str, span: Span) {
        self.declared.entry(name.to_string()).or_insert(span);
    }

    fn read(&mut self, name: &str) {
        self.reads.insert(name.to_string());
    }
}

pub fn collect_warnings(program: &Program) -> Vec<CompileWarning> {
    let mut warnings = Vec::new();
    collect_unreachable_recursive(program, &mut warnings);
    collect_function_like_warnings(program, &mut warnings);
    warnings
}

fn collect_function_like_warnings(stmts: &[Stmt], warnings: &mut Vec<CompileWarning>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FunctionDecl {
                params,
                variadic,
                body,
                ..
            } => analyze_function_like_scope(params, variadic.as_ref(), body, stmt.span, warnings),
            StmtKind::ClassDecl { methods, .. }
            | StmtKind::TraitDecl { methods, .. }
            | StmtKind::InterfaceDecl { methods, .. } => {
                for method in methods {
                    analyze_method_scope(method, warnings);
                }
            }
            StmtKind::NamespaceBlock { body, .. } => collect_function_like_warnings(body, warnings),
            _ => collect_closure_warnings_in_stmt(stmt, warnings),
        }
    }
}

fn analyze_method_scope(method: &ClassMethod, warnings: &mut Vec<CompileWarning>) {
    if !method.has_body {
        return;
    }
    analyze_function_like_scope(
        &method.params,
        method.variadic.as_ref(),
        &method.body,
        method.span,
        warnings,
    );
}

fn analyze_function_like_scope(
    params: &[(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)],
    variadic: Option<&String>,
    body: &[Stmt],
    declaration_span: Span,
    warnings: &mut Vec<CompileWarning>,
) {
    let mut scope = ScopeUsage::default();
    for (name, _, _, is_ref) in params {
        scope.declare(name, declaration_span);
        if *is_ref {
            scope.read(name);
        }
    }
    if let Some(name) = variadic {
        scope.declare(name, declaration_span);
    }
    collect_scope_reads(body, &mut scope, warnings);
    for (name, span) in scope.declared {
        if !scope.reads.contains(&name) && !name.starts_with('_') {
            warnings.push(CompileWarning::new(
                span,
                &format!("Unused variable: ${}", name),
            ));
        }
    }
}

fn collect_scope_reads(stmts: &[Stmt], scope: &mut ScopeUsage, warnings: &mut Vec<CompileWarning>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Assign { name, value } => {
                collect_expr_reads(value, scope, warnings);
                scope.declare(name, stmt.span);
            }
            StmtKind::TypedAssign { name, value, .. } => {
                collect_expr_reads(value, scope, warnings);
                scope.declare(name, stmt.span);
            }
            StmtKind::ArrayAssign { array, index, value } => {
                scope.read(array);
                collect_expr_reads(index, scope, warnings);
                collect_expr_reads(value, scope, warnings);
            }
            StmtKind::ArrayPush { array, value } => {
                scope.read(array);
                collect_expr_reads(value, scope, warnings);
            }
            StmtKind::Echo(expr)
            | StmtKind::Throw(expr)
            | StmtKind::ExprStmt(expr)
            | StmtKind::ConstDecl { value: expr, .. } => collect_expr_reads(expr, scope, warnings),
            StmtKind::Return(Some(expr)) => collect_expr_reads(expr, scope, warnings),
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                collect_expr_reads(condition, scope, warnings);
                collect_scope_reads(then_body, scope, warnings);
                for (cond, body) in elseif_clauses {
                    collect_expr_reads(cond, scope, warnings);
                    collect_scope_reads(body, scope, warnings);
                }
                if let Some(body) = else_body {
                    collect_scope_reads(body, scope, warnings);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                collect_scope_reads(then_body, scope, warnings);
                if let Some(body) = else_body {
                    collect_scope_reads(body, scope, warnings);
                }
            }
            StmtKind::While { condition, body } => {
                collect_expr_reads(condition, scope, warnings);
                collect_scope_reads(body, scope, warnings);
            }
            StmtKind::DoWhile { body, condition } => {
                collect_scope_reads(body, scope, warnings);
                collect_expr_reads(condition, scope, warnings);
            }
            StmtKind::For {
                init,
                condition,
                update,
                body,
            } => {
                if let Some(stmt) = init {
                    collect_scope_reads(std::slice::from_ref(stmt), scope, warnings);
                }
                if let Some(expr) = condition {
                    collect_expr_reads(expr, scope, warnings);
                }
                if let Some(stmt) = update {
                    collect_scope_reads(std::slice::from_ref(stmt), scope, warnings);
                }
                collect_scope_reads(body, scope, warnings);
            }
            StmtKind::Foreach {
                array,
                key_var,
                value_var,
                body,
            } => {
                collect_expr_reads(array, scope, warnings);
                if let Some(name) = key_var {
                    scope.declare(name, stmt.span);
                }
                scope.declare(value_var, stmt.span);
                collect_scope_reads(body, scope, warnings);
            }
            StmtKind::Switch {
                subject,
                cases,
                default,
            } => {
                collect_expr_reads(subject, scope, warnings);
                for (values, body) in cases {
                    for value in values {
                        collect_expr_reads(value, scope, warnings);
                    }
                    collect_scope_reads(body, scope, warnings);
                }
                if let Some(body) = default {
                    collect_scope_reads(body, scope, warnings);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                collect_scope_reads(try_body, scope, warnings);
                for catch_clause in catches {
                    if let Some(name) = &catch_clause.variable {
                        scope.declare(name, stmt.span);
                    }
                    collect_scope_reads(&catch_clause.body, scope, warnings);
                }
                if let Some(body) = finally_body {
                    collect_scope_reads(body, scope, warnings);
                }
            }
            StmtKind::ListUnpack { vars, value } => {
                collect_expr_reads(value, scope, warnings);
                for name in vars {
                    scope.declare(name, stmt.span);
                }
            }
            StmtKind::Global { vars } => {
                for name in vars {
                    scope.declare(name, stmt.span);
                }
            }
            StmtKind::StaticVar { name, init } => {
                collect_expr_reads(init, scope, warnings);
                scope.declare(name, stmt.span);
            }
            StmtKind::PropertyAssign {
                object,
                value,
                ..
            } => {
                collect_expr_reads(object, scope, warnings);
                collect_expr_reads(value, scope, warnings);
            }
            StmtKind::FunctionDecl {
                params,
                variadic,
                body,
                ..
            } => analyze_function_like_scope(params, variadic.as_ref(), body, stmt.span, warnings),
            StmtKind::ClassDecl { methods, .. }
            | StmtKind::TraitDecl { methods, .. }
            | StmtKind::InterfaceDecl { methods, .. } => {
                for method in methods {
                    analyze_method_scope(method, warnings);
                }
            }
            StmtKind::EnumDecl { .. }
            | StmtKind::PackedClassDecl { .. }
            | StmtKind::NamespaceDecl { .. }
            | StmtKind::NamespaceBlock { .. }
            | StmtKind::UseDecl { .. }
            | StmtKind::Include { .. }
            | StmtKind::ExternFunctionDecl { .. }
            | StmtKind::ExternClassDecl { .. }
            | StmtKind::ExternGlobalDecl { .. } => collect_closure_warnings_in_stmt(stmt, warnings),
        }
    }
}

fn collect_expr_reads(expr: &Expr, scope: &mut ScopeUsage, warnings: &mut Vec<CompileWarning>) {
    match &expr.kind {
        ExprKind::Variable(name) => scope.read(name),
        ExprKind::BinaryOp { left, right, .. } => {
            collect_expr_reads(left, scope, warnings);
            collect_expr_reads(right, scope, warnings);
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Spread(inner)
        | ExprKind::PtrCast { expr: inner, .. } => collect_expr_reads(inner, scope, warnings),
        ExprKind::NullCoalesce { value, default } => {
            collect_expr_reads(value, scope, warnings);
            collect_expr_reads(default, scope, warnings);
        }
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => scope.read(name),
        ExprKind::ClosureCall { var, args } => {
            scope.read(var);
            for arg in args {
                collect_expr_reads(arg, scope, warnings);
            }
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ExprCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::MethodCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. } => {
            if let ExprKind::ExprCall { callee, .. } = &expr.kind {
                collect_expr_reads(callee, scope, warnings);
            }
            if let ExprKind::MethodCall { object, .. } = &expr.kind {
                collect_expr_reads(object, scope, warnings);
            }
            for arg in args {
                collect_expr_reads(arg, scope, warnings);
            }
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_expr_reads(item, scope, warnings);
            }
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                collect_expr_reads(key, scope, warnings);
                collect_expr_reads(value, scope, warnings);
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_expr_reads(subject, scope, warnings);
            for (values, body) in arms {
                for value in values {
                    collect_expr_reads(value, scope, warnings);
                }
                collect_expr_reads(body, scope, warnings);
            }
            if let Some(default) = default {
                collect_expr_reads(default, scope, warnings);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_expr_reads(array, scope, warnings);
            collect_expr_reads(index, scope, warnings);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr_reads(condition, scope, warnings);
            collect_expr_reads(then_expr, scope, warnings);
            collect_expr_reads(else_expr, scope, warnings);
        }
        ExprKind::Cast { expr, .. } => collect_expr_reads(expr, scope, warnings),
        ExprKind::Closure {
            params,
            variadic,
            body,
            captures,
            is_arrow,
            ..
        } => {
            if *is_arrow {
                for name in collect_free_reads_in_function_like(body, params, variadic.as_ref()) {
                    scope.read(&name);
                }
            }
            for name in captures {
                scope.read(name);
            }
            analyze_function_like_scope(params, variadic.as_ref(), body, expr.span, warnings);
        }
        ExprKind::NamedArg { value, .. } => collect_expr_reads(value, scope, warnings),
        ExprKind::PropertyAccess { object, .. } => collect_expr_reads(object, scope, warnings),
        ExprKind::BufferNew { len, .. } => collect_expr_reads(len, scope, warnings),
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::EnumCase { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::This => {}
    }
}

fn collect_closure_warnings_in_stmt(stmt: &Stmt, warnings: &mut Vec<CompileWarning>) {
    match &stmt.kind {
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. } => {
            collect_expr_reads(expr, &mut ScopeUsage::default(), warnings);
        }
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::ArrayPush { value, .. }
        | StmtKind::Return(Some(value)) => {
            collect_expr_reads(value, &mut ScopeUsage::default(), warnings);
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            let mut scope = ScopeUsage::default();
            collect_expr_reads(index, &mut scope, warnings);
            collect_expr_reads(value, &mut scope, warnings);
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            let mut scope = ScopeUsage::default();
            collect_expr_reads(object, &mut scope, warnings);
            collect_expr_reads(value, &mut scope, warnings);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            let mut scope = ScopeUsage::default();
            collect_expr_reads(condition, &mut scope, warnings);
            for stmt in then_body {
                collect_closure_warnings_in_stmt(stmt, warnings);
            }
            for (cond, body) in elseif_clauses {
                collect_expr_reads(cond, &mut scope, warnings);
                for stmt in body {
                    collect_closure_warnings_in_stmt(stmt, warnings);
                }
            }
            if let Some(body) = else_body {
                for stmt in body {
                    collect_closure_warnings_in_stmt(stmt, warnings);
                }
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            for stmt in then_body {
                collect_closure_warnings_in_stmt(stmt, warnings);
            }
            if let Some(body) = else_body {
                for stmt in body {
                    collect_closure_warnings_in_stmt(stmt, warnings);
                }
            }
        }
        StmtKind::DoWhile { body, condition } | StmtKind::While { body, condition } => {
            for stmt in body {
                collect_closure_warnings_in_stmt(stmt, warnings);
            }
            collect_expr_reads(condition, &mut ScopeUsage::default(), warnings);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(stmt) = init {
                collect_closure_warnings_in_stmt(stmt, warnings);
            }
            if let Some(expr) = condition {
                collect_expr_reads(expr, &mut ScopeUsage::default(), warnings);
            }
            if let Some(stmt) = update {
                collect_closure_warnings_in_stmt(stmt, warnings);
            }
            for stmt in body {
                collect_closure_warnings_in_stmt(stmt, warnings);
            }
        }
        StmtKind::Foreach { array, body, .. } => {
            collect_expr_reads(array, &mut ScopeUsage::default(), warnings);
            for stmt in body {
                collect_closure_warnings_in_stmt(stmt, warnings);
            }
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            collect_expr_reads(subject, &mut ScopeUsage::default(), warnings);
            for (values, body) in cases {
                let mut scope = ScopeUsage::default();
                for value in values {
                    collect_expr_reads(value, &mut scope, warnings);
                }
                for stmt in body {
                    collect_closure_warnings_in_stmt(stmt, warnings);
                }
            }
            if let Some(body) = default {
                for stmt in body {
                    collect_closure_warnings_in_stmt(stmt, warnings);
                }
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            for stmt in try_body {
                collect_closure_warnings_in_stmt(stmt, warnings);
            }
            for catch_clause in catches {
                for stmt in &catch_clause.body {
                    collect_closure_warnings_in_stmt(stmt, warnings);
                }
            }
            if let Some(body) = finally_body {
                for stmt in body {
                    collect_closure_warnings_in_stmt(stmt, warnings);
                }
            }
        }
        StmtKind::FunctionDecl {
            params,
            variadic,
            body,
            ..
        } => analyze_function_like_scope(params, variadic.as_ref(), body, stmt.span, warnings),
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. } => {
            for method in methods {
                analyze_method_scope(method, warnings);
            }
        }
        StmtKind::EnumDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::Include { .. }
        | StmtKind::Global { .. }
        | StmtKind::StaticVar { .. }
        | StmtKind::Return(None)
        | StmtKind::ListUnpack { .. }
        | StmtKind::Break
        | StmtKind::Continue
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => {}
        StmtKind::NamespaceBlock { body, .. } => collect_function_like_warnings(body, warnings),
    }
}

fn collect_unreachable_recursive(stmts: &[Stmt], warnings: &mut Vec<CompileWarning>) {
    collect_unreachable_in_block(stmts, warnings);
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_unreachable_recursive(then_body, warnings);
                for (_, body) in elseif_clauses {
                    collect_unreachable_recursive(body, warnings);
                }
                if let Some(body) = else_body {
                    collect_unreachable_recursive(body, warnings);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                collect_unreachable_recursive(then_body, warnings);
                if let Some(body) = else_body {
                    collect_unreachable_recursive(body, warnings);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::Foreach { body, .. } => collect_unreachable_recursive(body, warnings),
            StmtKind::For {
                init, update, body, ..
            } => {
                if let Some(stmt) = init {
                    collect_unreachable_recursive(std::slice::from_ref(stmt), warnings);
                }
                if let Some(stmt) = update {
                    collect_unreachable_recursive(std::slice::from_ref(stmt), warnings);
                }
                collect_unreachable_recursive(body, warnings);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_unreachable_recursive(body, warnings);
                }
                if let Some(body) = default {
                    collect_unreachable_recursive(body, warnings);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                collect_unreachable_recursive(try_body, warnings);
                for catch_clause in catches {
                    collect_unreachable_recursive(&catch_clause.body, warnings);
                }
                if let Some(body) = finally_body {
                    collect_unreachable_recursive(body, warnings);
                }
            }
            StmtKind::FunctionDecl { body, .. } => collect_unreachable_recursive(body, warnings),
            StmtKind::ClassDecl { methods, .. }
            | StmtKind::TraitDecl { methods, .. }
            | StmtKind::InterfaceDecl { methods, .. } => {
                for method in methods {
                    collect_unreachable_recursive(&method.body, warnings);
                }
            }
            StmtKind::NamespaceBlock { body, .. } => collect_unreachable_recursive(body, warnings),
            _ => {}
        }
    }
}

fn collect_free_reads_in_function_like(
    body: &[Stmt],
    params: &[(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)],
    variadic: Option<&String>,
) -> Vec<String> {
    let mut inner = ScopeUsage::default();
    for (name, _, _, _) in params {
        inner.declare(name, Span::dummy());
    }
    if let Some(name) = variadic {
        inner.declare(name, Span::dummy());
    }
    let mut nested_warnings = Vec::new();
    collect_scope_reads(body, &mut inner, &mut nested_warnings);
    inner
        .reads
        .into_iter()
        .filter(|name| !inner.declared.contains_key(name))
        .collect()
}

fn collect_unreachable_in_block(stmts: &[Stmt], warnings: &mut Vec<CompileWarning>) {
    let mut terminated = false;
    for stmt in stmts {
        if terminated {
            warnings.push(CompileWarning::new(stmt.span, "Unreachable code"));
            continue;
        }
        if stmt_guarantees_termination(stmt) {
            terminated = true;
        }
    }
}

fn stmt_guarantees_termination(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Return(_) | StmtKind::Throw(_) | StmtKind::Break | StmtKind::Continue => true,
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body: Some(else_body),
            ..
        } => {
            block_guarantees_termination(then_body)
                && elseif_clauses
                    .iter()
                    .all(|(_, body)| block_guarantees_termination(body))
                && block_guarantees_termination(else_body)
        }
        _ => false,
    }
}

fn block_guarantees_termination(stmts: &[Stmt]) -> bool {
    stmts.last().is_some_and(stmt_guarantees_termination)
}
