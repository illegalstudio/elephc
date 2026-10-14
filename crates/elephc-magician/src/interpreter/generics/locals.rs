//! Purpose:
//! Enforces substituted generic contracts on explicitly typed local storage.
//!
//! Called from:
//! - Variable assignment and array mutation statements.
//!
//! Key details:
//! - Validation happens after operand evaluation and before storage mutation.

use super::*;

/// Collects contracts from a local alias group and its persistent shared storage.
pub(in crate::interpreter) fn generic_local_contracts(scope: &ElephcEvalScope, name: &str, context: &ElephcEvalContext) -> Vec<EvalParameterType> {
    let mut types = scope.local_type_contracts(name);
    if let Some(cell) = visible_scope_cell(context, scope, name) {
        for ty in context.local_reference_types(cell) {
            if !types.contains(&ty) { types.push(ty); }
        }
    }
    if let Some(global_name) = scope.global_alias_target(name) {
        if let Some(global) = context.global_scope_ptr().filter(|global| *global != scope as *const _ as *mut _) {
            if let Some(global) = unsafe { global.as_ref() } {
                for ty in global.local_type_contracts(global_name) { if !types.contains(&ty) { types.push(ty); } }
            }
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
    for declared in generic_local_contracts(scope, name, context) { validate_generic_local(&declared, name, value, context, values)?; }
    Ok(())
}

/// Checks an initialized or reassigned typed local without coercing its stored value.
pub(in crate::interpreter) fn validate_generic_local(declared: &EvalParameterType, name: &str, value: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<(), EvalStatus> {
    if generic_value_accepts(&generic_type(declared), value, context, values)? { return Ok(()); }
    eval_throw_type_error(&format!("Typed local ${name} expects {}", generic_type(declared)), context, values)
}

/// Checks an array write against its key and element contracts before mutating storage.
pub(in crate::interpreter) fn validate_generic_local_element(declared: Option<&EvalParameterType>, array: RuntimeCellHandle, index: RuntimeCellHandle, value: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<(), EvalStatus> {
    let Some(declared) = declared else { return Ok(()); };
    let ty = generic_type(declared);
    // Validate the stored key type without replacing the runtime's diagnostic operand.
    let normalized = match values.type_tag(index)? {
        EVAL_TAG_FLOAT | EVAL_TAG_BOOL => Some(values.cast_int(index)?),
        EVAL_TAG_NULL => Some(values.string("")?),
        EVAL_TAG_STRING => eval_numeric_string_array_key(&values.string_bytes(index)?)
            .map(|index| values.int(index)).transpose()?,
        _ => None,
    };
    let accepted = array_write_accepts(&ty, array, normalized.unwrap_or(index), value, context, values);
    let released = normalized.map_or(Ok(()), |key| values.release(key));
    if accepted.and_then(|accepted| released.map(|()| accepted))? { return Ok(()); }
    eval_throw_type_error(&format!("Array write violates typed local {ty}"), context, values)
}

/// Checks non-null array arms and requires integer keys for indexed array contracts.
fn array_write_accepts(ty: &Type, array: RuntimeCellHandle, index: RuntimeCellHandle, value: RuntimeCellHandle, context: &ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<bool, EvalStatus> {
    match ty {
        Type::Atom(name) => Ok(name == "array" || name == "mixed"),
        Type::Nullable(inner) => array_write_accepts(inner, array, index, value, context, values),
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
