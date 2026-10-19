//! Purpose:
//! Computes which locals a condition DEFINITELY assigns on its true edge and on its false edge,
//! following PHP's short-circuit evaluation: `$a && ($r = f())` assigns `$r` whenever it is
//! true, `!$x || !($y = g())` assigns `$y` whenever it is false.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::control_flow` (the `if` / `elseif` / loop bodies) and
//!   `crate::types::checker::inference::expr::effects` (ternary arms), which type those locals.
//! - `crate::ir_lower` (`if`, loops, ternary), which marks their slots initialized on that edge.
//!
//! Key details:
//! - Purely syntactic, so the checker and lowering agree on the same set.
//! - A store inside a lazily evaluated operand that this walk does not decompose (a nested
//!   `&&` inside a call argument, a `?:` default, a closure body) is never counted: it may not run.

use std::collections::BTreeSet;

use crate::parser::ast::{BinOp, Expr, ExprKind};

use super::loop_storage::visit_child_expressions;

/// Returns the locals `condition` assigns on every evaluation that yields `edge` (true/false).
pub(crate) fn definitely_assigned_on_edge(condition: &Expr, edge: bool) -> BTreeSet<String> {
    match &condition.kind {
        ExprKind::BinaryOp { left, op: BinOp::And, right } => {
            let left_true = definitely_assigned_on_edge(left, true);
            if edge {
                // Both operands ran and were true.
                left_true.union(&definitely_assigned_on_edge(right, true)).cloned().collect()
            } else {
                // Either the left was false, or it was true and the right was false.
                let left_false = definitely_assigned_on_edge(left, false);
                let through_right: BTreeSet<String> =
                    left_true.union(&definitely_assigned_on_edge(right, false)).cloned().collect();
                left_false.intersection(&through_right).cloned().collect()
            }
        }
        ExprKind::BinaryOp { left, op: BinOp::Or, right } => {
            let left_false = definitely_assigned_on_edge(left, false);
            if edge {
                let left_true = definitely_assigned_on_edge(left, true);
                let through_right: BTreeSet<String> =
                    left_false.union(&definitely_assigned_on_edge(right, true)).cloned().collect();
                left_true.intersection(&through_right).cloned().collect()
            } else {
                left_false.union(&definitely_assigned_on_edge(right, false)).cloned().collect()
            }
        }
        ExprKind::Not(inner) => definitely_assigned_on_edge(inner, !edge),
        _ => {
            let mut out = BTreeSet::new();
            collect_unconditional_assignments(condition, &mut out);
            out
        }
    }
}

/// Collects plain-local assignment targets that run whenever `expr` is evaluated.
fn collect_unconditional_assignments(expr: &Expr, out: &mut BTreeSet<String>) {
    match &expr.kind {
        ExprKind::Assignment { target, value, .. } => {
            collect_unconditional_assignments(value, out);
            if let ExprKind::Variable(name) = &target.kind {
                // `$x ??= v` stores only when `$x` is null, but leaves `$x` defined either way.
                out.insert(name.clone());
            } else {
                collect_unconditional_assignments(target, out);
            }
        }
        // Lazily evaluated operands may not run.
        ExprKind::BinaryOp { left, op: BinOp::And | BinOp::Or, .. } => {
            collect_unconditional_assignments(left, out)
        }
        ExprKind::ShortTernary { value, .. } | ExprKind::NullCoalesce { value, .. } => {
            collect_unconditional_assignments(value, out)
        }
        ExprKind::Ternary { condition, .. } => collect_unconditional_assignments(condition, out),
        ExprKind::Closure { .. } | ExprKind::Match { .. } => {}
        _ => visit_child_expressions(expr, &mut |child| collect_unconditional_assignments(child, out)),
    }
}

impl super::Checker {
    /// Returns the locals `condition` definitely defines on `edge` that `env` does not hold yet,
    /// with their types. The types come from re-inferring the condition on a copy of `env` as if
    /// every lazy operand ran, which is exactly the evaluation that edge implies for those names.
    pub(crate) fn condition_edge_definitions(
        &mut self,
        condition: &Expr,
        env: &crate::types::TypeEnv,
        edge: bool,
    ) -> crate::types::TypeEnv {
        let names = definitely_assigned_on_edge(condition, edge);
        let mut defined = crate::types::TypeEnv::new();
        if names.iter().all(|name| env.contains_key(name)) {
            return defined;
        }
        let all_ran = self.infer_with_every_lazy_operand(condition, env);
        for name in names {
            if env.contains_key(&name) {
                continue;
            }
            if let Some(ty) = all_ran.get(&name) {
                defined.insert(name, ty.clone());
            }
        }
        defined
    }

