//! Purpose:
//! Preserves successful eval local producer contracts across walker, VM and native references.
//!
//! Called from:
//! - Source assignments and bytecode stores before and after RHS evaluation.
//!
//! Key details:
//! - Producer facts are copied before effects; storage shape does not establish a source contract.
//! - Imported bindings stay open unless their existing source already supplies a constraint.

use super::*;

/// Captures a producer's declared alternatives without executing PHP or borrowing live maps.
pub(in crate::interpreter) fn local_producer_contract(expr: &EvalExpr,
    context: &ElephcEvalContext, scope: &ElephcEvalScope,
) -> Option<EvalParameterType> {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::Locals) { return None; }
    let mixed = || EvalParameterType::new(vec![EvalParameterTypeVariant::Mixed], false);
    match expr {
        EvalExpr::LoadVar(name) => Some(scope.local_type(name).or_else(|| scope.inferred_local_type(name))
            .cloned().unwrap_or_else(mixed)),
        EvalExpr::Assign { value, .. } => local_producer_contract(value, context, scope),
        EvalExpr::Call { name, .. } => {
            let name = name.trim_start_matches('\\').to_ascii_lowercase();
            Some(context.function(&name).and_then(|function| function.return_type()).cloned()
                .or_else(|| context.native_function(&name).and_then(|function| function.return_type().cloned()))
                .unwrap_or_else(mixed))
        }
        EvalExpr::DynamicCall { .. } | EvalExpr::DynamicMethodCall { .. }
            | EvalExpr::DynamicStaticMethodCall { .. } => Some(mixed()),
        _ => None,
    }
}

/// Infers a PHP value kind without accidentally enabling inferred array element constraints.
fn value_contract(value: RuntimeCellHandle, context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalParameterType, EvalStatus> {
    let variant = match values.type_tag(value)? {
        EVAL_TAG_NULL => return Ok(EvalParameterType::new(Vec::new(), true)),
        EVAL_TAG_BOOL => EvalParameterTypeVariant::Bool,
        EVAL_TAG_INT => EvalParameterTypeVariant::Int,
        EVAL_TAG_FLOAT => EvalParameterTypeVariant::Float,
        EVAL_TAG_STRING => EvalParameterTypeVariant::String,
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => EvalParameterTypeVariant::Array,
        EVAL_TAG_OBJECT => {
            let ty = runtime_generic_type(value, context, values, 0)?;
            EvalParameterTypeVariant::Generic(ty)
        }
        EVAL_TAG_CALLABLE => EvalParameterTypeVariant::Callable,
        _ => EvalParameterTypeVariant::Mixed,
    };
    Ok(EvalParameterType::new(vec![variant], false))
}

/// Consumes an assignment value, then establishes only a genuinely new successful binding.
pub(in crate::interpreter) fn store_local_with_origin(context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, name: String, value: RuntimeCellHandle,
    producer: Option<EvalParameterType>, values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let fresh = context.semantic_profile().contains(elephc_semantics::StrictFeature::Locals)
        && visible_scope_cell(context, scope, &name).is_none()
        && scope.local_type(&name).is_none();
    let origin = if fresh {
        match producer.map(Ok).unwrap_or_else(|| value_contract(value, context, values)) {
            Ok(origin) => Some(origin),
            Err(status) => { let _ = eval_release_value(context, values, value); return Err(status); }
        }
    } else { None };
    let replaced = set_owned_scope_cell(context, scope, name.clone(), value, values)?;
    if let Some(origin) = origin {
        let open = origin.variants().contains(&EvalParameterTypeVariant::Mixed);
        scope.infer_local_type(name.clone(), origin);
        if !open && values.supports_persistent_references() {
            eval_persistent_variable_reference(&name, context, scope, values)?;
        }
    }
    Ok(replaced)
}
