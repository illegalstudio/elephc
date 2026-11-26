//! Purpose:
//! Detaches a `global $x` alias after a straight-line `unset($GLOBALS['x'])`, matching PHP's
//! rule that the removed global stays reachable through the local reference while later writes
//! stay local instead of recreating the global.
//!
//! Called from:
//! - `crate::optimize::fold_constants` after CLI superglobal seeding and before folding.
//!
//! Key details:
//! - A direct, unconditional `unset($GLOBALS['x'])` at a function body's top level is rewritten
//!   by renaming: the pass inserts `$hidden = $x;` before the unset, keeps the unset, and
//!   rewrites later `$x` uses in that body (including nested blocks and closures) to `$hidden`.
//! - An unset nested inside control flow cannot be renamed, so `conditional` gives the alias a
//!   runtime choice between the global storage and a detached copy; it abandons the body when a
//!   shape it cannot express (references, closure captures, destructuring) is present.
//! - A later `global $x` reattaches to the global symbol in PHP, so the whole body is left
//!   untouched when one is present after the unset.
//! - Hidden names use the reserved `__elephc_detach_global_` prefix; user code should not use
//!   that compiler-internal namespace.

use std::collections::HashSet;

use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};

mod conditional;

/// Rewrites every function-like body in `program` that unset a live global alias.
pub(crate) fn detach_global_aliases(program: Vec<Stmt>) -> Vec<Stmt> {
    let mut counter = 0usize;
    rewrite_body(program, &mut counter)
}

/// Rewrites one statement list and descends into every nested body.
fn rewrite_body(mut statements: Vec<Stmt>, counter: &mut usize) -> Vec<Stmt> {
    let globals = direct_globals(&statements);
    // A body that unsets one of its aliases from inside control flow needs the path-sensitive
    // rewrite, which owns every use of that name; the straight-line rename below cannot express
    // the two possible storages after the join.
    let mut conditionally_detached: HashSet<String> = HashSet::new();
    for name in &globals {
        if has_nested_unset(&statements, name) {
            if let Some(updated) = conditional::detach(statements.clone(), name, counter) {
                statements = updated;
                conditionally_detached.insert(name.clone());
            }
        }
    }
    let mut index = 0usize;
    while index < statements.len() {
        if let Some(name) = direct_global_unset(&statements[index]) {
            let already_detached = index > 0 && is_detach_capture(&statements[index - 1], &name);
            if !already_detached
                && globals.contains(&name)
                && !conditionally_detached.contains(&name)
                && !reattaches_later(&statements[index + 1..], &name)
            {
                let hidden = format!("__elephc_detach_global_{}_{}", *counter, name);
                *counter += 1;
                let capture = Stmt::new(
                    StmtKind::Assign {
                        name: hidden.clone(),
                        value: Expr::new(
                            ExprKind::Variable(name.clone()),
                            statements[index].span,
                        ),
                    },
                    statements[index].span,
                );
                statements.insert(index, capture);
                index += 1;
                let unset = &mut statements[index];
                rewrite_after_unset(unset, &name, &hidden);
                for stmt in statements.iter_mut().skip(index + 1) {
                    rewrite_variables(stmt, &name, &hidden);
                }
            }
        }
        index += 1;
    }
    for stmt in &mut statements {
        rewrite_nested_bodies(stmt, counter);
    }
    statements
}

