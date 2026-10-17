//! Purpose:
//! Tracks deferred method-default errors separately from clean method bodies.
//!
//! Called from:
//! - `ExceptionFlowAnalysis::from_program()` and exact method-call exception queries.
//!
//! Key details:
//! - The shared argument planner determines which defaults may be selected.
//! - Supplied arguments bypass defaults, while unknown unpack lengths remain conservative.

use super::*;
use crate::types::call_args::plan_call_args;

/// Keeps unresolved default expressions conservative for the AST-only compatibility API.
pub(super) fn collect_unresolved(
    program: &[Stmt],
    static_methods: &mut HashMap<String, bool>,
    instance_methods: &mut HashMap<String, bool>,
) {
    for statement in program {
        match &statement.kind {
            StmtKind::ClassDecl { name, methods, .. } => {
                for method in methods.iter().filter(|method| {
                    method.params.iter().any(|(_, _, default, _)| {
                        default.as_ref().is_some_and(default_requires_metadata)
                    })
                }) {
                    let values = if method.is_static { &mut *static_methods } else { &mut *instance_methods };
                    values.insert(method_effect_key(name, &method.name), true);
                }
            }
            StmtKind::NamespaceBlock { body, .. } => collect_unresolved(body, static_methods, instance_methods),
            _ => {}
        }
    }
}

/// Literal defaults need no binding metadata; other expressions may throw while being resolved.
fn default_requires_metadata(default: &Expr) -> bool {
    match &default.kind {
        ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) | ExprKind::StringLiteral(_)
        | ExprKind::BoolLiteral(_) | ExprKind::Null => false,
        ExprKind::ArrayLiteral(items) => items.iter().any(default_requires_metadata),
        ExprKind::ArrayLiteralAssoc(items) => items.iter()
            .any(|(key, value)| default_requires_metadata(key) || default_requires_metadata(value)),
        _ => true,
    }
}

/// Collects signatures whose deferred defaults can throw before the method body executes.
pub(super) fn collect(
    classes: &HashMap<String, ClassInfo>,
    static_methods: bool,
) -> HashMap<String, FunctionSig> {
    classes.iter().flat_map(|(class_name, class)| {
        let methods = if static_methods { &class.static_methods } else { &class.methods };
        methods.iter()
            .filter(|(_, signature)| signature.defaults.iter().flatten()
                .any(crate::types::deferred_defaults::contains_deferred_error))
            .map(move |(method, signature)| (method_effect_key(class_name, method), signature.clone()))
    }).collect()
}

/// Reports whether argument binding can select a deferred Error default at this call site.
pub(super) fn can_throw(signature: &FunctionSig, args: &[Expr], span: crate::span::Span) -> bool {
    let Ok(plan) = plan_call_args(signature, args, span, false, true) else {
        return true;
    };
    plan.possible_default_argument_mask(signature).into_iter()
        .zip(&signature.defaults)
        .any(|(possible, default)| possible && default.as_ref()
            .is_some_and(crate::types::deferred_defaults::contains_deferred_error))
}
