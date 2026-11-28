//! Purpose:
//! Builds EvalIR array literals and computes PHP-compatible next keys for mixed array construction.
//!
//! Called from:
//! - `crate::interpreter::eval_expr()` for indexed and associative array literal nodes.
//!
//! Key details:
//! - Explicit keys and reference metadata share the selected exact or PHP-normalized identity.
//! - Unkeyed elements continue from the next PHP integer key after explicit keys.
//! - Construction owns operand leases until insertion finishes, including failure paths.

use super::*;

mod source_contracts;
pub(in crate::interpreter) use source_contracts::{evaluate_literal, evaluate_argument_array, evaluate_value};

/// Recognizes only normalized native-default metadata, not PHP-visible function calls.
pub(super) fn native_default_array_spread(element: &EvalArrayElement) -> Option<&EvalExpr> {
    let EvalArrayElement::Value(EvalExpr::Call { name, args }) = element else { return None; };
    if name != "\0elephc_native_default_array_spread" { return None; }
    let [arg] = args.as_slice() else { return None; };
    (arg.name().is_none() && !arg.is_spread()).then(|| arg.value())
}

/// Evaluates an indexed array literal into a boxed runtime Mixed array.
pub(super) fn eval_indexed_array(
    elements: &[EvalArrayElement],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
    mut observed: Option<&mut source_contracts::LiteralSource>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut array = values.array_new(elements.len())?;
    context.clear_array_metadata(array);
    let mut operands = Vec::new();
    let result = (|| {
        for (index, element) in elements.iter().enumerate() {
            let index = values.int(index as i64)?;
            operands.push(index);
            let mut producer = None;
            let mut proof = None;
            let (value, target) = match element {
                EvalArrayElement::Value(element) if observed.is_some() => {
                    let (value, observed, source) = source_contracts::evaluate_literal_value(element, context, scope, values)?;
                    producer = observed; proof = source;
                    (value, None)
                }
                EvalArrayElement::Value(element) => (eval_owned_expr(element, context, scope, values)?, None),
                EvalArrayElement::Reference(element) => {
                    if observed.is_some() { producer = local_binding_origins::source_view(element, context, scope); }
                    let result = eval_reference_array_element_value(element, context, scope, values)?;
                    proof = producer.as_ref().map(|ty| ArgumentSource::capture(ty.clone(), context));
                    result
                }
                EvalArrayElement::KeyValue { .. } | EvalArrayElement::KeyReference { .. } => {
                    return Err(EvalStatus::UnsupportedConstruct);
                }
            };
            operands.push(value);
            array = values.array_set(array, index, value)?;
            if let Some(source) = observed.as_deref_mut() { source.record(index, producer, proof, value, context, values)?; }
            if let Some(target) = target {
                bind_array_element_reference(context, array, index, target, values)?;
            }
        }
        Ok(())
    })();
    finish_array_literal(array, result, operands, context, values)
}