/// Returns the names bound by direct top-level `global` statements in one body.
pub(super) fn direct_globals(statements: &[Stmt]) -> HashSet<String> {
    statements
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::Global { vars } => Some(vars.iter().cloned()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// Returns the global name when `stmt` is exactly `unset($GLOBALS['name'])`.
fn direct_global_unset(stmt: &Stmt) -> Option<String> {
    let StmtKind::ExprStmt(expr) = &stmt.kind else {
        return None;
    };
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return None;
    };
    if !name.as_str().eq_ignore_ascii_case("unset") || args.len() != 1 {
        return None;
    }
    let ExprKind::ArrayAccess { array, index } = &args[0].kind else {
        return None;
    };
    if !matches!(&array.kind, ExprKind::Variable(base) if base == "GLOBALS") {
        return None;
    }
    match &index.kind {
        ExprKind::StringLiteral(name) => Some(name.clone()),
        _ => None,
    }
}

/// Returns whether one of the body's target unsets sits below the top level.
///
/// A direct `unset($GLOBALS[$name])` statement is handled by the rename path; anything inside an
/// `if`, loop, `switch`, `try`, or synthetic block needs the path-sensitive rewrite.
fn has_nested_unset(statements: &[Stmt], name: &str) -> bool {
    statements.iter().any(|stmt| {
        if direct_global_unset(stmt).as_deref() == Some(name) {
            return false;
        }
        nested_unset_in(stmt, name)
    })
}

/// Reports whether one statement contains a target unset below it.
///
/// Nested function-like bodies are separate scopes: their own `global`/unset pairs are rewritten
/// when `rewrite_nested_bodies` reaches them, so this scan stops at their boundary.
fn nested_unset_in(stmt: &Stmt, name: &str) -> bool {
    let list_has = |list: &[Stmt]| -> bool {
        list.iter().any(|stmt| {
            direct_global_unset(stmt).as_deref() == Some(name) || nested_unset_in(stmt, name)
        })
    };
    match &stmt.kind {
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            list_has(then_body)
                || elseif_clauses.iter().any(|(_, body)| list_has(body))
                || else_body.as_deref().is_some_and(list_has)
        }
        StmtKind::IfDef {
            then_body, else_body, ..
        } => list_has(then_body) || else_body.as_deref().is_some_and(list_has),
        StmtKind::While { body, .. } | StmtKind::DoWhile { body, .. } => list_has(body),
        StmtKind::For {
            init, update, body, ..
        } => {
            init.as_deref().is_some_and(|stmt| list_has(std::slice::from_ref(stmt)))
                || update
                    .as_deref()
                    .is_some_and(|stmt| list_has(std::slice::from_ref(stmt)))
                || list_has(body)
        }
        StmtKind::Foreach { body, .. } => list_has(body),
        StmtKind::Switch { cases, default, .. } => {
            cases.iter().any(|(_, body)| list_has(body)) || default.as_deref().is_some_and(list_has)
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            list_has(try_body)
                || catches.iter().any(|catch| list_has(&catch.body))
                || finally_body.as_deref().is_some_and(list_has)
        }
        StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body) => list_has(body),
        _ => false,
    }
}

/// Returns whether `stmt` is the hidden capture this pass inserts before an unset.
fn is_detach_capture(stmt: &Stmt, name: &str) -> bool {
    let StmtKind::Assign { name: hidden, value } = &stmt.kind else {
        return false;
    };
    hidden.starts_with("__elephc_detach_global_")
        && matches!(&value.kind, ExprKind::Variable(variable) if variable == name)
}

/// Returns whether a later statement re-declares `global $name`, which would reattach it.
pub(super) fn reattaches_later(statements: &[Stmt], name: &str) -> bool {
    statements.iter().any(|stmt| match &stmt.kind {
        StmtKind::Global { vars } => vars.iter().any(|var| var == name),
        _ => false,
    })
}

/// Rewrites `$name` uses inside the unset statement itself, if any expression mentions it.
fn rewrite_after_unset(stmt: &mut Stmt, name: &str, hidden: &str) {
    if let StmtKind::ExprStmt(expr) = &mut stmt.kind {
        rewrite_expr(expr, name, hidden);
    }
}

