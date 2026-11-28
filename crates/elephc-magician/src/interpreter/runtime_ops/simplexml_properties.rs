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
use crate::runtime_hooks::owner_frames::{
    RuntimeOwnerSlot, StagedRuntimeOwners, with_runtime_owners,
};

/// Applies a native read-modify-write after its RHS has been evaluated by the caller.
pub(in crate::interpreter) fn update(
    values: &mut impl RuntimeValueOps, object: RuntimeCellHandle, property: &str,
    operation: EvalBinOp, right: RuntimeCellHandle, post: bool,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    use elephc_builtin_contract::ObjectPropertyUpdate as Update;
    let operation = match operation {
        EvalBinOp::Add => Update::Add, EvalBinOp::Sub => Update::Sub,
        EvalBinOp::Mul => Update::Mul, EvalBinOp::Div => Update::Div,
        EvalBinOp::Mod => Update::Mod, EvalBinOp::Pow => Update::Pow,
        EvalBinOp::Concat => Update::Concat, EvalBinOp::BitAnd => Update::BitAnd,
        EvalBinOp::BitOr => Update::BitOr, EvalBinOp::BitXor => Update::BitXor,
        EvalBinOp::ShiftLeft => Update::ShiftLeft, EvalBinOp::ShiftRight => Update::ShiftRight,
        _ => return Ok(None),
    };
    if !is_native(values, object, property)? { return Ok(None); }
    let method = elephc_builtin_contract::lookup_class("SimpleXMLElement")
        .and_then(|class| class.operators)
        .and_then(|operators| operators.property_method(elephc_builtin_contract::ObjectProperty::Update))
        .ok_or(EvalStatus::RuntimeFatal)?;
    use elephc_memoir::runtime_owners::{CleanupErrorPolicy, ReleaseOrder};
    let owners = StagedRuntimeOwners::default();
    let output = RuntimeOwnerSlot::empty();
    with_runtime_owners(&(&owners, &output), || {
        let operation = (|| {
            let name = owners.stage(values.string(property)?);
            let operation = owners.stage(values.int(operation as i64)?);
            let post = owners.stage(values.bool_value(post)?);
            let result = values.operator_method_call(object.borrowed(), method,
                vec![name.borrowed(), right.borrowed(), operation.borrowed(), post.borrowed()])?;
            output.replace(Some(result));
            Ok(())
        })();
        let cleanup = owners.take_all().release(
            ReleaseOrder::Reverse,
            CleanupErrorPolicy::First,
            |owner| values.release(owner),
        );
        output.finish(operation.and(cleanup), |value| values.release(value)).map(Some)
    })
}

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

/// Appends without supplying an explicit PHP null dimension to the native writer.
pub(in crate::interpreter) fn append(
    values: &mut impl RuntimeValueOps, object: RuntimeCellHandle, value: RuntimeCellHandle,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    dimension(values, object, elephc_builtin_contract::ObjectDimension::Append, vec![value])
}

/// Invokes a final native dimension adapter while preserving public user method semantics.
pub(in crate::interpreter) fn dimension(
    values: &mut impl RuntimeValueOps, object: RuntimeCellHandle,
    operation: elephc_builtin_contract::ObjectDimension, arguments: Vec<RuntimeCellHandle>,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !is_native(values, object, "")? { return Ok(None); }
    let method = elephc_builtin_contract::lookup_class("SimpleXMLElement")
        .and_then(|class| class.operators).and_then(|ops| ops.dimension_method(operation))
        .ok_or(EvalStatus::RuntimeFatal)?;
    values.operator_method_call(object, method, arguments).map(Some)
}

/// Tests native container emptiness while borrowing the caller-owned receiver and index.
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
        Some(values.operator_method_call(object.borrowed(), "__elephc_dimension_empty", vec![index.borrowed()])?)
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
    let name = RuntimeOwnerSlot::new(values.string(property)?);
    let output = RuntimeOwnerSlot::empty();
    with_runtime_owners(&(&name, &output), || {
        let mut arguments = vec![name.borrowed()];
        arguments.extend(assigned);
        let operation = values.operator_method_call(object, method, arguments).map(|value| {
            output.replace(Some(value));
        });
        let cleanup = values.release(name.take());
        output.finish(operation.and(cleanup), |value| values.release(value)).map(Some)
    })
}
