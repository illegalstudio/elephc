//! Purpose:
//! Desugars the scope builtins `compact()` and `extract()` over the frame's compile-time names.
//!
//! Called from:
//! - `super::rewrite::Rewriter` when it meets a call to either builtin.
//!
//! Key details:
//! - `compact('a', ['b', 'c'])` with literal names becomes `['a' => $a, 'b' => $b, ...]` in
//!   argument order. A name the frame never spells cannot exist, so it is skipped with PHP's
//!   `compact(): Undefined variable $x` warning. A spelled name reads like any direct read of
//!   that local, so the checker's definedness rules apply to it.
//! - `extract($array[, EXTR_OVERWRITE | EXTR_SKIP])` as a statement becomes one
//!   `if (array_key_exists('n', $tmp)) { $n = $tmp['n']; }` per frame name. Keys naming no frame
//!   local would create locals only a dynamic lookup could read, so `extract()` is rejected in a
//!   scope that also reads variables dynamically (`$$x`, `get_defined_vars()`).
//! - `EXTR_SKIP` must not overwrite a local that already exists, which elephc cannot test at
//!   runtime for a null-valued local; it is accepted only as a function-body statement before
//!   which no other local is mentioned and nothing is unset, so the existing set is exactly the
//!   parameters plus the closure's `use` captures (bound on entry, as null when undefined).
//! - A `this` key: `EXTR_SKIP` ignores it; `EXTR_OVERWRITE` writes the keys before it in array
//!   order and then throws `Error("Cannot re-assign $this")`.
//! - Every other mode (prefixes, `EXTR_IF_EXISTS`, `EXTR_REFS`), a runtime mode, the result
//!   value (the import count), top-level code and arrow functions (where it is an expression)
//!   are compile errors.

use std::collections::BTreeSet;

use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind};

use super::rewrite::{builtin_call_name, Rewriter};
use super::walk::{walk_expr_children, Visitor};
use super::{binop, global_call, is_frame_reachable_name, str_lit, var, warning_then_null, FRAME_LOCAL};

/// Placement facts for an `extract()` statement, computed by the enclosing block walk.
pub(super) struct ExtractContext {
    /// The statement is a direct child of the function (or top-level) body.
    pub(super) function_body: bool,
    /// Frame names mentioned by the statements before it in that block.
    pub(super) preceding_names: BTreeSet<String>,
    /// Locals some statement before it `unset()`s.
    pub(super) preceding_unsets: std::collections::HashSet<String>,
}

/// `EXTR_OVERWRITE`, PHP's default extract mode.
const EXTR_OVERWRITE: i64 = 0;
/// `EXTR_SKIP`: keep an existing local.
const EXTR_SKIP: i64 = 1;

impl Rewriter {
    /// Rewrites a `compact(...)` call with literal names into an associative array literal.
    pub(super) fn rewrite_compact(&mut self, expr: &mut Expr) {
        let span = expr.span;
        let ExprKind::FunctionCall { args, .. } = &expr.kind else {
            unreachable!("caller matched a call");
        };
        let mut names = Vec::new();
        for arg in args {
            if !collect_literal_names(arg, &mut names) {
                self.error(
                    span,
                    "compact() is only supported with literal variable names (elephc): a runtime name list cannot be resolved against the frame",
                );
                return;
            }
        }
        let mut seen = BTreeSet::new();
        let mut pairs = Vec::new();
        let mut warnings = Vec::new();
        for name in names {
            if !seen.insert(name.clone()) {
                continue;
            }
            if name == "this" {
                self.error(span, "compact() of $this is not supported (elephc)");
                return;
            }
            let frame = &self.scope.frame;
            let defined = self.scope.definite.contains(&name);
            let never_spelled = !frame.spelled.contains(&name) && !frame.has_extract;
            if defined && is_frame_reachable_name(&name) {
                pairs.push((str_lit(&name, span), var(&name, span)));
            } else if !never_spelled && is_frame_reachable_name(&name) {
                // PHP omits a local that is undefined at runtime; elephc cannot yet test that
                // for a local that is only conditionally assigned here.
                self.error(
                    span,
                    &format!(
                        "compact() of ${name}, which may be undefined at this point, is not supported (elephc): assign it unconditionally before the call"
                    ),
                );
                return;
            } else {
                warnings.push(warning_then_null(
                    str_lit(&format!("compact(): Undefined variable ${name}"), span),
                    span,
                ));
            }
        }
        let array = Expr::new(ExprKind::ArrayLiteralAssoc(pairs), span);
        let mut out = if warnings.is_empty() {
            array
        } else {
            // `(warn1 ?? warn2 ?? ...) ?? array`: every warning is `null`, so all fire in order
            // and the array is the result.
            let mut chain = array;
            for warning in warnings.into_iter().rev() {
                chain = Expr::new(
                    ExprKind::NullCoalesce {
                        value: Box::new(warning),
                        default: Box::new(chain),
                    },
                    span,
                );
            }
            chain
        };
        walk_expr_children(self, &mut out);
        *expr = out;
    }

