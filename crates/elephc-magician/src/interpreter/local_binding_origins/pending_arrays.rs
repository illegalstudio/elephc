//! Purpose:
//! Refines only inferred pending array leaves after an informative nested write candidate.
//!
//! Called from:
//! - Nested dimension root publication through the shared local binding helpers.
//!
//! Key details:
//! - Closed producer alternatives remain unchanged and every prospective schema is validated.
//! - Source hints describe the written path and never replace optional argument-entry proofs.

use super::*;
use crate::eval_ir::{array_contents_schema, array_contents_type, contains_pending_array_contents,
    merge_array_element_sources, pending_array_contents};

/// Prepares refined local contents while the original root and its binding metadata are unchanged.
pub(in crate::interpreter) fn prepare(scope: &ElephcEvalScope, name: &str, value: RuntimeCellHandle,
    source: Option<&Type>, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalParameterType>, EvalStatus> {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes) { return Ok(None); }
    let contracts = generic_local_contracts(scope, name, context);
    let mut changed = false;
    let mut origins = Vec::new();
    for contract in contracts {
        let original = generic_type(&contract);
        if array_contents_schema(&original).is_none() || !contains_pending_array_contents(&original) { continue; }
        let refined = match refine_origin(&contract, name, value, source, context, values)? {
            Some(refined) => { changed = true; generic_type(&refined) },
            None => original,
        };
        if !origins.contains(&refined) { origins.push(refined); }
    }
    if !changed { return Ok(None); }
    let origin = if origins.len() == 1 { origins.remove(0) } else { Type::Intersection(origins) };
    let origin = parameter_type(&origin);
    if let Some(reference) = visible_scope_cell(context, scope, name) {
        if values.is_reference(reference)? {
            array_reference_contracts::validate_new_owner(reference, value, std::slice::from_ref(&origin), context, values)?;
        }
    }
    Ok(Some(origin))
}

/// Validates a prospective refinement before either local or property storage can publish it.
pub(in crate::interpreter) fn refine_origin(origin: &EvalParameterType, name: &str, value: RuntimeCellHandle,
    source: Option<&Type>, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalParameterType>, EvalStatus> {
    let original = generic_type(origin);
    if !contains_pending_array_contents(&original) { return Ok(None); }
    let refined = refine(&original, value, source, context, values, 0)?;
    if refined == original { return Ok(None); }
    let refined = parameter_type(&refined);
    validate_generic_local(&refined, name, value, context, values)?;
    Ok(Some(refined))
}

/// Refines conditional origins and nested array elements without replacing closed terms with occupants.
fn refine(ty: &Type, value: RuntimeCellHandle, source: Option<&Type>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps, depth: usize,
) -> Result<Type, EvalStatus> {
    if depth > elephc_generics::MAX_TYPE_ARGUMENT_DEPTH {
        return eval_throw_type_error("--strict-array-types: inferred array nesting exceeds the type depth limit", context, values);
    }
    if !contains_pending_array_contents(ty) { return Ok(ty.clone()); }
    let array = matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC);
    if pending_array_contents(ty) {
        if !array { return Ok(ty.clone()); }
        let schema = match source.and_then(arrays::source_schema).filter(|ty| !pending_array_contents(ty)) {
            Some(schema) => schema,
            None if values.array_len(value)? == 0 => return Ok(ty.clone()),
            None => arrays::infer_schema(value, context, values, depth)?,
        };
        return array_contents_type(schema).ok_or(EvalStatus::RuntimeFatal);
    }
    if let Some(schema) = array_contents_schema(ty) {
        if !array { return Ok(ty.clone()); }
        return array_contents_type(refine(schema, value, source, context, values, depth)?)
            .ok_or(EvalStatus::RuntimeFatal);
    }
    match ty {
        Type::Intersection(members) => {
            let mut refined = Vec::new();
            for member in members { refined.push(refine(member, value, source, context, values, depth)?); }
            Ok(arrays::element_source_type(Type::Intersection(refined), array))
        }
        Type::Union(members) => {
            let mut refined = Vec::new();
            for member in members {
                refined.push(if generic_value_accepts(member, value, context, values)? {
                    refine(member, value, source, context, values, depth)?
                } else { member.clone() });
            }
            Ok(Type::Union(refined))
        }
        Type::Nullable(inner) if values.type_tag(value)? != EVAL_TAG_NULL =>
            Ok(Type::Nullable(Box::new(refine(inner, value, source, context, values, depth)?))),
        Type::Apply(name, arguments) if name == "array" && array => {
            let Some(element) = arguments.last() else { return Ok(ty.clone()); };
            let hint = source.and_then(arrays::source_schema).and_then(|source| match source {
                Type::Apply(name, arguments) if name == "array" => arguments.last().cloned(), _ => None,
            });
            let mut merged = None;
            for index in 0..values.array_len(value)? {
                let child = values.array_iter_value(value, index)?;
                let result = refine(element, child, hint.as_ref(), context, values, depth + 1);
                let released = values.release(child);
                let next = result.and_then(|next| released.map(|()| next))?;
                merged = Some(match merged { None => next,
                    Some(previous) => merge_array_element_sources(previous, next) });
            }
            let mut arguments = arguments.clone();
            if let Some(merged) = merged { *arguments.last_mut().unwrap() = merged; }
            Ok(Type::Apply(name.clone(), arguments))
        }
        _ => Ok(ty.clone()),
    }
}

/// Builds the data-source schema of one written array layer using its normalized semantic key.
pub(in crate::interpreter) fn wrap_source(index: Option<RuntimeCellHandle>, element: Type,
    context: &ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Type, EvalStatus> {
    let key = match index.map(|index| eval_array_reference_key(index, context.semantic_profile(), values)).transpose()?.flatten() {
        Some(EvalArrayReferenceKey::String(_)) => Type::atom("string"),
        _ => Type::atom("int"),
    };
    Ok(Type::Apply("array".into(), vec![key, element]))
}
