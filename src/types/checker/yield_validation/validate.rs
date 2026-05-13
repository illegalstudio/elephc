//! Purpose:
//! Walks the parsed program enforcing where `yield` / `yield from` may
//! appear. Tracks function/method depth and `try`/`catch`/`finally` depth so
//! illegal yields surface as `CompileError`s rather than codegen panics.
//!
//! Called from:
//!  - `super::validate_yield_contexts` re-export consumers (the checker driver).
//!
//! Key details:
//!  - The walker treats every function/method declaration and closure body
//!    as a fresh generator scope (function_depth++, try_finally_depth reset
//!    on entering a closure). A yield can only appear when function_depth > 0
//!    and try_finally_depth == 0.
//!  - The walker collects all violations into one `Vec<CompileError>` so the
//!    user sees every illegal yield in a single compile pass.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, Program, Stmt, StmtKind};

/// Walk the program AST and reject misuses of `yield` / `yield from`:
///
/// 1. Outside any function/method/closure body — yield is only valid as part
///    of a generator function.
/// 2. Inside any `try`, `catch`, or `finally` body — elephc v1 of generators
///    does not support resuming through unwinding. The error is explicit so
///    users can refactor before the codegen reports the same.
pub(crate) fn validate_yield_contexts(program: &Program) -> Vec<CompileError> {
    let mut state = State {
        function_depth: 0,
        try_finally_depth: 0,
        errors: Vec::new(),
    };
    for stmt in program {
        visit_stmt(stmt, &mut state);
    }
    state.errors
}

struct State {
    function_depth: u32,
    try_finally_depth: u32,
    errors: Vec<CompileError>,
}

fn visit_stmt(stmt: &Stmt, st: &mut State) {
    match &stmt.kind {
        StmtKind::FunctionDecl { body, .. } => {
            st.function_depth += 1;
            for s in body {
                visit_stmt(s, st);
            }
            st.function_depth -= 1;
        }
        StmtKind::ClassDecl { methods, .. } | StmtKind::TraitDecl { methods, .. } => {
            for m in methods {
                if !m.has_body {
                    continue;
                }
                st.function_depth += 1;
                for s in &m.body {
                    visit_stmt(s, st);
                }
                st.function_depth -= 1;
            }
        }
        StmtKind::InterfaceDecl { .. } => {}
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            // The `try` body itself, plus any `catch`/`finally`, all forbid yield.
            st.try_finally_depth += 1;
            for s in try_body {
                visit_stmt(s, st);
            }
            for c in catches {
                for s in &c.body {
                    visit_stmt(s, st);
                }
            }
            if let Some(fin) = finally_body {
                for s in fin {
                    visit_stmt(s, st);
                }
            }
            st.try_finally_depth -= 1;
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            visit_expr(condition, st);
            for s in then_body {
                visit_stmt(s, st);
            }
            for (cond, body) in elseif_clauses {
                visit_expr(cond, st);
                for s in body {
                    visit_stmt(s, st);
                }
            }
            if let Some(else_body) = else_body {
                for s in else_body {
                    visit_stmt(s, st);
                }
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                visit_stmt(s, st);
            }
            if let Some(else_body) = else_body {
                for s in else_body {
                    visit_stmt(s, st);
                }
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            visit_expr(condition, st);
            for s in body {
                visit_stmt(s, st);
            }
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                visit_stmt(init, st);
            }
            if let Some(cond) = condition {
                visit_expr(cond, st);
            }
            if let Some(up) = update {
                visit_stmt(up, st);
            }
            for s in body {
                visit_stmt(s, st);
            }
        }
        StmtKind::Foreach {
            array, body, ..
        } => {
            visit_expr(array, st);
            for s in body {
                visit_stmt(s, st);
            }
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            visit_expr(subject, st);
            for (vals, body) in cases {
                for v in vals {
                    visit_expr(v, st);
                }
                for s in body {
                    visit_stmt(s, st);
                }
            }
            if let Some(default) = default {
                for s in default {
                    visit_stmt(s, st);
                }
            }
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for s in stmts {
                visit_stmt(s, st);
            }
        }
        StmtKind::Echo(e) | StmtKind::ExprStmt(e) | StmtKind::Throw(e) => visit_expr(e, st),
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::ConstDecl { value, .. }
        | StmtKind::ListUnpack { value, .. }
        | StmtKind::StaticVar { init: value, .. } => visit_expr(value, st),
        StmtKind::ArrayAssign { index, value, .. } => {
            visit_expr(index, st);
            visit_expr(value, st);
        }
        StmtKind::ArrayPush { value, .. } => visit_expr(value, st),
        StmtKind::Return(opt) => {
            if let Some(e) = opt {
                visit_expr(e, st);
            }
        }
        StmtKind::Include { path, .. } => visit_expr(path, st),
        StmtKind::PropertyAssign { object, value, .. } => {
            visit_expr(object, st);
            visit_expr(value, st);
        }
        StmtKind::PropertyArrayPush { object, value, .. } => {
            visit_expr(object, st);
            visit_expr(value, st);
        }
        StmtKind::PropertyArrayAssign { object, index, value, .. } => {
            visit_expr(object, st);
            visit_expr(index, st);
            visit_expr(value, st);
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => visit_expr(value, st),
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            visit_expr(index, st);
            visit_expr(value, st);
        }
        // Statements that don't carry expressions or sub-bodies for yield checks.
        StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::Global { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::IncludeOnceGuard { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. } => {}
    }
}