/// Evaluates an associative array literal into a boxed runtime Mixed hash.
pub(super) fn eval_assoc_array(
    elements: &[EvalArrayElement],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
    mut observed: Option<&mut source_contracts::LiteralSource>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut array = values.assoc_new(elements.len())?;
    context.clear_array_metadata(array);
    let mut next_key = None;
    let mut operands = Vec::new();
    let result = (|| {
        for element in elements {
            if let Some(source) = native_default_array_spread(element) {
                let source = eval_owned_expr(source, context, scope, values)?;
                operands.push(source);
                if !matches!(values.type_tag(source)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
                    return throwables::eval_throw_error("Only arrays can be unpacked in constant expression", context, values);
                }
                let len = values.array_len(source)?;
                for position in 0..len {
                    let source_key = values.array_iter_key(source, position)?;
                    let source_key = if source_key.is_borrowed() { values.retain(source_key)? } else { source_key };
                    operands.push(source_key);
                    let value = values.array_get(source, source_key)?;
                    if !value.is_borrowed() { operands.push(value); }
                    let key = if values.type_tag(source_key)? == EVAL_TAG_STRING {
                        source_key
                    } else {
                        let key = take_next_array_key(&mut next_key, &mut operands, values)?;
                        validate_array_append_key(key, context, values)?;
                        key
                    };
                    array = values.array_set(array, key, value)?;
                    if let Some(source) = observed.as_deref_mut() { source.record(key, None, None, value, context, values)?; }
                }
                continue;
            }
            let (explicit_key, value, by_ref) = match element {
                EvalArrayElement::Value(value) => (None, value, false),
                EvalArrayElement::Reference(value) => (None, value, true),
                EvalArrayElement::KeyValue { key, value } => (Some(key), value, false),
                EvalArrayElement::KeyReference { key, value } => (Some(key), value, true),
            };
            let key = if let Some(key) = explicit_key {
                let key = eval_owned_expr(key, context, scope, values)?;
                operands.push(key);
                next_key = eval_array_next_key_after_explicit_key(key, next_key, &mut operands, context.semantic_profile(), values)?;
                key
            } else {
                take_next_array_key(&mut next_key, &mut operands, values)?
            };
            let mut producer = None;
            let mut proof = None;
            let (value, target) = if by_ref {
                if observed.is_some() { producer = local_binding_origins::source_view(value, context, scope); }
                let result = eval_reference_array_element_value(value, context, scope, values)?;
                proof = producer.as_ref().map(|ty| ArgumentSource::capture(ty.clone(), context));
                result
            } else if observed.is_some() {
                let (value, observed, source) = source_contracts::evaluate_literal_value(value, context, scope, values)?;
                producer = observed; proof = source;
                (value, None)
            } else {
                (eval_owned_expr(value, context, scope, values)?, None)
            };
            operands.push(value);
            if explicit_key.is_none() { validate_array_append_key(key, context, values)?; }
            checked_array_keys::for_receiver(array, key, context, values)?;
            array = values.array_set(array, key, value)?;
            if let Some(source) = observed.as_deref_mut() { source.record(key, producer, proof, value, context, values)?; }
            if let Some(target) = target {
                bind_array_element_reference(context, array, key, target, values)?;
            }
        }
        Ok(())
    })();
    finish_array_literal(array, result, operands, context, values)
}

/// Advances the shared next-index state for ordinary entries and unpacked integer keys.
fn take_next_array_key(
    next_key: &mut Option<RuntimeCellHandle>, operands: &mut Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = if let Some(key) = *next_key { key } else {
        let key = values.int(0)?;
        operands.push(key);
        key
    };
    let one = values.int(1)?;
    operands.push(one);
    let next = values.add(key, one)?;
    operands.push(next);
    *next_key = Some(next);
    Ok(key)
}

/// Raises PHP's catchable append overflow after the element's source evaluation.
fn validate_array_append_key(
    key: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if values.type_tag(key)? != EVAL_TAG_INT {
        return throwables::eval_throw_error(
            "Cannot add element to the array as the next element is already occupied", context, values,
        );
    }
    Ok(())
}

/// Releases construction leases and abandons the partial array on evaluation or insertion failure.
fn finish_array_literal(
    array: RuntimeCellHandle,
    result: Result<(), EvalStatus>,
    operands: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = result;
    for operand in operands {
        let released = eval_release_value(context, values, operand);
        if result.is_ok() { result = released; }
    }
    if let Err(status) = result {
        let _ = eval_release_value(context, values, array);
        return Err(status);
    }
    Ok(array)
}

