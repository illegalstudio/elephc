//! Purpose:
//! Carries the checked view of element-forming values into EIR, so a boxed operand does not
//! widen a container the checker keeps concrete.
//!
//! Called from:
//! - Array append and keyed-write checking, and array literal inference.
//!
//! Key details:
//! - A hash `foreach` binding, a guard-narrowed `mixed` local and a branch merge are boxed
//!   `Mixed` cells in EIR while the checker types them `int`, `float`, `bool`, `string` or a
//!   concrete array. Lowering widened the receiving container for such an operand, and the
//!   checker did not, so every consumer compiled against the checker's type read boxed cells
//!   as raw payloads.
//! - A view is published only for a proven value: the intact binding of an array `foreach` or
//!   a type guard's view or kept complement, read from a flow-stable, definitely bound local.
//!   An ordinary assigned local is not proof: its checked type can drop a stored `null`.
//! - A global or a by-reference captured local is a boxed cell other code can rewrite, so it
//!   contributes `mixed` to the container's checked element type instead of a view.
//! - A scalar branch merge publishes its type on each proven local its arms read, so the
//!   merge itself is concrete and a literal's predicted storage matches its lowered items.
//! - A body that may not run is a proof scope: a proof recorded inside it never reaches a
//!   sibling body, and a proof it ends stays ended. The code after the statement keeps only
//!   the proofs every body that reaches it ended with.
//! - Conflicting authoritative visits disable the site, and storage probes publish nothing.

use std::collections::{HashMap, HashSet};

use super::Checker;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

impl Checker {
    /// Records the type the checker proved for a value written as a container element.
    ///
    /// The key is the current body scope, the value expression's own span and the local the
    /// value reads directly (`None` when it is not one plain variable). A second authoritative
    /// visit that proves a different view for that key, or none, permanently disables it,
    /// which leaves lowering on its representation-preserving widening path.
    pub(crate) fn record_element_value_view(&mut self, value: &Expr, ty: &PhpType, env: &TypeEnv) {
        if !value.span.identifies_a_node()
            || self.member_receiver_analysis_scopes.iter().any(|(function, scope)| {
                function == &self.current_function
                    && crate::types::loop_storage_scope_is_within(
                        &self.current_loop_storage_scope,
                        scope,
                    )
            })
        {
            return;
        }
        let view = self.projectable_element_view(value, ty, env);
        self.publish_merge_arm_views(value, ty, env);
        self.publish_element_value_view(value.span, direct_local(value), view);
    }

    /// Stores one site's view, disabling the site when an earlier visit proved another one.
    ///
    /// `local` is the variable the value reads directly, which is part of the site's identity.
    fn publish_element_value_view(
        &mut self,
        span: crate::span::Span,
        local: Option<&str>,
        view: Option<PhpType>,
    ) {
        if !span.identifies_a_node() {
            return;
        }
        let key = (self.current_loop_storage_scope.clone(), span, local.map(str::to_string));
        self.element_value_views
            .entry(key)
            .and_modify(|previous| {
                if *previous != view {
                    *previous = None;
                }
            })
            .or_insert(view);
    }

    /// Publishes a branch merge's scalar type on each proven local its arms read.
    ///
    /// A merge of a boxed arm and a concrete one materializes in a boxed temp, whose storage a
    /// literal is predicted from before any item is lowered. Projecting the proven arms
    /// themselves makes the merge concrete instead, so the predicted storage, the merge temp
    /// and the stored element agree for every kind of site.
    ///
    /// Each arm is judged on its own: reading a proven local as the scalar it holds is exact
    /// whatever the other arms are, so a sibling call or constant does not withhold it. An arm
    /// local that is not proven, and every arm of a merge that is not one scalar, publishes
    /// `None`, which disables the arm like any other conflicting visit.
    fn publish_merge_arm_views(&mut self, value: &Expr, ty: &PhpType, env: &TypeEnv) {
        let Some(arms) = merge_arms(value) else {
            return;
        };
        let scalar = is_projectable_scalar(ty);
        for arm in arms {
            if let ExprKind::Variable(name) = &arm.kind {
                let arm_view =
                    (scalar && self.element_value_is_proven(arm, ty, env)).then(|| ty.clone());
                self.publish_element_value_view(arm.span, Some(name.as_str()), arm_view);
            } else {
                self.publish_merge_arm_views(arm, ty, env);
            }
        }
    }

