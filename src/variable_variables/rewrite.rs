//! Purpose:
//! Rewrites variable-variable reads, probes and writes into dispatches over the current
//! scope's compile-time frame names.
//!
//! Called from:
//! - `super::desugar()`.
//!
//! Key details:
//! - Expression positions (reads and `isset`/`empty`/`??` probes) become a `match` on the
//!   runtime name; statement positions (writes, `unset`, by-reference `foreach`, `= &$$n`)
//!   become an `if`/`elseif` chain on a string temporary holding the name, evaluated once.
//! - Each rewritten fragment is walked again, so variable variables nested in names, indexes
//!   or assigned values (`$$$n`, `$$a[$$b]`, `$$a = $$b`) are rewritten too.
//! - Any frame access that reaches a position this module does not model is reported as an
//!   explicit compile error; none is allowed to survive to the checker.

use std::collections::BTreeSet;

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind, TypeExpr};
use crate::span::Span;

use super::scope::{
    collect_frame_names, known_values, literal_foreach_values, region_may_rewrite, FrameNames,
};
use super::respan::{respan_expr, respan_stmts};
use super::walk::{walk_expr_children, walk_stmt_children, Visitor};
use super::{
    box_mixed, binop, cast_string, frame_access_name, global_call,
    is_identifier_name, is_pure_name_expr, place_frame_root, replace_place_root, str_lit,
    throw_error, var, warning_then_null, FRAME_LOCAL,
};

/// The frame the rewriter is currently inside.
pub(super) struct Scope {
    /// `true` for top-level code, where only literal names are supported.
    pub(super) top_level: bool,
    /// Whether `$this` is bound in this frame (non-static methods and their closures).
    pub(super) this_available: bool,
    /// The frame's compile-time names.
    pub(super) frame: FrameNames,
    /// Narrowed name bindings from enclosing literal `foreach` loops and `switch` cases.
    pub(super) narrow: Vec<(String, BTreeSet<String>)>,
    /// Frame names definitely assigned at the statement being rewritten (straight-line
    /// assignments of enclosing blocks plus parameters); used by `compact()`.
    pub(super) definite: std::collections::HashSet<String>,
    /// `true` inside a closure or arrow function body.
    pub(super) in_closure: bool,
    /// Locals some `unset()` may have removed before the statement being rewritten: the
    /// unsets of already-rewritten statements plus those of every enclosing loop (a previous
    /// iteration may have run them).
    pub(super) unset_before: std::collections::HashSet<String>,
    /// Nesting depth of the statement block being rewritten (0 = the function body).
    pub(super) block_depth: usize,
}

/// What a statement-level dispatch does to the local it resolves.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DispatchKind {
    /// Stores a value (may retype the local).
    Write,
    /// Binds a reference to the local (`foreach ... &$v`, `= &$$n`), creating it in PHP.
    Bind,
    /// `unset()`: an unknown name is a no-op.
    Unset,
}

/// In-place desugarer for variable variables and literal-name scope builtins.
pub(super) struct Rewriter {
    pub(super) scope: Scope,
    errors: Vec<CompileError>,
    counter: usize,
    /// Placement facts for the `extract()` statement being rewritten, if any.
    pub(super) extract_context: Option<super::builtins::ExtractContext>,
    /// Set once a dispatch wraps its arms in the injected mixed-boxing helper.
    pub(super) used_box: bool,
    /// Set when the statement just rewritten became a `Synthetic` sequence to splice.
    pub(super) pending_splice: bool,
}

impl Rewriter {
    /// Creates a rewriter positioned at top-level scope.
    pub(super) fn new() -> Self {
        Self {
            scope: Scope {
                top_level: true,
                definite: Default::default(),
                in_closure: false,
                this_available: false,
                frame: FrameNames::default(),
                narrow: Vec::new(),
                block_depth: 0,
                unset_before: Default::default(),
            },
            errors: Vec::new(),
            counter: 0,
            extract_context: None,
            used_box: false,
            pending_splice: false,
        }
    }

    /// Rewrites the whole program, starting in its top-level frame.
    /// Returns whether anything needed rewriting.
    pub(super) fn run_top_level(&mut self, program: &mut Vec<Stmt>) -> bool {
        if !super::scan::needs_rewrite(program) {
            return false;
        }
        self.scope.frame = collect_frame_names(&[], program);
        self.block(program);
        true
    }

    /// Returns the diagnostics collected during the rewrite.
    pub(super) fn into_errors(self) -> Vec<CompileError> {
        self.errors
    }

    pub(super) fn error(&mut self, span: Span, message: &str) {
        self.errors.push(CompileError::new(span, message));
    }

    /// Returns a fresh compiler-generated local name for a dispatch at `span`.
    pub(super) fn fresh_temp(&mut self, stem: &str, span: Span) -> String {
        self.counter += 1;
        crate::names::generated_local_name(&format!(
            "__elephc_varvar_{stem}_{}_{}_{}",
            span.line, span.col, self.counter
        ))
    }

    /// Rewrites a function-like body in its own frame.
    fn enter_frame(
        &mut self,
        params: Vec<(String, Option<TypeExpr>)>,
        captures: &[String],
        this_available: bool,
        in_closure: bool,
        body: &mut Vec<Stmt>,
    ) {
        // Most frames hold no variable variable and no scope builtin: skip them without
        // collecting their names.
        if !super::scan::needs_rewrite(body) {
            return;
        }
        let names: Vec<String> = params.iter().map(|(name, _)| name.clone()).collect();
        let mut frame = super::scope::collect_frame_names_with_captures(&names, captures, body);
        frame.param_types = params
            .into_iter()
            .filter_map(|(name, ty)| ty.map(|ty| (name, ty)))
            .collect();
        let definite = names
            .iter()
            .chain(captures.iter())
            .filter(|name| !frame.unset_names.contains(*name))
            .cloned()
            .collect();
        let outer = std::mem::replace(
            &mut self.scope,
            Scope {
                definite,
                top_level: false,
                in_closure,
                this_available,
                frame,
                narrow: Vec::new(),
                block_depth: 0,
                unset_before: Default::default(),
            },
        );
        self.block(body);
        self.scope = outer;
    }