    /// Rewrites a discarded `extract($array[, $flags])` statement into per-name assignments.
    pub(super) fn rewrite_extract_stmt(&mut self, stmt: &mut Stmt) {
        let span = stmt.span;
        let StmtKind::ExprStmt(expr) = &stmt.kind else {
            unreachable!("caller matched an expression statement");
        };
        let ExprKind::FunctionCall { args, .. } = &expr.kind else {
            unreachable!("caller matched a call");
        };
        if self.scope.top_level {
            self.error(
                span,
                "extract() is only supported inside functions and methods (elephc)",
            );
            return;
        }
        let mode = match args.get(1) {
            None => Some(EXTR_OVERWRITE),
            Some(flag) => extract_flag(flag),
        };
        let Some(mode) = mode.filter(|mode| *mode == EXTR_OVERWRITE || *mode == EXTR_SKIP) else {
            self.error(
                span,
                "extract() is only supported with EXTR_OVERWRITE or EXTR_SKIP given literally (elephc)",
            );
            return;
        };
        if args.is_empty() || args.len() > 2 {
            self.error(span, "extract() expects an array and an optional mode (elephc)");
            return;
        }
        if self.scope.frame.reads_dynamically {
            self.error(
                span,
                "extract() is not supported in a function that also reads variables dynamically (`$$name`, get_defined_vars()) (elephc): keys naming no compile-time local would be lost",
            );
            return;
        }
        // The locals that exist on entry: parameters and closure `use` captures (PHP binds an
        // undefined capture as null, which still exists for `EXTR_SKIP`).
        let entry: std::collections::HashSet<String> = self
            .scope
            .frame
            .params
            .union(&self.scope.frame.captures)
            .cloned()
            .collect();
        if mode == EXTR_SKIP {
            let context = self.extract_context.as_ref();
            let placed_first = context.is_some_and(|context| {
                context.function_body
                    && context.preceding_names.iter().all(|name| entry.contains(name))
                    && context.preceding_unsets.is_empty()
            });
            if !placed_first {
                self.error(
                    span,
                    "extract(..., EXTR_SKIP) is only supported before the function mentions any local other than its parameters and `use` captures, and before any unset() (elephc): elephc cannot tell at runtime whether a null local exists",
                );
                return;
            }
        }
        let mut source = args[0].clone();
        self.expr(&mut source);
        let temp = self.fresh_temp("extract", span);
        let mut out = vec![Stmt::new(
            StmtKind::Assign {
                name: temp.clone(),
                value: source,
            },
            span,
        )];
        let names: Vec<String> = self
            .scope
            .frame
            .names
            .iter()
            .filter(|name| !(mode == EXTR_SKIP && entry.contains(*name)))
            .cloned()
            .collect();
        // A conditional store into a local an earlier `unset()` removed loses its value once
        // another statement follows (the read yields null), so refuse rather than miscompile.
        // Only unsets that can run before this statement count (see `Scope::unset_before`).
        if let Some(name) = names.iter().find(|name| self.scope.unset_before.contains(*name)) {
            let message = format!(
                "extract() is not supported in a function that also unset()s ${name} (elephc): the conditional store into an unset local is not compiled reliably"
            );
            self.error(span, &message);
            return;
        }
        // `EXTR_SKIP` ignores a `this` key. `EXTR_OVERWRITE` writes the keys before it in array
        // order and then throws: a boolean per name records whether its key precedes `this`,
        // so each local keeps a single `$n = $tmp['n']` store (no retyping from a loop value).
        let key_exists =
            |name: &str| global_call("array_key_exists", vec![str_lit(name, span), var(&temp, span)], span);
        let bool_assign = |name: &str, value: Expr| {
            Stmt::new(
                StmtKind::Assign {
                    name: name.to_string(),
                    value,
                },
                span,
            )
        };
        let has_this = (mode == EXTR_OVERWRITE).then(|| self.fresh_temp("extract_this", span));
        let mut writes = Vec::new();
        for name in &names {
            let condition = match &has_this {
                Some(_) => {
                    let flag = self.fresh_temp("extract_ok", span);
                    out.push(bool_assign(&flag, key_exists(name)));
                    var(&flag, span)
                }
                None => key_exists(name),
            };
            writes.push((name.clone(), condition));
        }
        if let Some(has_this) = &has_this {
            out.push(bool_assign(has_this, key_exists("this")));
            let key = self.fresh_temp("extract_key", span);
            let value = self.fresh_temp("extract_value", span);
            let mut reset = Vec::new();
            let mut loop_body = vec![if_then(
                binop(var(&key, span), BinOp::StrictEq, str_lit("this", span)),
                vec![Stmt::new(StmtKind::Break(1), span)],
                span,
            )];
            for (name, condition) in &writes {
                let ExprKind::Variable(flag) = &condition.kind else {
                    unreachable!("overwrite mode uses flag locals");
                };
                reset.push(bool_assign(flag, Expr::new(ExprKind::BoolLiteral(false), span)));
                loop_body.push(if_then(
                    binop(var(&key, span), BinOp::StrictEq, str_lit(name, span)),
                    vec![bool_assign(flag, Expr::new(ExprKind::BoolLiteral(true), span))],
                    span,
                ));
            }
            reset.push(Stmt::new(
                StmtKind::Foreach {
                    array: var(&temp, span),
                    key_var: Some(key),
                    value_var: value,
                    value_by_ref: false,
                    body: loop_body,
                },
                span,
            ));
            out.push(if_then(var(has_this, span), reset, span));
        }
        for (name, condition) in writes {
            out.push(if_then(
                condition,
                vec![Stmt::new(
                    StmtKind::Assign {
                        name: name.clone(),
                        value: Expr::new(
                            ExprKind::ArrayAccess {
                                array: Box::new(var(&temp, span)),
                                index: Box::new(str_lit(&name, span)),
                            },
                            span,
                        ),
                    },
                    span,
                )],
                span,
            ));
        }
        if let Some(has_this) = &has_this {
            out.push(if_then(
                var(has_this, span),
                vec![Stmt::new(
                    StmtKind::Throw(super::new_error(
                        str_lit("Cannot re-assign $this", span),
                        span,
                    )),
                    span,
                )],
                span,
            ));
        }
        super::rewrite::inherit_profile(&mut out, stmt);
        for generated in out.iter_mut() {
            super::respan::respan_stmt(generated);
        }
        stmt.kind = StmtKind::Synthetic(out);
        self.pending_splice = true;
    }
}

