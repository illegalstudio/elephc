//! Purpose:
//! Snapshots assignment source views independently from enforcing stable binding contracts.
//!
//! Called from:
//! - Walker and bytecode assignment preparation, and first array element writes.
//!
//! Key details:
//! - Views belong to scope bindings and generations, never shared runtime cell identities.
//! - Shared/reference/global bindings conservatively use existing authoritative contracts.

use super::*;

/// Separates a stable producer obligation from its current assignment source view.
pub(in crate::interpreter) struct LocalAssignmentProducer {
    pub(super) binding: Option<EvalParameterType>,
    pub(super) source: Option<EvalParameterType>,
}

/// Captures both metadata roles before evaluating an assignment's source effects.
pub(in crate::interpreter) fn prepare_assignment(expr: &EvalExpr,
    context: &ElephcEvalContext, scope: &ElephcEvalScope,
) -> LocalAssignmentProducer {
    LocalAssignmentProducer { binding: local_producer_contract(expr, context, scope),
        source: source_view(expr, context, scope) }
}

/// Uses a current unshared source view without turning it into a whole-binding restriction.
pub(in crate::interpreter) fn source_view(expr: &EvalExpr,
    context: &ElephcEvalContext, scope: &ElephcEvalScope,
) -> Option<EvalParameterType> {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes) { return None; }
    match expr {
        EvalExpr::LoadVar(name) => {
            let view = scope.local_type(name).or_else(|| scope.local_source_view(name)).cloned();
            if view.as_ref().is_some_and(|ty| generic_type(ty) == Type::atom("array"))
                && generic_local_contracts(scope, name, context).iter()
                    .any(|ty| arrays::contains_contents(&generic_type(ty))) {
                local_producer_contract(expr, context, scope)
            } else { view.or_else(|| local_producer_contract(expr, context, scope)) }
        },
        EvalExpr::Assign { value, .. } => source_view(value, context, scope),
        _ => local_producer_contract(expr, context, scope),
    }
}
