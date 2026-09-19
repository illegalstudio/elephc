//! Purpose:
//! Pre-declares the hidden internal-array-pointer cursor slots used inside one loop body,
//! before that body is lowered.
//!
//! Called from:
//! - `crate::ir_lower::stmt::lower_stmt()` when it reaches a looping statement.
//!
//! Key details:
//! - WHY THIS EXISTS: `LoweringContext::store_local` rewinds a local's cursor whenever the
//!   variable is bound to a different array, but it can only do that once the cursor slot
//!   exists, and the slot is created lazily at the variable's first pointer call. Outside a
//!   loop, lowering order matches execution order, so a store lowered before the first
//!   pointer call also RUNS before it and the entry-block seed of `0` is already correct.
//!   Inside a loop that correspondence breaks: `for (...) { $z = [7,8]; ...; next($z); }`
//!   lowers the store first, so without this pre-pass the second iteration would inherit
//!   the first iteration's cursor. Declaring the slots up front makes the store hook fire.
//! - The scan is deliberately BEST-EFFORT and safe in both directions. Over-approximating
//!   costs one unused `i64` frame slot plus one seed store; under-approximating (an
//!   expression shape this walker does not recurse into) only restores the behaviour that
//!   existed before the pre-pass. Neither can miscompile, which is why the expression walk
//!   ends in a catch-all instead of an exhaustive match.
//! - Closure bodies are NOT scanned: a closure is lowered into its own function with its
//!   own frame, so its cursors belong to that frame.

use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};

use super::context::LoweringContext;

/// Declares a cursor slot for every plain-variable receiver of an internal-array-pointer
/// builtin appearing inside `body`.
///
/// Call this before lowering a loop body so that assignments inside the body see an
/// existing cursor slot and emit their rewind.
pub(crate) fn predeclare_loop_cursors(ctx: &mut LoweringContext<'_, '_>, body: &[Stmt]) {
    let mut receivers = Vec::new();
    scan_stmts(body, &mut receivers);
    for name in receivers {
        ctx.array_pointer_cursor_slot(&name);
    }
}

/// Records `name` as a pointer receiver unless it is already known.
fn record(receivers: &mut Vec<String>, name: &str) {
    if !receivers.iter().any(|known| known == name) {
        receivers.push(name.to_string());
    }
}

/// Collects pointer receivers from a statement list.
fn scan_stmts(stmts: &[Stmt], receivers: &mut Vec<String>) {
    for stmt in stmts {
        scan_stmt(stmt, receivers);
    }
}

/// Collects pointer receivers from one statement and every statement list it owns.
fn scan_stmt(stmt: &Stmt, receivers: &mut Vec<String>) {
    match &stmt.kind {
        StmtKind::Echo(expr) | StmtKind::Throw(expr) | StmtKind::ExprStmt(expr) => {
            scan_expr(expr, receivers)
        }
        StmtKind::Return(expr) => {
            if let Some(expr) = expr {
                scan_expr(expr, receivers);
            }
        }
        StmtKind::Assign { value, .. } | StmtKind::TypedAssign { value, .. } => {
            scan_expr(value, receivers)
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            scan_expr(index, receivers);
            scan_expr(value, receivers);
        }
        StmtKind::ArrayPush { value, .. } => scan_expr(value, receivers),
        StmtKind::Synthetic(body) => scan_stmts(body, receivers),
        StmtKind::If {
            condition,
            then_body,
            else_body,
            ..
        } => {
            scan_expr(condition, receivers);
            scan_stmts(then_body, receivers);
            if let Some(body) = else_body {
                scan_stmts(body, receivers);
            }
        }
        StmtKind::While { condition, body }
        | StmtKind::DoWhile { condition, body } => {
            scan_expr(condition, receivers);
            scan_stmts(body, receivers);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                scan_stmt(init, receivers);
            }
            if let Some(condition) = condition {
                scan_expr(condition, receivers);
            }
            if let Some(update) = update {
                scan_stmt(update, receivers);
            }
            scan_stmts(body, receivers);
        }
        StmtKind::Foreach { array, body, .. } => {
            scan_expr(array, receivers);
            scan_stmts(body, receivers);
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            scan_expr(subject, receivers);
            for (case_exprs, body) in cases {
                for case in case_exprs {
                    scan_expr(case, receivers);
                }
                scan_stmts(body, receivers);
            }
            if let Some(body) = default {
                scan_stmts(body, receivers);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            scan_stmts(try_body, receivers);
            for catch in catches {
                scan_stmts(&catch.body, receivers);
            }
            if let Some(body) = finally_body {
                scan_stmts(body, receivers);
            }
        }
        // Every other statement either owns no expression that can hold a pointer call in
        // this frame (declarations, `break`, `use`, …) or lowers into its own function.
        _ => {}
    }
}

