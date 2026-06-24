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

use std::collections::HashSet;

use super::walk::collect_called_function_names;
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

/// Optional `autoload.files` helper functions whose definition guards are pruned when the
/// program never calls them, keeping the heavy classes their bodies reference out of the
/// closed-world closure. Names are canonical fully-qualified, lowercased, to match PHP's
/// case-insensitive, namespace-aware function resolution.
///
/// - `Symfony\Component\String\{u,b,s}` (`symfony/string` `Resources/functions.php`) construct
///   `UnicodeString` / `ByteString`.
/// - `dump` / `dd` (`symfony/var-dumper` `Resources/functions/dump.php`) construct `VarDumper`.
const OPTIONAL_HELPER_FUNCTIONS: &[&str] = &[
    "symfony\\component\\string\\u",
    "symfony\\component\\string\\b",
    "symfony\\component\\string\\s",
    "dump",
    "dd",
];

/// Removes provided-function polyfill guards from the program, replacing each matched
/// `if`/`else` with its statically live branch and recursing into nested bodies.
pub fn prune_provided_function_polyfills(program: Program) -> Program {
    prune_stmt_list(program, &provided_guard_live_branch)
}

/// Removes definition guards (`if (!function_exists('X')) { function X(...) { ... } }`) for the
/// optional `autoload.files` helpers in `OPTIONAL_HELPER_FUNCTIONS` that the program never calls,
/// so the heavy classes those helper bodies reference (Symfony's `UnicodeString`/`ByteString`
/// behind `u()`/`b()`, `VarDumper` behind `dump()`/`dd()`) never enter the closed-world closure.
///
/// Conservative: a helper is dropped only when no `FunctionCall` or first-class callable names it
/// anywhere in the program assembled so far (the main file plus eagerly-spliced helpers). A later
/// direct call from a lazily-loaded class would surface a clean "undefined function" compile error
/// rather than a miscompile.
pub fn prune_unused_optional_helpers(program: Program) -> Program {
    let called = collect_called_function_names(&program);
    prune_stmt_list(program, &move |stmt| {
        unused_optional_guard_live_branch(stmt, &called)
    })
}

/// Rewrites a statement list, splicing in the live branch of every guard `classify` matches and
/// recursing into the child statement lists of all other statements.
fn prune_stmt_list(
    stmts: Vec<Stmt>,
    classify: &dyn Fn(Stmt) -> Result<Vec<Stmt>, Stmt>,
) -> Vec<Stmt> {
    let mut out = Vec::with_capacity(stmts.len());
    for stmt in stmts {
        match classify(stmt) {
            Ok(live) => out.extend(prune_stmt_list(live, classify)),
            Err(stmt) => out.push(recurse_stmt(stmt, classify)),
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
fn recurse_stmt(stmt: Stmt, classify: &dyn Fn(Stmt) -> Result<Vec<Stmt>, Stmt>) -> Stmt {
    let Stmt { kind, span, attributes } = stmt;
    let kind = match kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => StmtKind::If {
            condition,
            then_body: prune_stmt_list(then_body, classify),
            elseif_clauses: elseif_clauses
                .into_iter()
                .map(|(cond, body)| (cond, prune_stmt_list(body, classify)))
                .collect(),
            else_body: else_body.map(|body| prune_stmt_list(body, classify)),
        },
        StmtKind::While { condition, body } => StmtKind::While {
            condition,
            body: prune_stmt_list(body, classify),
        },
        StmtKind::DoWhile { body, condition } => StmtKind::DoWhile {
            body: prune_stmt_list(body, classify),
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
            body: prune_stmt_list(body, classify),
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
            body: prune_stmt_list(body, classify),
        },
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => StmtKind::Switch {
            subject,
            cases: cases
                .into_iter()
                .map(|(exprs, body)| (exprs, prune_stmt_list(body, classify)))
                .collect(),
            default: default.map(|body| prune_stmt_list(body, classify)),
        },
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => StmtKind::Try {
            try_body: prune_stmt_list(try_body, classify),
            catches: catches
                .into_iter()
                .map(|mut catch| {
                    catch.body = prune_stmt_list(catch.body, classify);
                    catch
                })
                .collect(),
            finally_body: finally_body.map(|body| prune_stmt_list(body, classify)),
        },
        StmtKind::Synthetic(body) => StmtKind::Synthetic(prune_stmt_list(body, classify)),
        StmtKind::IncludeOnceGuard { label, body } => StmtKind::IncludeOnceGuard {
            label,
            body: prune_stmt_list(body, classify),
        },
        StmtKind::NamespaceBlock { name, body } => StmtKind::NamespaceBlock {
            name,
            body: prune_stmt_list(body, classify),
        },
        other => other,
    };
    Stmt {
        kind,
        span,
        attributes,
    }
}

/// Classifies a statement for unused-optional-helper pruning. Returns `Ok(live_branch)` to
/// replace a definition guard `if (!function_exists('X')) { function X(...) { ... } }` with its
/// (typically empty) else branch when `X` is an optional helper the program never calls; returns
/// `Err(stmt)` to leave the statement in place (and let `recurse_stmt` descend into its bodies).
fn unused_optional_guard_live_branch(
    stmt: Stmt,
    called: &HashSet<String>,
) -> Result<Vec<Stmt>, Stmt> {
    let Stmt { kind, span, attributes } = stmt;
    match kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } if elseif_clauses.is_empty()
            && is_unused_optional_def_guard(&condition, &then_body, called) =>
        {
            Ok(else_body.unwrap_or_default())
        }
        kind => Err(Stmt {
            kind,
            span,
            attributes,
        }),
    }
}

