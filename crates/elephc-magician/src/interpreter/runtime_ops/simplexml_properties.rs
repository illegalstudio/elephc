//! Purpose:
//! Routes eval SimpleXML properties through the compiled native-view adapters.
//!
//! Called from:
//! - Eval instance property reads, writes, isset, and unset.
//!
//! Key details:
//! - Bridge operands are borrowed and temporary property-name cells are always released.
//! - Embedders without native class capabilities keep their ordinary property behavior.

use super::*;

/// Recognizes native XML properties without fetching or materializing a selection.
pub(in crate::interpreter) fn is_native(
    values: &mut impl RuntimeValueOps, object: RuntimeCellHandle, property: &str,
) -> Result<bool, EvalStatus> {
    if property.starts_with("__elephc_") || values.type_tag(object)? != EVAL_TAG_OBJECT { return Ok(false); }
    match values.object_is_a(object, "SimpleXMLElement", false) {
        Ok(found) => Ok(found),
        Err(EvalStatus::UnsupportedConstruct) => Ok(false),
        Err(status) => Err(status),
    }
}

/// Tests native container emptiness without applying the object's boolean cast.
pub(in crate::interpreter) fn empty(
    values: &mut impl RuntimeValueOps, object: RuntimeCellHandle,
    property: Option<&str>, index: Option<RuntimeCellHandle>,
) -> Result<Option<bool>, EvalStatus> {
    let result = if let Some(property) = property {
        invoke(values, object, property, elephc_builtin_contract::ObjectProperty::Empty, None)?
    } else {
        if values.type_tag(object)? != EVAL_TAG_OBJECT { return Ok(None); }
        match values.object_is_a(object, "SimpleXMLElement", false) {
            Ok(true) => (),
            Ok(false) | Err(EvalStatus::UnsupportedConstruct) => return Ok(None),
            Err(status) => return Err(status),
        }
        let index = index.ok_or(EvalStatus::RuntimeFatal)?;
        Some(values.operator_method_call(object, "__elephc_dimension_empty", vec![index])?)
    };
    let Some(result) = result else { return Ok(None); };
    let empty = values.truthy(result);
    let released = values.release(result);
    empty.and_then(|empty| released.map(|()| Some(empty)))
}

/// Dispatches a native XML property operation, or returns None for an ordinary object.
pub(in crate::interpreter) fn invoke(
    values: &mut impl RuntimeValueOps, object: RuntimeCellHandle, property: &str,
    operation: elephc_builtin_contract::ObjectProperty, assigned: Option<RuntimeCellHandle>,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !is_native(values, object, property)? { return Ok(None); }
    let method = elephc_builtin_contract::lookup_class("SimpleXMLElement")
        .and_then(|class| class.operators).and_then(|operators| operators.property_method(operation))
        .ok_or(EvalStatus::RuntimeFatal)?;
    let name = values.string(property)?;
    let mut arguments = vec![name];
    arguments.extend(assigned);
    let result = values.operator_method_call(object, method, arguments);
    let released = values.release(name);
    match result {
        Ok(value) => match released {
            Ok(()) => Ok(Some(value)),
            Err(status) => { let _ = values.release(value); Err(status) },
        },
        Err(status) => Err(status),
    }
}