    /// Candidate locals a statement through name expression `name` may reach.
    ///
    /// A bounded name (see `known_values`) is narrowed to the frame names it can take. With
    /// `retypes` (a plain write), an unbounded name excludes the parameters: elephc fixes a
    /// parameter's storage type from its call sites, so a dispatch arm retyping one would fail
    /// the whole compile even when the program never writes it. Every frame name left out is
    /// routed to an explicit `Error` by [`Self::dispatch_fallback`].
    fn dispatch_candidates(&self, name: &Expr, retypes: bool) -> Vec<String> {
        let frame = &self.scope.frame;
        match known_values(name, frame, &self.scope.narrow) {
            Some(values) => values
                .into_iter()
                .filter(|value| frame.names.contains(value))
                .collect(),
            None => frame
                .names
                .iter()
                .filter(|name| !(retypes && frame.params.contains(*name)))
                .cloned()
                .collect(),
        }
    }

    /// Returns the literal name when `name` is a string literal naming a plain variable.
    ///
    /// Superglobals are included: PHP resolves `${'_GET'}` at compile time like `$_GET`, while a
    /// runtime name cannot reach them inside a function.
    fn literal_name(name: &Expr) -> Option<String> {
        match &name.kind {
            ExprKind::StringLiteral(value)
                if is_identifier_name(value) && value != "this" =>
            {
                Some(value.clone())
            }
            _ => None,
        }
    }

    /// Reports a dynamic variable variable inside a closure; returns `true` when it did.
    ///
    /// The dispatch reads captured variables inside `match` arms, which closure lowering does
    /// not yet compile correctly when a captured value is passed to a call inside a
    /// conditional arm (it reads back `0`). Refusing is the only option that cannot miscompile.
    fn reject_closure(&mut self, span: Span) -> bool {
        if self.scope.in_closure {
            self.error(
                span,
                "Variable variables with a non-literal name are not supported inside closures and arrow functions (elephc); move the code into a named function or method",
            );
            return true;
        }
        false
    }

    /// Reports a dynamic variable variable in top-level code; returns `true` when it did.
    fn reject_top_level(&mut self, span: Span) -> bool {
        if self.scope.top_level {
            self.error(
                span,
                "Variable variables with a non-literal name are only supported inside functions and methods (elephc): top-level locals cannot be dispatched by runtime name",
            );
            return true;
        }
        false
    }

    // --- statement-level dispatch ---

    /// Builds the `if`/`elseif` chain that runs `arm(candidate)` for the local named by the
    /// string temporary `temp`, falling through to `fallback`.
    fn name_dispatch(
        &self,
        temp: &str,
        candidates: &[String],
        span: Span,
        mut arm: impl FnMut(&str) -> Vec<Stmt>,
        fallback: Vec<Stmt>,
    ) -> Vec<Stmt> {
        let test = |candidate: &str| {
            let mut test = binop(var(temp, span), BinOp::StrictEq, str_lit(candidate, span));
            respan_expr(&mut test);
            test
        };
        let mut arm = |candidate: &str| {
            let mut body = arm(candidate);
            respan_stmts(&mut body);
            body
        };
        let mut fallback = fallback;
        respan_stmts(&mut fallback);
        let Some((first, rest)) = candidates.split_first() else {
            return fallback;
        };
        let elseif_clauses = rest.iter().map(|c| (test(c), arm(c))).collect();
        vec![Stmt::new(
            StmtKind::If {
                condition: test(first),
                then_body: arm(first),
                elseif_clauses,
                else_body: (!fallback.is_empty()).then_some(fallback),
            },
            span,
        )]
    }

