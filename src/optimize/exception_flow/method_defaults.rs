//! Purpose:
//! Tracks callable-default exception effects separately from clean function and method bodies.
//!
//! Called from:
//! - `ExceptionFlowAnalysis::from_program()` and function/method call exception queries.
//!
//! Key details:
//! - The shared argument planner determines which defaults may be selected.
//! - Supplied arguments bypass defaults, while unknown unpack lengths remain conservative.

use super::*;
use crate::types::call_args::plan_call_args;

/// Retains nonliteral free-function defaults when only the source AST is available.
pub(super) fn collect_unresolved_functions(program: &[Stmt], functions: &mut HashSet<String>) {
    for statement in program {
        match &statement.kind {
            StmtKind::FunctionDecl { name, params, .. } if params.iter().any(|(_, _, default, _)| {
                default.as_ref().is_some_and(default_requires_metadata)
            }) => { functions.insert(name.clone()); }
            StmtKind::NamespaceBlock { body, .. } => collect_unresolved_functions(body, functions),
            _ => {}
        }
    }
}

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
pub(super) fn default_requires_metadata(default: &Expr) -> bool {
    match &default.kind {
        ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) | ExprKind::StringLiteral(_)
        | ExprKind::BoolLiteral(_) | ExprKind::Null => false,
        ExprKind::ArrayLiteral(items) => items.iter().any(default_requires_metadata),
        ExprKind::ArrayLiteralAssoc(items) => items.iter()
            .any(|(key, value)| default_requires_metadata(key) || default_requires_metadata(value)),
        _ => true,
    }
}

/// Collects signatures whose nonliteral defaults can throw before the method body executes.
pub(super) fn collect(
    classes: &HashMap<String, ClassInfo>,
    static_methods: bool,
) -> HashMap<String, FunctionSig> {
    classes.iter().flat_map(|(class_name, class)| {
        let methods = if static_methods { &class.static_methods } else { &class.methods };
        methods.iter()
            .filter(|(_, signature)| signature.defaults.iter().flatten()
                .any(default_requires_metadata))
            .map(move |(method, signature)| (method_effect_key(class_name, method), signature.clone()))
    }).collect()
}

/// Reports whether argument binding can select a potentially throwing default at this call site.
pub(super) fn can_throw(signature: &FunctionSig, args: &[Expr], span: crate::span::Span) -> bool {
    !selected_default_throws(signature, args, span).is_empty()
}

/// Conservatively includes effects of selected nonliteral defaults before the callee body.
pub(super) fn selected_default_throws(
    signature: &FunctionSig, args: &[Expr], span: crate::span::Span,
) -> ThrownTypes {
    let Ok(plan) = plan_call_args(signature, args, span, false, true) else {
        return ThrownTypes::unknown();
    };
    plan.possible_default_argument_mask(signature).into_iter()
        .zip(&signature.defaults)
        .filter_map(|(possible, default)| possible.then_some(default.as_ref()).flatten())
        .filter(|default| default_requires_metadata(default))
        .fold(ThrownTypes::default(), |thrown, default| {
            let current = if crate::types::deferred_defaults::deferred_error_message(default).is_some() {
                ThrownTypes::exact("Error")
            } else {
                // Constructors, conversions and cleanup in a preceding subtree may throw first.
                ThrownTypes::unknown()
            };
            thrown.combined(current)
        })
}
