//! Purpose:
//! Refines pending array contents after owned builtin replacement candidates succeed.
//!
//! Called from:
//! - Shared direct/dynamic builtin mutation adapters.
//!
//! Key details:
//! - Source argument alternatives survive separately from candidate storage tags.
//! - Runtime owner frames root replacements and returned values through validation and failure.

use super::*;
use crate::eval_ir::{array_contents_type, pending_array_contents};
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

/// Projects PHP array-cast replacement elements without narrowing an explicitly mixed producer.
pub(in crate::interpreter) fn replacement_element_source(source: Option<&EvalParameterType>) -> Type {
    source.map(generic_type).as_ref().map(element_type).unwrap_or_else(|| Type::atom("mixed"))
}

/// Reads declared array elements and scalar array casts, retaining complete union alternatives.
fn element_type(ty: &Type) -> Type {
    match ty {
        Type::Apply(name, args) if name == "array" => args.last().cloned().unwrap_or_else(|| Type::atom("mixed")),
        Type::Atom(name) if matches!(name.as_str(), "int" | "float" | "string" | "bool" | "false") => ty.clone(),
        Type::Atom(name) if name == "null" => Type::atom("never"),
        Type::Nullable(inner) => element_type(inner),
        Type::Union(members) => Type::Union(members.iter().map(element_type).collect()),
        Type::Intersection(members) => Type::Intersection(members.iter().map(element_type).collect()),
        _ => Type::atom("mixed"),
    }
}

/// Validates and publishes one replacement while balancing both candidate and result owners.
pub(in crate::interpreter) fn finish_array_mutation(target: &EvalReferenceTarget,
    original: RuntimeCellHandle, replacement: RuntimeCellHandle, result: RuntimeCellHandle,
    element: Type, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let replacement = RuntimeOwnerSlot::new(replacement); let result = RuntimeOwnerSlot::new(result);
    with_runtime_owners(&(&replacement, &result), || {
        let outcome = (|| {
            let local_pending = generic_reference_contracts(target, context).iter()
                .any(|ty| pending_array_contents(&generic_type(ty)));
            let property_pending = if let EvalReferenceTarget::Cell { cell } = target {
                context.property_reference_types(*cell).iter().any(|source| pending_array_contents(&generic_type(&source.declared)))
            } else { false };
            let pending = local_pending || property_pending;
            let origin = if context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes)
                && pending && values.array_len(original)? == 0 && values.array_len(replacement.borrowed())? != 0 {
                let ty = array_contents_type(Type::Apply("array".to_string(), vec![Type::atom("int"), element]))
                    .ok_or(EvalStatus::RuntimeFatal)?;
                let origin = parameter_type(&ty);
                validate_generic_local(&origin, "reference", replacement.borrowed(), context, values)?;
                Some(origin)
            } else { None };
            if let Some(origin) = &origin {
                if write_member_reference_origin(target, replacement.borrowed(), Some(origin), context, values,
                    |context, _, origin, cell| {
                        if local_pending { context.bind_local_reference_types(cell, std::slice::from_ref(origin)); }
                        context.refine_property_array_reference_types(cell, origin);
                        Ok(())
                    })? { return Ok(result.take()); }
            }
            let returned = finish_owned_direct_ref_replacement(target, replacement.take(), result.take(), context, values)?;
            if let Some(origin) = origin {
                match target {
                    EvalReferenceTarget::Variable { scope, name } => {
                        if let Some(scope) = unsafe { scope.as_mut() } {
                            scope.infer_array_local_type(name.clone(), origin.clone());
                            refresh_reference_sources(scope, name, context, values)?;
                            if let Some(cell) = visible_scope_cell(context, scope, name) {
                                context.bind_local_reference_types(cell, &[origin]);
                            }
                        }
                    }
                    EvalReferenceTarget::Cell { cell } => context.bind_local_reference_types(*cell, &[origin]),
                    _ => {}
                }
            }
            Ok(returned)
        })();
        finish_eval_argument_values(outcome, [replacement.take(), result.take()], context, values)
    })
}