/// Recursively rewrites `$name` variable expressions in one statement.
fn rewrite_variables(stmt: &mut Stmt, name: &str, hidden: &str) {
    match &mut stmt.kind {
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::Return(Some(expr))
        | StmtKind::Include { path: expr, .. } => rewrite_expr(expr, name, hidden),
        StmtKind::Assign { name: target, value }
        | StmtKind::TypedAssign { name: target, value, .. } => {
            if target == name {
                *target = hidden.to_string();
            }
            rewrite_expr(value, name, hidden);
        }
        StmtKind::RefAssign { target, source } => {
            if target == name {
                *target = hidden.to_string();
            }
            rewrite_expr(source, name, hidden);
        }
        StmtKind::ConstDecl { value, .. }
        | StmtKind::ListUnpack { value, .. }
        | StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. }
        | StmtKind::StaticVar { init: value, .. } => rewrite_expr(value, name, hidden),
        StmtKind::ArrayPush { array, value } => {
            if array == name {
                *array = hidden.to_string();
            }
            rewrite_expr(value, name, hidden);
        }
        StmtKind::ArrayAssign { array, index, value } => {
            if array == name {
                *array = hidden.to_string();
            }
            rewrite_expr(index, name, hidden);
            rewrite_expr(value, name, hidden);
        }
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            rewrite_expr(index, name, hidden);
            rewrite_expr(value, name, hidden);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            rewrite_expr(target, name, hidden);
            rewrite_expr(value, name, hidden);
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            rewrite_expr(object, name, hidden);
            rewrite_expr(value, name, hidden);
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            rewrite_expr(object, name, hidden);
            rewrite_expr(index, name, hidden);
            rewrite_expr(value, name, hidden);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            rewrite_expr(condition, name, hidden);
            for stmt in then_body {
                rewrite_variables(stmt, name, hidden);
            }
            for (condition, body) in elseif_clauses {
                rewrite_expr(condition, name, hidden);
                for stmt in body {
                    rewrite_variables(stmt, name, hidden);
                }
            }
            if let Some(body) = else_body {
                for stmt in body {
                    rewrite_variables(stmt, name, hidden);
                }
            }
        }
        StmtKind::IfDef {
            then_body, else_body, ..
        } => {
            for stmt in then_body {
                rewrite_variables(stmt, name, hidden);
            }
            if let Some(body) = else_body {
                for stmt in body {
                    rewrite_variables(stmt, name, hidden);
                }
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            rewrite_expr(condition, name, hidden);
            for stmt in body {
                rewrite_variables(stmt, name, hidden);
            }
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
            ..
        } => {
            rewrite_expr(array, name, hidden);
            if key_var.as_deref() == Some(name) {
                *key_var = Some(hidden.to_string());
            }
            if value_var == name {
                *value_var = hidden.to_string();
            }
            for stmt in body {
                rewrite_variables(stmt, name, hidden);
            }
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                rewrite_variables(init, name, hidden);
            }
            if let Some(condition) = condition {
                rewrite_expr(condition, name, hidden);
            }
            if let Some(update) = update {
                rewrite_variables(update, name, hidden);
            }
            for stmt in body {
                rewrite_variables(stmt, name, hidden);
            }
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            rewrite_expr(subject, name, hidden);
            for (labels, body) in cases {
                for label in labels {
                    rewrite_expr(label, name, hidden);
                }
                for stmt in body {
                    rewrite_variables(stmt, name, hidden);
                }
            }
            if let Some(body) = default {
                for stmt in body {
                    rewrite_variables(stmt, name, hidden);
                }
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            for stmt in try_body {
                rewrite_variables(stmt, name, hidden);
            }
            for catch in catches {
                for stmt in &mut catch.body {
                    rewrite_variables(stmt, name, hidden);
                }
            }
            if let Some(body) = finally_body {
                for stmt in body {
                    rewrite_variables(stmt, name, hidden);
                }
            }
        }
        StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body) => {
            for stmt in body {
                rewrite_variables(stmt, name, hidden);
            }
        }
        StmtKind::FunctionDecl { body, .. } => {
            for stmt in body {
                rewrite_variables(stmt, name, hidden);
            }
        }
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. }
        | StmtKind::EnumDecl { methods, .. } => {
            // A closure created after the unset captures the detached local only when the
            // method body itself is rewritten; each method is its own body.
            for method in methods {
                for stmt in &mut method.body {
                    rewrite_variables(stmt, name, hidden);
                }
            }
        }
        StmtKind::Return(None)
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::Global { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        // `goto` lowering runs before this pass, so no jump statement reaches it.
        | StmtKind::Goto(_)
        | StmtKind::GotoLabel(_) => {}
    }
}

