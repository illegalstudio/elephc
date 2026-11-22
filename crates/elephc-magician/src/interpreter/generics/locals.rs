//! Purpose:
//! Enforces substituted generic contracts on explicitly typed local storage.
//!
//! Called from:
//! - Variable assignment and array mutation statements.
//!
//! Key details:
//! - Validation happens after operand evaluation and before storage mutation.

use super::*;
use crate::eval_ir::parameter_type;

/// Collects live declaration obligations on canonical storage without retaining their owners.
fn generic_cell_contracts(cell: RuntimeCellHandle, context: &ElephcEvalContext, property_contents: bool) -> Vec<EvalParameterType> {
    let mut contracts = context.local_reference_types(cell);
    if property_contents && context.semantic_profile().checked_property_references() {
        for source in context.property_reference_types(cell) {
            let Some(contents) = local_binding_origins::array_source_schema(&generic_type(&source.declared))
                .and_then(crate::eval_ir::array_contents_type) else { continue; };
            let contents = parameter_type(&contents);
            if !contracts.contains(&contents) { contracts.push(contents); }
        }
    }
    contracts
}

/// Collects contracts from a local alias group and its persistent shared storage.
pub(in crate::interpreter) fn generic_local_contracts(scope: &ElephcEvalScope, name: &str, context: &ElephcEvalContext) -> Vec<EvalParameterType> {
    collect_local_contracts(scope, name, context, false)
}

/// Reads temporary property obligations for an interior write without copying them into persistent local metadata.
pub(in crate::interpreter) fn generic_local_element_contracts(scope: &ElephcEvalScope, name: &str,
    context: &ElephcEvalContext,
) -> Vec<EvalParameterType> {
    collect_local_contracts(scope, name, context, true)
}

/// Traverses the live alias graph, selecting binding metadata or the current interior-write obligations.
fn collect_local_contracts(scope: &ElephcEvalScope, name: &str, context: &ElephcEvalContext,
    property_contents: bool,
) -> Vec<EvalParameterType> {
    let mut types = Vec::new();
    let mut pending = vec![(scope as *const ElephcEvalScope, name.to_string())];
    let mut seen = std::collections::HashSet::new();
    while let Some((scope, name)) = pending.pop() {
        if !seen.insert((scope, name.clone())) { continue; }
        let Some(scope) = (unsafe { scope.as_ref() }) else { continue; };
        for ty in scope.local_type_contracts(&name) { if !types.contains(&ty) { types.push(ty); } }
        if let Some(cell) = visible_scope_cell(context, scope, &name) {
            for ty in generic_cell_contracts(cell, context, property_contents) { if !types.contains(&ty) { types.push(ty); } }
        }
        if let Some(global_name) = scope.global_alias_target(&name) {
            if let Some(global) = context.global_scope_ptr() { pending.push((global, global_name.to_string())); }
        }
        match scope.reference_target(&name) {
            Some(EvalReferenceTarget::Variable { scope, name }) => pending.push((*scope, name.clone())),
            Some(EvalReferenceTarget::Cell { cell }) => {
                for ty in generic_cell_contracts(*cell, context, property_contents) { if !types.contains(&ty) { types.push(ty); } }
            }
            _ => {}
        }
    }
    types
}

/// Obtains contracts for a whole variable reference before binding an untyped parameter or capture.
pub(in crate::interpreter) fn generic_reference_contracts(target: &EvalReferenceTarget, context: &ElephcEvalContext) -> Vec<EvalParameterType> {
    match target {
        EvalReferenceTarget::Variable { scope, name } => unsafe { scope.as_ref() }.map(|scope| generic_local_contracts(scope, name, context)).unwrap_or_default(),
        EvalReferenceTarget::Cell { cell } => context.local_reference_types(*cell),
        _ => Vec::new(),
    }
}