fn visit_expr(expr: &Expr, st: &mut State) {
    match &expr.kind {
        ExprKind::Yield { key, value } => {
            check_yield_context(expr.span, st);
            if let Some(k) = key {
                visit_expr(k, st);
            }
            if let Some(v) = value {
                visit_expr(v, st);
            }
        }
        ExprKind::YieldFrom(inner) => {
            check_yield_context(expr.span, st);
            visit_expr(inner, st);
        }
        ExprKind::Closure { body, .. } => {
            // Closures introduce a fresh function scope. A yield inside a
            // closure refers to that closure (which would make it a generator
            // closure — currently unsupported in v1, but lex/parse/typecheck
            // accept the syntax). Reset try-depth for the new scope.
            let saved_try = st.try_finally_depth;
            st.try_finally_depth = 0;
            st.function_depth += 1;
            for s in body {
                visit_stmt(s, st);
            }
            st.function_depth -= 1;
            st.try_finally_depth = saved_try;
        }
        // Expressions with sub-expressions to recurse into.
        ExprKind::BinaryOp { left, right, .. } => {
            visit_expr(left, st);
            visit_expr(right, st);
        }
        ExprKind::InstanceOf { value, .. } => visit_expr(value, st),
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Spread(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::PtrCast { expr: inner, .. } => visit_expr(inner, st),
        ExprKind::NullCoalesce { value, default } => {
            visit_expr(value, st);
            visit_expr(default, st);
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. }
        | ExprKind::StaticMethodCall { args, .. } => {
            for a in args {
                visit_expr(a, st);
            }
        }
        ExprKind::ExprCall { callee, args } => {
            visit_expr(callee, st);
            for a in args {
                visit_expr(a, st);
            }
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            visit_expr(object, st);
            for a in args {
                visit_expr(a, st);
            }
        }
        ExprKind::ArrayLiteral(items) => {
            for it in items {
                visit_expr(it, st);
            }
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (k, v) in pairs {
                visit_expr(k, st);
                visit_expr(v, st);
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            visit_expr(subject, st);
            for (patterns, value) in arms {
                for p in patterns {
                    visit_expr(p, st);
                }
                visit_expr(value, st);
            }
            if let Some(d) = default {
                visit_expr(d, st);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            visit_expr(array, st);
            visit_expr(index, st);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            visit_expr(condition, st);
            visit_expr(then_expr, st);
            visit_expr(else_expr, st);
        }
        ExprKind::ShortTernary { value, default } => {
            visit_expr(value, st);
            visit_expr(default, st);
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => visit_expr(object, st),
        ExprKind::NamedArg { value, .. } => visit_expr(value, st),
        ExprKind::BufferNew { len, .. } => visit_expr(len, st),
        // Leaves
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::Variable(_)
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::This
        | ExprKind::FirstClassCallable(_)
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::ClassConstant { .. }
        | ExprKind::MagicConstant(_) => {}
        ExprKind::Print(inner) => visit_expr(inner, st),
        ExprKind::Assignment { target, value, .. } => {
            visit_expr(target, st);
            visit_expr(value, st);
        }
    }
}

fn check_yield_context(span: crate::span::Span, st: &mut State) {
    if st.function_depth == 0 {
        st.errors.push(CompileError::new(
            span,
            "yield can only be used inside a function or method body",
        ));
    } else if st.try_finally_depth > 0 {
        st.errors.push(CompileError::new(
            span,
            "yield inside try/catch/finally is not yet supported",
        ));
    }
}