    /// Returns the view lowering may project a boxed operand to, if this value has one.
    ///
    /// A scalar is projected with a cast and a concrete array or hash is extracted from its
    /// box as it is. Both are exact only for a value the checker can prove holds that type on
    /// this path, which [`Checker::element_value_is_proven`] decides.
    fn projectable_element_view(
        &self,
        value: &Expr,
        ty: &PhpType,
        env: &TypeEnv,
    ) -> Option<PhpType> {
        let scalar = is_projectable_scalar(ty);
        let local = direct_local(value);
        let projectable = if scalar {
            self.element_value_is_proven(value, ty, env)
        } else {
            // A guard proves a value is an array, never which layout the array has, so only a
            // loop binding whose source stores this very container qualifies.
            matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. })
                && local.is_some_and(|name| {
                    self.element_local_read_is_stable(name, ty, env)
                        && self.proven_local_views.get(name) == Some(ty)
                })
        };
        projectable.then(|| ty.clone())
    }

    /// Returns whether the checked type of an element-forming value is a proof, not a guess.
    ///
    /// An ordinary assigned local is not one: `$t = null; if ($c) { $t = "s"; }` reads `string`
    /// afterwards, and casting the boxed `null` it may hold would store `""`. A local qualifies
    /// while it has a recorded proof (`Checker::proven_local_views`: an array `foreach` binding,
    /// or the complement of a guard that exits on every other path) or is read inside an open
    /// guard region that has not stored it, where its type is the guard's view or complement.
    /// A branch merge (`?:`, `??`, `match`) is as proven as its arms, and a literal arm is its
    /// own type. Any other expression publishes nothing: its operand is concrete already, or
    /// boxed for a reason this pass cannot see.
    pub(crate) fn element_value_is_proven(&self, value: &Expr, ty: &PhpType, env: &TypeEnv) -> bool {
        match &value.kind {
            ExprKind::Variable(name) => {
                self.element_local_read_is_stable(name, ty, env)
                    && (self.proven_local_views.get(name) == Some(ty)
                        || self.narrowed_local_origins.get(name).is_some_and(|origin| {
                            !origin.stored_in_region && origin.view.is_some()
                        }))
            }
            ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::StringLiteral(_) => true,
            _ => merge_arms(value).is_some_and(|arms| {
                arms.into_iter().all(|arm| self.element_value_is_proven(arm, ty, env))
            }),
        }
    }

    /// Returns whether a read of `name` sees exactly the binding the environment types `ty`.
    ///
    /// The local must be unaliased, definitely bound and still typed `ty`, so no call, reference
    /// or skipped path can have put another value in its cell.
    fn element_local_read_is_stable(&self, name: &str, ty: &PhpType, env: &TypeEnv) -> bool {
        self.local_flow_read_is_stable(name)
            && self.definitely_bound_locals.contains(name)
            && env.get(name) == Some(ty)
    }

    /// Records the bindings an array `foreach` gives its key and by-value value variables.
    ///
    /// Returns the names the loop binds, for [`Checker::end_foreach_views`]. A by-reference
    /// value aliases its element and an object or `mixed` source binds `mixed`, so neither is
    /// recorded; a name left out also loses any proof an enclosing loop gave it, since this
    /// loop rebinds it either way.
    pub(crate) fn bind_foreach_views(
        &mut self,
        source: &PhpType,
        key: Option<&str>,
        value: &str,
        value_by_ref: bool,
        env: &TypeEnv,
    ) -> Vec<String> {
        let array_source = matches!(source, PhpType::Array(_) | PhpType::AssocArray { .. });
        let bindings = key.map(|name| (name, true)).into_iter().chain([(value, !value_by_ref)]);
        let mut bound_names = Vec::new();
        for (name, by_value) in bindings {
            match env.get(name).filter(|_| array_source && by_value) {
                Some(view) => {
                    self.proven_local_views.insert(name.to_string(), view.clone());
                }
                None => self.end_local_proof(name),
            }
            bound_names.push(name.to_string());
        }
        bound_names
    }

    /// Records the complement views an exhaustively exiting guard leaves for the code after it.
    ///
    /// Only plain locals are recorded. A narrowed property place has no cell of its own to
    /// project, and a name the guard left `mixed` proves nothing.
    pub(crate) fn record_kept_guard_views<'n>(
        &mut self,
        names: impl Iterator<Item = &'n str>,
        env: &TypeEnv,
    ) {
        for name in names {
            if Self::narrowed_place_key_is_property(name) {
                continue;
            }
            if let Some(view) = env.get(name) {
                self.proven_local_views.insert(name.to_string(), view.clone());
            }
        }
    }

    /// Ends a `foreach` loop's binding proofs once its body has been checked.
    ///
    /// After the loop a name holds whatever it held before a source that turned out to be
    /// empty, and an enclosing loop that bound the same name was rebound by this one, so no
    /// earlier proof is reinstated.
    pub(crate) fn end_foreach_views(&mut self, bound_names: Vec<String>) {
        for name in bound_names {
            self.end_local_proof(&name);
        }
    }

    /// Returns the type a value contributes to the storage of the container it is written into.
    ///
    /// A `global` binding and a local a closure captured by reference live in boxed cells that
    /// code outside this body's flow can rewrite, so the checked type of such a read is neither
    /// the operand lowering stores nor a proof of what the cell holds when the write runs.
    /// The element is therefore `mixed`: the container widens in the checker exactly where
    /// lowering boxes it, instead of keeping a concrete element type nothing backs.
    ///
    /// Only the payloads a boxed operand is otherwise projected to are affected: scalars and
    /// concrete arrays. An object keeps its class, which member access on the elements reads.
    pub(crate) fn element_storage_type(&self, value: &Expr, ty: PhpType) -> PhpType {
        let raw_payload = matches!(
            ty,
            PhpType::Int
                | PhpType::Float
                | PhpType::Bool
                | PhpType::Str
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
        );
        if raw_payload && self.boxed_volatile_element_source(value).is_some() {
            PhpType::Mixed
        } else {
            ty
        }
    }

    /// Carries a proof across a plain copy: `$copy = $value` holds what `$value` holds.
    ///
    /// Called after the statement-form assignment has been checked, so the store has already
    /// ended whatever proof `target` had. A join or a loop that can skip or repeat the copy
    /// ends the new proof like any other.
    pub(crate) fn record_copied_local_view(&mut self, target: &str, value: &Expr, env: &TypeEnv) {
        let ExprKind::Variable(source) = &value.kind else {
            return;
        };
        let Some(ty) = env.get(target).cloned() else {
            return;
        };
        if source != target
            && self.local_flow_read_is_stable(target)
            && self.element_value_is_proven(value, &ty, env)
        {
            self.proven_local_views.insert(target.to_string(), ty);
        }
    }

    /// Ends the proof recorded for `name`, here and in every enclosing proof scope.
    pub(crate) fn end_local_proof(&mut self, name: &str) {
        self.proven_local_views.remove(name);
        for invalidated in &mut self.proof_scope_invalidations {
            invalidated.insert(name.to_string());
        }
    }

    /// Opens a proof scope for a body that may not run, returning the proofs it starts with.
    ///
    /// A copy or a kept guard complement recorded inside an `if` arm, a `switch` case, a `try`
    /// block or a loop body holds only where that body ran. Sibling bodies are checked right
    /// after it against the same checker state, so without a scope the skipped arm's proof
    /// would authorize a cast in the arm that does run.
    pub(crate) fn begin_proof_scope(&mut self) -> HashMap<String, PhpType> {
        self.proof_scope_invalidations.push(HashSet::new());
        self.proven_local_views.clone()
    }

    /// Closes a proof scope: the entry proofs the body did not end are all that survives it.
    ///
    /// Returns the proofs that held where the body ended, for the statement that joins it with
    /// its sibling bodies.
    pub(crate) fn end_proof_scope(
        &mut self,
        mut entry: HashMap<String, PhpType>,
    ) -> HashMap<String, PhpType> {
        let invalidated = self
            .proof_scope_invalidations
            .pop()
            .expect("proof scopes must be balanced");
        entry.retain(|name, _| !invalidated.contains(name));
        std::mem::replace(&mut self.proven_local_views, entry)
    }

    /// Records the proofs every body that reaches the code after a statement ended with.
    ///
    /// `exits` holds one entry per body that falls through, and the caller passes it only when
    /// one of those bodies always runs (an `if` chain with an `else`, a `try` with its
    /// `catch` blocks). A proof all of them hold is a fact on every path to the next statement:
    /// `if ($c) { $t = $v; } else { $t = $w; }` leaves `$t` proven, and so does a `try` body
    /// that completed.
    pub(crate) fn record_joined_body_proofs(&mut self, exits: Vec<HashMap<String, PhpType>>) {
        let mut exits = exits.into_iter();
        let Some(mut joined) = exits.next() else {
            return;
        };
        for exit in exits {
            joined.retain(|name, ty| exit.get(name) == Some(ty));
        }
        self.proven_local_views.extend(joined);
    }

    /// Opens the scope of a body that always runs after proofs waiting to be recorded.
    pub(crate) fn begin_proof_carry(&mut self) {
        self.proof_scope_invalidations.push(HashSet::new());
    }

    /// Records the proofs carried past a body opened with `begin_proof_carry`.
    ///
    /// A `finally` block runs between the bodies it follows and the next statement, so their
    /// joined proofs survive it except for the names it stores.
    pub(crate) fn end_proof_carry(&mut self, exits: Vec<HashMap<String, PhpType>>) {
        let invalidated = self
            .proof_scope_invalidations
            .pop()
            .expect("proof scopes must be balanced");
        let exits = exits
            .into_iter()
            .map(|mut exit| {
                exit.retain(|name, _| !invalidated.contains(name));
                exit
            })
            .collect();
        self.record_joined_body_proofs(exits);
    }

    /// Ends the proofs of every local a loop can store, before its body is checked.
    ///
    /// The body is checked once against the entry facts, but a read at its top also runs after
    /// every store a previous iteration made, and such a store need not be a proven one.
    pub(crate) fn end_loop_stored_views<'n>(&mut self, stored: impl Iterator<Item = &'n str>) {
        for name in stored {
            self.end_local_proof(name);
        }
    }

    /// Returns the local a value reads directly when that local is a boxed, volatile cell.
    pub(crate) fn boxed_volatile_element_source<'e>(&self, value: &'e Expr) -> Option<&'e str> {
        match &value.kind {
            ExprKind::Variable(name) if self.local_storage_is_boxed_volatile(name) => {
                Some(name.as_str())
            }
            _ => None,
        }
    }

    /// Returns whether a local's value lives in a boxed cell other code can replace.
    ///
    /// Statics, by-reference parameters and plain `=&` aliases keep typed storage, so their
    /// reads are the concrete operand the checker types and are not listed here.
    pub(crate) fn local_storage_is_boxed_volatile(&self, name: &str) -> bool {
        self.active_globals.contains(name)
            || self.by_ref_captured_locals.contains(name)
            || self.boxed_ref_aliased_locals.contains(name)
            || self.top_level_binding_is_program_global(name)
    }
}