    /// The branch for a runtime name no dispatch arm matched.
    ///
    /// A frame name left out of `candidates` always throws: silently skipping it would read or
    /// write the wrong storage. A name the frame does not know throws for writes (PHP would
    /// create the local) and does nothing when `unknown_is_error` is false (`unset`).
    fn dispatch_fallback(
        &self,
        temp: &str,
        candidates: &[String],
        unknown_is_error: bool,
        span: Span,
    ) -> Vec<Stmt> {
        let throw_with = |suffix: &str| {
            Stmt::new(
                StmtKind::Throw(super::new_error(
                    binop(
                        binop(
                            str_lit("Variable variable $", span),
                            BinOp::Concat,
                            var(temp, span),
                        ),
                        BinOp::Concat,
                        str_lit(suffix, span),
                    ),
                    span,
                )),
                span,
            )
        };
        let unknown: Vec<Stmt> = if unknown_is_error {
            // PHP's own error for `$$n = v` with `$n === 'this'`.
            vec![Stmt::new(
                StmtKind::If {
                    condition: binop(var(temp, span), BinOp::StrictEq, str_lit("this", span)),
                    then_body: vec![Stmt::new(
                        StmtKind::Throw(super::new_error(
                            str_lit("Cannot re-assign $this", span),
                            span,
                        )),
                        span,
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![throw_with(
                        ": creating a new local through a variable variable is not supported (elephc); only locals the function names at compile time can be written",
                    )]),
                },
                span,
            )]
        } else {
            Vec::new()
        };
        let excluded: Vec<String> = self
            .scope
            .frame
            .names
            .iter()
            .filter(|name| !candidates.contains(name))
            .cloned()
            .collect();
        if excluded.is_empty() {
            return unknown;
        }
        let outside = throw_with(
            ": this local is outside the names elephc can write through this variable variable (elephc); parameters and names outside a statically known set are not dispatched",
        );
        let excluded_list = Expr::new(
            ExprKind::ArrayLiteral(excluded.iter().map(|name| str_lit(name, span)).collect()),
            span,
        );
        vec![Stmt::new(
            StmtKind::If {
                condition: global_call(
                    "in_array",
                    vec![
                        var(temp, span),
                        excluded_list,
                        Expr::new(ExprKind::BoolLiteral(true), span),
                    ],
                    span,
                ),
                then_body: vec![outside],
                elseif_clauses: Vec::new(),
                else_body: (!unknown.is_empty()).then_some(unknown),
            },
            span,
        )]
    }

    /// Lowers a statement whose effect goes through the frame access rooting `place`.
    ///
    /// `make(replacement)` builds the statements for one resolved local. `kind` selects the
    /// candidate policy and what an unknown name does.
    fn dispatch_place_stmt(
        &mut self,
        place: &Expr,
        span: Span,
        kind: DispatchKind,
        make: impl Fn(&Expr) -> Vec<Stmt>,
    ) -> Vec<Stmt> {
        let root = place_frame_root(place).expect("caller checked the frame root");
        let name = frame_access_name(root).expect("root is a frame access").clone();
        if let Some(literal) = Self::literal_name(&name) {
            return make(&var(&literal, span));
        }
        if self.reject_top_level(span) || self.reject_closure(span) {
            return Vec::new();
        }
        let temp = self.fresh_temp("name", span);
        let candidates = self.dispatch_candidates(&name, kind == DispatchKind::Write);
        let mut out = vec![Stmt::new(
            StmtKind::Assign {
                name: temp.clone(),
                value: cast_string(name),
            },
            span,
        )];
        let fallback =
            self.dispatch_fallback(&temp, &candidates, kind != DispatchKind::Unset, span);
        out.extend(self.name_dispatch(
            &temp,
            &candidates,
            span,
            |candidate| make(&var(candidate, span)),
            fallback,
        ));
        out
    }

    /// Replaces `stmt` with the rewritten statement sequence, walking it again.
    pub(super) fn replace_with(&mut self, stmt: &mut Stmt, mut stmts: Vec<Stmt>) {
        inherit_profile(&mut stmts, stmt);
        self.block(&mut stmts);
        stmt.kind = StmtKind::Synthetic(stmts);
        self.pending_splice = true;
    }

    /// Rewrites a write `place = value` statement (plain, compound, dimension, property).
    fn rewrite_write_stmt(&mut self, stmt: &mut Stmt) {
        let span = stmt.span;
        let StmtKind::ExprStmt(expr) = &mut stmt.kind else {
            unreachable!("caller matched an expression statement");
        };
        let ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp,
        } = &mut expr.kind
        else {
            unreachable!("caller matched an assignment");
        };
        if conditional_value_temp.is_some() {
            self.error(
                span,
                "Null-coalescing assignment (`??=`) to a variable variable is not supported (elephc); test it with isset() and assign separately",
            );
            return;
        }
        let target = (**target).clone();
        let value = (**value).clone();
        let result_target = result_target.clone();
        let mut out = std::mem::take(prelude);
        out.extend(self.dispatch_place_stmt(&target, span, DispatchKind::Write, |local| {
            let new_target = replace_place_root(&target, local);
            let kind = match &new_target.kind {
                ExprKind::Variable(name) => StmtKind::Assign {
                    name: name.clone(),
                    value: value.clone(),
                },
                _ => StmtKind::ExprStmt(Expr::new(
                    ExprKind::Assignment {
                        target: Box::new(new_target),
                        value: Box::new(value.clone()),
                        result_target: result_target.clone(),
                        prelude: Vec::new(),
                        conditional_value_temp: None,
                    },
                    span,
                )),
            };
            vec![Stmt::new(kind, span)]
        }));
        self.replace_with(stmt, out);
    }

    /// Rewrites `unset(...)` whose arguments include variable-variable places.
    fn rewrite_unset_stmt(&mut self, stmt: &mut Stmt) {
        let span = stmt.span;
        let StmtKind::ExprStmt(expr) = &stmt.kind else {
            unreachable!("caller matched an expression statement");
        };
        let ExprKind::FunctionCall { name, args } = &expr.kind else {
            unreachable!("caller matched a call");
        };
        let name = name.clone();
        let mut out = Vec::new();
        for arg in args.clone() {
            if place_frame_root(&arg).is_none() {
                out.push(Stmt::new(
                    StmtKind::ExprStmt(Expr::new(
                        ExprKind::FunctionCall {
                            name: name.clone(),
                            args: vec![arg],
                        },
                        span,
                    )),
                    span,
                ));
                continue;
            }
            let name = name.clone();
            out.extend(self.dispatch_place_stmt(&arg, span, DispatchKind::Unset, |local| {
                vec![Stmt::new(
                    StmtKind::ExprStmt(Expr::new(
                        ExprKind::FunctionCall {
                            name: name.clone(),
                            args: vec![replace_place_root(&arg, local)],
                        },
                        span,
                    )),
                    span,
                )]
            }));
        }
        self.replace_with(stmt, out);
    }