/// Recursively rewrites one expression's variable references and nested closure bodies.
fn rewrite_expr(expr: &mut Expr, name: &str, hidden: &str) {
    match &mut expr.kind {
        ExprKind::Variable(variable) if variable == name => {
            *variable = hidden.to_string();
        }
        ExprKind::Closure {
            body,
            params,
            captures,
            capture_refs,
            ..
        } => {
            for (_, _, default, _) in params {
                if let Some(default) = default {
                    rewrite_expr(default, name, hidden);
                }
            }
            for capture in captures.iter_mut().chain(capture_refs.iter_mut()) {
                if capture == name {
                    *capture = hidden.to_string();
                }
            }
            for stmt in body {
                rewrite_variables(stmt, name, hidden);
            }
        }
        ExprKind::Assignment {
            target,
            value,
            prelude,
            result_target,
            ..
        } => {
            for stmt in prelude {
                rewrite_variables(stmt, name, hidden);
            }
            rewrite_expr(target, name, hidden);
            rewrite_expr(value, name, hidden);
            if let Some(target) = result_target {
                rewrite_expr(target, name, hidden);
            }
        }
        ExprKind::BinaryOp { left, right, .. } => {
            rewrite_expr(left, name, hidden);
            rewrite_expr(right, name, hidden);
        }
        ExprKind::Reference(value)
        | ExprKind::InstanceOf { value, .. }
        | ExprKind::Negate(value)
        | ExprKind::Not(value)
        | ExprKind::BitNot(value)
        | ExprKind::Throw(value)
        | ExprKind::ErrorSuppress(value)
        | ExprKind::Print(value)
        | ExprKind::Cast { expr: value, .. }
        | ExprKind::NamedArg { value, .. }
        | ExprKind::IncludeValue { path: value, .. }
        | ExprKind::Spread(value)
        | ExprKind::Clone(value)
        | ExprKind::PropertyAccess { object: value, .. }
        | ExprKind::NullsafePropertyAccess { object: value, .. }
        | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::BufferNew { len: value, .. }
        | ExprKind::ObjectClassName { object: value }
        | ExprKind::YieldFrom(value) => rewrite_expr(value, name, hidden),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            rewrite_expr(value, name, hidden);
            rewrite_expr(default, name, hidden);
        }
        ExprKind::Pipe { value, callable } => {
            rewrite_expr(value, name, hidden);
            rewrite_expr(callable, name, hidden);
        }
        ExprKind::ArrayAccess { array, index } => {
            rewrite_expr(array, name, hidden);
            rewrite_expr(index, name, hidden);
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            rewrite_expr(object, name, hidden);
            rewrite_expr(property, name, hidden);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            rewrite_expr(condition, name, hidden);
            rewrite_expr(then_expr, name, hidden);
            rewrite_expr(else_expr, name, hidden);
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            rewrite_expr(subject, name, hidden);
            for (conditions, result) in arms {
                for condition in conditions {
                    rewrite_expr(condition, name, hidden);
                }
                rewrite_expr(result, name, hidden);
            }
            if let Some(default) = default {
                rewrite_expr(default, name, hidden);
            }
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewScopedObject { args, .. }
        | ExprKind::ArrayLiteral(args) => {
            for arg in args {
                rewrite_expr(arg, name, hidden);
            }
        }
        ExprKind::ArrayLiteralAssoc(entries) => {
            for (key, value) in entries {
                rewrite_expr(key, name, hidden);
                rewrite_expr(value, name, hidden);
            }
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            for entry in entries {
                for operand in entry.exprs_mut() {
                    rewrite_expr(operand, name, hidden);
                }
            }
        }
        ExprKind::ExprCall { callee, args } => {
            rewrite_expr(callee, name, hidden);
            for arg in args {
                rewrite_expr(arg, name, hidden);
            }
        }
        ExprKind::NewDynamic {
            name_expr: receiver,
            args,
        }
        | ExprKind::NewDynamicObject {
            class_name: receiver,
            args,
            ..
        }
        | ExprKind::MethodCall {
            object: receiver,
            args,
            ..
        }
        | ExprKind::NullsafeMethodCall {
            object: receiver,
            args,
            ..
        } => {
            rewrite_expr(receiver, name, hidden);
            for arg in args {
                rewrite_expr(arg, name, hidden);
            }
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            rewrite_expr(object, name, hidden);
            rewrite_expr(method, name, hidden);
            for arg in args {
                rewrite_expr(arg, name, hidden);
            }
        }
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                rewrite_expr(key, name, hidden);
            }
            if let Some(value) = value {
                rewrite_expr(value, name, hidden);
            }
        }
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::This
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_)
        | ExprKind::Runtime(_) => {}
        ExprKind::Variable(_) => {}
        ExprKind::NewGeneric { args, .. } => {
            for arg in args {
                rewrite_expr(arg, name, hidden);
            }
        }
    }
}

/// Descends into every nested function-like body once per statement.
fn rewrite_nested_bodies(stmt: &mut Stmt, counter: &mut usize) {
    match &mut stmt.kind {
        StmtKind::FunctionDecl { body, .. } => {
            *body = rewrite_body(std::mem::take(body), counter);
        }
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. }
        | StmtKind::EnumDecl { methods, .. } => {
            for method in methods {
                method.body = rewrite_body(std::mem::take(&mut method.body), counter);
            }
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            *then_body = rewrite_body(std::mem::take(then_body), counter);
            for (_, body) in elseif_clauses {
                *body = rewrite_body(std::mem::take(body), counter);
            }
            if let Some(body) = else_body {
                *body = rewrite_body(std::mem::take(body), counter);
            }
        }
        StmtKind::IfDef {
            then_body, else_body, ..
        } => {
            *then_body = rewrite_body(std::mem::take(then_body), counter);
            if let Some(body) = else_body {
                *body = rewrite_body(std::mem::take(body), counter);
            }
        }
        StmtKind::While { body, .. } | StmtKind::DoWhile { body, .. } => {
            *body = rewrite_body(std::mem::take(body), counter);
        }
        StmtKind::Foreach { body, .. } | StmtKind::For { body, .. } => {
            *body = rewrite_body(std::mem::take(body), counter);
        }
        StmtKind::Switch { cases, default, .. } => {
            for (_, body) in cases {
                *body = rewrite_body(std::mem::take(body), counter);
            }
            if let Some(body) = default {
                *body = rewrite_body(std::mem::take(body), counter);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            *try_body = rewrite_body(std::mem::take(try_body), counter);
            for catch in catches {
                catch.body = rewrite_body(std::mem::take(&mut catch.body), counter);
            }
            if let Some(body) = finally_body {
                *body = rewrite_body(std::mem::take(body), counter);
            }
        }
        StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body) => {
            *body = rewrite_body(std::mem::take(body), counter);
        }
        _ => {}
    }
}
