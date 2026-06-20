//! Purpose:
//! Compile-time pruning of PHP polyfill redefinition guards for functions elephc
//! declares to provide. Removes `if (!function_exists('X')) { function X() {...} }`
//! (and the inverse `if (function_exists('X'))`) blocks for an allowlisted set of
//! provided functions so their bodies — and any class references those bodies
//! carry — never enter the autoload reference graph.
//!
//! Called from:
//! - `crate::autoload::run()`, after the always-included prefix is assembled and
//!   before class-reference collection.
//!
//! Key details:
//! - Only an explicit allowlist (`PROVIDED_POLYFILL_FUNCTIONS`) is pruned, keeping
//!   the blast radius small: unrelated `function_exists` guards are left untouched.
//! - The rewrite is purely structural over `if` / `!` / `function_exists('literal')`.
//!   It never reasons about control flow, returns, or runtime values, so it cannot
//!   change the behavior of any code outside a matched guard.
//! - Guards with `elseif` clauses are left alone; the polyfill idiom never uses them
//!   and skipping them avoids having to model PHP's elseif-chain evaluation.

use crate::parser::ast::{Expr, ExprKind, Program, Stmt, StmtKind};

/// Functions elephc declares to provide, whose PHP polyfill redefinition guards are
/// pruned at compile time.
///
/// These are the PHP 8.5 `deepclone` surface that the `symfony/polyfill-deepclone`
/// package guards with `if (!function_exists('X')) { function X(...) { ... } }`.
/// The wrapper bodies delegate to a 97 KB `DeepClone` class; pruning the guards keeps
/// that class out of the closed-world compile when nothing calls the functions.
/// Comparison is case-insensitive to match PHP function-name semantics.
const PROVIDED_POLYFILL_FUNCTIONS: &[&str] = &[
    "deepclone_to_array",
    "deepclone_from_array",
    "deepclone_hydrate",
];

/// Removes provided-function polyfill guards from the program, replacing each matched
/// `if`/`else` with its statically live branch and recursing into nested bodies.
pub fn prune_provided_function_polyfills(program: Program) -> Program {
    prune_stmt_list(program)
}

/// Rewrites a statement list, splicing in the live branch of every provided-function
/// guard and recursing into the child statement lists of all other statements.
fn prune_stmt_list(stmts: Vec<Stmt>) -> Vec<Stmt> {
    let mut out = Vec::with_capacity(stmts.len());
    for stmt in stmts {
        match provided_guard_live_branch(stmt) {
            Ok(live) => out.extend(prune_stmt_list(live)),
            Err(stmt) => out.push(recurse_stmt(stmt)),
        }
    }
    out
}

/// If `stmt` is a provided-function guard (`if (function_exists('X'))` or
/// `if (!function_exists('X'))` with no `elseif` clauses), returns `Ok` with the
/// statically live branch's statements. Otherwise returns `Err(stmt)` unchanged.
fn provided_guard_live_branch(stmt: Stmt) -> Result<Vec<Stmt>, Stmt> {
    let Stmt { kind, span, attributes } = stmt;
    match kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } if elseif_clauses.is_empty() => match provided_function_exists_condition(&condition) {
            // `function_exists('X')` is true for a provided function: the then-branch lives.
            Some(true) => Ok(then_body),
            // `!function_exists('X')` is false for a provided function: the else-branch lives.
            Some(false) => Ok(else_body.unwrap_or_default()),
            None => Err(Stmt {
                kind: StmtKind::If {
                    condition,
                    then_body,
                    elseif_clauses,
                    else_body,
                },
                span,
                attributes,
            }),
        },
        kind => Err(Stmt {
            kind,
            span,
            attributes,
        }),
    }
}

/// Recurses into the child statement lists of control-flow and grouping statements,
/// leaving leaf statements untouched. Declaration bodies (functions, classes) are not
/// descended into: the polyfill idiom never nests its guards there.
fn recurse_stmt(stmt: Stmt) -> Stmt {
    let Stmt { kind, span, attributes } = stmt;
    let kind = match kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => StmtKind::If {
            condition,
            then_body: prune_stmt_list(then_body),
            elseif_clauses: elseif_clauses
                .into_iter()
                .map(|(cond, body)| (cond, prune_stmt_list(body)))
                .collect(),
            else_body: else_body.map(prune_stmt_list),
        },
        StmtKind::While { condition, body } => StmtKind::While {
            condition,
            body: prune_stmt_list(body),
        },
        StmtKind::DoWhile { body, condition } => StmtKind::DoWhile {
            body: prune_stmt_list(body),
            condition,
        },
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => StmtKind::For {
            init,
            condition,
            update,
            body: prune_stmt_list(body),
        },
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            value_by_ref,
            body,
        } => StmtKind::Foreach {
            array,
            key_var,
            value_var,
            value_by_ref,
            body: prune_stmt_list(body),
        },
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => StmtKind::Switch {
            subject,
            cases: cases
                .into_iter()
                .map(|(exprs, body)| (exprs, prune_stmt_list(body)))
                .collect(),
            default: default.map(prune_stmt_list),
        },
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => StmtKind::Try {
            try_body: prune_stmt_list(try_body),
            catches: catches
                .into_iter()
                .map(|mut catch| {
                    catch.body = prune_stmt_list(catch.body);
                    catch
                })
                .collect(),
            finally_body: finally_body.map(prune_stmt_list),
        },
        StmtKind::Synthetic(body) => StmtKind::Synthetic(prune_stmt_list(body)),
        StmtKind::IncludeOnceGuard { label, body } => StmtKind::IncludeOnceGuard {
            label,
            body: prune_stmt_list(body),
        },
        StmtKind::NamespaceBlock { name, body } => StmtKind::NamespaceBlock {
            name,
            body: prune_stmt_list(body),
        },
        other => other,
    };
    Stmt {
        kind,
        span,
        attributes,
    }
}

