//! Purpose:
//! Detects user functions that always return one of their own by-value
//! parameters without ever rebinding or mutating it (a "borrowed passthrough",
//! e.g. `function ident($s) { return $s; }`).
//!
//! Called from:
//! - `crate::ir_lower::program::lower()` to build a name set consulted while
//!   lowering call expressions.
//!
//! Key details:
//! - The result of calling such a function aliases the caller's argument, so the
//!   call result must be lowered with `Ownership::Borrowed` to avoid a
//!   double-free / use-after-free on `$x = f($x)` self-reassignment. See
//!   `crate::ir_lower::context::store_local`.
//! - Detection is intentionally conservative: any write whose target roots at a
//!   returned parameter disqualifies the function. A missed mutation form would
//!   at worst leak a temporary, never corrupt the heap, but the goal is to mark
//!   only provably-borrowed passthroughs.
//! - Nested function/closure scopes are not descended into when scanning a
//!   function body; their statements belong to a different parameter scope.

use std::collections::HashSet;

use crate::parser::ast::{Expr, ExprKind, Program, Stmt, StmtKind, TypeExpr};

/// AST parameter tuple shape shared by function declarations: `(name, type, default, by_ref)`.
type AstParam = (String, Option<TypeExpr>, Option<Expr>, bool);

/// Returns the canonical names of user functions that are borrowed parameter
/// passthroughs. The set is keyed both with and without a leading namespace
/// separator so call-site lookups match regardless of canonical spelling.
pub(crate) fn collect_borrowed_passthrough_functions(program: &Program) -> HashSet<String> {
    let mut set = HashSet::new();
    collect_in_statements(program, &mut set);
    set
}

/// Walks a statement list, registering qualifying function declarations and
/// recursing into control-flow bodies to reach nested declarations.
fn collect_in_statements(stmts: &[Stmt], set: &mut HashSet<String>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FunctionDecl {
                name,
                params,
                variadic,
                body,
                ..
            } => {
                if function_returns_borrowed_param(params, variadic.as_deref(), body) {
                    set.insert(name.clone());
                    set.insert(name.trim_start_matches('\\').to_string());
                }
                collect_in_statements(body, set);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_in_statements(then_body, set);
                for (_, body) in elseif_clauses {
                    collect_in_statements(body, set);
                }
                if let Some(body) = else_body {
                    collect_in_statements(body, set);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                collect_in_statements(then_body, set);
                if let Some(body) = else_body {
                    collect_in_statements(body, set);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::Foreach { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. }
            | StmtKind::NamespaceBlock { body, .. } => {
                collect_in_statements(body, set);
            }
            StmtKind::For {
                init,
                update,
                body,
                ..
            } => {
                if let Some(init) = init.as_deref() {
                    collect_in_statements(std::slice::from_ref(init), set);
                }
                if let Some(update) = update.as_deref() {
                    collect_in_statements(std::slice::from_ref(update), set);
                }
                collect_in_statements(body, set);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_in_statements(body, set);
                }
                if let Some(body) = default {
                    collect_in_statements(body, set);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                collect_in_statements(try_body, set);
                for catch in catches {
                    collect_in_statements(&catch.body, set);
                }
                if let Some(body) = finally_body {
                    collect_in_statements(body, set);
                }
            }
            _ => {}
        }
    }
}

/// Returns true when every `return` in `body` yields a by-value parameter that
/// is never rebound or mutated, and there is at least one such `return`.
fn function_returns_borrowed_param(
    params: &[AstParam],
    variadic: Option<&str>,
    body: &[Stmt],
) -> bool {
    let by_value: HashSet<&str> = params
        .iter()
        .filter(|(name, _, _, by_ref)| !*by_ref && Some(name.as_str()) != variadic)
        .map(|(name, _, _, _)| name.as_str())
        .collect();
    if by_value.is_empty() {
        return false;
    }

    let mut scan = ReturnScan::default();
    scan_returns(body, &by_value, &mut scan);
    if scan.disqualified || !scan.saw_return || scan.returned.is_empty() {
        return false;
    }

    !scan
        .returned
        .iter()
        .any(|param| local_is_written(body, param))
}

/// Accumulated state while scanning `return` statements in a function body.
#[derive(Default)]
struct ReturnScan {
    returned: HashSet<String>,
    saw_return: bool,
    disqualified: bool,
}

/// Records returned by-value parameters; any other `return` shape disqualifies.
///
/// Recurses into control-flow bodies but not into nested function declarations,
/// whose `return`s belong to a separate parameter scope.
fn scan_returns(stmts: &[Stmt], by_value: &HashSet<&str>, scan: &mut ReturnScan) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Return(value) => {
                scan.saw_return = true;
                match value {
                    Some(expr) => match &expr.kind {
                        ExprKind::Variable(name) if by_value.contains(name.as_str()) => {
                            scan.returned.insert(name.clone());
                        }
                        _ => scan.disqualified = true,
                    },
                    None => scan.disqualified = true,
                }
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                scan_returns(then_body, by_value, scan);
                for (_, body) in elseif_clauses {
                    scan_returns(body, by_value, scan);
                }
                if let Some(body) = else_body {
                    scan_returns(body, by_value, scan);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                scan_returns(then_body, by_value, scan);
                if let Some(body) = else_body {
                    scan_returns(body, by_value, scan);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::Foreach { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. }
            | StmtKind::NamespaceBlock { body, .. } => scan_returns(body, by_value, scan),
            StmtKind::For { body, .. } => scan_returns(body, by_value, scan),
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    scan_returns(body, by_value, scan);
                }
                if let Some(body) = default {
                    scan_returns(body, by_value, scan);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                scan_returns(try_body, by_value, scan);
                for catch in catches {
                    scan_returns(&catch.body, by_value, scan);
                }
                if let Some(body) = finally_body {
                    scan_returns(body, by_value, scan);
                }
            }
            // Nested function/class declarations open separate scopes.
            _ => {}
        }
    }
}

