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
use crate::eval_ir::{array_contents_schema, array_contents_type, pending_array_contents, pending_array_contents_type};

/// Prepares a new content origin without publishing metadata or consuming the candidate owner.
pub(super) fn prepare(scope: &ElephcEvalScope, name: &str, producer: Option<&EvalParameterType>,
    value: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalParameterType>, EvalStatus> {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes)
        || !matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Ok(None);
    }
    if let Some(existing) = scope.array_local_type(name) {
        let ty = generic_type(existing);
        if !crate::eval_ir::contains_pending_array_contents(&ty) { return Ok(None); }
        if !pending_array_contents(&ty) {
            let source = producer.map(generic_type);
            return pending_arrays::refine_origin(existing, name, value, source.as_ref(), context, values);
        }
    }
    let declared = scope.local_type(name).or_else(|| scope.inferred_local_type(name)
        .filter(|ty| contains_contents(&generic_type(ty))));
    prepare_value(declared, producer, value, name, context, values)
}

/// Prepares a binding-owned content obligation from its declaration and evaluated producer.
pub(in crate::interpreter) fn prepare_value(declared: Option<&EvalParameterType>, producer: Option<&EvalParameterType>,
    value: RuntimeCellHandle, name: &str, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalParameterType>, EvalStatus> {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes)
        || !matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) { return Ok(None); }
    let retained = declared.and_then(|ty| source_schema(&generic_type(ty)))
        .filter(|schema| array_contents_type(schema.clone()).is_none_or(|ty| !pending_array_contents(&ty)));
    let schema = if let Some(schema) = retained { schema }
    else if let Some(producer) = producer {
        source_schema(&generic_type(producer)).unwrap_or_else(|| Type::atom("array"))
    } else {
        if values.array_len(value)? == 0 { return Ok(Some(parameter_type(&pending_array_contents_type()))); }
        infer_schema(value, context, values, 0)?
    };
    let contract = array_contents_type(schema).ok_or(EvalStatus::RuntimeFatal)?;
    let contract = parameter_type(&contract);
    validate_generic_local(&contract, name, value, context, values)?;
    Ok(Some(contract))
}

/// Projects array alternatives without narrowing explicitly open declaration arms.
pub(in crate::interpreter) fn source_schema(ty: &Type) -> Option<Type> {
    if let Some(schema) = array_contents_schema(ty) { return Some(schema.clone()); }
    match ty {
        Type::Atom(name) if matches!(name.as_str(), "array" | "mixed") => Some(Type::atom("array")),
        Type::Apply(name, _) if name == "array" => Some(ty.clone()),
        Type::Nullable(inner) => source_schema(inner),
        Type::Intersection(members) => {
            let mut schemas = Vec::new();
            let mut pending = false;
            for member in members {
                if let Some(schema) = source_schema(member) {
                    if pending_array_contents(&array_contents_type(schema.clone())?) { pending = true; }
                    else if schema != Type::atom("array") && !schemas.contains(&schema) { schemas.push(schema); }
                }
            }
            match schemas.len() {
                0 if pending => array_contents_schema(&pending_array_contents_type()).cloned(),
                0 => Some(Type::atom("array")), 1 => schemas.pop(), _ => Some(Type::Intersection(schemas)),
            }
        },
        Type::Union(members) => {
            let schemas = members.iter().filter_map(source_schema).collect::<Vec<_>>();
            match schemas.len() { 0 => None, 1 => schemas.into_iter().next(), _ => Some(Type::Union(schemas)) }
        }
        _ => None,
    }
}

/// Infers one recursive homogeneous element schema and retains actual semantic key kinds.
pub(super) fn infer_schema(value: RuntimeCellHandle, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps, depth: usize,
) -> Result<Type, EvalStatus> {
    if depth > elephc_generics::MAX_TYPE_ARGUMENT_DEPTH {
        return eval_throw_type_error("--strict-array-types: array nesting exceeds the type depth limit", context, values);
    }
    if !matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return runtime_generic_type(value, context, values, depth);
    }
    if values.array_len(value)? == 0 {
        return Ok(Type::Intersection(vec![Type::atom("array"), pending_array_contents_type()]));
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

/// Projects internal content metadata only for an actual array, retaining explicit mixed alternatives.
pub(in crate::interpreter) fn element_source_type(ty: Type, array: bool) -> Type {
    if array && contains_contents(&ty) {
        let schema = source_schema(&ty).unwrap_or(ty);
        if pending_array_contents(&schema) {
            Type::Intersection(vec![Type::atom("array"), schema])
        } else { schema }
    } else { ty }
}

/// Resolves a source for an evaluated element without letting conditional array facts reopen scalars.
pub(in crate::interpreter) fn element_value_source_type(source: Option<&EvalParameterType>,
    value: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Type, EvalStatus> {
    if let Some(source) = source {
        let source = generic_type(source);
        if matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
            return Ok(element_source_type(source, true));
        }
        if let Some(source) = non_array_source(source) { return Ok(source); }
    }
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes) {
        infer_schema(value, context, values, 0)
    } else { runtime_generic_type(value, context, values, 0) }
}

