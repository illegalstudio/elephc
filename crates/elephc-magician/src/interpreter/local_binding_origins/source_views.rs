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

/// Evaluates a literal RHS with source entry metadata while preserving an existing destination context.
pub(in crate::interpreter) fn evaluate_assignment(expr: &EvalExpr, name: &str,
    producer: &mut LocalAssignmentProducer, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes) {
        if let EvalExpr::Array(elements) = expr {
            let (value, observed) = array_literals::evaluate_literal(elements, context, scope, values)?;
            let mut schemas = Vec::new();
            for contract in scope.local_type(name).cloned().into_iter()
                .chain(generic_local_contracts(scope, name, context).into_iter()
                    .filter(|ty| arrays::contains_contents(&generic_type(ty)))) {
                if let Some(schema) = arrays::source_schema(&generic_type(&contract)) {
                    let pending = crate::eval_ir::array_contents_type(schema.clone())
                        .is_some_and(|ty| crate::eval_ir::pending_array_contents(&ty));
                    if !pending && !schemas.contains(&schema) { schemas.push(schema); }
                }
            }
            let source = match schemas.len() {
                0 => observed, 1 => parameter_type(&schemas.remove(0)),
                _ => parameter_type(&Type::Intersection(schemas)),
            };
            producer.binding = Some(source.clone()); producer.source = Some(source);
            return Ok(value);
        }
    }
    eval_owned_expr(expr, context, scope, values)
}