/// Appends the literal names of one `compact()` argument; returns `false` for a runtime name.
pub(super) fn collect_literal_names(arg: &Expr, out: &mut Vec<String>) -> bool {
    match &arg.kind {
        ExprKind::StringLiteral(name) => {
            out.push(name.clone());
            true
        }
        ExprKind::ArrayLiteral(items) => items.iter().all(|item| collect_literal_names(item, out)),
        _ => false,
    }
}

/// Reads a literal `extract()` mode flag.
fn extract_flag(flag: &Expr) -> Option<i64> {
    match &flag.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::ConstRef(name) => match name.last_segment()? {
            "EXTR_OVERWRITE" => Some(EXTR_OVERWRITE),
            "EXTR_SKIP" => Some(EXTR_SKIP),
            _ => None,
        },
        _ => None,
    }
}

/// Returns whether `expr` reads the frame by runtime name (a non-literal variable variable or
/// `get_defined_vars()`), ignoring nested scopes.
pub(super) fn reads_frame_dynamically(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::ArrayAccess { array, index } => {
            matches!(&array.kind, ExprKind::Variable(name) if name == FRAME_LOCAL)
                && !matches!(index.kind, ExprKind::StringLiteral(_))
        }
        ExprKind::FunctionCall { name, .. } => {
            builtin_call_name(name).as_deref() == Some("get_defined_vars")
        }
        _ => false,
    }
}

/// Builds `if (<condition>) { <body> }`.
fn if_then(condition: Expr, then_body: Vec<Stmt>, span: crate::span::Span) -> Stmt {
    Stmt::new(
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        span,
    )
}