/// Returns the local a value reads directly, when the value is one plain variable.
fn direct_local(value: &Expr) -> Option<&str> {
    match &value.kind {
        ExprKind::Variable(name) => Some(name.as_str()),
        _ => None,
    }
}

/// Returns whether a checked type is a scalar a boxed operand is cast to exactly.
fn is_projectable_scalar(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Str)
}

/// Returns the expressions one of which becomes the value of a branch merge.
///
/// `@expr` is its operand, a ternary and `??` / `?:` have two results, and a `match` has one
/// per arm plus its default. `None` for every expression that is not a merge.
fn merge_arms(value: &Expr) -> Option<Vec<&Expr>> {
    match &value.kind {
        ExprKind::ErrorSuppress(inner) => Some(vec![inner.as_ref()]),
        ExprKind::Ternary { then_expr, else_expr, .. } => {
            Some(vec![then_expr.as_ref(), else_expr.as_ref()])
        }
        ExprKind::ShortTernary { value, default } | ExprKind::NullCoalesce { value, default } => {
            Some(vec![value.as_ref(), default.as_ref()])
        }
        ExprKind::Match { arms, default, .. } => Some(
            arms.iter()
                .map(|(_, result)| result)
                .chain(default.as_deref())
                .collect(),
        ),
        _ => None,
    }
}