/// Returns true when local `name` is rebound or mutated anywhere in `stmts`.
///
/// Conservative: any assignment, push, unpack, loop binding, increment, or
/// by-reference capture whose target roots at `name` counts. Nested
/// function/class declarations are not descended into.
fn local_is_written(stmts: &[Stmt], name: &str) -> bool {
    stmts.iter().any(|stmt| stmt_writes_local(stmt, name))
}

/// Returns true when a single statement rebinds or mutates local `name`.
fn stmt_writes_local(stmt: &Stmt, name: &str) -> bool {
    match &stmt.kind {
        StmtKind::Assign { name: target, value } | StmtKind::TypedAssign { name: target, value, .. } => {
            target == name || expr_writes_local(value, name)
        }
        StmtKind::RefAssign { target, source } => target == name || source == name,
        StmtKind::ArrayAssign { array, index, value } => {
            array == name || expr_writes_local(index, name) || expr_writes_local(value, name)
        }
        StmtKind::ArrayPush { array, value } => array == name || expr_writes_local(value, name),
        StmtKind::NestedArrayAssign { target, value } => {
            assignment_target_root(target) == Some(name)
                || expr_writes_local(target, name)
                || expr_writes_local(value, name)
        }
        StmtKind::ListUnpack { vars, value } => {
            vars.iter().any(|var| var == name) || expr_writes_local(value, name)
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
            ..
        } => {
            value_var == name
                || key_var.as_deref() == Some(name)
                || expr_writes_local(array, name)
                || local_is_written(body, name)
        }
        StmtKind::Global { vars } => vars.iter().any(|var| var == name),
        StmtKind::StaticVar { name: target, init } => target == name || expr_writes_local(init, name),
        StmtKind::Echo(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::Throw(expr)
        | StmtKind::Return(Some(expr)) => expr_writes_local(expr, name),
        StmtKind::Return(None) => false,
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_writes_local(condition, name)
                || local_is_written(then_body, name)
                || elseif_clauses
                    .iter()
                    .any(|(cond, body)| expr_writes_local(cond, name) || local_is_written(body, name))
                || else_body.as_ref().is_some_and(|body| local_is_written(body, name))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            local_is_written(then_body, name)
                || else_body.as_ref().is_some_and(|body| local_is_written(body, name))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_writes_local(condition, name) || local_is_written(body, name)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(|stmt| stmt_writes_local(stmt, name))
                || condition.as_ref().is_some_and(|cond| expr_writes_local(cond, name))
                || update.as_deref().is_some_and(|stmt| stmt_writes_local(stmt, name))
                || local_is_written(body, name)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_writes_local(subject, name)
                || cases.iter().any(|(patterns, body)| {
                    patterns.iter().any(|pattern| expr_writes_local(pattern, name))
                        || local_is_written(body, name)
                })
                || default.as_ref().is_some_and(|body| local_is_written(body, name))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            local_is_written(try_body, name)
                || catches.iter().any(|catch| local_is_written(&catch.body, name))
                || finally_body.as_ref().is_some_and(|body| local_is_written(body, name))
        }
        StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. } => local_is_written(body, name),
        StmtKind::Include { path, .. } => expr_writes_local(path, name),
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_writes_local(object, name) || expr_writes_local(value, name)
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            expr_writes_local(object, name)
                || expr_writes_local(index, name)
                || expr_writes_local(value, name)
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => expr_writes_local(value, name),
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            expr_writes_local(index, name) || expr_writes_local(value, name)
        }
        StmtKind::ConstDecl { value, .. } => expr_writes_local(value, name),
        // Declarations and markers open separate scopes or cannot write a local.
        _ => false,
    }
}