    /// Rewrites `foreach ($$n as ... => &$v)`, which must iterate the local itself.
    fn rewrite_foreach_ref_stmt(&mut self, stmt: &mut Stmt) {
        let span = stmt.span;
        let StmtKind::Foreach { array, .. } = &stmt.kind else {
            unreachable!("caller matched a foreach");
        };
        let array = array.clone();
        let template = stmt.clone();
        let out = self.dispatch_place_stmt(&array, span, DispatchKind::Bind, |local| {
            let mut clone = template.clone();
            if let StmtKind::Foreach { array: cloned, .. } = &mut clone.kind {
                *cloned = replace_place_root(&array, local);
            }
            vec![clone]
        });
        self.replace_with(stmt, out);
    }

    /// Rewrites `$r = &$$n`, which binds `$r` to the local itself.
    fn rewrite_ref_assign_stmt(&mut self, stmt: &mut Stmt) {
        let span = stmt.span;
        let StmtKind::RefAssign { target, source } = &stmt.kind else {
            unreachable!("caller matched a reference assignment");
        };
        let target = target.clone();
        let source = source.clone();
        let out = self.dispatch_place_stmt(&source, span, DispatchKind::Bind, |local| {
            vec![Stmt::new(
                StmtKind::RefAssign {
                    target: target.clone(),
                    source: replace_place_root(&source, local),
                },
                span,
            )]
        });
        self.replace_with(stmt, out);
    }

    // --- expression-level dispatch ---

    /// Builds `match ((string) name) { 'a' => make($a), ..., default => fallback }` for the
    /// frame access rooting `place`. Returns `None` after reporting an unsupported shape.
    fn dispatch_place_expr(
        &mut self,
        place: &Expr,
        span: Span,
        make: impl Fn(&Expr) -> Expr,
        fallback: impl FnOnce(&Expr) -> Expr,
    ) -> Option<Expr> {
        let root = place_frame_root(place).expect("caller checked the frame root");
        let mut name = frame_access_name(root).expect("root is a frame access").clone();
        // Rewrite variable variables nested in the name first (`$$$n`).
        self.expr(&mut name);
        let place = replace_place_root(place, &super::frame_access(name.clone(), root.span));
        if let Some(literal) = Self::literal_name(&name) {
            let mut out = make(&replace_place_root(&place, &var(&literal, span)));
            self.expr(&mut out);
            return Some(out);
        }
        if self.reject_top_level(span) || self.reject_closure(span) {
            return None;
        }
        // `$$n` itself is replaced by the local; a deeper place (`$$n['k']`, `$$n->p`) reads
        // the local boxed, so the dimension or property applies to a mixed value whatever the
        // local's native type is.
        let is_root = frame_access_name(&place).is_some();
        if !is_root {
            self.used_box = true;
        }
        let local_root = |local: Expr| {
            if is_root || matches!(local.kind, ExprKind::This) {
                local
            } else {
                let probe = Expr::new(
                    ExprKind::NullCoalesce {
                        value: Box::new(local),
                        default: Box::new(Expr::new(ExprKind::Null, span)),
                    },
                    span,
                );
                box_mixed(probe)
            }
        };
        // The name is evaluated once: directly when it is a plain variable or literal,
        // otherwise through a string temporary bound in the match subject.
        let (subject, name_for_fallback) = if is_pure_name_expr(&name) {
            (cast_string(name.clone()), name.clone())
        } else {
            let temp = self.fresh_temp("read", span);
            let bind = Expr::new(
                ExprKind::Assignment {
                    target: Box::new(var(&temp, span)),
                    value: Box::new(cast_string(name.clone())),
                    result_target: None,
                    prelude: Vec::new(),
                    conditional_value_temp: None,
                },
                span,
            );
            (bind, var(&temp, span))
        };
        let mut arms: Vec<(Vec<Expr>, Expr)> = self
            .scope
            .frame
            .names
            .iter()
            .map(|candidate| {
                let mut arm = make(&replace_place_root(&place, &local_root(var(candidate, span))));
                respan_expr(&mut arm);
                (vec![str_lit(candidate, span)], arm)
            })
            .collect();
        if self.scope.this_available {
            let mut arm = make(&replace_place_root(
                &place,
                &local_root(Expr::new(ExprKind::This, span)),
            ));
            respan_expr(&mut arm);
            arms.push((vec![str_lit("this", span)], arm));
        }
        let mut default = fallback(&name_for_fallback);
        respan_expr(&mut default);
        let mut out = if arms.is_empty() {
            default
        } else {
            Expr::new(
                ExprKind::Match {
                    subject: Box::new(subject),
                    arms,
                    default: Some(Box::new(default)),
                },
                span,
            )
        };
        // Arms may hold further variable variables (indexes, `??` defaults); the subject was
        // already rewritten above.
        if let ExprKind::Match { arms, default, .. } = &mut out.kind {
            for (_, arm) in arms.iter_mut() {
                self.expr(arm);
            }
            if let Some(default) = default {
                self.expr(default);
            }
        } else {
            self.expr(&mut out);
        }
        Some(out)
    }

    /// Rewrites a plain read of a frame access or of a place rooted at one.
    fn rewrite_read(&mut self, expr: &mut Expr) {
        let span = expr.span;
        if let Some(literal) = frame_access_name(expr).and_then(Self::literal_name) {
            // `${'name'}` is exactly `$name`, including its definedness diagnostics.
            *expr = var(&literal, span);
            return;
        }
        if let Some(typed) = self.typed_param_read(expr) {
            *expr = typed;
            return;
        }
        let place = expr.clone();
        self.used_box = true;
        let rewritten = self.dispatch_place_expr(
            &place,
            span,
            |local| {
                let value = if matches!(local.kind, ExprKind::Variable(_)) {
                    Expr::new(
                        ExprKind::NullCoalesce {
                            value: Box::new(local.clone()),
                            default: Box::new(Expr::new(ExprKind::Null, span)),
                        },
                        span,
                    )
                } else {
                    local.clone()
                };
                box_mixed(value)
            },
            |name| {
                box_mixed(warning_then_null(
                    binop(
                        str_lit("Undefined variable $", span),
                        BinOp::Concat,
                        cast_string(name.clone()),
                    ),
                    span,
                ))
            },
        );
        if let Some(rewritten) = rewritten {
            *expr = rewritten;
        }
    }