    /// Infers `expr` on a copy of `env`, applying the assignments of every lazy operand the
    /// edge walk decomposes (`&&`, `||`, `!`) in evaluation order.
    fn infer_with_every_lazy_operand(
        &mut self,
        expr: &Expr,
        env: &crate::types::TypeEnv,
    ) -> crate::types::TypeEnv {
        match &expr.kind {
            ExprKind::BinaryOp { left, op: BinOp::And | BinOp::Or, right } => {
                let after_left = self.infer_with_every_lazy_operand(left, env);
                self.infer_with_every_lazy_operand(right, &after_left)
            }
            ExprKind::Not(inner) => self.infer_with_every_lazy_operand(inner, env),
            _ => {
                let mut copy = env.clone();
                let _ = self.infer_type_with_assignment_effects(expr, &mut copy);
                copy
            }
        }
    }

    /// Inserts edge definitions into `env` for a guarded region, returning what was inserted so
    /// [`Self::remove_edge_definitions`] can take it back out afterwards.
    pub(crate) fn insert_edge_definitions(
        env: &mut crate::types::TypeEnv,
        defined: &crate::types::TypeEnv,
    ) -> Vec<(String, crate::types::PhpType)> {
        let mut inserted = Vec::new();
        for (name, ty) in defined {
            if !env.contains_key(name) {
                env.insert(name.clone(), ty.clone());
                inserted.push((name.clone(), ty.clone()));
            }
        }
        inserted
    }

    /// Removes edge definitions a guarded region did not overwrite: past the region the edge no
    /// longer holds, so the local is only POSSIBLY defined and reading it stays an error.
    pub(crate) fn remove_edge_definitions(
        env: &mut crate::types::TypeEnv,
        inserted: &[(String, crate::types::PhpType)],
    ) {
        for (name, ty) in inserted {
            if env.get(name) == Some(ty) {
                env.remove(name);
            }
        }
    }
}

/// Maybe-undefined locals.
///
/// A local a join point may leave undefined — assigned in one `if` arm, on one lazy edge, or only
/// inside a loop body — is tracked in `Checker::maybe_undefined_locals` while the body is walked.
/// A read of such a local is recorded in `maybe_undefined_result` and typed `mixed`, and EIR
/// lowering answers it from the local's hidden "defined" flag: PHP null (after an `Undefined
/// variable` warning for a plain read) when the local was never assigned on the path taken.
/// Before this, the read loaded whatever the slot held: a previous call's value for `isset($r)`,
/// a folded null for `$r ?? $d` even when the edge had assigned it.
impl super::Checker {
    /// Whether `name` can be tracked: a frame local this body owns and writes only by name.
    pub(crate) fn local_is_flaggable(&self, name: &str) -> bool {
        !self.body_contains_eval
            && name != "this"
            && !name.starts_with('\u{1}')
            && !self.active_ref_params.contains(name)
            && !self.ref_aliased_locals.contains(name)
            && !self.by_ref_argument_locals.contains(name)
            && !self.active_globals.contains(name)
            && !self.static_local_names.contains(name)
            && !self.name_is_seeded_program_storage(name)
            && !self.top_level_binding_is_program_global(name)
    }

    /// Marks `name` maybe-undefined from this point of the walk.
    pub(crate) fn mark_maybe_undefined(&mut self, name: &str) {
        if self.local_is_flaggable(name) {
            self.maybe_undefined_locals.insert(name.to_string());
        }
    }

    /// Records a read of a maybe-undefined local; returns whether the read is flagged.
    pub(crate) fn record_maybe_undefined_read(&mut self, name: &str, span: crate::span::Span) -> bool {
        if !self.maybe_undefined_locals.contains(name) || !self.local_is_flaggable(name) {
            return false;
        }
        let warns = self.null_probe_depth == 0;
        self.maybe_undefined_result
            .reads
            .insert((span, name.to_string()), warns);
        self.maybe_undefined_result
            .by_scope
            .entry(self.current_loop_storage_scope.clone())
            .or_default()
            .insert(name.to_string());
        true
    }

    /// Whether the closure created at `span` captures `name` while it may be undefined.
    pub(crate) fn capture_may_be_undefined(&self, name: &str, span: crate::span::Span) -> bool {
        (self.maybe_undefined_locals.contains(name) && self.local_is_flaggable(name))
            || self
                .maybe_undefined_result
                .reads
                .contains_key(&(span, name.to_string()))
    }

