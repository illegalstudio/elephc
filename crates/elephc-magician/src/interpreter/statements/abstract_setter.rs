//! Purpose:
//! Resolves and validates inherited abstract setters retained by readonly storage.
//!
//! Called from:
//! - Eval instance writes and Reflection property-hook inspection.
//!
//! Key details:
//! - Storage stays concrete while the original setter owns its parameter contract.
//! - Successful coercions retire only their newly-created argument value.

use super::*;

/// Finds the original setter owner and parameter type without changing the storage owner.
pub(in crate::interpreter) fn eval_inherited_abstract_setter_contract(
    declaring_class: &str,
    property_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, Option<EvalParameterType>)> {
    let mut current = declaring_class.to_string();
    let mut visited = std::collections::HashSet::new();
    while visited.insert(current.to_ascii_lowercase()) {
        if let Some(class) = context.class(&current) {
            if let Some(property) = class.properties().iter().find(|property| {
                property.name() == property_name && property.is_abstract() && property.requires_set_hook()
            }) {
                return Some((class.name().to_string(), property.settable_type().cloned()));
            }
            current = class.parent()?.to_string();
        } else {
            return context.native_abstract_property_requirements(&current).into_iter()
                .find(|(_, property)| property.name() == property_name && property.requires_set())
                .map(|(owner, property)| (owner, property.property_type().cloned()));
        }
    }
    None
}

/// Applies the inherited setter's argument contract before its no-op body returns.
pub(in crate::interpreter) fn eval_inherited_abstract_setter_write(
    declaring_class: &str,
    property_name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let (owner, parameter_type) = eval_inherited_abstract_setter_contract(
        declaring_class, property_name, context,
    ).ok_or(EvalStatus::RuntimeFatal)?;
    let Some(parameter_type) = parameter_type else { return Ok(()); };
    match eval_method_parameter_value(&parameter_type, value, context, values) {
        Ok(coerced) => {
            if coerced != value { eval_release_value(context, values, coerced)?; }
            Ok(())
        }
        Err(EvalStatus::RuntimeFatal) => {
            let expected = eval_parameter_type_error_label(&parameter_type)
                .ok_or(EvalStatus::RuntimeFatal)?;
            let given = eval_given_type_name(value, context, values)?;
            eval_throw_type_error(&format!(
                "{owner}::${property_name}::set(): Argument #1 ($value) must be of type {expected}, {given} given",
            ), context, values)
        }
        Err(status) => Err(status),
    }
}