/// Classifies an `if` condition as a provided-function existence guard.
///
/// Returns `Some(true)` for `function_exists('X')` and `Some(false)` for
/// `!function_exists('X')` when `X` is in `PROVIDED_POLYFILL_FUNCTIONS`; `None`
/// otherwise.
fn provided_function_exists_condition(condition: &Expr) -> Option<bool> {
    match &condition.kind {
        ExprKind::Not(inner) => is_provided_function_exists_call(inner).then_some(false),
        _ => is_provided_function_exists_call(condition).then_some(true),
    }
}

/// Returns whether `expr` is a `function_exists('X')` call naming a provided function.
///
/// Matches `function_exists` case-insensitively after trimming a leading namespace
/// separator (PHP resolves the call to the global builtin), and requires a single
/// string-literal argument naming one of `PROVIDED_POLYFILL_FUNCTIONS`.
fn is_provided_function_exists_call(expr: &Expr) -> bool {
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return false;
    };
    if !name
        .as_str()
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("function_exists")
    {
        return false;
    }
    let [arg] = args.as_slice() else {
        return false;
    };
    let ExprKind::StringLiteral(fn_name) = &arg.kind else {
        return false;
    };
    let key = fn_name.trim_start_matches('\\');
    PROVIDED_POLYFILL_FUNCTIONS
        .iter()
        .any(|provided| provided.eq_ignore_ascii_case(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::Name;
    use crate::span::Span;

    /// Builds a `function_exists("name")` call expression.
    fn function_exists_call(name: &str) -> Expr {
        Expr::new(
            ExprKind::FunctionCall {
                name: Name::unqualified("function_exists"),
                args: vec![Expr::new(ExprKind::StringLiteral(name.to_string()), Span::dummy())],
            },
            Span::dummy(),
        )
    }

    /// Builds a placeholder statement standing in for a guarded function definition.
    fn marker_stmt() -> Stmt {
        Stmt::new(
            StmtKind::ExprStmt(Expr::new(ExprKind::IntLiteral(1), Span::dummy())),
            Span::dummy(),
        )
    }

    /// Builds `if (condition) { marker_stmt() }` with no elseif/else clauses.
    fn guard_if(condition: Expr) -> Stmt {
        Stmt::new(
            StmtKind::If {
                condition,
                then_body: vec![marker_stmt()],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            Span::dummy(),
        )
    }

    /// `if (!function_exists('deepclone_to_array')) { def }` is removed entirely:
    /// the function is provided, so the redefinition then-branch is dead.
    #[test]
    fn prunes_negated_guard_for_provided_function() {
        let program = vec![guard_if(Expr::new(
            ExprKind::Not(Box::new(function_exists_call("deepclone_to_array"))),
            Span::dummy(),
        ))];
        let pruned = prune_provided_function_polyfills(program);
        assert!(pruned.is_empty(), "provided-function guard should be removed");
    }

    /// `if (function_exists('deepclone_hydrate')) { body }` keeps its then-branch:
    /// the function is provided, so the positive condition is statically true.
    #[test]
    fn keeps_then_branch_for_positive_provided_guard() {
        let program = vec![guard_if(function_exists_call("deepclone_hydrate"))];
        let pruned = prune_provided_function_polyfills(program);
        assert_eq!(pruned.len(), 1, "live then-branch statement should remain");
        assert!(matches!(pruned[0].kind, StmtKind::ExprStmt(_)));
    }

    /// A guard for a function elephc does not provide is left untouched.
    #[test]
    fn leaves_unrelated_function_exists_guard_intact() {
        let program = vec![guard_if(Expr::new(
            ExprKind::Not(Box::new(function_exists_call("some_other_helper"))),
            Span::dummy(),
        ))];
        let pruned = prune_provided_function_polyfills(program);
        assert_eq!(pruned.len(), 1, "unrelated guard should be preserved");
        assert!(matches!(pruned[0].kind, StmtKind::If { .. }));
    }

    /// Matching is case-insensitive and tolerant of a leading namespace separator,
    /// since PHP resolves the call to the global builtin regardless of namespace.
    #[test]
    fn matches_case_insensitively_and_through_namespace_separator() {
        let call = Expr::new(
            ExprKind::FunctionCall {
                name: Name::unqualified("\\Function_Exists"),
                args: vec![Expr::new(
                    ExprKind::StringLiteral("DeepClone_To_Array".to_string()),
                    Span::dummy(),
                )],
            },
            Span::dummy(),
        );
        let program = vec![guard_if(Expr::new(ExprKind::Not(Box::new(call)), Span::dummy()))];
        let pruned = prune_provided_function_polyfills(program);
        assert!(pruned.is_empty(), "case/namespace variations should still prune");
    }

    /// Guards nested inside another statement's body are pruned by recursion.
    #[test]
    fn prunes_guard_nested_in_outer_block() {
        let inner = guard_if(Expr::new(
            ExprKind::Not(Box::new(function_exists_call("deepclone_from_array"))),
            Span::dummy(),
        ));
        let program = vec![Stmt::new(StmtKind::Synthetic(vec![inner]), Span::dummy())];
        let pruned = prune_provided_function_polyfills(program);
        assert_eq!(pruned.len(), 1, "outer wrapper should remain");
        let StmtKind::Synthetic(body) = &pruned[0].kind else {
            panic!("expected synthetic wrapper");
        };
        assert!(body.is_empty(), "nested provided-function guard should be removed");
    }
}
