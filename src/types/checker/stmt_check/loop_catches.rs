//! Purpose:
//! Selects loop catch bindings with conservative local-retirement upper bounds.
//!
//! Called from:
//! - Loop-entry stabilization before publishing backedge read views.
//!
//! Key details:
//! - Catch bindings enter the type fixed point only after their handlers become reachable.
//! - References, shared bindings, container writes, and opaque caller writes lose precision.
//! - Failed inference remains Mixed; syntactic scalar guesses cannot suppress cleanup throws.
//! - A node budget widens recursive type growth before another fixed-point round clones it.

use std::cell::RefCell;
use std::collections::HashSet;
use crate::parser::ast::{Expr, Stmt};
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};
use super::super::Checker;
use super::super::loop_storage::{AssignedValue, LoopWrites};

/// Grows reachable catch bindings until retirement and handler reachability agree.
pub(super) fn backedge_writes<'a>(
    checker: &mut Checker, span: Span, body: &'a [Stmt], update: Option<&'a Stmt>,
    condition: Option<&'a Expr>, entry: &TypeEnv, evidence: &LoopWrites<'a>,
) -> LoopWrites<'a> {
    if !evidence.has_catches {
        return super::super::loop_backedges::backedge_writes(body, update, condition,
            &|expr| checker.expr_is_declared_never_call(expr), &|_, _| Vec::new());
    }
    let mut reachable_bindings = HashSet::new();
    loop {
        let retirement = retirement_types(checker, entry, evidence, &reachable_bindings);
        let query = checker.exception_analysis.as_ref().map(|analysis| analysis.catch_query(retirement));
        let discovered = RefCell::new(HashSet::new());
        let writes = super::super::loop_backedges::backedge_writes(body, update, condition,
            &|expr| checker.expr_is_declared_never_call(expr),
            &|body, catches| {
                let reachable = query.as_ref().map_or_else(|| vec![true; catches.len()],
                    |analysis| analysis.catch_reachability(body, catches, checker.current_class.as_deref()));
                catches.iter().zip(reachable).map(|(catch, reachable)| reachable.then(|| {
                    // Terminal catches still own their binding before any subsequent cleanup.
                    if let Some(name) = &catch.variable { discovered.borrow_mut().insert(name.clone()); }
                    let classes = catch.exception_types.iter().map(|name|
                        checker.resolve_catch_type_name(name, span).unwrap_or_else(|_| name.to_string()))
                        .collect::<Vec<_>>();
                    PhpType::Object(checker.common_catch_type_name(&classes))
                })).collect()
            });
        let discovered = discovered.into_inner();
        if discovered.is_subset(&reachable_bindings) { return writes; }
        reachable_bindings.extend(discovered);
    }
}

/// Computes upper bounds for every value that a loop-local retirement can observe.
fn retirement_types(
    checker: &mut Checker, entry: &TypeEnv, evidence: &LoopWrites<'_>,
    reachable_bindings: &HashSet<String>,
) -> TypeEnv {
    let mut fixed = entry.iter().map(|(name, ty)| {
        let mut remaining = MAX_RETIREMENT_TYPE_NODES;
        let bound = if retirement_type_fits(ty, &mut remaining) { ty.clone() } else { PhpType::Mixed };
        (name.clone(), bound)
    }).collect::<TypeEnv>();
    for (name, _) in &evidence.values { fixed.entry((*name).to_string()).or_insert(PhpType::Void); }
    for name in &evidence.unbound { fixed.entry((*name).to_string()).or_insert(PhpType::Void); }
    let mut invalid = evidence.references.iter().chain(&evidence.containers)
        .chain(evidence.shared_bindings.iter()).map(|name| (*name).to_string()).collect::<HashSet<_>>();
    let mut invalidate_all = evidence.unbind_all;
    if let Some(analysis) = &checker.exception_analysis {
        match analysis.caller_writes(&evidence.calls, !checker.null_probe_scope_is_top_level) {
            Some(names) => invalid.extend(names),
            None => invalidate_all = true,
        }
    } else if !evidence.calls.is_empty() { invalidate_all = true; }
    invalid.extend(fixed.keys().filter(|name| !checker.local_binding_is_unaliased(name)).cloned());
    for name in invalid { fixed.insert(name, PhpType::Mixed); }
    if invalidate_all {
        for ty in fixed.values_mut() { *ty = PhpType::Mixed; }
        return fixed;
    }
    // The per-value node budget bounds memory before each join and subsequent clone.
    // The iteration cap remains a convergence backstop for the whole environment.
    for _ in 0..64 {
        let previous = fixed.clone();
        for (name, value) in &evidence.values {
            let incoming = match value {
                AssignedValue::Known(_) if !reachable_bindings.contains(*name) => continue,
                AssignedValue::Known(ty) => ty.clone(),
                AssignedValue::Opaque => PhpType::Mixed,
                AssignedValue::Increment(_) => PhpType::Union(vec![
                    PhpType::Int, PhpType::Float, PhpType::Str, PhpType::Bool, PhpType::Void]),
                AssignedValue::Expr(expr) => {
                    checker.member_receiver_analysis_scopes.push((
                        checker.current_function.clone(), checker.current_loop_storage_scope.clone()));
                    let inferred = checker.infer_type(expr, &fixed).unwrap_or(PhpType::Mixed);
                    let _ = checker.member_receiver_analysis_scopes.pop();
                    inferred
                }
            };
            let incoming = bounded_retirement_type(incoming);
            let old = fixed.get(*name).cloned().unwrap_or(PhpType::Void);
            let joined = checker.normalize_union_type(vec![old, incoming]);
            fixed.insert((*name).to_string(), bounded_retirement_type(joined));
        }
        if fixed == previous { return fixed; }
    }
    for ty in fixed.values_mut() { *ty = PhpType::Mixed; }
    fixed
}

/// Maximum structural nodes retained for one local's speculative retirement upper bound.
const MAX_RETIREMENT_TYPE_NODES: usize = 64;

/// Widens an oversized bound to the sound unknown value before it can grow recursively.
fn bounded_retirement_type(ty: PhpType) -> PhpType {
    let mut remaining = MAX_RETIREMENT_TYPE_NODES;
    if retirement_type_fits(&ty, &mut remaining) { ty } else { PhpType::Mixed }
}

/// Counts nested payloads with an early stop, bounding both traversal depth and node count.
fn retirement_type_fits(ty: &PhpType, remaining: &mut usize) -> bool {
    let Some(next) = remaining.checked_sub(1) else { return false; };
    *remaining = next;
    match ty {
        PhpType::Array(element) | PhpType::Buffer(element) => retirement_type_fits(element, remaining),
        PhpType::AssocArray { key, value } =>
            retirement_type_fits(key, remaining) && retirement_type_fits(value, remaining),
        PhpType::Union(members) => members.iter().all(|member| retirement_type_fits(member, remaining)),
        _ => true,
    }
}