/// Returns true when expression `expr` (or a sub-expression) rebinds or mutates
/// local `name`.
fn expr_writes_local(expr: &Expr, name: &str) -> bool {
    match &expr.kind {
        ExprKind::Assignment {
            target,
            value,
            prelude,
            result_target,
            ..
        } => {
            assignment_target_root(target) == Some(name)
                || expr_writes_local(target, name)
                || expr_writes_local(value, name)
                || local_is_written(prelude, name)
                || result_target
                    .as_deref()
                    .is_some_and(|target| expr_writes_local(target, name))
        }
        ExprKind::PreIncrement(var)
        | ExprKind::PostIncrement(var)
        | ExprKind::PreDecrement(var)
        | ExprKind::PostDecrement(var) => var == name,
        ExprKind::Closure {
            capture_refs, body, ..
        } => {
            // A by-reference capture can rebind the outer local; by-value
            // captures copy and the closure body is a separate scope.
            capture_refs.iter().any(|capture| capture == name) || local_is_written(body, name)
        }
        ExprKind::BinaryOp { left, right, .. } => {
            expr_writes_local(left, name) || expr_writes_local(right, name)
        }
        ExprKind::InstanceOf { value, .. } => expr_writes_local(value, name),
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Spread(inner)
        | ExprKind::Print(inner)
        | ExprKind::YieldFrom(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::PtrCast { expr: inner, .. } => expr_writes_local(inner, name),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_writes_local(value, name) || expr_writes_local(default, name)
        }
        ExprKind::Pipe { value, callable } => {
            expr_writes_local(value, name) || expr_writes_local(callable, name)
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. }
        | ExprKind::StaticMethodCall { args, .. } => {
            args.iter().any(|arg| expr_writes_local(arg, name))
        }
        ExprKind::NewDynamic { name_expr, args } => {
            expr_writes_local(name_expr, name) || args.iter().any(|arg| expr_writes_local(arg, name))
        }
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => {
            expr_writes_local(class_name, name) || args.iter().any(|arg| expr_writes_local(arg, name))
        }
        ExprKind::ExprCall { callee, args } => {
            expr_writes_local(callee, name) || args.iter().any(|arg| expr_writes_local(arg, name))
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_writes_local(object, name) || args.iter().any(|arg| expr_writes_local(arg, name))
        }
        ExprKind::ArrayLiteral(items) => items.iter().any(|item| expr_writes_local(item, name)),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs
            .iter()
            .any(|(key, value)| expr_writes_local(key, name) || expr_writes_local(value, name)),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_writes_local(subject, name)
                || arms.iter().any(|(patterns, value)| {
                    patterns.iter().any(|pattern| expr_writes_local(pattern, name))
                        || expr_writes_local(value, name)
                })
                || default.as_deref().is_some_and(|d| expr_writes_local(d, name))
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_writes_local(array, name) || expr_writes_local(index, name)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_writes_local(condition, name)
                || expr_writes_local(then_expr, name)
                || expr_writes_local(else_expr, name)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_writes_local(object, name),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_writes_local(object, name) || expr_writes_local(property, name)
        }
        ExprKind::NamedArg { value, .. } => expr_writes_local(value, name),
        ExprKind::BufferNew { len, .. } => expr_writes_local(len, name),
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(|k| expr_writes_local(k, name))
                || value.as_deref().is_some_and(|v| expr_writes_local(v, name))
        }
        _ => false,
    }
}

/// Returns the root local variable name an assignment target writes through,
/// following array-index and property-access chains (e.g. `$a[0]->b` roots at `a`).
fn assignment_target_root(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Variable(name) => Some(name.as_str()),
        ExprKind::ArrayAccess { array, .. } => assignment_target_root(array),
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. }
        | ExprKind::DynamicPropertyAccess { object, .. }
        | ExprKind::NullsafeDynamicPropertyAccess { object, .. } => assignment_target_root(object),
        _ => None,
    }
}
