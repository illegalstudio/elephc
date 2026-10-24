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

/// Dispatches a native XML property operation, or returns None for an ordinary object.
pub(in crate::interpreter) fn invoke(
    values: &mut impl RuntimeValueOps, object: RuntimeCellHandle, property: &str,
    operation: elephc_builtin_contract::ObjectProperty, assigned: Option<RuntimeCellHandle>,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if property.starts_with("__elephc_") { return Ok(None); }
    if values.type_tag(object)? != EVAL_TAG_OBJECT { return Ok(None); }
    match values.object_is_a(object, "SimpleXMLElement", false) {
        Ok(true) => (),
        Ok(false) | Err(EvalStatus::UnsupportedConstruct) => return Ok(None),
        Err(status) => return Err(status),
    }
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