/// Collects pointer receivers from one expression and its sub-expressions.
///
/// The final catch-all is intentional: see the module preamble for why an incomplete walk
/// is safe here.
fn scan_expr(expr: &Expr, receivers: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::FunctionCall { name, args } => {
            if let Some(name) = pointer_receiver_name(name.as_str(), args) {
                record(receivers, name);
            }
            for arg in args {
                scan_expr(arg, receivers);
            }
        }
        ExprKind::BinaryOp { left, right, .. } => {
            scan_expr(left, receivers);
            scan_expr(right, receivers);
        }
        ExprKind::NullCoalesce { value, default } => {
            scan_expr(value, receivers);
            scan_expr(default, receivers);
        }
        ExprKind::Pipe { value, callable } => {
            scan_expr(value, receivers);
            scan_expr(callable, receivers);
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::Clone(inner)
        | ExprKind::YieldFrom(inner) => scan_expr(inner, receivers),
        ExprKind::Assignment { value, .. } => scan_expr(value, receivers),
        ExprKind::Cast { expr: inner, .. } => scan_expr(inner, receivers),
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                scan_expr(item, receivers);
            }
        }
        ExprKind::ArrayLiteralAssoc(entries) => {
            for (key, value) in entries {
                scan_expr(key, receivers);
                scan_expr(value, receivers);
            }
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            for entry in entries {
                for expr in entry.exprs() {
                    scan_expr(expr, receivers);
                }
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            scan_expr(array, receivers);
            scan_expr(index, receivers);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            scan_expr(condition, receivers);
            scan_expr(then_expr, receivers);
            scan_expr(else_expr, receivers);
        }
        ExprKind::ShortTernary { value, default } => {
            scan_expr(value, receivers);
            scan_expr(default, receivers);
        }
        ExprKind::NamedArg { value, .. } => scan_expr(value, receivers),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            scan_expr(object, receivers);
            for arg in args {
                scan_expr(arg, receivers);
            }
        }
        ExprKind::StaticMethodCall { args, .. } | ExprKind::NewObject { args, .. } => {
            for arg in args {
                scan_expr(arg, receivers);
            }
        }
        ExprKind::ClosureCall { args, .. } | ExprKind::ExprCall { args, .. } => {
            for arg in args {
                scan_expr(arg, receivers);
            }
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => scan_expr(object, receivers),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            scan_expr(subject, receivers);
            for (conditions, body) in arms {
                for condition in conditions {
                    scan_expr(condition, receivers);
                }
                scan_expr(body, receivers);
            }
            if let Some(default) = default {
                scan_expr(default, receivers);
            }
        }
        // Leaves, declarations, and forms lowered into their own frame (closures) stop the
        // walk. Missing a container here only forgoes a pre-declaration; it cannot produce
        // a wrong cursor.
        _ => {}
    }
}

/// Returns the receiver variable name when `name`/`args` is an internal-array-pointer call.
///
/// The builtin is recognized through the registry's typed argument-lowering descriptor, the
/// same metadata EIR lowering dispatches on, rather than by matching PHP name strings here.
fn pointer_receiver_name<'a>(name: &str, args: &'a [Expr]) -> Option<&'a str> {
    if args.len() != 1 {
        return None;
    }
    let canonical = crate::names::php_symbol_key(name.trim_start_matches('\\'));
    let def = crate::builtins::registry::lookup(&canonical)?;
    if !matches!(
        def.spec.semantics.argument_lowering,
        crate::builtins::semantics::BuiltinArgumentLowering::ArrayInternalPointer(_)
    ) {
        return None;
    }
    match &args[0].kind {
        ExprKind::Variable(variable) => Some(variable.as_str()),
        _ => None,
    }
}