/// Enforces every contract on the destination before a whole-value replacement.
pub(in crate::interpreter) fn validate_generic_local_contracts(scope: &ElephcEvalScope, name: &str, value: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<(), EvalStatus> {
    let contracts = generic_local_contracts(scope, name, context);
    for declared in &contracts { validate_generic_local(declared, name, value, context, values)?; }
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes) {
        if let Some(reference) = visible_scope_cell(context, scope, name) {
            if values.is_reference(reference)? {
                array_reference_contracts::validate_new_owner(reference, value, &contracts, context, values)?;
            }
        }
    }
    Ok(())
}

/// Checks an initialized or reassigned typed local without coercing its stored value.
pub(in crate::interpreter) fn validate_generic_local(declared: &EvalParameterType, name: &str, value: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<(), EvalStatus> {
    if generic_value_accepts(&generic_type(declared), value, context, values)? { return Ok(()); }
    let message = if let Some(schema) = crate::eval_ir::array_contents_schema(&generic_type(declared)) {
        let prefix = if context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes) {
            "--strict-array-types: "
        } else { "" };
        format!("{prefix}${name} requires {schema} when holding an array")
    } else { format!("Typed local ${name} expects {}", crate::eval_ir::php_type_label(declared)) };
    eval_throw_type_error(&message, context, values)
}

/// Checks an array write against its key and element contracts before mutating storage.
pub(in crate::interpreter) fn validate_generic_local_element(declared: Option<&EvalParameterType>, array: RuntimeCellHandle, index: RuntimeCellHandle, value: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<(), EvalStatus> {
    let Some(declared) = declared else { return Ok(()); };
    let ty = generic_type(declared);
    // Validate the stored key type without replacing the runtime's diagnostic operand.
    let normalized = match values.type_tag(index)? {
        EVAL_TAG_FLOAT | EVAL_TAG_BOOL => Some(values.cast_int(index)?),
        EVAL_TAG_NULL => Some(values.string("")?),
        EVAL_TAG_STRING => eval_numeric_string_array_key(&values.string_bytes(index)?, context.semantic_profile())
            .map(|index| values.int(index)).transpose()?,
        _ => None,
    };
    let accepted = array_write_accepts(&ty, array, normalized.unwrap_or(index), value, context, values);
    let released = normalized.map_or(Ok(()), |key| values.release(key));
    if accepted.and_then(|accepted| released.map(|()| accepted))? { return Ok(()); }
    let message = if let Some(schema) = crate::eval_ir::array_contents_schema(&ty) {
        let prefix = if context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes) {
            "--strict-array-types: "
        } else { "" };
        format!("{prefix}array write requires {schema}")
    } else { format!("Array write violates typed local {ty}") };
    eval_throw_type_error(&message, context, values)
}

/// Checks non-null array arms and requires integer keys for indexed array contracts.
fn array_write_accepts(ty: &Type, array: RuntimeCellHandle, index: RuntimeCellHandle, value: RuntimeCellHandle, context: &ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<bool, EvalStatus> {
    if crate::eval_ir::pending_array_contents(ty) { return Ok(true); }
    if let Some(schema) = crate::eval_ir::array_contents_schema(ty) {
        return array_write_accepts(schema, array, index, value, context, values);
    }
    match ty {
        Type::Atom(name) => Ok(matches!(name.as_str(), "array" | "iterable" | "mixed")),
        Type::Nullable(inner) => array_write_accepts(inner, array, index, value, context, values),
        Type::Intersection(types) => {
            for ty in types { if !array_write_accepts(ty, array, index, value, context, values)? { return Ok(false); } }
            Ok(true)
        }
        Type::Union(types) => {
            for ty in types {
                if generic_value_accepts(ty, array, context, values)? && array_write_accepts(ty, array, index, value, context, values)? { return Ok(true); }
            }
            Ok(false)
        }
        Type::Apply(name, args) if name == "array" && !args.is_empty() => {
            let key = if args.len() == 1 { values.type_tag(index)? == EVAL_TAG_INT }
                else { generic_value_accepts(&args[0], index, context, values)? };
            Ok(key && generic_value_accepts(args.last().unwrap(), value, context, values)?)
        }
        _ => Ok(false),
    }
}