    /// Reads a bounded name whose every candidate is a parameter of one declared type.
    ///
    /// `match ((string) n) { 'from' => $from, 'to' => $to, default => throw }` keeps the
    /// declared type instead of boxing to mixed: parameters are always defined, their storage
    /// type is fixed, and the arms are homogeneous. A runtime name outside the bound throws.
    fn typed_param_read(&mut self, expr: &Expr) -> Option<Expr> {
        let span = expr.span;
        let name = frame_access_name(expr)?;
        if self.scope.top_level || self.scope.in_closure || !is_pure_name_expr(name) {
            return None;
        }
        let frame = &self.scope.frame;
        let values = known_values(name, frame, &self.scope.narrow)?;
        let mut declared: Option<&TypeExpr> = None;
        for value in &values {
            let ty = frame.param_types.get(value)?;
            if matches!(ty, TypeExpr::Named(n) if n.as_str().eq_ignore_ascii_case("mixed")) {
                return None;
            }
            match declared {
                None => declared = Some(ty),
                Some(previous) if previous == ty => {}
                Some(_) => return None,
            }
        }
        declared?;
        let arms = values
            .iter()
            .map(|value| (vec![str_lit(value, span)], var(value, span)))
            .collect();
        let message = binop(
            binop(
                str_lit("Variable variable $", span),
                BinOp::Concat,
                cast_string(name.clone()),
            ),
            BinOp::Concat,
            str_lit(
                ": the name is outside the parameters elephc inferred for this read (elephc)",
                span,
            ),
        );
        Some(Expr::new(
            ExprKind::Match {
                subject: Box::new(cast_string(name.clone())),
                arms,
                default: Some(Box::new(throw_error(message, span))),
            },
            span,
        ))
    }

    /// Rewrites `isset(...)` / `empty(...)` whose operands include variable-variable places.
    fn rewrite_isset_empty(&mut self, expr: &mut Expr, is_isset: bool) {
        let span = expr.span;
        let ExprKind::FunctionCall { name, args } = &expr.kind else {
            unreachable!("caller matched a call");
        };
        let name = name.clone();
        let args = args.clone();
        let call = |arg: Expr| Expr::new(
            ExprKind::FunctionCall {
                name: name.clone(),
                args: vec![arg],
            },
            span,
        );
        let mut parts = Vec::new();
        for arg in args {
            if place_frame_root(&arg).is_none() {
                let mut plain = call(arg);
                walk_expr_children(self, &mut plain);
                parts.push(plain);
                continue;
            }
            let probe = self.dispatch_place_expr(
                &arg,
                span,
                |local| call(local.clone()),
                |_| Expr::new(ExprKind::BoolLiteral(!is_isset), span),
            );
            match probe {
                Some(probe) => parts.push(probe),
                None => return,
            }
        }
        let combined = parts
            .into_iter()
            .reduce(|left, right| binop(left, BinOp::And, right))
            .unwrap_or_else(|| Expr::new(ExprKind::BoolLiteral(is_isset), span));
        *expr = combined;
    }

    /// Rewrites `$$n ?? default` (and `$$n[k] ?? default`), which never warns.
    fn rewrite_null_coalesce(&mut self, expr: &mut Expr) {
        let span = expr.span;
        let ExprKind::NullCoalesce { value, default } = &expr.kind else {
            unreachable!("caller matched a null coalesce");
        };
        let place = (**value).clone();
        let default = (**default).clone();
        self.used_box = true;
        let rewritten = self.dispatch_place_expr(
            &place,
            span,
            |local| {
                box_mixed(Expr::new(
                    ExprKind::NullCoalesce {
                        value: Box::new(local.clone()),
                        default: Box::new(default.clone()),
                    },
                    span,
                ))
            },
            |_| box_mixed(default.clone()),
        );
        if let Some(rewritten) = rewritten {
            *expr = rewritten;
        }
    }

    /// Rejects a variable-variable place passed to a builtin's by-reference parameter.
    fn check_by_ref_builtin_args(&mut self, name: &str, args: &[Expr], span: Span) {
        let Some(def) = crate::builtins::registry::lookup(name) else {
            return;
        };
        for (index, arg) in args.iter().enumerate() {
            let by_ref = def.ref_params.get(index).copied().unwrap_or(false);
            if by_ref && place_frame_root(arg).is_some() {
                self.error(
                    span,
                    &format!(
                        "Passing a variable variable by reference to {name}() is not supported (elephc); assign it to a local, call {name}() on that local, and write it back"
                    ),
                );
            }
        }
    }
}

/// Returns the unqualified builtin name a call resolves to, when it can be a global builtin.
pub(super) fn builtin_call_name(name: &crate::names::Name) -> Option<String> {
    let last = name.last_segment()?;
    if name.parts.len() > 1 && name.function_fallback().is_none() {
        return None;
    }
    Some(php_symbol_key(last))
}

