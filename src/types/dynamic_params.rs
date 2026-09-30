//! Purpose:
//! Decides whether an un-hinted declaration hands one of its untyped parameters straight back.
//!
//! Called from:
//! - `Checker::widen_dynamic_only_passthrough_returns` in
//!   `crate::types::checker::driver::functions` when it records `mixed` for a function no
//!   direct call site ever passed arguments to.
//! - `crate::ir_lower::program::metadata` when it normalizes method ABIs for EIR.
//! - `crate::ir_lower::expr::eir_user_function_return_type`, through `caller_visible_return_type`,
//!   and `Checker::caller_visible_direct_call_return` in
//!   `crate::types::checker::callables::closures`, through the predicates it is built from and
//!   the union-preserving `dynamic_param_container_php_type`.
//!
//! Key details:
//! - An untyped parameter starts as the checker's `Int` PLACEHOLDER, which direct call sites
//!   specialize away. A parameter that is never specialized keeps it, and a `return $param;`
//!   inferred from the placeholder records `Int` for a value that is really whatever the caller
//!   passed — so a returned string came back as `int(0)` through the runtime-callable invoker,
//!   with no diagnostic (issue #576).
//! - The rule lives here, not in either caller, because BOTH the declaration's own return type
//!   and every call site's view of it have to reach the same answer. When they disagreed, the
//!   callee returned a boxed cell that the caller read as a raw integer.
//! - It is deliberately narrow: only a `return` that yields the parameter itself, through the
//!   pass-through shapes (ternary, `??`, `match`, `@`, assignment), counts. A body that computes
//!   its own result keeps its inferred type, so this is not a blanket widening of every untyped
//!   declaration.
//! - `caller_visible_return_type` is the other half of that agreement: a callee with an untyped
//!   by-value parameter receives it as a boxed Mixed cell, so a container it builds from that
//!   parameter holds Mixed elements whatever element type the placeholder inferred. EIR call
//!   lowering and the checker's typing of a closure that returns such a call both read that rule
//!   here.

use std::collections::{HashMap, HashSet};

use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::types::{FunctionSig, PhpType};

/// Returns whether an un-hinted body hands one of its untyped by-value parameters back.
///
/// A declared return type is authoritative and is never overridden.
pub fn return_exposes_dynamic_param(
    body: &[Stmt],
    signature: &FunctionSig,
    owner_name: &str,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
) -> bool {
    if signature.declared_return {
        return false;
    }
    let dynamic_params = dynamic_untyped_param_names(owner_name, signature, callable_param_sigs);
    !dynamic_params.is_empty() && body_returns_dynamic_param(body, &dynamic_params)
}

/// Collects untyped by-value parameter names that need a boxed EIR ABI.
///
/// A `callable` parameter, and one whose callable signature was recorded against this owner,
/// keep their own contract: those are invoked rather than forwarded, and boxing them would
/// desynchronize the descriptor from the body.
pub fn dynamic_untyped_param_names(
    owner_name: &str,
    signature: &FunctionSig,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
) -> HashSet<String> {
    let mut names = HashSet::new();
    for (index, (name, php_type)) in signature.params.iter().enumerate() {
        let declared = signature
            .declared_params
            .get(index)
            .copied()
            .unwrap_or(false);
        let by_ref = signature.ref_params.get(index).copied().unwrap_or(false);
        let variadic = signature.variadic.as_deref() == Some(name.as_str());
        let preserved = matches!(php_type.codegen_repr(), PhpType::Callable)
            || callable_param_sigs.contains_key(&(owner_name.to_string(), name.to_string()));
        if !declared && !by_ref && !variadic && !preserved {
            names.insert(name.clone());
        }
    }
    names
}

/// Recursively scans a body for returns that expose dynamic parameters.
pub fn body_returns_dynamic_param(body: &[Stmt], dynamic_params: &HashSet<String>) -> bool {
    body.iter()
        .any(|stmt| stmt_returns_dynamic_param(stmt, dynamic_params))
}

/// Returns true when one statement can return a dynamic parameter directly.
pub fn stmt_returns_dynamic_param(stmt: &Stmt, dynamic_params: &HashSet<String>) -> bool {
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => expr_exposes_dynamic_param(expr, dynamic_params),
        StmtKind::Return(None) => false,
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            body_returns_dynamic_param(then_body, dynamic_params)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| body_returns_dynamic_param(body, dynamic_params))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body_returns_dynamic_param(body, dynamic_params))
        }
        // `ifdef` is resolved before type checking — `check_stmt` rejects a surviving one with
        // "Unresolved ifdef statement" — so neither caller can reach this arm today. It recurses
        // anyway, because the sibling body walkers (`mixed_storage_scan`,
        // `binding_decision_ambiguity`) do, and a walker that silently answers "no returns here"
        // for a construct it does not know is the wrong default for this question.
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            body_returns_dynamic_param(then_body, dynamic_params)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body_returns_dynamic_param(body, dynamic_params))
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body) => body_returns_dynamic_param(body, dynamic_params),
        StmtKind::For {
            init, update, body, ..
        } => {
            init.as_ref()
                .is_some_and(|stmt| stmt_returns_dynamic_param(stmt.as_ref(), dynamic_params))
                || update
                    .as_ref()
                    .is_some_and(|stmt| stmt_returns_dynamic_param(stmt.as_ref(), dynamic_params))
                || body_returns_dynamic_param(body, dynamic_params)
        }
        StmtKind::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|(_, body)| body_returns_dynamic_param(body, dynamic_params))
                || default
                    .as_ref()
                    .is_some_and(|body| body_returns_dynamic_param(body, dynamic_params))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            body_returns_dynamic_param(try_body, dynamic_params)
                || catches
                    .iter()
                    .any(|catch| body_returns_dynamic_param(&catch.body, dynamic_params))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body_returns_dynamic_param(body, dynamic_params))
        }
        _ => false,
    }
}

