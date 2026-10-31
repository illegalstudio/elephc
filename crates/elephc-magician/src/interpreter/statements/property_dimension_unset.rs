//! Purpose:
//! Removes property dimensions after capturing keys and computed receiver operands.
//!
//! Called from:
//! - Instance property and nested dimension unset statement dispatch.
//!
//! Key details:
//! - Variable receivers stay live through key effects; computed names convert afterwards.
//! - Native XML reads use probes, ordinary reads call getters without an isset guard.
//! - Every captured owner is released in reverse order on success and failure.

use super::*;
use super::property_write_receivers::{prepare_index, finish_name, release_receivers, PreparedPropertyName, finish_property_unset_receiver, finish_dimension_unset_receiver, require_unset_array_access};

/// Captures nested keys before fetching live storage, preserving computed roots and quiet hops.
pub(super) fn eval_nested_dimension_unset(
    array: &EvalExpr, index: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut owners = Vec::new();
    let result = (|| {
        let receiver = prepare_property_write_receiver(array, &mut owners, context, scope, values)?;
        let index = eval_owned_expr(index, context, scope, values)?;
        owners.push(index);
        let array = finish_dimension_unset_receiver(receiver, &mut owners, context, scope, values)?;
        owners.push(array);
        if matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
            return eval_throw_error(
                "Unsupported unset(): erased array storage has no attached removal path", context, values,
            );
        }
        // Objects mutate by identity; all other admitted values only produce scalar diagnostics.
        let replacement = unset_value(array, index, context, values)?;
        debug_assert!(replacement.is_none());
        Ok(())
    })();
    result.and(release_receivers(owners, context, values))
}

/// Captures operands once and resolves the current property only after the key has run.
pub(super) fn eval_property_dimension_unset(
    object: &EvalExpr, property: DimensionProperty<'_>, index: &EvalExpr,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut owners = Vec::new();
    let result = (|| {
        let receiver = prepare_property_write_receiver(object, &mut owners, context, scope, values)?;
        let property = match property {
            DimensionProperty::Literal(name) => PreparedPropertyName::Literal(name.to_string()),
            DimensionProperty::Dynamic(name) => PreparedPropertyName::Dynamic(
                prepare_index(name, &mut owners, context, scope, values)?),
        };
        let index = eval_owned_expr(index, context, scope, values)?;
        owners.push(index);
        let object = finish_property_unset_receiver(receiver, &mut owners, context, scope, values)?;
        owners.push(object);
        if !matches!(values.type_tag(object)?, EVAL_TAG_OBJECT | EVAL_TAG_CALLABLE) { return Ok(()); }
        let property = match property {
            PreparedPropertyName::Literal(name) => name,
            PreparedPropertyName::Dynamic(name) => finish_name(name, &mut owners, context, scope, values)?,
        };
        let array = eval_unset_property_get_result(object, &property, context, values)?;
        // A reference-backed property may return a borrowed cell; acquire its own lease.
        with_eval_value_lease(array, context, values, |array, context, values| {
            if let Some(replacement) = unset_value(array, index, context, values)? {
                with_eval_value_lease(replacement, context, values, |replacement, context, values| {
                    eval_property_set_result(object, &property, replacement, context, values)
                })?;
            }
            Ok(())
        })
    })();
    result.and(release_receivers(owners, context, values))
}

/// Removes an already captured key without evaluating it again after a getter callback.
fn unset_value(
    array: RuntimeCellHandle, index: RuntimeCellHandle, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    match values.type_tag(array)? {
        EVAL_TAG_OBJECT | EVAL_TAG_CALLABLE => {
            require_unset_array_access(array, context, values)?;
            let result = match super::super::runtime_ops::simplexml_properties::dimension(
                values, array, elephc_builtin_contract::ObjectDimension::Unset, vec![index.borrowed()],
            )? {
                Some(result) => result,
                None => eval_method_call_result(array, "offsetUnset", vec![index.borrowed()], context, values)?,
            };
            eval_release_value(context, values, result)?;
            Ok(None)
        }
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            let normalized = if values.type_tag(index)? == EVAL_TAG_STRING {
                eval_numeric_string_array_key(&values.string_bytes(index)?).map(|key| values.int(key)).transpose()?
            } else { None };
            let index = normalized.unwrap_or(index);
            let result = (|| {
                let key = eval_array_reference_key(index, values)?;
                let replacement = eval_array_without_key_result(array, index, values)?;
                context.clone_array_element_aliases(array, replacement, key.as_ref());
                Ok(replacement)
            })();
            let released = normalized.map_or(Ok(()), |key| values.release(key));
            match (result, released) {
                (Err(status), _) => Err(status),
                (Ok(value), Err(status)) => { let _ = values.release(value); Err(status) }
                (Ok(value), Ok(())) => Ok(Some(value)),
            }
        }
        EVAL_TAG_NULL => Ok(None),
        EVAL_TAG_BOOL if !values.truthy(array)? => {
            values.warning("Deprecated: Automatic conversion of false to array is deprecated")?;
            Ok(None)
        }
        EVAL_TAG_STRING => eval_throw_error("Cannot unset string offsets", context, values),
        _ => eval_throw_error("Cannot unset offset in a non-array variable", context, values),
    }
}

/// Materializes PHP's missing null slot without dispatching a magic setter for an unset fetch.
pub(super) fn eval_missing_unset_property(
    object: RuntimeCellHandle, class: &str, property: &str, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let has_set = context.class_method(class, "__set").is_some()
        || values.reflection_method_flags(class, "__set")?.is_some();
    if has_set {
        return eval_throw_error("Unsupported unset(): missing opaque property requires a raw storage fetch", context, values);
    }
    let value = values.null()?;
    match values.property_set(object, property, value) {
        Ok(()) => Ok(value),
        Err(status) => { let _ = values.release(value); Err(status) }
    }
}
