//! Purpose:
//! Builds EvalIR array literals and computes PHP-compatible next keys for mixed array construction.
//!
//! Called from:
//! - `crate::interpreter::eval_expr()` for indexed and associative array literal nodes.
//!
//! Key details:
//! - Explicit keys are normalized through runtime string conversion to match PHP array-key rules.
//! - Unkeyed elements continue from the next PHP integer key after explicit keys.
//! - Construction owns operand leases until insertion finishes, including failure paths.

use super::*;

/// Evaluates an indexed array literal into a boxed runtime Mixed array.
pub(super) fn eval_indexed_array(
    elements: &[EvalArrayElement],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut array = values.array_new(elements.len())?;
    let mut operands = Vec::new();
    let result = (|| {
        for (index, element) in elements.iter().enumerate() {
            let index = values.int(index as i64)?;
            operands.push(index);
            let (value, target) = match element {
                EvalArrayElement::Value(element) => (eval_owned_expr(element, context, scope, values)?, None),
                EvalArrayElement::Reference(element) => {
                    let (value, target) =
                        eval_reference_array_element_value(element, context, scope, values)?;
                    (value, Some(target))
                }
                EvalArrayElement::KeyValue { .. } | EvalArrayElement::KeyReference { .. } => {
                    return Err(EvalStatus::UnsupportedConstruct);
                }
            };
            operands.push(value);
            array = values.array_set(array, index, value)?;
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
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut array = values.assoc_new(elements.len())?;
    let mut next_key = None;
    let mut operands = Vec::new();
    let result = (|| {
        for element in elements {
            let (explicit_key, value, by_ref) = match element {
                EvalArrayElement::Value(value) => (None, value, false),
                EvalArrayElement::Reference(value) => (None, value, true),
                EvalArrayElement::KeyValue { key, value } => (Some(key), value, false),
                EvalArrayElement::KeyReference { key, value } => (Some(key), value, true),
            };
            let key = if let Some(key) = explicit_key {
                let key = eval_owned_expr(key, context, scope, values)?;
                operands.push(key);
                next_key = eval_array_next_key_after_explicit_key(key, next_key, &mut operands, values)?;
                key
            } else {
                let key = match next_key {
                    Some(key) => key,
                    None => {
                        let key = values.int(0)?;
                        operands.push(key);
                        key
                    }
                };
                let one = values.int(1)?;
                operands.push(one);
                let next = values.add(key, one)?;
                operands.push(next);
                next_key = Some(next);
                key
            };
            let (value, target) = if by_ref {
                let (value, target) = eval_reference_array_element_value(value, context, scope, values)?;
                (value, Some(target))
            } else {
                (eval_owned_expr(value, context, scope, values)?, None)
            };
            operands.push(value);
            array = values.array_set(array, key, value)?;
            if let Some(target) = target {
                bind_array_element_reference(context, array, key, target, values)?;
            }
        }
        Ok(())
    })();
    finish_array_literal(array, result, operands, context, values)
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
) -> Result<(RuntimeCellHandle, EvalReferenceTarget), EvalStatus> {
    let (value, target) = eval_call_arg_value(value, context, scope, values)?;
    let Some(target) = target else {
        release_expr_result(value, context, values)?;
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = if value.is_borrowed() { values.retain(value)? } else { value };
    Ok((value, target))
}

/// Records one by-reference array element on the eval context side table.
fn bind_array_element_reference(
    context: &mut ElephcEvalContext,
    array: RuntimeCellHandle,
    key: RuntimeCellHandle,
    target: EvalReferenceTarget,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let key = eval_array_reference_key(key, values)?.ok_or(EvalStatus::RuntimeFatal)?;
    context.bind_array_element_alias(array, key, target);
    Ok(())
}

/// Normalizes a PHP array key for eval reference metadata lookups.
pub(in crate::interpreter) fn eval_array_reference_key(
    key: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalArrayReferenceKey>, EvalStatus> {
    Ok(Some(match values.type_tag(key)? {
        EVAL_TAG_INT => EvalArrayReferenceKey::Int(eval_int_value(key, values)?),
        EVAL_TAG_STRING => {
            let bytes = values.string_bytes(key)?;
            if let Some(key) = eval_numeric_string_array_key(&bytes) {
                EvalArrayReferenceKey::Int(key)
            } else {
                EvalArrayReferenceKey::String(bytes)
            }
        }
        EVAL_TAG_NULL => EvalArrayReferenceKey::String(Vec::new()),
        EVAL_TAG_BOOL | EVAL_TAG_FLOAT => EvalArrayReferenceKey::Int(eval_int_value(key, values)?),
        _ => return Ok(None),
    }))
}

/// Advances an array literal's automatic key after an integer-normalized explicit key.
fn eval_array_next_key_after_explicit_key(
    key: RuntimeCellHandle,
    current_next_key: Option<RuntimeCellHandle>,
    operands: &mut Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let key = match values.type_tag(key)? {
        EVAL_TAG_INT => key,
        EVAL_TAG_STRING => {
            let bytes = values.string_bytes(key)?;
            let Some(key) = eval_numeric_string_array_key(&bytes) else {
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

/// Parses PHP integer-string array keys that normalize to integer keys.
pub(in crate::interpreter) fn eval_numeric_string_array_key(bytes: &[u8]) -> Option<i64> {
    if bytes.is_empty() {
        return None;
    }

    let (negative, digits) = if bytes[0] == b'-' {
        if bytes.len() == 1 {
            return None;
        }
        (true, &bytes[1..])
    } else {
        (false, bytes)
    };

    if digits[0] == b'0' {
        return if !negative && digits.len() == 1 {
            Some(0)
        } else {
            None
        };
    }
    if digits.iter().any(|byte| !byte.is_ascii_digit()) {
        return None;
    }

    let limit = if negative {
        i64::MAX as u128 + 1
    } else {
        i64::MAX as u128
    };
    let mut value = 0u128;
    for digit in digits {
        value = (value * 10) + u128::from(digit - b'0');
        if value > limit {
            return None;
        }
    }

    if negative {
        if value == i64::MAX as u128 + 1 {
            Some(i64::MIN)
        } else {
            Some(-(value as i64))
        }
    } else {
        Some(value as i64)
    }
}