/// Drops conditional array obligations from scalar producers while retaining declared alternatives.
fn non_array_source(ty: Type) -> Option<Type> {
    if array_contents_schema(&ty).is_some() { return None; }
    match ty {
        Type::Intersection(members) => {
            let mut retained = members.into_iter().filter_map(non_array_source).collect::<Vec<_>>();
            match retained.len() { 0 => None, 1 => retained.pop(), _ => Some(Type::Intersection(retained)) }
        }
        Type::Union(members) => members.into_iter().map(non_array_source)
            .collect::<Option<Vec<_>>>().map(Type::Union),
        Type::Nullable(inner) => non_array_source(*inner).map(|ty| Type::Nullable(Box::new(ty))),
        ty => Some(ty),
    }
}

/// Recognizes internal content metadata inside a compound producer without using value tags.
pub(in crate::interpreter) fn contains_contents(ty: &Type) -> bool {
    array_contents_schema(ty).is_some() || match ty {
        Type::Intersection(members) | Type::Union(members) => members.iter().any(contains_contents),
        Type::Nullable(inner) => contains_contents(inner),
        _ => false,
    }
}

/// Prepares the first informative local write while operands and shared storage remain unchanged.
pub(in crate::interpreter) fn prepare_write(scope: &ElephcEvalScope, name: &str,
    array: RuntimeCellHandle, index: Option<RuntimeCellHandle>, value: RuntimeCellHandle, producer: Option<&EvalParameterType>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalParameterType>, EvalStatus> {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes)
        || scope.local_type(name).is_some() || values.array_len(array)? != 0
        || generic_local_contracts(scope, name, context).iter().any(|ty| {
            let ty = generic_type(ty);
            contains_closed_contents(&ty)
        }) { return Ok(None); }
    let key = match index.map(|key| values.type_tag(key)).transpose()? {
        None | Some(EVAL_TAG_INT | EVAL_TAG_FLOAT | EVAL_TAG_BOOL) => Type::atom("int"),
        Some(EVAL_TAG_STRING | EVAL_TAG_NULL) => Type::atom("string"),
        _ => return Ok(None),
    };
    let element = element_value_source_type(producer, value, context, values)?;
    if !generic_value_accepts(&element, value, context, values)? {
        return eval_throw_type_error("--strict-array-types: first array value violates its source contract", context, values);
    }
    let schema = Type::Apply("array".to_string(), vec![key, element]);
    Ok(array_contents_type(schema).map(|ty| parameter_type(&ty)))
}

/// Publishes an established schema after successful storage write-back and refreshes shared guards.
pub(in crate::interpreter) fn publish_write(scope: &mut ElephcEvalScope, name: &str,
    origin: Option<EvalParameterType>, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let Some(origin) = origin {
        scope.infer_array_local_type(name.to_string(), origin.clone());
        if let Some(cell) = visible_scope_cell(context, scope, name) {
            if values.is_reference(cell)? { context.bind_local_reference_types(cell, &[origin]); }
        }
    }
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed) {
        scope.record_local_source_view(name.to_string(), parameter_type(&Type::atom("array")), context.source_view_epoch());
    }
    refresh_reference_sources(scope, name, context, values)
}

/// Finds an established content constraint while keeping pending captures neutral until first write.
fn contains_closed_contents(ty: &Type) -> bool {
    if array_contents_schema(ty).is_some() { return !pending_array_contents(ty); }
    match ty {
        Type::Union(members) | Type::Intersection(members) => members.iter().any(contains_closed_contents),
        Type::Nullable(inner) => contains_closed_contents(inner),
        _ => false,
    }
}

/// Refreshes array-owned element constraints after a binding has successfully published its value.
pub(in crate::interpreter) fn refresh_reference_sources(scope: &ElephcEvalScope, name: &str,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes) { return Ok(()); }
    let Some(value) = visible_scope_cell(context, scope, name) else { return Ok(()); };
    let contracts = generic_local_contracts(scope, name, context);
    refresh_value_sources(value, &contracts, context, values)
}

/// Replaces one value owner's source constraints, including explicit open contexts on independent copies.
pub(in crate::interpreter) fn refresh_value_sources(value: RuntimeCellHandle, contracts: &[EvalParameterType],
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes) { return Ok(()); }
    if !matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        context.clear_array_metadata(value); return Ok(());
    }
    let mut contents = Vec::new();
    for contract in contracts {
        if let Some(schema) = source_schema(&generic_type(contract)).and_then(crate::eval_ir::array_contents_type) {
            let schema = parameter_type(&schema);
            if !contents.contains(&schema) { contents.push(schema); }
        }
    }
    context.set_array_reference_contracts(value, contents);
    Ok(())
}
