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
//! - Conflicting authoritative visits disable the site, and storage probes publish nothing.

use super::Checker;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{ElementValueView, PhpType, TypeEnv};

impl Checker {
    /// Records the type the checker proved for a value written as a container element.
    ///
    /// The key is the value expression's own span inside the current body scope. A second
    /// authoritative visit that proves a different view, or none, permanently disables the
    /// site, which leaves lowering on its representation-preserving widening path.
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
        self.publish_element_value_view(value.span, view);
    }

    /// Stores one site's view, disabling the site when an earlier visit proved another one.
    fn publish_element_value_view(&mut self, span: crate::span::Span, view: Option<ElementValueView>) {
        if !span.identifies_a_node() {
            return;
        }
        let key = (self.current_loop_storage_scope.clone(), span);
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
                let arm_view = (scalar && self.element_value_is_proven(arm, ty, env)).then(|| {
                    ElementValueView { ty: ty.clone(), local: Some(name.clone()) }
                });
                self.publish_element_value_view(arm.span, arm_view);
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
    ) -> Option<ElementValueView> {
        let scalar = is_projectable_scalar(ty);
        let local = match &value.kind {
            ExprKind::Variable(name) => Some(name.clone()),
            _ => None,
        };
        let projectable = if scalar {
            self.element_value_is_proven(value, ty, env)
        } else {
            // A guard proves a value is an array, never which layout the array has, so only a
            // loop binding whose source stores this very container qualifies.
            matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. })
                && local.as_ref().is_some_and(|name| {
                    self.element_local_read_is_stable(name, ty, env)
                        && self.proven_local_views.get(name) == Some(ty)
                })
        };
        projectable.then(|| ElementValueView { ty: ty.clone(), local })
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
                None => {
                    self.proven_local_views.remove(name);
                }
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
            self.proven_local_views.remove(&name);
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

    /// Ends the proofs of every local a loop can store, before its body is checked.
    ///
    /// The body is checked once against the entry facts, but a read at its top also runs after
    /// every store a previous iteration made, and such a store need not be a proven one.
    pub(crate) fn end_loop_stored_views<'n>(&mut self, stored: impl Iterator<Item = &'n str>) {
        for name in stored {
            self.proven_local_views.remove(name);
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
