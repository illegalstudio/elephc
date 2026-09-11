//! Purpose:
//! Builds EvalIR array literals and computes PHP-compatible next keys for mixed array construction.
//!
//! Called from:
//! - `crate::interpreter::eval_expr()` for indexed and associative array literal nodes.
//!
//! Key details:
//! - Explicit keys are normalized through runtime string conversion to match PHP array-key rules.
//! - Unkeyed elements continue from the next PHP integer key after explicit keys.

use super::*;

/// Evaluates an indexed array literal into a boxed runtime Mixed array.
pub(super) fn eval_indexed_array(
    elements: &[EvalArrayElement],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut array = values.array_new(elements.len())?;
    context.clear_array_metadata(array);
    let result = (|| {
        for (index, element) in elements.iter().enumerate() {
            eval_indexed_literal_element(&mut array, index, element, context, scope, values)?;
        }
        Ok(array)
    })();
    if result.is_err() {
        context.clear_array_metadata(array);
        let _ = eval_release_value(context, values, array);
    }
    result
}

/// Inserts one indexed element and releases its temporary key even when source evaluation fails.
fn eval_indexed_literal_element(
    array: &mut RuntimeCellHandle, index: usize, element: &EvalArrayElement,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let key = values.int(index as i64)?;
    let result = (|| {
        let (value, target, owns_value) = match element {
            EvalArrayElement::Value(element) => (eval_array_literal_value(element, context, scope, values)?, None, true),
            EvalArrayElement::Reference(element) => {
                let (value, target) = eval_reference_array_element_value(element, context, scope, values)?;
                (value, target, false)
            }
            EvalArrayElement::KeyValue { .. } | EvalArrayElement::KeyReference { .. } => {
                return Err(EvalStatus::UnsupportedConstruct);
            }
        };
        let updated = values.array_set(*array, key, value);
        if let Ok(updated) = updated { *array = updated; }
        let cleanup = if owns_value { eval_release_value(context, values, value) } else { Ok(()) };
        updated?;
        cleanup?;
        if let Some(target) = target {
            bind_array_element_reference(context, *array, key, target, values)?;
        }
        Ok(())
    })();
    let cleanup = values.release(key);
    result.and(cleanup)
}

/// Evaluates an associative array literal into a boxed runtime Mixed hash.
pub(super) fn eval_assoc_array(
    elements: &[EvalArrayElement],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut array = values.assoc_new(elements.len())?;
    context.clear_array_metadata(array);
    let mut next_key = None;
    let result = (|| {
        for element in elements {
            eval_assoc_literal_element(&mut array, &mut next_key, element, context, scope, values)?;
        }
        Ok(())
    })();
    let cleanup = if let Some(key) = next_key { values.release(key) } else { Ok(()) };
    if let Err(status) = result.and(cleanup) {
        context.clear_array_metadata(array);
        let _ = eval_release_value(context, values, array);
        return Err(status);
    }
    Ok(array)
}

/// Owns one associative key until insertion or failure and releases replaced next-key temporaries.
fn eval_assoc_literal_element(
    array: &mut RuntimeCellHandle, next_key: &mut Option<RuntimeCellHandle>, element: &EvalArrayElement,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let explicit = matches!(element, EvalArrayElement::KeyValue { .. } | EvalArrayElement::KeyReference { .. });
    let key = match element {
        EvalArrayElement::KeyValue { key, .. } | EvalArrayElement::KeyReference { key, .. } =>
            eval_owned_expr(key, context, scope, values)?,
        _ => match next_key.take() { Some(key) => key, None => values.int(0)? },
    };
    let result = (|| {
        if explicit {
            let previous = *next_key;
            *next_key = eval_array_next_key_after_explicit_key(key, previous, values)?;
            if previous != *next_key {
                if let Some(previous) = previous { values.release(previous)?; }
            }
        } else {
            let one = values.int(1)?;
            let next = values.add(key, one);
            let cleanup = values.release(one);
            *next_key = Some(next?);
            cleanup?;
        }
        let (value, target, owns_value) = match element {
            EvalArrayElement::Value(value) | EvalArrayElement::KeyValue { value, .. } =>
                (eval_array_literal_value(value, context, scope, values)?, None, true),
            EvalArrayElement::Reference(value) | EvalArrayElement::KeyReference { value, .. } => {
                let (value, target) = eval_reference_array_element_value(value, context, scope, values)?;
                (value, target, false)
            }
        };
        let updated = values.array_set(*array, key, value);
        if let Ok(updated) = updated { *array = updated; }
        let cleanup = if owns_value { eval_release_value(context, values, value) } else { Ok(()) };
        updated?;
        cleanup?;
        if let Some(target) = target { bind_array_element_reference(context, *array, key, target, values)?; }
        Ok(())
    })();
    let cleanup = eval_release_value(context, values, key);
    result.and(cleanup)
}

/// Detaches an ordinary literal element from any source variable reference.
fn eval_array_literal_value(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_owned_expr(expr, context, scope, values)
}

/// Evaluates a by-reference array literal element and captures its writable source target.
fn eval_reference_array_element_value(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    if let EvalExpr::LoadVar(local) = expr {
        if let Some(reference) = eval_persistent_variable_reference(local, context, scope, values)? {
            return Ok((reference, None));
        }
    }
    let (value, target) = eval_call_arg_value(expr, context, scope, values)?;
    let target = target.ok_or(EvalStatus::RuntimeFatal)?;
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
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let (numeric, owned) = match values.type_tag(key)? {
        EVAL_TAG_INT => (key, false),
        EVAL_TAG_STRING => {
            let bytes = values.string_bytes(key)?;
            let Some(key) = eval_numeric_string_array_key(&bytes) else { return Ok(current_next_key); };
            (values.int(key)?, true)
        }
        EVAL_TAG_NULL => return Ok(current_next_key),
        _ => (values.cast_int(key)?, true),
    };
    let result = (|| {
        let one = values.int(1)?;
        let candidate = values.add(numeric, one);
        let cleanup = values.release(one);
        let candidate = candidate?;
        let replace = (|| {
            cleanup?;
            if let Some(current) = current_next_key {
                let greater = values.compare(EvalBinOp::Gt, candidate, current)?;
                let truthy = values.truthy(greater);
                let cleanup = values.release(greater);
                cleanup?;
                truthy
            } else { Ok(true) }
        })();
        match replace {
            Ok(true) => Ok(Some(candidate)),
            Ok(false) => { values.release(candidate)?; Ok(current_next_key) }
            Err(status) => { let _ = values.release(candidate); Err(status) }
        }
    })();
    if owned {
        if let Err(status) = values.release(numeric) {
            if let Ok(Some(candidate)) = result {
                if Some(candidate) != current_next_key { let _ = values.release(candidate); }
            }
            return Err(status);
        }
    }
    result
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