    /// Records the captures of the closure created at `span` that may be undefined there,
    /// each as a read at the closure's span that EIR lowering answers
    /// from the defined-flag:
    /// - `use (&$x)` creates `$x` as null when it is undefined, so it is defined from here on;
    /// - `use ($x)` warns `Undefined variable` and captures null;
    /// - an arrow function captures silently, and the local stays undefined in its body (EIR
    ///   lowering passes the flag along). Those names are returned, for the body's entry marks.
    pub(crate) fn record_maybe_undefined_captures(
        &mut self,
        captures: &[String],
        capture_refs: &[String],
        is_arrow: bool,
        span: crate::span::Span,
    ) -> Vec<String> {
        let mut arrow_undefined = Vec::new();
        for cap in captures {
            if !self.maybe_undefined_locals.contains(cap) || !self.local_is_flaggable(cap) {
                continue;
            }
            let by_ref = capture_refs.iter().any(|name| name == cap);
            self.maybe_undefined_result
                .reads
                .insert((span, cap.clone()), !by_ref && !is_arrow);
            self.maybe_undefined_result
                .by_scope
                .entry(self.current_loop_storage_scope.clone())
                .or_default()
                .insert(cap.clone());
            if by_ref {
                self.maybe_undefined_locals.remove(cap);
            } else if is_arrow {
                arrow_undefined.push(cap.clone());
            }
        }
        arrow_undefined
    }

    /// Joins a construct whose body may not run (a loop) or whose parts the walk cannot split
    /// into arms (`switch`, `try`): a mark the body cleared does not hold after it, and a local
    /// the body introduced, outside `definite`, may be undefined after it.
    pub(crate) fn join_maybe_undefined_after_optional_region(
        &mut self,
        marks_before: &std::collections::HashSet<String>,
        keys_before: &std::collections::HashSet<String>,
        definite: &BTreeSet<String>,
        mark_new: bool,
        env: &crate::types::TypeEnv,
    ) {
        for name in marks_before {
            self.maybe_undefined_locals.insert(name.clone());
        }
        if mark_new {
            let new = env
                .keys()
                .filter(|name| !keys_before.contains(*name) && !definite.contains(*name))
                .cloned()
                .collect::<Vec<_>>();
            for name in new {
                self.mark_maybe_undefined(&name);
            }
        }
    }
}

/// Assignments a statement runs before its body for sure (`for` init, the unconditional part of
/// a loop condition), which stay defined after the loop.
pub(crate) fn loop_head_definite_assignments(stmt: &crate::parser::ast::Stmt) -> BTreeSet<String> {
    use crate::parser::ast::StmtKind;
    let mut out = BTreeSet::new();
    match &stmt.kind {
        StmtKind::While { condition, .. } => collect_unconditional_assignments(condition, &mut out),
        StmtKind::For { init, condition, .. } => {
            if let Some(init) = init {
                if let StmtKind::Assign { name, .. } = &init.kind {
                    out.insert(name.clone());
                } else if let StmtKind::ExprStmt(expr) = &init.kind {
                    collect_unconditional_assignments(expr, &mut out);
                } else if let StmtKind::Synthetic(stmts) = &init.kind {
                    for stmt in stmts {
                        if let StmtKind::Assign { name, .. } = &stmt.kind {
                            out.insert(name.clone());
                        }
                    }
                }
            }
            if let Some(condition) = condition {
                collect_unconditional_assignments(condition, &mut out);
            }
        }
        _ => {}
    }
    out
}

/// Locals named by an `unset($x)` statement anywhere inside `stmts`, nested bodies included.
/// A loop body may run again after such an `unset`, and a `break` may leave
/// a loop or `switch` right after one, so these names may be undefined at the body's start and
/// after the construct even where the walk's own join cannot see the `unset` path.
pub(crate) fn unset_locals_in(stmts: &[crate::parser::ast::Stmt]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    collect_unset_locals(stmts, &mut out);
    out
}

fn collect_unset_locals(stmts: &[crate::parser::ast::Stmt], out: &mut BTreeSet<String>) {
    use crate::parser::ast::StmtKind;
    for stmt in stmts {
        if let StmtKind::ExprStmt(expr) = &stmt.kind {
            if let ExprKind::FunctionCall { name, args } = &expr.kind {
                if super::is_unset_call(name.as_str()) {
                    for arg in args {
                        if let ExprKind::Variable(var) = &arg.kind {
                            out.insert(var.clone());
                        }
                    }
                }
            }
        }
        if let StmtKind::Foreach { body, .. } = &stmt.kind {
            collect_unset_locals(body, out);
        }
        for body in super::loop_storage::nested_statement_bodies(stmt) {
            collect_unset_locals(body, out);
        }
    }
}

