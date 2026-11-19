//! Purpose:
//! Establishes inferred array contents independently from local value-kind stability.
//!
//! Called from:
//! - Successful whole-local assignment preparation before storage publication.
//!
//! Key details:
//! - Declared producer alternatives remain authoritative, including explicit openness.
//! - Inference checks every recursive element instead of silently widening to mixed.

use super::*;
use crate::eval_ir::{array_contents_schema, array_contents_type};

/// Prepares a new content origin without publishing metadata or consuming the candidate owner.
pub(super) fn prepare(scope: &ElephcEvalScope, name: &str, producer: Option<&EvalParameterType>,
    value: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalParameterType>, EvalStatus> {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes)
        || scope.array_local_type(name).is_some()
        || !matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Ok(None);
    }
    let declared = scope.local_type(name).or_else(|| scope.inferred_local_type(name)
        .filter(|ty| contains_contents(&generic_type(ty))));
    let retained = declared.and_then(|ty| source_schema(&generic_type(ty)));
    let schema = if let Some(schema) = retained { schema }
    else if let Some(producer) = producer {
        source_schema(&generic_type(producer)).unwrap_or_else(|| Type::atom("array"))
    } else {
        if values.array_len(value)? == 0 { return Ok(None); }
        infer_schema(value, context, values, 0)?
    };
    let contract = array_contents_type(schema).ok_or(EvalStatus::RuntimeFatal)?;
    let contract = parameter_type(&contract);
    validate_generic_local(&contract, name, value, context, values)?;
    Ok(Some(contract))
}

/// Projects array alternatives without narrowing explicitly open declaration arms.
fn source_schema(ty: &Type) -> Option<Type> {
    if let Some(schema) = array_contents_schema(ty) { return Some(schema.clone()); }
    match ty {
        Type::Atom(name) if matches!(name.as_str(), "array" | "mixed") => Some(Type::atom("array")),
        Type::Apply(name, _) if name == "array" => Some(ty.clone()),
        Type::Nullable(inner) => source_schema(inner),
        Type::Intersection(members) => members.iter().rev().find_map(source_schema),
        Type::Union(members) => {
            let schemas = members.iter().filter_map(source_schema).collect::<Vec<_>>();
            match schemas.len() { 0 => None, 1 => schemas.into_iter().next(), _ => Some(Type::Union(schemas)) }
        }
        _ => None,
    }
}

/// Infers one recursive homogeneous element schema and retains actual semantic key kinds.
fn infer_schema(value: RuntimeCellHandle, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps, depth: usize,
) -> Result<Type, EvalStatus> {
    if depth > elephc_generics::MAX_TYPE_ARGUMENT_DEPTH {
        return eval_throw_type_error("--strict-array-types: array nesting exceeds the type depth limit", context, values);
    }
    if !matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return runtime_generic_type(value, context, values, depth);
    }
    let mut keys = Vec::new();
    let mut element = None;
    for position in 0..values.array_len(value)? {
        let key = values.array_iter_key(value, position)?;
        let result = runtime_generic_type(key, context, values, depth + 1);
        let released = values.release(key);
        let key = result.and_then(|ty| released.map(|()| ty))?;
        if !keys.contains(&key) { keys.push(key); }
        let child = values.array_iter_value(value, position)?;
        let result = infer_schema(child, context, values, depth + 1);
        let released = values.release(child);
        let ty = result.and_then(|ty| released.map(|()| ty))?;
        if let Some(previous) = &element {
            if previous != &ty {
                return eval_throw_type_error("--strict-array-types: inferred array elements require one consistent type", context, values);
            }
        } else { element = Some(ty); }
    }
    let key = match keys.len() { 0 => Type::Union(vec![Type::atom("int"), Type::atom("string")]),
        1 => keys.remove(0), _ => Type::Union(keys) };
    Ok(Type::Apply("array".to_string(), vec![key, element.unwrap_or_else(|| Type::atom("mixed"))]))
}

/// Recognizes internal content metadata inside a compound producer without using value tags.
pub(super) fn contains_contents(ty: &Type) -> bool {
    array_contents_schema(ty).is_some() || match ty {
        Type::Intersection(members) | Type::Union(members) => members.iter().any(contains_contents),
        Type::Nullable(inner) => contains_contents(inner),
        _ => false,
    }
}
