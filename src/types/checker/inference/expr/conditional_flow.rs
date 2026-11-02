//! Purpose:
//! Isolates guarded local storage while checking conditional alternatives.
//!
//! Called from:
//! - Assignment-effect inference, short-circuit guards, and statement flow joins.
//!
//! Key details:
//! - Arms share entry contracts; guarded and boxable local exits are joined afterwards.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::parser::ast::Expr;
use crate::types::{PhpType, TypeEnv};

use super::super::super::{Checker, GuardedLocalStorage, NarrowedLocalOrigin};

/// Entry contracts and accumulated exit views for one conditional construct.
pub(in crate::types::checker) struct ConditionalFlow {
    all_locals: bool,
    bound_entry: HashSet<String>,
    bound_joined: Option<HashSet<String>>,
    storage: HashMap<String, GuardedLocalStorage>,
    origins: HashMap<String, NarrowedLocalOrigin>,
    views: TypeEnv,
    joined: TypeEnv,
    physical: TypeEnv,
    changed: HashSet<String>,
    /// Guard-region stores observed on any alternative, including terminal paths.
    stored: HashSet<String>,
}

/// Logical reads and physical bindings at one control-flow edge.
#[derive(Clone)]
pub(in crate::types::checker) struct FlowSnapshot {
    pub views: TypeEnv,
    pub bound: HashSet<String>,
    storage: HashMap<String, GuardedLocalStorage>,
    origins: HashMap<String, NarrowedLocalOrigin>,
}

impl Checker {
    /// Saves guarded reads and boxed locals that a fresh guard may narrow before a write.
    pub(in crate::types::checker) fn snapshot_conditional_flow(&self, env: &TypeEnv) -> ConditionalFlow {
        self.snapshot_conditional_flow_inputs(env, false)
    }

    /// Tracks every visible local when alternatives replace a complete environment.
    pub(in crate::types::checker) fn snapshot_all_locals_flow(&self, env: &TypeEnv) -> ConditionalFlow {
        self.snapshot_conditional_flow_inputs(env, true)
    }