impl Visitor for Rewriter {
    fn stmt(&mut self, stmt: &mut Stmt) {
        match &mut stmt.kind {
            StmtKind::FunctionDecl {
                params,
                variadic,
                body,
                ..
            } => {
                let mut names: Vec<(String, Option<TypeExpr>)> =
                    params.iter().map(|(n, ty, ..)| (n.clone(), ty.clone())).collect();
                names.extend(variadic.iter().map(|v| (v.clone(), None)));
                self.enter_frame(names, &[], false, false, body);
                return;
            }
            StmtKind::ClassDecl { methods, .. }
            | StmtKind::TraitDecl { methods, .. }
            | StmtKind::InterfaceDecl { methods, .. }
            | StmtKind::EnumDecl { methods, .. } => {
                for method in methods.iter_mut() {
                    let mut names: Vec<(String, Option<TypeExpr>)> = method
                        .params
                        .iter()
                        .map(|(n, ty, ..)| (n.clone(), ty.clone()))
                        .collect();
                    names.extend(method.variadic.iter().map(|v| (v.clone(), None)));
                    let this_available = !method.is_static;
                    self.enter_frame(names, &[], this_available, false, &mut method.body);
                }
                return;
            }
            // Destructuring (`[$$x, $$y] = ...`, `list($$x) = ...`, `foreach (... as [$$x])`)
            // lowers each element to a plain store on the frame access: route it through the
            // ordinary write dispatch.
            StmtKind::ArrayAssign {
                array,
                index,
                value,
            } if array == FRAME_LOCAL => {
                let span = stmt.span;
                let target = super::frame_access(index.clone(), span);
                stmt.kind = StmtKind::ExprStmt(plain_assignment(target, value.clone(), span));
                self.rewrite_write_stmt(stmt);
                return;
            }
            StmtKind::NestedArrayAssign { target, value } if place_frame_root(target).is_some() => {
                let span = stmt.span;
                let assignment = plain_assignment(target.clone(), value.clone(), span);
                stmt.kind = StmtKind::ExprStmt(assignment);
                self.rewrite_write_stmt(stmt);
                return;
            }
            StmtKind::ArrayPush { array, .. } if array == FRAME_LOCAL => {
                let span = stmt.span;
                self.error(span, "Unsupported variable-variable form (elephc): a variable variable needs a name");
                return;
            }
            StmtKind::ExprStmt(expr) => match &expr.kind {
                ExprKind::Assignment { target, .. } if place_frame_root(target).is_some() => {
                    self.rewrite_write_stmt(stmt);
                    return;
                }
                ExprKind::FunctionCall { name, args } => {
                    let builtin = builtin_call_name(name);
                    if builtin.as_deref() == Some("unset")
                        && args.iter().any(|arg| place_frame_root(arg).is_some())
                    {
                        self.rewrite_unset_stmt(stmt);
                        return;
                    }
                    if builtin.as_deref() == Some("extract") {
                        self.rewrite_extract_stmt(stmt);
                        return;
                    }
                }
                _ => {}
            },
            StmtKind::Foreach {
                array,
                value_by_ref: true,
                ..
            } if place_frame_root(array).is_some() => {
                self.rewrite_foreach_ref_stmt(stmt);
                return;
            }
            StmtKind::RefAssign { source, .. } if place_frame_root(source).is_some() => {
                self.rewrite_ref_assign_stmt(stmt);
                return;
            }
            StmtKind::Foreach {
                array,
                key_var,
                value_var,
                value_by_ref,
                body,
            } => {
                self.expr(array);
                let mut pushed = 0;
                if !*value_by_ref {
                    if let Some(values) = literal_foreach_values(array, false) {
                        if !region_may_rewrite(body, value_var, &values) {
                            self.scope.narrow.push((value_var.clone(), values));
                            pushed += 1;
                        }
                    }
                }
                if let Some(key) = key_var {
                    if let Some(values) = literal_foreach_values(array, true) {
                        if !region_may_rewrite(body, key, &values) {
                            self.scope.narrow.push((key.clone(), values));
                            pushed += 1;
                        }
                    }
                }
                let saved = self.scope.definite.clone();
                for var in std::iter::once(value_var.clone()).chain(key_var.clone()) {
                    if !self.scope.frame.unset_names.contains(&var) {
                        self.scope.definite.insert(var);
                    }
                }
                self.block(body);
                self.scope.definite = saved;
                for _ in 0..pushed {
                    self.scope.narrow.pop();
                }
                return;
            }
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                self.expr(condition);
                let bounds = stable_bounds(condition_bounds(condition), then_body);
                self.block_narrowed(bounds, then_body);
                for (condition, body) in elseif_clauses.iter_mut() {
                    self.expr(condition);
                    let bounds = stable_bounds(condition_bounds(condition), body);
                    self.block_narrowed(bounds, body);
                }
                if let Some(body) = else_body {
                    self.block(body);
                }
                return;
            }
            StmtKind::Switch {
                subject,
                cases,
                default,
            } => {
                self.expr(subject);
                // A case body that rewrites the subject (possibly falling through into the
                // next group) invalidates the label bound.
                let all_labels: BTreeSet<String> = cases
                    .iter()
                    .flat_map(|(conditions, _)| conditions.iter())
                    .filter_map(|condition| match &condition.kind {
                        ExprKind::StringLiteral(value) => Some(value.clone()),
                        _ => None,
                    })
                    .collect();
                let subject_var = match &subject.kind {
                    ExprKind::Variable(name)
                        if !cases
                            .iter()
                            .any(|(_, body)| region_may_rewrite(body, name, &all_labels)) =>
                    {
                        Some(name.clone())
                    }
                    _ => None,
                };
                let mut labels: Option<BTreeSet<String>> = Some(BTreeSet::new());
                for (conditions, body) in cases.iter_mut() {
                    for condition in conditions.iter_mut() {
                        self.expr(condition);
                        match (&mut labels, &condition.kind) {
                            (Some(set), ExprKind::StringLiteral(value)) => {
                                set.insert(value.clone());
                            }
                            _ => labels = None,
                        }
                    }
                    let pushed = match (&subject_var, &labels) {
                        (Some(var), Some(set)) if !set.is_empty() => {
                            self.scope.narrow.push((var.clone(), set.clone()));
                            true
                        }
                        _ => false,
                    };
                    self.block(body);
                    if pushed {
                        self.scope.narrow.pop();
                    }
                    // A body that cannot fall through starts a fresh label group.
                    if body.last().is_some_and(ends_case) {
                        labels = Some(BTreeSet::new());
                    }
                }
                if let Some(body) = default {
                    self.block(body);
                }
                return;
            }
            _ => {}
        }
        walk_stmt_children(self, stmt);
    }

    fn block(&mut self, stmts: &mut Vec<Stmt>) {
        self.block_inner(stmts);
    }

    fn expr(&mut self, expr: &mut Expr) {
        self.rewrite_expr(expr);
    }
}

