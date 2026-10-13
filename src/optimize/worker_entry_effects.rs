//! Purpose:
//! Projects callable-body effects for the checked Parallel worker entry boundary.
//!
//! Called from:
//! - `ParallelSafetyAnalysis::from_program()` and its callable safety queries.
//!
//! Key details:
//! - Transfer validation forbids object arguments that could dispatch Stringable at entry.
//! - Body summaries use full callee effects, never stripped transitively.
//! - Ordinary optimizer calls and calls inside worker closures retain binding effects.

use super::*;

/// Projects transfer-validated worker entry effects while keeping full summaries for nested calls.
pub(super) fn project(program: &[Stmt], full: &CallableEffectAnalysis) -> CallableEffectAnalysis {
    let mut result = full.clone();
    with_callable_effect_analysis(full, || collect(program, &mut result));
    result
}

/// Replaces function and concrete method entry summaries using their bodies and return boundaries.
fn collect(stmts: &[Stmt], result: &mut CallableEffectAnalysis) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FunctionDecl { name, body, return_type, params, .. } => {
                Rc::make_mut(&mut result.function_effects).insert(
                    name.clone(), body_effect(params, body, return_type),
                );
            }
            StmtKind::ClassDecl { name, extends, methods, .. } => {
                let context = ClassEffectContext {
                    class_name: name.clone(),
                    parent_name: extends.as_ref().map(|parent| parent.as_str().to_string()),
                };
                for method in methods.iter().filter(|method| method.has_body) {
                    let effect = with_class_effect_context(Some(context.clone()), || {
                        body_effect(&method.params, &method.body, &method.return_type)
                    });
                    let effects = if method.is_static {
                        &mut result.static_method_effects
                    } else { &mut result.instance_method_effects };
                    Rc::make_mut(effects).insert(method_effect_key(name, &method.name), effect);
                }
            }
            StmtKind::NamespaceBlock { body, .. } => collect(body, result),
            _ => {}
        }
    }
}

/// Combines full body effects with return verification and treats never-returning bodies as effects.
fn body_effect(params: &[(String, Option<TypeExpr>, Option<Expr>, bool)], body: &[Stmt], return_type: &Option<TypeExpr>) -> Effect {
    let effect = block_effect(body).combine(return_binding_effects::effect(params, return_type, body));
    if matches!(return_type, Some(TypeExpr::Never)) { effect.with_side_effects() } else { effect }
}

/// Composite/unresolved entries stay conservative: only direct entry targets
/// use the projection, never calls nested inside their bodies.
pub(super) fn direct_target(callable: &Expr) -> bool {
    matches!(callable.kind, ExprKind::FirstClassCallable(_) | ExprKind::StringLiteral(_))
}

/// Closure roots also bind only transfer-validated input values. Nested closure
/// invocations encountered by block_effect still use the full binding analysis.
pub(super) fn callable_effect(callable: &Expr) -> Effect {
    match &callable.kind {
        ExprKind::Closure { body, return_type, params, .. } => body_effect(params, body, return_type),
        _ => effects::callable_value_effect(callable),
    }
}