/// Whether `stmts` holds a `break`/`continue` other than a trailing top-level `break`, i.e. an
/// exit whose maybe-undefined state the walk does not record (a `switch` case that leaves from
/// inside an `if`).
pub(crate) fn has_inner_break_or_continue(stmts: &[crate::parser::ast::Stmt]) -> bool {
    use crate::parser::ast::StmtKind;
    let rest = match stmts.split_last() {
        Some((last, rest)) if matches!(last.kind, StmtKind::Break(_)) => rest,
        _ => stmts,
    };
    rest.iter().any(stmt_has_break_or_continue)
}

fn stmt_has_break_or_continue(stmt: &crate::parser::ast::Stmt) -> bool {
    use crate::parser::ast::StmtKind;
    match &stmt.kind {
        StmtKind::Break(_) | StmtKind::Continue(_) => true,
        StmtKind::Foreach { body, .. } => body.iter().any(stmt_has_break_or_continue),
        _ => super::loop_storage::nested_statement_bodies(stmt)
            .into_iter()
            .any(|body| body.iter().any(stmt_has_break_or_continue)),
    }
}

type NameSet = std::collections::HashSet<String>;

fn env_keys(env: &crate::types::TypeEnv) -> NameSet {
    env.keys().cloned().collect()
}

/// Sets the walk's marks to the join of `exits` (marks, env keys at the exit): every exit's
/// marks, plus each local introduced since `keys_entry` that some exit has not bound.
fn join_exits(
    checker: &mut super::Checker,
    exits: &[(NameSet, NameSet)],
    keys_entry: &NameSet,
    fallback: NameSet,
    env: &crate::types::TypeEnv,
) {
    let mut joined: NameSet = if exits.is_empty() {
        fallback
    } else {
        exits.iter().flat_map(|(marks, _)| marks.iter().cloned()).collect()
    };
    for name in env.keys() {
        if !keys_entry.contains(name) && exits.iter().any(|(_, keys)| !keys.contains(name)) {
            joined.insert(name.clone());
        }
    }
    checker.maybe_undefined_locals.clear();
    for name in joined {
        checker.mark_maybe_undefined(&name);
    }
}

/// Maybe-undefined join of a `switch`. A case body is entered by a match
/// or by falling through from the previous body, so it starts with the switch's entry marks, the
/// previous body's end marks and every local an earlier body introduced. The switch is left by
/// a trailing `break`, by falling off the last body, by a `break` nested deeper (recorded
/// conservatively), or, without `default`, by matching nothing.
pub(crate) struct SwitchMaybeUndefined {
    entry: NameSet,
    keys_entry: NameSet,
    introduced: NameSet,
    previous_end: Option<NameSet>,
    body_start: NameSet,
    body_keys: NameSet,
    exits: Vec<(NameSet, NameSet)>,
}

impl SwitchMaybeUndefined {
    pub(crate) fn enter(checker: &super::Checker, env: &crate::types::TypeEnv) -> Self {
        Self {
            entry: checker.maybe_undefined_locals.clone(),
            keys_entry: env_keys(env),
            introduced: NameSet::new(),
            previous_end: None,
            body_start: NameSet::new(),
            body_keys: NameSet::new(),
            exits: Vec::new(),
        }
    }

    pub(crate) fn start_body(&mut self, checker: &mut super::Checker, env: &crate::types::TypeEnv) {
        let mut start = self.entry.clone();
        if let Some(previous) = self.previous_end.take() {
            start.extend(previous);
        }
        checker.maybe_undefined_locals = start;
        for name in &self.introduced {
            checker.mark_maybe_undefined(name);
        }
        self.body_start = checker.maybe_undefined_locals.clone();
        self.body_keys = env_keys(env);
    }

    pub(crate) fn end_body(
        &mut self,
        checker: &mut super::Checker,
        body: &[crate::parser::ast::Stmt],
        env: &crate::types::TypeEnv,
    ) {
        use crate::parser::ast::StmtKind;
        let marks = checker.maybe_undefined_locals.clone();
        let keys = env_keys(env);
        let introduced_here = keys
            .iter()
            .filter(|name| !self.body_keys.contains(*name))
            .cloned()
            .collect::<NameSet>();
        if matches!(body.last().map(|stmt| &stmt.kind), Some(StmtKind::Break(levels)) if *levels <= 1) {
            self.exits.push((marks.clone(), keys.clone()));
        } else if !checker.body_cannot_fall_through(body) {
            self.previous_end = Some(marks.clone());
        }
        if has_inner_break_or_continue(body) {
            let mut conservative = self.body_start.clone();
            conservative.extend(marks);
            conservative.extend(introduced_here.iter().cloned());
            self.exits.push((conservative, self.body_keys.clone()));
        }
        self.introduced.extend(introduced_here);
    }