impl Rewriter {
    /// Rewrites a branch body with `bounds` pushed onto the narrowing stack.
    fn block_narrowed(&mut self, bounds: Vec<(String, BTreeSet<String>)>, body: &mut Vec<Stmt>) {
        let pushed = bounds.len();
        self.scope.narrow.extend(bounds);
        self.block(body);
        for _ in 0..pushed {
            self.scope.narrow.pop();
        }
    }

    fn block_inner(&mut self, stmts: &mut Vec<Stmt>) {
        let function_body = self.scope.block_depth == 0;
        let saved_definite = self.scope.definite.clone();
        self.scope.block_depth += 1;
        let mut index = 0;
        while index < stmts.len() {
            let tracks_unsets = !self.scope.frame.unset_names.is_empty();
            if tracks_unsets && is_loop_stmt(&stmts[index]) {
                let mut own = vec![stmts[index].clone()];
                let unsets = collect_frame_names(&[], &mut own).unset_names;
                self.scope.unset_before.extend(unsets);
            }
            if is_extract_stmt(&stmts[index]) {
                // `extract(..., EXTR_SKIP)` needs to know which locals may already exist.
                let mut preceding = stmts[..index].to_vec();
                let preceding_frame = collect_frame_names(&[], &mut preceding);
                self.extract_context = Some(super::builtins::ExtractContext {
                    function_body,
                    preceding_names: preceding_frame.names,
                    preceding_unsets: preceding_frame.unset_names,
                });
            }
            self.pending_splice = false;
            self.stmt(&mut stmts[index]);
            self.extract_context = None;
            let mut count = 1;
            if std::mem::take(&mut self.pending_splice) {
                // Splice the desugared sequence into this block rather than leaving it
                // wrapped: several later passes do not look inside `Synthetic` for reference
                // exposure and local writes.
                if let StmtKind::Synthetic(body) =
                    std::mem::replace(&mut stmts[index].kind, StmtKind::Synthetic(Vec::new()))
                {
                    count = body.len();
                    stmts.splice(index..=index, body);
                }
            }
            if tracks_unsets {
                let mut done = stmts[index..index + count].to_vec();
                let unsets = collect_frame_names(&[], &mut done).unset_names;
                self.scope.unset_before.extend(unsets);
            }
            for stmt in &stmts[index..index + count] {
                for name in definitely_assigned(stmt) {
                    if !self.scope.frame.unset_names.contains(&name) {
                        self.scope.definite.insert(name);
                    }
                }
            }
            index += count;
        }
        self.scope.definite = saved_definite;
        self.scope.block_depth -= 1;
    }

    fn rewrite_expr(&mut self, expr: &mut Expr) {
        match &mut expr.kind {
            ExprKind::Closure {
                params,
                variadic,
                body,
                is_static,
                is_arrow,
                captures,
                ..
            } => {
                let mut names: Vec<(String, Option<TypeExpr>)> =
                    params.iter().map(|(n, ty, ..)| (n.clone(), ty.clone())).collect();
                names.extend(variadic.iter().map(|v| (v.clone(), None)));
                let this_available = self.scope.this_available && !*is_static;
                // An arrow function's implicit captures may be undefined in its body; only an
                // explicit `use` list binds them on entry.
                let captures = if *is_arrow { Vec::new() } else { captures.clone() };
                self.enter_frame(names, &captures, this_available, true, body);
                return;
            }
            ExprKind::Variable(name) if name == FRAME_LOCAL => {
                let span = expr.span;
                self.error(span, "Unsupported variable-variable form (elephc)");
                return;
            }
            ExprKind::Assignment { target, .. } if place_frame_root(target).is_some() => {
                let span = expr.span;
                self.error(
                    span,
                    "Assigning to a variable variable inside an expression is not supported (elephc); use a separate assignment statement",
                );
                return;
            }
            ExprKind::NullCoalesce { value, .. } if place_frame_root(value).is_some() => {
                self.rewrite_null_coalesce(expr);
                return;
            }
            ExprKind::FunctionCall { name, args } => {
                let builtin = builtin_call_name(name);
                match builtin.as_deref() {
                    Some("isset") | Some("empty")
                        if args.iter().any(|arg| place_frame_root(arg).is_some()) =>
                    {
                        let is_isset = builtin.as_deref() == Some("isset");
                        self.rewrite_isset_empty(expr, is_isset);
                        return;
                    }
                    Some("compact") => {
                        self.rewrite_compact(expr);
                        return;
                    }
                    Some("extract") => {
                        let span = expr.span;
                        self.error(
                            span,
                            "extract() is only supported as a statement whose result is discarded (elephc)",
                        );
                        return;
                    }
                    Some(builtin) => {
                        let span = expr.span;
                        let builtin = builtin.to_string();
                        let args = args.clone();
                        self.check_by_ref_builtin_args(&builtin, &args, span);
                    }
                    None => {}
                }
            }
            _ => {}
        }
        if frame_access_name(expr).is_some() {
            self.rewrite_read(expr);
            return;
        }
        walk_expr_children(self, expr);
    }
}