/// Acquires a by-reference array element lease and captures its writable source target.
fn eval_reference_array_element_value(
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    if let EvalExpr::LoadVar(local) = value {
        if let Some(reference) = eval_persistent_variable_reference(local, context, scope, values)? {
            return Ok((values.retain(reference)?, Some(EvalReferenceTarget::Cell { cell: reference })));
        }
    }
    let (value, target) = eval_call_arg_value(value, context, scope, values)?;
    let Some(target) = target else {
        release_expr_result(value, context, values)?;
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = if value.is_borrowed() { values.retain(value)? } else { value };
    Ok((value, Some(persistent_reference_target(target)?)))
}

/// Records one by-reference array element on the eval context side table.
fn bind_array_element_reference(
    context: &mut ElephcEvalContext,
    array: RuntimeCellHandle,
    key: RuntimeCellHandle,
    target: EvalReferenceTarget,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let key = eval_array_reference_key(key, context.semantic_profile(), values)?.ok_or(EvalStatus::RuntimeFatal)?;
    context.bind_array_element_alias(array, key, target);
    Ok(())
}

/// Normalizes a PHP array key for eval reference metadata lookups.
pub(in crate::interpreter) fn eval_array_reference_key(
    key: RuntimeCellHandle,
    profile: elephc_semantics::SemanticProfile,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalArrayReferenceKey>, EvalStatus> {
    Ok(Some(match values.type_tag(key)? {
        EVAL_TAG_INT => EvalArrayReferenceKey::Int(eval_int_value(key, values)?),
        EVAL_TAG_STRING => {
            let bytes = values.string_bytes(key)?;
            if let Some(key) = eval_numeric_string_array_key(&bytes, profile) {
                EvalArrayReferenceKey::Int(key)
            } else {
                EvalArrayReferenceKey::String(bytes)
            }
        }
        EVAL_TAG_NULL => EvalArrayReferenceKey::String(Vec::new()),
        EVAL_TAG_BOOL | EVAL_TAG_FLOAT => EvalArrayReferenceKey::Int(eval_int_value(key, values)?),
        EVAL_TAG_RESOURCE => {
            let integer = values.cast_int(key)?;
            let result = eval_int_value(integer, values).map(EvalArrayReferenceKey::Int);
            let released = values.release(integer);
            result.and_then(|key| released.map(|()| key))?
        }
        _ => return Ok(None),
    }))
}

/// Advances an array literal's automatic key after an integer-normalized explicit key.
fn eval_array_next_key_after_explicit_key(
    key: RuntimeCellHandle,
    current_next_key: Option<RuntimeCellHandle>,
    operands: &mut Vec<RuntimeCellHandle>,
    profile: elephc_semantics::SemanticProfile,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let key = match values.type_tag(key)? {
        EVAL_TAG_INT => key,
        EVAL_TAG_STRING => {
            let bytes = values.string_bytes(key)?;
            let Some(key) = eval_numeric_string_array_key(&bytes, profile) else {
                return Ok(current_next_key);
            };
            let key = values.int(key)?;
            operands.push(key);
            key
        }
        EVAL_TAG_NULL => return Ok(current_next_key),
        _ => {
            let key = values.cast_int(key)?;
            operands.push(key);
            key
        }
    };
    let one = values.int(1)?;
    operands.push(one);
    let candidate = values.add(key, one)?;
    operands.push(candidate);
    let replace = if let Some(current_next_key) = current_next_key {
        let is_greater = values.compare(EvalBinOp::Gt, candidate, current_next_key)?;
        operands.push(is_greater);
        values.truthy(is_greater)?
    } else {
        true
    };
    Ok(if replace {
        Some(candidate)
    } else {
        current_next_key
    })
}

/// Projects the shared string-key policy without allocating or altering original bytes.
pub(in crate::interpreter) fn eval_numeric_string_array_key(
    bytes: &[u8], profile: elephc_semantics::SemanticProfile,
) -> Option<i64> {
    match elephc_semantics::array_keys::classify_string_key(profile, bytes) {
        elephc_semantics::array_keys::StringKey::Integer(key) => Some(key),
        elephc_semantics::array_keys::StringKey::String(_) => None,
    }
}