/// Returns true when an expression can yield one of the dynamic parameters directly.
pub fn expr_exposes_dynamic_param(expr: &Expr, dynamic_params: &HashSet<String>) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => dynamic_params.contains(name),
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            expr_exposes_dynamic_param(value, dynamic_params)
                || expr_exposes_dynamic_param(default, dynamic_params)
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            expr_exposes_dynamic_param(then_expr, dynamic_params)
                || expr_exposes_dynamic_param(else_expr, dynamic_params)
        }
        ExprKind::Match { arms, default, .. } => {
            arms.iter()
                .any(|(_, arm)| expr_exposes_dynamic_param(arm, dynamic_params))
                || default
                    .as_ref()
                    .is_some_and(|expr| expr_exposes_dynamic_param(expr, dynamic_params))
        }
        ExprKind::ErrorSuppress(inner) => expr_exposes_dynamic_param(inner, dynamic_params),
        ExprKind::Assignment { value, .. } => expr_exposes_dynamic_param(value, dynamic_params),
        _ => false,
    }
}

/// Returns the return type a CALLER of a user function observes.
///
/// A declared return type is authoritative. So is the inferred type of a callee whose parameters
/// are all typed, by reference, or variadic. An untyped by-value parameter, however, reaches the
/// callee as a boxed Mixed cell, so an array the callee builds from it holds Mixed elements even
/// when the checker inferred `array<int>` from the placeholder: the container element is widened
/// to Mixed. Scalars are left alone; `return_exposes_dynamic_param` owns the pass-through case.
///
/// EIR call lowering (`crate::ir_lower::expr::eir_user_function_return_type`) uses this, and the
/// checker's typing of a closure whose body is `return <user call>;` applies the same two
/// predicates, so the element the caller reads has one type on both sides (issue #1270).
pub fn caller_visible_return_type(signature: &FunctionSig) -> PhpType {
    if signature.declared_return || !signature_has_dynamic_untyped_param(signature) {
        return signature.return_type.clone();
    }
    dynamic_param_container_return_type(&signature.return_type)
}

/// Returns true when a signature has a parameter EIR must receive as a boxed Mixed cell: an
/// untyped, by-value, non-variadic parameter.
pub fn signature_has_dynamic_untyped_param(signature: &FunctionSig) -> bool {
    signature.params.iter().enumerate().any(|(index, (name, _))| {
        let declared = signature.declared_params.get(index).copied().unwrap_or(false);
        let by_ref = signature.ref_params.get(index).copied().unwrap_or(false);
        let variadic = signature.variadic.as_deref() == Some(name.as_str());
        !declared && !by_ref && !variadic
    })
}

/// Widens the element of an inferred container return type to Mixed, member by member for a
/// union, because the container may be built from boxed dynamic parameters. Non-container types
/// are returned unchanged (in their codegen representation).
pub fn dynamic_param_container_return_type(return_type: &PhpType) -> PhpType {
    // A resource can only leave such a function BOXED. `codegen_repr()` collapses
    // Resource to Int, so the body ended up casting its now-Mixed parameter down to a
    // plain integer: `function f($c) { return $c; }` handed back the resource's display
    // id, and `is_resource()` on the result answered false.
    if matches!(return_type, PhpType::Resource(_)) {
        return PhpType::Mixed;
    }
    match return_type.codegen_repr() {
        PhpType::Array(_) => PhpType::Array(Box::new(PhpType::Mixed)),
        PhpType::AssocArray { key, .. } => PhpType::AssocArray {
            key,
            value: Box::new(PhpType::Mixed),
        },
        PhpType::Union(members) => PhpType::Union(
            members
                .iter()
                .map(dynamic_param_container_return_type)
                .collect(),
        ),
        other => other,
    }
}

/// Checker-facing form of [`dynamic_param_container_return_type`]: widens the element of every
/// container member to Mixed but keeps a union's members, so `?array<int>` becomes
/// `?array<mixed>` instead of collapsing to the boxed `Mixed` that its codegen representation (and
/// therefore EIR lowering) uses for the same value. Non-container members are left as they are.
pub fn dynamic_param_container_php_type(return_type: &PhpType) -> PhpType {
    match return_type {
        PhpType::Array(_) => PhpType::Array(Box::new(PhpType::Mixed)),
        PhpType::AssocArray { key, .. } => PhpType::AssocArray {
            key: key.clone(),
            value: Box::new(PhpType::Mixed),
        },
        PhpType::Union(members) => {
            PhpType::Union(members.iter().map(dynamic_param_container_php_type).collect())
        }
        other => other.clone(),
    }
}

/// Returns whether a type is an indexed or associative array, or a union with such a member.
pub fn type_contains_container(ty: &PhpType) -> bool {
    match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } => true,
        PhpType::Union(members) => members.iter().any(type_contains_container),
        _ => false,
    }
}