    /// Selects a construct's read inputs without changing their physical provenance.
    fn snapshot_conditional_flow_inputs(&self, env: &TypeEnv, all_locals: bool) -> ConditionalFlow {
        let views: TypeEnv = self.guarded_union_types.keys()
            .chain(self.narrowed_local_origins.iter().filter_map(|(name, origin)|
                origin.view.as_ref().filter(|_| matches!(origin.origin.codegen_repr(),
                    PhpType::Mixed | PhpType::TaggedScalar)).map(|_| name)))
            .chain(env.iter().filter(|(name, ty)| !name.starts_with('\u{1}')
                && (all_locals || matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::TaggedScalar)))
                .map(|(name, _)| name))
            .filter_map(|name| env.get(name).map(|ty| (name.clone(), ty.clone())))
            .collect();
        let physical = views.iter().map(|(name, view)| {
            let ty = self.guarded_local_storage_type(&Expr::var(name), env).unwrap_or(view);
            (name.clone(), ty.clone())
        }).collect();
        ConditionalFlow {
            all_locals, bound_entry: self.definitely_bound_locals.clone(),
            bound_joined: Some(self.definitely_bound_locals.clone()),
            storage: self.guarded_union_types.clone(),
            origins: self.narrowed_local_origins.clone(),
            joined: views.clone(), views, physical, changed: HashSet::new(), stored: HashSet::new(),
        }
    }

    /// Saves an edge before a branch restores its temporary read views.
    pub(in crate::types::checker) fn snapshot_flow_exit(&self, env: &TypeEnv) -> FlowSnapshot {
        let mut edge = FlowSnapshot { views: env.clone(), bound: self.definitely_bound_locals.clone(), storage: self.guarded_union_types.clone(),
            origins: self.narrowed_local_origins.clone() };
        for (name, origin) in &mut edge.origins {
            let Some(view) = edge.views.get_mut(name) else { continue; };
            if !origin.stored_in_region && origin.view.as_ref() == Some(&*view)
                && !self.type_contains_values(&origin.read_input, view)
            {
                *view = origin.read_input.clone();
                // Keep the physical origin behind this contained snapshot read.
                origin.view = Some(view.clone());
            }
        }
        edge
    }

    /// Restores one handler's independent incoming state before checking its body.
    pub(in crate::types::checker) fn restore_flow_snapshot(&mut self, edge: &FlowSnapshot, env: &mut TypeEnv) {
        self.guarded_union_types = edge.storage.clone();
        self.narrowed_local_origins = edge.origins.clone();
        self.definitely_bound_locals = edge.bound.clone();
        *env = edge.views.clone();
    }

    /// Resolves an edge's physical binding independently of its guarded read type.
    fn snapshot_storage_type<'a>(&self, edge: &'a FlowSnapshot, name: &str) -> Option<&'a PhpType> {
        let view = edge.views.get(name)?;
        Some(edge.storage.get(name).filter(|entry|
            self.merged_assignment_type(&entry.ty, view).as_ref() == Some(&entry.ty))
            .map(|entry| &entry.ty).or_else(|| edge.origins.get(name)
                .filter(|entry| entry.view.as_ref() == Some(view)).map(|entry| &entry.origin))
            .unwrap_or(view))
    }

    /// Joins continuing edges while retaining the physical storage behind each read view.
    pub(in crate::types::checker) fn join_flow_snapshots(&self, edges: &[FlowSnapshot]) -> Option<FlowSnapshot> {
        let mut joined = edges.first()?.clone();
        for edge in &edges[1..] {
            joined.bound.retain(|name| edge.bound.contains(name));
            for (name, view) in &edge.views {
                let next_storage = self.snapshot_storage_type(edge, name).expect("edge local");
                let physical = self.snapshot_storage_type(&joined, name).map_or_else(
                    || next_storage.clone(), |previous| self.join_flow_views(previous, next_storage));
                let read = joined.views.get(name).map_or_else(|| view.clone(),
                    |previous| self.join_flow_views(previous, view));
                if physical != read || matches!(physical.codegen_repr(), PhpType::Mixed | PhpType::TaggedScalar) {
                    joined.storage.insert(name.clone(), GuardedLocalStorage { ty: physical, coalesced: false });
                } else { joined.storage.remove(name); }
                joined.views.insert(name.clone(), read);
                joined.origins.remove(name);
            }
        }
        // A single edge can carry its physical type only in a guard origin.
        for (name, view) in &joined.views {
            let storage = self.snapshot_storage_type(&joined, name).unwrap_or(view).clone();
            if storage != *view {
                joined.storage.insert(name.clone(), GuardedLocalStorage { ty: storage, coalesced: false });
            }
        }
        Some(joined)
    }

    /// Restores tail storage without reopening guard-origin scopes that ended in the body.
    pub(in crate::types::checker) fn restore_tail_flow_storage(&mut self, edge: &FlowSnapshot) {
        self.definitely_bound_locals = edge.bound.clone();
        self.guarded_union_types = edge.storage.clone();
        for name in edge.views.keys() {
            if self.narrowed_local_origins.get(name).is_some_and(|origin|
                origin.view.as_ref() != edge.views.get(name)) {
                self.record_store_over_flow_narrowing(name);
            }
        }
    }

    /// Tracks every local on loop exits, excluding property facts that iterations invalidate.
    pub(in crate::types::checker) fn snapshot_loop_flow(&self, env: &TypeEnv) -> ConditionalFlow {
        let mut locals = env.clone();
        Self::purge_property_narrowings(&mut locals);
        self.snapshot_conditional_flow_inputs(&locals, true)
    }

    /// Restores an alternative's entry contracts without discarding other checker metadata.
    fn restore_conditional_flow(&mut self, flow: &ConditionalFlow) {
        self.definitely_bound_locals = flow.bound_entry.clone();
        self.guarded_union_types = flow.storage.clone();
        self.narrowed_local_origins = flow.origins.clone();
        self.restore_introduced_storage(flow, flow);
    }

    /// Keeps newly introduced boxed frame storage independent of a sibling's read view.
    fn restore_introduced_storage(&mut self, flow: &ConditionalFlow, entry: &ConditionalFlow) {
        for (name, ty) in &flow.physical {
            if !entry.bound_entry.contains(name) && !entry.storage.contains_key(name)
                && Self::binding_storage_admits_null(ty)
            {
                self.guarded_union_types.insert(name.clone(), GuardedLocalStorage {
                    ty: ty.clone(), coalesced: false,
                });
            }
        }
    }

    /// Restores the unguarded read inputs before checking a sibling conditional alternative.
    pub(in crate::types::checker) fn restore_conditional_read_inputs(&self, flow: &ConditionalFlow, env: &mut TypeEnv) {
        for (name, view) in &flow.views { env.insert(name.clone(), view.clone()); }
    }

    /// Restores the evaluated condition prefix while retaining newly introduced frame storage.
    pub(in crate::types::checker) fn restore_statement_flow_storage(
        &mut self, flow: &ConditionalFlow, entry: &ConditionalFlow,
    ) {
        self.guarded_union_types = entry.storage.clone();
        self.restore_introduced_storage(flow, entry);
    }

    /// Keeps accepted sibling writes available to assignment checks without changing entry reads.
    pub(in crate::types::checker) fn restore_accumulated_assignment_storage(
        &mut self, flow: &ConditionalFlow, env: &TypeEnv, rebound: &HashSet<String>,
    ) {
        for name in &flow.changed {
            if rebound.contains(name) { continue; }
            let storage = &flow.physical[name];
            if matches!(storage.codegen_repr(), PhpType::Mixed | PhpType::TaggedScalar)
                && env.get(name).is_some_and(|view|
                    self.merged_assignment_type(storage, view).as_ref() == Some(storage))
            {
                self.guarded_union_types.insert(name.clone(), GuardedLocalStorage {
                    ty: storage.clone(), coalesced: false,
                });
            }
        }
    }

    /// Restores one alternative's storage entry without touching sibling guard facts.
    fn restore_conditional_local_storage(&mut self, flow: &ConditionalFlow, name: &str) {
        if let Some(storage) = flow.storage.get(name) {
            self.guarded_union_types.insert(name.to_string(), storage.clone());
        } else {
            self.guarded_union_types.remove(name);
        }
    }

    /// Restores presence facts after a diagnostic-only branch walk.
    pub(in crate::types::checker) fn restore_conditional_bindings(&mut self, flow: &ConditionalFlow) {
        self.definitely_bound_locals = flow.bound_entry.clone();
    }

    /// Starts an exhaustive set of binding alternatives without a skipped entry path.
    pub(in crate::types::checker) fn exclude_conditional_binding_entry(&self, flow: &mut ConditionalFlow) {
        flow.bound_joined = None;
    }

    /// Includes an implicit skip path in a statement branch binding join.
    pub(in crate::types::checker) fn observe_conditional_binding_exit(&self, flow: &mut ConditionalFlow) {
        if let Some(bound) = &mut flow.bound_joined {
            bound.retain(|name| self.definitely_bound_locals.contains(name));
        } else { flow.bound_joined = Some(self.definitely_bound_locals.clone()); }
    }

    /// Checks one alternative and remembers its writes before restoring sibling-arm contracts.
    pub(in crate::types::checker) fn with_conditional_flow_arm<T>(
        &mut self, flow: &mut ConditionalFlow, env: &mut TypeEnv,
        body: impl FnOnce(&mut Self, &mut TypeEnv) -> Result<T, CompileError>,
    ) -> Result<T, CompileError> {
        self.restore_conditional_flow(flow);
        let result = body(self, env);
        if result.is_ok() {
            self.observe_conditional_flow_exit(flow, env);
        }
        self.restore_conditional_flow(flow);
        result
    }

    /// Accumulates writes before a construct restores its temporary guard read views.
    pub(in crate::types::checker) fn observe_conditional_flow_exit(&self, flow: &mut ConditionalFlow, env: &TypeEnv) {
        self.observe_flow_snapshot(flow, &self.snapshot_flow_exit(env), true);
    }

    /// Keeps storage effects from a terminal branch without publishing its unreachable reads.
    pub(in crate::types::checker) fn observe_terminal_flow_storage(&self, flow: &mut ConditionalFlow, env: &TypeEnv) {
        self.observe_flow_snapshot(flow, &self.snapshot_flow_exit(env), false);
    }

    /// Joins an edge's logical values separately from its retained physical representation.
    pub(in crate::types::checker) fn observe_flow_snapshot(&self, flow: &mut ConditionalFlow, edge: &FlowSnapshot, reads: bool) {
        flow.stored.extend(edge.origins.iter().filter(|(_, origin)| origin.stored_in_region)
            .map(|(name, _)| name.clone()));
        flow.stored.extend(flow.origins.keys().filter(|name| !edge.origins.contains_key(*name)).cloned());
        if reads {
            if let Some(bound) = &mut flow.bound_joined { bound.retain(|name| edge.bound.contains(name)); }
            else { flow.bound_joined = Some(edge.bound.clone()); }
        }
        for (name, view) in &edge.views {
            if name.starts_with('\u{1}') || flow.views.contains_key(name) { continue; }
            let physical = self.snapshot_storage_type(edge, name).unwrap_or(view);
            if flow.all_locals || Self::binding_storage_admits_null(physical) {
                flow.views.insert(name.clone(), view.clone());
                flow.physical.insert(name.clone(), physical.clone());
            }
        }
        for (name, entry_view) in &flow.views {
            let storage_changed = match (flow.storage.get(name), edge.storage.get(name)) {
                (Some(before), Some(after)) => before.ty != after.ty || before.coalesced != after.coalesced,
                // A fresh guard records physical storage without writing the variable.
                (None, Some(after)) => after.coalesced,
                (Some(_), None) => true,
                (None, None) => false,
            };
            let origin_changed = match (flow.origins.get(name), edge.origins.get(name)) {
                (Some(before), Some(after)) => before.view != after.view
                    || before.stored_in_region != after.stored_in_region,
                (None, Some(after)) => after.stored_in_region,
                (Some(_), None) => true,
                (None, None) => false,
            };
            let Some(exit_view) = edge.views.get(name) else { continue; };
            // Snapshot reads are contained before joins or closed scopes lose their origins.
            let exit_storage = edge.storage.get(name).filter(|storage|
                self.merged_assignment_type(&storage.ty, exit_view).as_ref() == Some(&storage.ty))
                .map(|storage| &storage.ty)
                .or_else(|| edge.origins.get(name).filter(|origin|
                    origin.view.as_ref() == Some(exit_view)).map(|origin| &origin.origin))
                .unwrap_or(exit_view);
            let previous_storage = &flow.physical[name];
            let storage_join = if self.merged_assignment_type(previous_storage, exit_storage).as_ref() == Some(previous_storage) {
                previous_storage.clone()
            } else {
                self.join_flow_views(previous_storage, exit_storage)
            };
            if &storage_join != previous_storage { flow.changed.insert(name.clone()); }
            flow.physical.insert(name.clone(), storage_join);
            let previous = flow.joined.get(name);
            let joined = if reads { previous.map_or_else(|| exit_view.clone(), |previous|
                self.join_flow_views(previous, exit_view)) }
                else { previous.unwrap_or(entry_view).clone() };
            if &joined != entry_view || storage_changed || origin_changed {
                flow.changed.insert(name.clone());
            }
            if reads || previous.is_some() { flow.joined.insert(name.clone(), joined); }
        }
    }

    /// Joins PHP value alternatives while retaining an existing object supertype.
    pub(in crate::types::checker) fn join_flow_views(&self, left: &PhpType, right: &PhpType) -> PhpType {
        if left == right || (matches!((left, right), (PhpType::Object(_), PhpType::Object(_)))
            && self.type_accepts(left, right)) { left.clone() }
        else if matches!((left, right), (PhpType::Object(_), PhpType::Object(_)))
            && self.type_accepts(right, left) { right.clone() }
        // A skipped loop retains its empty literal, which can use the populated hash's
        // representation at the return boundary instead of inventing a packed/hash union.
        else if matches!(left, PhpType::Array(element) if **element == PhpType::Never)
            && matches!(right, PhpType::AssocArray { .. }) { right.clone() }
        else if matches!(right, PhpType::Array(element) if **element == PhpType::Never)
            && matches!(left, PhpType::AssocArray { .. }) { left.clone() }
        // A general PHP-array contract absorbs a more precise container without losing
        // the array family when the value itself remains boxed.
        else if (left.is_php_array() && matches!(right, PhpType::Array(_) | PhpType::AssocArray { .. }))
            || (right.is_php_array() && matches!(left, PhpType::Array(_) | PhpType::AssocArray { .. }))
        { PhpType::php_array() }
        else if let (PhpType::AssocArray { key: left_key, value: left_value },
            PhpType::AssocArray { key: right_key, value: right_value }) = (left, right)
        {
            let payload = if left_value.is_php_array() || right_value.is_php_array() {
                self.join_flow_views(left_value, right_value)
            } else {
                PhpType::widen_array_branch_element((**left_value).clone(), (**right_value).clone())
            };
            PhpType::AssocArray {
                key: Box::new(PhpType::widen_array_branch_element((**left_key).clone(), (**right_key).clone())),
                value: Box::new(payload),
            }
        }
        else if let Some(array) = super::merge_array_branch_types(left, right) { array }
        else { self.normalize_union_type(vec![left.clone(), right.clone()]) }
    }

    /// Conservatively carries finally writes to a jump whose guarded view was already saved.
    pub(in crate::types::checker) fn join_finally_flow_snapshot(&self, edge: &mut FlowSnapshot, after: &FlowSnapshot) {
        edge.bound = after.bound.clone();
        for (name, ty) in &after.views {
            if let Some(before) = edge.views.get(name).cloned() {
                let read = self.join_flow_views(&before, ty);
                let old_storage = edge.storage.get(name).map(|entry| &entry.ty).unwrap_or(&before);
                let new_storage = after.storage.get(name).map(|entry| &entry.ty).unwrap_or(ty);
                let storage = self.join_flow_views(old_storage, new_storage);
                // Physical joins do not recreate a raw proof that the finally store killed.
                if storage != read || !Self::is_raw_non_null_contract(&storage)
                    || after.storage.get(name).is_some_and(|entry| entry.ty == storage)
                {
                    edge.storage.insert(name.clone(), GuardedLocalStorage { ty: storage, coalesced: false });
                } else { edge.storage.remove(name); }
                edge.views.insert(name.clone(), read);
                edge.origins.remove(name);
            }
        }
    }

    /// Keeps terminal finally inputs out of continuing reads without narrowing physical storage.
    pub(in crate::types::checker) fn restore_finally_continuation_reads(
        &mut self, normal: &FlowSnapshot, body: &[crate::parser::ast::Stmt], env: &mut TypeEnv,
    ) {
        use crate::parser::ast::{ExprKind, StmtKind};
        use super::super::super::loop_storage::{collect_value_assignment_stmt, AssignedValue, LoopWrites};
        let mut evidence = LoopWrites::default();
        for stmt in body { collect_value_assignment_stmt(stmt, &mut evidence); }
        let mut blocked: HashSet<String> = evidence.references.iter().map(|name| (*name).to_string()).collect();
        let containers: HashSet<String> = evidence.containers.iter().filter(|name| normal.views.get(**name)
            .is_some_and(|ty| matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. })
                || ty.is_php_array() || matches!(ty, PhpType::Union(members)
                    if members.iter().any(|member| matches!(member,
                        PhpType::Array(_) | PhpType::AssocArray { .. })))))
            .map(|name| (*name).to_string()).collect();
        let mut writes: HashMap<&str, Vec<&AssignedValue<'_>>> = HashMap::new();
        for (name, value) in &evidence.values { writes.entry(name).or_default().push(value); }
        // Literal output and discarded literals cannot re-enter PHP to mutate an exposed cell.
        let literal_observations = body.iter().all(|stmt| matches!(&stmt.kind,
            StmtKind::Echo(expr) | StmtKind::ExprStmt(expr) if matches!(expr.kind,
                ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) | ExprKind::BoolLiteral(_)
                | ExprKind::StringLiteral(_) | ExprKind::Null)));
        for call in evidence.calls {
            if let ExprKind::FunctionCall { name, args } = &call.kind {
                // These calls can create arbitrary caller bindings without an argument lvalue.
                if matches!(crate::names::php_symbol_key(name.as_str()).as_str(), "eval" | "extract") { return; }
                if super::super::super::is_unset_call(name.as_str()) {
                    for arg in args {
                        let mut arg = arg;
                        while let ExprKind::NamedArg { value, .. }
                            | ExprKind::ErrorSuppress(value) = &arg.kind { arg = value; }
                        if let ExprKind::Variable(name) = &arg.kind { blocked.insert(name.clone()); }
                    }
                }
            }
            if let Some(outputs) = self.boxed_reference_outputs.get(&(self.current_loop_storage_scope.clone(), call.span)) {
                blocked.extend(outputs.keys().cloned());
            }
        }
        // Only a normal-entry guard can establish this array fact. Payload-only writes
        // preserve non-nullness, but the element view must come from the updated container.
        let array_continuations: Vec<_> = containers.iter().filter(|name|
            !blocked.contains(*name) && !writes.contains_key(name.as_str())
                && self.local_flow_read_is_stable(name)
                && normal.views.get(*name).is_some_and(|ty|
                    matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) || ty.is_php_array()))
            .filter_map(|name| {
                let PhpType::Union(members) = env.get(name)? else { return None; };
                if !members.contains(&PhpType::Void) { return None; }
                let view = self.normalize_union_type(members.iter()
                    .filter(|member| **member != PhpType::Void).cloned().collect());
                (matches!(view, PhpType::Array(_) | PhpType::AssocArray { .. }) || view.is_php_array())
                    .then(|| (name.clone(), view))
            }).collect();
        blocked.extend(containers);
        let mut preserved: HashSet<String> = normal.views.keys().filter(|name|
            !name.starts_with('\u{1}') && !blocked.contains(*name) && env.contains_key(*name)
                && (literal_observations || self.local_flow_read_is_stable(name)))
            .cloned().collect();
        // Starting with the normal-entry views proves an invariant for compatible stores and
        // copies. Removing a source fact also removes every dependent copy in the next round.
        loop {
            let invalid: Vec<_> = preserved.iter().filter(|name| {
                let view = &normal.views[*name];
                writes.get(name.as_str()).is_some_and(|values| values.iter().any(|value| {
                    let ty = match value {
                        AssignedValue::Expr(expr) => {
                            let observed = self.finally_assigned_value_types.get(&(
                                self.current_loop_storage_scope.clone(), *expr as *const Expr as usize))
                                .filter(|(span, _)| *span == expr.span);
                            // Only ordinary stores contribute observations. A typed declaration
                            // or catch/foreach rebind must not revive the previous binding.
                            let Some((_, observed)) = observed else { return true; };
                            if let ExprKind::Variable(source) = &expr.kind {
                                if preserved.contains(source) { return self.join_flow_views(view, &normal.views[source]) != *view; }
                            }
                            Some(observed)
                        }
                        AssignedValue::Known(_) | AssignedValue::Opaque | AssignedValue::Increment(_) => None,
                    };
                    ty.is_none_or(|ty| self.join_flow_views(view, ty) != *view)
                }))
            }).cloned().collect();
            if invalid.is_empty() { break; }
            for name in invalid { preserved.remove(&name); }
        }
        for (name, view) in &normal.views {
            if !preserved.contains(name) { continue; }
            if normal.bound.contains(name) { self.definitely_bound_locals.insert(name.clone()); }
            let Some(current) = env.get(name) else { continue; };
            // A guard inside finally may establish a stronger continuing view of its own.
            if view == current || self.join_flow_views(view, current) != *current { continue; }
            let physical = self.guarded_local_storage_type(&Expr::var(name), env).unwrap_or(current);
            let storage = self.join_flow_views(self.snapshot_storage_type(normal, name).unwrap_or(view), physical);
            env.insert(name.clone(), view.clone());
            if storage != *view || matches!(storage.codegen_repr(), PhpType::Mixed | PhpType::TaggedScalar) {
                let coalesced = normal.storage.get(name).is_some_and(|entry| entry.coalesced);
                self.guarded_union_types.insert(name.clone(), GuardedLocalStorage { ty: storage, coalesced });
            } else { self.guarded_union_types.remove(name); }
            if let Some(origin) = normal.origins.get(name) { self.narrowed_local_origins.insert(name.clone(), origin.clone()); }
            else { self.narrowed_local_origins.remove(name); }
        }
        for (name, view) in array_continuations {
            let Some(current) = env.get(&name) else { continue; };
            let storage = self.guarded_local_storage_type(&Expr::var(&name), env)
                .unwrap_or(current).clone();
            self.record_store_over_flow_narrowing(&name);
            env.insert(name.clone(), view);
            self.guarded_union_types.insert(name, GuardedLocalStorage { ty: storage, coalesced: false });
        }
    }

    /// Publishes possible writes after all alternatives have been checked independently.
    pub(in crate::types::checker) fn finish_conditional_flow(&mut self, flow: ConditionalFlow, env: &mut TypeEnv) {
        self.restore_conditional_flow(&flow);
        self.publish_conditional_flow(flow, env);
    }

    /// Publishes statement exits after their guard-origin scopes have been closed.
    pub(in crate::types::checker) fn finish_statement_conditional_flow(&mut self, flow: ConditionalFlow, env: &mut TypeEnv) {
        // Untouched locals may now have a valid kept complement from a terminal guard.
        for name in &flow.changed {
            self.restore_conditional_local_storage(&flow, name);
        }
        self.publish_conditional_flow(flow, env);
    }

    /// Applies the only reachable complement after all guarded clauses leave the construct.
    pub(in crate::types::checker) fn keep_conditional_views(&self, flow: &mut ConditionalFlow, env: &TypeEnv, names: impl Iterator<Item = String>) {
        for name in names {
            if let Some(view) = env.get(&name) { flow.joined.insert(name, view.clone()); }
        }
    }

    /// Removes the logical zero-iteration exit while retaining entry storage requirements.
    pub(in crate::types::checker) fn exclude_target_entry_exit(&self, flow: &mut ConditionalFlow) {
        flow.bound_joined = None;
        flow.joined.clear();
    }

    /// Joins loop entry, normal back edges, and recorded control exits before leaving the loop.
    pub(in crate::types::checker) fn finish_target_flow(&mut self, flow: ConditionalFlow, env: &mut TypeEnv, is_loop: bool) {
        let reads = flow.joined.clone();
        self.finish_statement_conditional_flow(flow, env);
        for (name, ty) in reads { env.insert(name, ty); }
        if is_loop { Self::purge_property_narrowings(env); }
    }

    /// Drops old binding provenance while retaining reads defined on every reaching exit.
    pub(in crate::types::checker) fn forget_conditional_bindings(
        &self, flow: &mut ConditionalFlow, names: &HashSet<String>, bound_reads: &TypeEnv,
    ) {
        for name in names {
            flow.views.remove(name);
            if let Some(read) = bound_reads.get(name) { flow.joined.insert(name.clone(), read.clone()); }
            else { flow.joined.remove(name); }
            flow.physical.remove(name);
            flow.changed.remove(name);
            flow.storage.remove(name);
            flow.origins.remove(name);
            flow.stored.remove(name);
        }
    }

    /// Updates tracked reads and invalidates contracts that no longer cover all possible writes.
    fn publish_conditional_flow(&mut self, flow: ConditionalFlow, env: &mut TypeEnv) {
        self.definitely_bound_locals = flow.bound_joined.unwrap_or(flow.bound_entry);
        // Raw entries are value proofs, so an optional arm cannot publish a new one.
        // Include keys absent from flow.views: their unchanged Int read hides the write.
        self.guarded_union_types.retain(|name, storage| {
            !Self::is_raw_non_null_contract(&storage.ty)
                || (!flow.changed.contains(name)
                    && flow.storage.get(name).is_some_and(|entry| entry.ty == storage.ty))
        });
        for name in flow.changed {
            let ty = flow.joined.get(&name).unwrap_or(&flow.views[&name]).clone();
            let storage = flow.physical[&name].clone();
            // A read-only join can change a guard view without rebinding its value.
            // Preserve that provenance only when no alternative recorded an actual store.
            let guard = self.narrowed_local_origins.get(&name).filter(|origin|
                !flow.stored.contains(&name) && !origin.stored_in_region
                    && origin.view.is_some() && origin.origin == storage).cloned();
            self.record_local_storage_effect(&name, &storage);
            if let Some(mut origin) = guard {
                origin.view = Some(ty.clone());
                self.narrowed_local_origins.insert(name.clone(), origin);
            }
            if storage != ty && !Self::narrowed_place_key_is_property(&name)
                && Self::binding_storage_admits_null(&storage)
            {
                let coalesced = flow.storage.get(&name).is_some_and(|entry|
                    entry.coalesced && entry.ty == storage);
                self.guarded_union_types.insert(name.clone(), GuardedLocalStorage { ty: storage, coalesced });
            }
            env.insert(name, ty);
        }
        self.publish_binding_read_views(env);
    }
}