    pub(crate) fn finish(
        mut self,
        checker: &mut super::Checker,
        has_default: bool,
        env: &crate::types::TypeEnv,
    ) {
        // Falling off the last body leaves the switch.
        if let Some(previous) = self.previous_end.take() {
            self.exits.push((previous, env_keys(env)));
        }
        if !has_default {
            self.exits.push((self.entry.clone(), self.keys_entry.clone()));
        }
        let fallback = self.entry.clone();
        join_exits(checker, &self.exits, &self.keys_entry, fallback, env);
    }
}

/// Maybe-undefined join of a `try`. A handler can start anywhere in the
/// `try` body, so it sees the entry marks, every mark the body set and every local it (or an
/// earlier handler) introduced; the construct is left by the `try` body or a handler falling
/// through, and a `finally` body runs on every path (normal completion keeps what it defines).
pub(crate) struct TryMaybeUndefined {
    entry: NameSet,
    keys_entry: NameSet,
    try_marks: NameSet,
    introduced: NameSet,
    catch_keys: NameSet,
    exits: Vec<(NameSet, NameSet)>,
}

impl TryMaybeUndefined {
    pub(crate) fn enter(checker: &super::Checker, env: &crate::types::TypeEnv) -> Self {
        Self {
            entry: checker.maybe_undefined_locals.clone(),
            keys_entry: env_keys(env),
            try_marks: NameSet::new(),
            introduced: NameSet::new(),
            catch_keys: NameSet::new(),
            exits: Vec::new(),
        }
    }

    pub(crate) fn end_try(
        &mut self,
        checker: &mut super::Checker,
        body: &[crate::parser::ast::Stmt],
        env: &crate::types::TypeEnv,
    ) {
        let marks = checker.maybe_undefined_locals.clone();
        let keys = env_keys(env);
        if !checker.body_cannot_fall_through(body) {
            self.exits.push((marks.clone(), keys.clone()));
        }
        self.try_marks = marks;
        self.introduced = keys.difference(&self.keys_entry).cloned().collect();
    }

    pub(crate) fn start_catch(&mut self, checker: &mut super::Checker, env: &crate::types::TypeEnv) {
        let mut start = self.entry.clone();
        start.extend(self.try_marks.iter().cloned());
        checker.maybe_undefined_locals = start;
        for name in &self.introduced {
            checker.mark_maybe_undefined(name);
        }
        self.catch_keys = env_keys(env);
    }

    pub(crate) fn end_catch(
        &mut self,
        checker: &mut super::Checker,
        body: &[crate::parser::ast::Stmt],
        env: &crate::types::TypeEnv,
    ) {
        let keys = env_keys(env);
        if !checker.body_cannot_fall_through(body) {
            self.exits.push((checker.maybe_undefined_locals.clone(), keys.clone()));
        }
        self.introduced
            .extend(keys.difference(&self.catch_keys).cloned());
    }

    pub(crate) fn join(&mut self, checker: &mut super::Checker, env: &crate::types::TypeEnv) {
        let mut fallback = self.entry.clone();
        fallback.extend(self.try_marks.iter().cloned());
        fallback.extend(self.introduced.iter().cloned());
        join_exits(checker, &self.exits, &self.keys_entry, fallback, env);
    }

    /// Enters `finally`, which also runs on the exceptional and early-exit paths; returns the
    /// normal-completion marks and the body's entry marks.
    pub(crate) fn start_finally(&self, checker: &mut super::Checker) -> (NameSet, NameSet) {
        let normal = checker.maybe_undefined_locals.clone();
        let mut entry = normal.clone();
        entry.extend(self.entry.iter().cloned());
        entry.extend(self.try_marks.iter().cloned());
        for name in &self.introduced {
            if checker.local_is_flaggable(name) {
                entry.insert(name.clone());
            }
        }
        checker.maybe_undefined_locals = entry.clone();
        (normal, entry)
    }

    /// Leaves `finally`: normal completion keeps its marks minus what the body defined, plus
    /// what it unset.
    pub(crate) fn end_finally(&self, checker: &mut super::Checker, (normal, entry): (NameSet, NameSet)) {
        let now = checker.maybe_undefined_locals.clone();
        let mut result: NameSet = normal
            .into_iter()
            .filter(|name| !entry.contains(name) || now.contains(name))
            .collect();
        result.extend(now.into_iter().filter(|name| !entry.contains(name)));
        checker.maybe_undefined_locals = result;
    }
}
