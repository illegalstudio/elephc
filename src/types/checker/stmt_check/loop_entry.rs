//! Purpose:
//! Stabilizes loop storage and refines local reads for every possible iteration.
//!
//! Called from:
//! - Statement control-flow checking before loop conditions and bodies.
//!
//! Key details:
//! - Physical contracts include all writes; read views include only continuing paths.
//! - Speculative expression inference restores source-ordered guard and capture facts.

use std::collections::HashSet;
use crate::parser::ast::{Expr, ExprKind, Stmt};
use crate::types::{PhpType, TypeEnv};
use super::super::Checker;

/// Computes and records fixed-point array storage contracts before checking a loop body.
///
/// The shared analysis iterates over rebinds and growth sites with an evolving environment, so
/// cascading promotions, non-literal RHSs, and raw-to-raw element changes converge before any
/// header/body read is checked. EIR lowering later consumes the recorded contract for the same
/// loop span rather than repeating expression inference.
pub(super) fn stabilize_loop_storage(
    checker: &mut Checker,
    loop_span: crate::span::Span,
    body: &[Stmt],
    update: Option<&Stmt>,
    condition: Option<&Expr>,
    env: &mut TypeEnv,
    packed_counter: Option<&str>,
) {
    let key = (checker.current_loop_storage_scope.clone(), loop_span);
    let snapshot = env.clone();
    let mut evidence = super::super::loop_storage::LoopWrites::default();
    for stmt in body { super::super::loop_storage::collect_value_assignment_stmt(stmt, &mut evidence); }
    if let Some(update) = update { super::super::loop_storage::collect_value_assignment_stmt(update, &mut evidence); }
    if let Some(condition) = condition { super::super::loop_storage::collect_value_assignments_from_expr(condition, &mut evidence); }
    let incremented: HashSet<_> = evidence.values.iter().filter_map(|(name, value)| {
        matches!(value, super::super::loop_storage::AssignedValue::Increment(_)).then_some(*name)
    }).collect();
    // A coalesced read may be Int while its physical parameter slot is still tagged nullable.
    let tagged_integer_locals: HashSet<_> = snapshot.iter().filter_map(|(name, ty)| {
        // Incoming and explicitly aliased cells keep their external representation. Their
        // checked updates diagnose overflow instead of replacing the cell with a float box.
        if checker.has_fixed_tagged_reference_storage(name, &snapshot) {
            return None;
        }
        let physical = checker.guarded_local_storage_type(&Expr::var(name), &snapshot).unwrap_or(ty);
        (physical.codegen_repr() == PhpType::TaggedScalar
            && (physical != ty || incremented.contains(name.as_str())))
            .then(|| name.clone())
    }).collect();
    let mut call_types: std::collections::HashMap<crate::span::Span, PhpType> =
        std::collections::HashMap::new();
    let entry_storage = checker.guarded_union_types.clone();
    let entry_origins = checker.narrowed_local_origins.clone();
    let entry_captures = checker.by_ref_captured_locals.clone();
    let entry_pending_captures = checker.pending_ref_capture_invalidations.clone();
    let narrowed: HashSet<_> = snapshot.iter().filter_map(|(name, ty)| {
        checker.guarded_local_storage_type(&Expr::var(name), &snapshot)
            .filter(|physical| *physical != ty).map(|_| name.clone())
    }).collect();
    let backedges = if narrowed.is_empty() { super::super::loop_storage::LoopWrites::default() }
        else { super::super::loop_backedges::backedge_writes(body, update, condition,
            &|expr| checker.expr_is_declared_never_call(expr)) };
    let mut infer = |expr: &Expr, analysis_env: &TypeEnv| {
        let is_call = matches!(
            expr.kind,
            ExprKind::FunctionCall { .. }
                | ExprKind::MethodCall { .. }
                | ExprKind::StaticMethodCall { .. }
                | ExprKind::ClosureCall { .. }
                | ExprKind::ExprCall { .. }
        );
        // The memo is keyed by span, so it may only answer for a span that names one call.
        // Under `Span::dummy()` every method / static / closure call in a prelude loop body
        // shared a single entry and the FIRST call's inferred type was handed to all of
        // them ; 42 times while checking one `new PDO("sqlite::memory:")` program.
        let memoizable = is_call && expr.span.identifies_a_node();
        if memoizable {
            if let Some(cached) = call_types.get(&expr.span) {
                return Some(cached.clone());
            }
        }
        // This body and its lexical closures see an unguarded environment. Independently
        // checked callees still need their facts, including a user function named `main`.
        checker.member_receiver_analysis_scopes.push((
            checker.current_function.clone(), checker.current_loop_storage_scope.clone(),
        ));
        let inferred = checker.infer_type(expr, analysis_env);
        let _ = checker.member_receiver_analysis_scopes.pop();
        let inferred = inferred.ok()?;
        if memoizable {
            call_types.insert(expr.span, inferred.clone());
        }
        Some(inferred)
    };
    let analysis = super::super::loop_storage::analyze_loop_storage(body, update, condition,
        &snapshot, packed_counter, &tagged_integer_locals, &mut infer);
    let mut incoming = Vec::new();
    for name in &backedges.references {
        if !narrowed.contains(*name) { continue; }
        let storage = entry_storage.get(*name).map(|entry| &entry.ty)
            .or_else(|| entry_origins.get(*name).map(|entry| &entry.origin));
        if let Some(storage) = storage { incoming.push(((*name).to_string(), storage.clone())); }
    }
    for (name, value) in &backedges.values {
        if !narrowed.contains(*name) { continue; }
        use super::super::loop_storage::AssignedValue;
        let ty = match value {
            AssignedValue::Expr(expr) => infer(expr, &analysis.fixed).unwrap_or(PhpType::Mixed),
            AssignedValue::Known(ty) => ty.clone(),
            AssignedValue::Opaque => PhpType::Mixed,
            AssignedValue::Increment(_) => analysis.fixed.get(*name).cloned().unwrap_or(PhpType::Mixed),
        };
        incoming.push(((*name).to_string(), ty));
    }
    for call in &backedges.calls { let _ = infer(call, &analysis.fixed); }
    for call in &backedges.calls {
        if let Some(outputs) = checker.boxed_reference_outputs.get(&(
            checker.current_loop_storage_scope.clone(), call.span)) {
            incoming.extend(outputs.iter().filter(|(name, _)| narrowed.contains(*name))
                .map(|(name, ty)| (name.clone(), ty.clone())));
        }
    }
    // Speculative RHS/call inference must not make a later capture precede earlier updates.
    checker.guarded_union_types = entry_storage;
    checker.narrowed_local_origins = entry_origins;
    checker.by_ref_captured_locals = entry_captures;
    checker.pending_ref_capture_invalidations = entry_pending_captures;
    let contracts = analysis.contracts;
    // An update-only binding can be read by a later iteration before this source position.
    // Preserve the existing for-header visibility without applying its writes out of order.
    if let Some(update) = update {
        let mut writes = super::super::loop_storage::LoopWrites::default();
        super::super::loop_storage::collect_value_assignment_stmt(update, &mut writes);
        for (name, _) in writes.values {
            if !env.contains_key(name) {
                if let Some(ty) = analysis.fixed.get(name) { env.insert(name.to_string(), ty.clone()); }
            }
        }
    }
    for (name, ty) in incoming {
        let Some(current) = env.get(&name) else { continue; };
        let joined = checker.join_flow_views(current, &ty);
        if &joined != current {
            checker.record_local_storage_effect(&name, &joined);
            env.insert(name, joined);
        }
    }
    let (scope, span) = key;
    if !checker.loop_storage_types.contains_key(&(scope.clone(), span)) {
        checker
            .loop_storage_scope_spans
            .entry(scope.clone())
            .or_default()
            .push(span);
    }
    let recorded = checker.loop_storage_types.entry((scope, span)).or_default();
    for (name, storage_type) in contracts {
        recorded.insert(name.clone(), storage_type.clone());
        env.insert(name, storage_type);
    }
}

