//! Purpose:
//! Validates original operand domains before mutable member and array consumers publish values.
//!
//! Called from:
//! - Shared receiver staging and local/property dimension mutation coordinators.
//!
//! Key details:
//! - Producer packets own no values and stale shared facts become unknown.
//! - Content declarations reuse the same argument binding relation as whole member writes.

use super::*;
use elephc_generics::Type;
use crate::eval_ir::{generic_type, parameter_type};

/// Requires the complete original receiver domain to support ordinary object member access.
pub(in crate::interpreter) fn object(source: Option<&Type>, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let source = source.cloned().map(mixed_source_proofs::value_source).unwrap_or_else(|| Type::atom("mixed"));
    mixed_source_proofs::require_source(&source, "property writes", context, values, mixed_method_results::object_source)
}

/// Checks declared array element inputs without deriving a logical producer from the stored occupant.
pub(in crate::interpreter) fn elements(contracts: &[EvalParameterType], array: RuntimeCellHandle,
    source: Option<&ArgumentSource>, value: RuntimeCellHandle, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed) { return Ok(()); }
    for contract in contracts { element(&generic_type(contract), array, source, value, context, values)?; }
    Ok(())
}

/// Projects only the selected structural array input and preserves union alternatives as one domain.
fn element(ty: &Type, array: RuntimeCellHandle, source: Option<&ArgumentSource>, value: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if crate::eval_ir::pending_array_contents(ty) { return Ok(()); }
    if let Some(schema) = crate::eval_ir::array_contents_schema(ty) {
        return element(schema, array, source, value, context, values);
    }
    match ty {
        Type::Apply(name, arguments) if name == "array" => {
            if let Some(expected) = arguments.last() {
                mixed_source_proofs::require_exact_argument(&parameter_type(expected), source, value, context, values)?;
            }
        }
        Type::Nullable(inner) => element(inner, array, source, value, context, values)?,
        Type::Intersection(members) => {
            for member in members { element(member, array, source, value, context, values)?; }
        }
        Type::Union(members) => {
            let mut inputs = Vec::new();
            for member in members {
                if !generic_value_accepts(member, array, context, values)? { continue; }
                if let Some(schema) = local_binding_origins::array_source_schema(member) {
                    if let Some(input) = local_binding_origins::declared_offset_result(&schema) { inputs.push(input); }
                }
            }
            if !inputs.is_empty() {
                let expected = if inputs.len() == 1 { inputs.remove(0) } else { Type::Union(inputs) };
                mixed_source_proofs::require_exact_argument(&parameter_type(&expected), source, value, context, values)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Requires reached offset operands to support PHP array/string access or the selected object handler.
pub(in crate::interpreter) fn offset(receiver: Option<&Type>, key: Option<&Type>, has_key: bool,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let receiver = receiver.cloned().map(mixed_source_proofs::value_source).unwrap_or_else(|| Type::atom("mixed"));
    mixed_source_proofs::require_source(&receiver, "offset writes", context, values, |ty, context|
        matches!(ty, Type::Atom(name) | Type::Apply(name, _) if matches!(name.as_str(), "array" | "string" | "null" | "false"))
            || context.class_is_a(&ty.to_string(), "ArrayAccess", false))?;
    if has_key { nullsafe_chains::validate_offset_key_source(Some(&receiver), key, context, values)?; }
    Ok(())
}

/// Reads a staged proof only while its original shared reference epoch remains valid.
pub(in crate::interpreter) fn source_type(source: Option<&ArgumentSource>, value: RuntimeCellHandle,
    context: &ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<Type>, EvalStatus> {
    let shared = values.is_reference(value)?;
    Ok(source.filter(|source| !shared || context.source_view_is_current(value, source.write_epoch))
        .map(|source| generic_type(&source.ty)))
}