/// Returns the name bounds a condition establishes for the branch it guards:
/// `$v === 'a'`, `'a' == $v`, `$v === 'a' || $v === 'b'`, conjunctions of those, and
/// `in_array($v, ['a', 'b'], ...)`.
fn condition_bounds(condition: &Expr) -> Vec<(String, BTreeSet<String>)> {
    match &condition.kind {
        ExprKind::BinaryOp {
            left,
            op: BinOp::StrictEq | BinOp::Eq,
            right,
        } => match (&left.kind, &right.kind) {
            (ExprKind::Variable(var), ExprKind::StringLiteral(value))
            | (ExprKind::StringLiteral(value), ExprKind::Variable(var)) => {
                vec![(var.clone(), BTreeSet::from([value.clone()]))]
            }
            _ => Vec::new(),
        },
        ExprKind::BinaryOp {
            left,
            op: BinOp::Or,
            right,
        } => {
            let (left, right) = (condition_bounds(left), condition_bounds(right));
            match (left.as_slice(), right.as_slice()) {
                ([(lv, ls)], [(rv, rs)]) if lv == rv => {
                    vec![(lv.clone(), ls.union(rs).cloned().collect())]
                }
                _ => Vec::new(),
            }
        }
        ExprKind::BinaryOp {
            left,
            op: BinOp::And,
            right,
        } => {
            let mut bounds = condition_bounds(left);
            bounds.extend(condition_bounds(right));
            bounds
        }
        ExprKind::FunctionCall { name, args }
            if builtin_call_name(name).as_deref() == Some("in_array") =>
        {
            match (args.first().map(|a| &a.kind), args.get(1)) {
                (Some(ExprKind::Variable(var)), Some(list)) => {
                    match literal_foreach_values(list, false) {
                        Some(values) => vec![(var.clone(), values)],
                        None => Vec::new(),
                    }
                }
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

/// Gives every generated statement the source profile (mode, `strict_types`) of `origin`, so the
/// checker treats the desugared code exactly like the statement it replaces.
pub(super) fn inherit_profile(stmts: &mut Vec<Stmt>, origin: &Stmt) {
    struct Profile(crate::source::SourceMode, bool);
    impl Visitor for Profile {
        fn stmt(&mut self, stmt: &mut Stmt) {
            stmt.source_mode = self.0;
            stmt.strict_types = self.1;
            walk_stmt_children(self, stmt);
        }
        fn expr(&mut self, _expr: &mut Expr) {}
    }
    Profile(origin.source_mode, origin.strict_types).block(stmts);
}

/// Builds a plain `target = value` assignment expression (no prelude, no read-back).
fn plain_assignment(target: Expr, value: Expr, span: Span) -> Expr {
    let result_target = (!matches!(target.kind, ExprKind::Variable(_))).then(|| Box::new(value.clone()));
    Expr::new(
        ExprKind::Assignment {
            target: Box::new(target),
            value: Box::new(value),
            result_target,
            prelude: Vec::new(),
            conditional_value_temp: None,
        },
        span,
    )
}

/// Drops the condition bounds a branch body may invalidate.
fn stable_bounds(
    bounds: Vec<(String, BTreeSet<String>)>,
    body: &[Stmt],
) -> Vec<(String, BTreeSet<String>)> {
    bounds
        .into_iter()
        .filter(|(var, bound)| !region_may_rewrite(body, var, bound))
        .collect()
}

/// Locals a statement assigns unconditionally (for `compact()`'s definite-assignment check).
fn definitely_assigned(stmt: &Stmt) -> Vec<String> {
    match &stmt.kind {
        StmtKind::Assign { name, .. }
        | StmtKind::TypedAssign { name, .. }
        | StmtKind::RefAssign { target: name, .. }
        | StmtKind::ArrayAssign { array: name, .. }
        | StmtKind::ArrayPush { array: name, .. }
        | StmtKind::StaticVar { name, .. } => vec![name.clone()],
        StmtKind::ListUnpack { vars, .. } | StmtKind::Global { vars } => vars.clone(),
        // `$a[k] = &$v` defines `$v` (as null) when it did not exist.
        StmtKind::RefAssignPlace { source, .. } => match &source.kind {
            ExprKind::Variable(name) => vec![name.clone()],
            _ => Vec::new(),
        },
        StmtKind::ExprStmt(Expr {
            kind: ExprKind::Assignment { target, .. },
            ..
        }) => match &target.kind {
            ExprKind::Variable(name) => vec![name.clone()],
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Returns whether `stmt` is a discarded `extract(...)` call.
fn is_extract_stmt(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::ExprStmt(Expr {
            kind: ExprKind::FunctionCall { name, .. },
            ..
        }) => builtin_call_name(name).as_deref() == Some("extract"),
        _ => false,
    }
}

/// Returns whether a case body's last statement prevents fallthrough into the next case.
fn ends_case(stmt: &Stmt) -> bool {
    matches!(
        stmt.kind,
        StmtKind::Break(_) | StmtKind::Continue(_) | StmtKind::Return(_) | StmtKind::Throw(_)
    )
}

/// Returns whether `stmt` is a loop, whose body may run again after its own `unset()`s.
fn is_loop_stmt(stmt: &Stmt) -> bool {
    matches!(
        stmt.kind,
        StmtKind::While { .. }
            | StmtKind::DoWhile { .. }
            | StmtKind::For { .. }
            | StmtKind::Foreach { .. }
    )
}