/// Returns whether an `if` is the definition guard of an optional helper the program never calls:
/// the condition is `!function_exists(...)` and the then-body declares a function whose canonical
/// name is in `OPTIONAL_HELPER_FUNCTIONS` and absent from `called`. Matching the declared function
/// keeps this robust to the guard argument form (`'Name'` vs `Name::class`, which is not yet
/// constant-folded at autoload time).
fn is_unused_optional_def_guard(
    condition: &Expr,
    then_body: &[Stmt],
    called: &HashSet<String>,
) -> bool {
    let ExprKind::Not(inner) = &condition.kind else {
        return false;
    };
    let ExprKind::FunctionCall { name, .. } = &inner.kind else {
        return false;
    };
    if !name
        .as_str()
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("function_exists")
    {
        return false;
    }
    then_body.iter().any(|stmt| match &stmt.kind {
        StmtKind::FunctionDecl { name, .. } => {
            let key = name.trim_start_matches('\\').to_ascii_lowercase();
            OPTIONAL_HELPER_FUNCTIONS.contains(&key.as_str()) && !called.contains(&key)
        }
        _ => false,
    })
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

    /// Builds `if (!function_exists('name')) { function name() {} }`, the definition-guard shape
    /// the unused-optional-helper prune matches (by the declared function, not the guard argument).
    fn guard_def(name: &str) -> Stmt {
        let def = Stmt::new(
            StmtKind::FunctionDecl {
                name: name.to_string(),
                params: Vec::new(),
                variadic: None,
                variadic_type: None,
                return_type: None,
                body: Vec::new(),
            },
            Span::dummy(),
        );
        Stmt::new(
            StmtKind::If {
                condition: Expr::new(
                    ExprKind::Not(Box::new(function_exists_call(name))),
                    Span::dummy(),
                ),
                then_body: vec![def],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            Span::dummy(),
        )
    }

    /// Builds a `name();` call statement.
    fn call_stmt(name: &str) -> Stmt {
        Stmt::new(
            StmtKind::ExprStmt(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified(name),
                    args: Vec::new(),
                },
                Span::dummy(),
            )),
            Span::dummy(),
        )
    }

    /// An optional helper (`dump`) defined but never called is pruned, dropping its definition.
    #[test]
    fn prunes_unused_optional_helper_definition() {
        let program = vec![guard_def("dump")];
        let pruned = prune_unused_optional_helpers(program);
        assert!(pruned.is_empty(), "unused optional helper should be removed");
    }

    /// An optional helper that the program calls is kept, so its definition (and any class its
    /// body would reference) survives.
    #[test]
    fn keeps_called_optional_helper_definition() {
        let program = vec![guard_def("dump"), call_stmt("dump")];
        let pruned = prune_unused_optional_helpers(program);
        assert_eq!(pruned.len(), 2, "called optional helper guard must be preserved");
        assert!(matches!(pruned[0].kind, StmtKind::If { .. }));
    }

    /// A `function_exists` definition guard for a function not in the optional allowlist is left
    /// intact even when uncalled, confirming the prune is allowlist-scoped.
    #[test]
    fn leaves_non_optional_unused_helper_intact() {
        let program = vec![guard_def("my_app_helper")];
        let pruned = prune_unused_optional_helpers(program);
        assert_eq!(pruned.len(), 1, "non-allowlisted guard should be preserved");
        assert!(matches!(pruned[0].kind, StmtKind::If { .. }));
    }
}
