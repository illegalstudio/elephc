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
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut array = values.array_new(elements.len())?;
    context.clear_array_metadata(array);
    let mut operands = Vec::new();
    // A running counter rather than the element POSITION: one `...` element can contribute any
    // number of entries, and PHP renumbers the integer keys it contributes from this same
    // counter -- `["a", "b"]` spread after two literals occupies keys 2 and 3, and a bare value
    // after it takes key 4.
    let mut next_index = 0_i64;
    let result = (|| {
        for element in elements {
            if let EvalArrayElement::Spread(operand) = element {
                array = eval_spread_into_array(
                    array,
                    operand,
                    &mut next_index,
                    context,
                    scope,
                    values,
                )?;
                continue;
            }
            let index = values.int(next_index)?;
            next_index += 1;
            operands.push(index);
            let (value, target) = match element {
                EvalArrayElement::Value(element) => (eval_owned_expr(element, context, scope, values)?, None),
                EvalArrayElement::Reference(element) => {
                    eval_reference_array_element_value(element, context, scope, values)?
                }
                EvalArrayElement::Spread(_) => unreachable!("handled above"),
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
    context.clear_array_metadata(array);
    let mut next_key = None;
    let mut operands = Vec::new();
    let result = (|| {
        for (element_index, element) in elements.iter().enumerate() {
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
                }
                continue;
            }
            if let EvalArrayElement::Spread(operand) = element {
                // The integer counter is SHARED with the keyed elements around it, which is what
                // makes `["z" => 0, ...["a", "b"], "y" => 9, 7]` put `7` at key 2 rather than 0.
                let mut next_index = match next_key {
                    Some(next_key) => eval_int_value(next_key, values)?,
                    None => 0,
                };
                array = eval_spread_into_array(
                    array,
                    operand,
                    &mut next_index,
                    context,
                    scope,
                    values,
                )
                .map_err(|status| trace_array_literal_error("spread", element_index, status, context))?;
                let counter = values.int(next_index)?;
                operands.push(counter);
                next_key = Some(counter);
                continue;
            }
            let (explicit_key, value, by_ref) = match element {
                EvalArrayElement::Value(value) => (None, value, false),
                EvalArrayElement::Reference(value) => (None, value, true),
                EvalArrayElement::KeyValue { key, value } => (Some(key), value, false),
                EvalArrayElement::KeyReference { key, value } => (Some(key), value, true),
                EvalArrayElement::Spread(_) => unreachable!("handled above"),
            };
            let key = if let Some(key) = explicit_key {
                let key = eval_owned_expr(key, context, scope, values)?;
                operands.push(key);
                next_key = eval_array_next_key_after_explicit_key(key, next_key, &mut operands, values)?;
                key
            } else {
                take_next_array_key(&mut next_key, &mut operands, values)?
            };
            let (value, target) = if by_ref {
                eval_reference_array_element_value(value, context, scope, values)?
            } else {
                (eval_owned_expr(value, context, scope, values)?, None)
            };
            operands.push(value);
            if explicit_key.is_none() { validate_array_append_key(key, context, values)?; }
            array = values.array_set(array, key, value)?;
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

/// Appends one `...operand` element's entries to the array literal under construction.
///
/// PHP's two unpacking rules, which is why this cannot be desugared into a plain element:
/// INTEGER keys are renumbered from the literal's own running counter, and STRING keys are
/// carried through untouched. Writing a string key through `array_set` promotes an indexed
/// container to an associative one, so `[...["a" => 1]]` ends up a hash and `[...[5 => "a"]]`
/// stays a list -- the same two shapes `php -n` produces.
fn eval_spread_into_array(
    mut array: RuntimeCellHandle,
    operand: &EvalExpr,
    next_index: &mut i64,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // The source is read through an OWNED evaluation (a variable's cell is copied, a temporary
    // is taken as is), so it is released unconditionally once its entries are collected.
    let source = eval_owned_expr(operand, context, scope, values)?;
    let entries = eval_spread_source_entries(source, context, values);
    let released_source = eval_release_value(context, values, source);
    let entries = match (entries, released_source) {
        (Ok(entries), Ok(())) => entries,
        (Ok(entries), Err(status)) => {
            let _ = release_spread_entry_owners(entries, values);
            return Err(status);
        }
        (Err(status), _) => return Err(status),
    };
    let mut entries = entries.into_iter();
    while let Some((key, value)) = entries.next() {
        let stored = (|| {
            if values.type_tag(key)? == EVAL_TAG_STRING {
                array = values.array_set(array, key, value)?;
            } else {
                let index = values.int(*next_index)?;
                *next_index += 1;
                let stored = values.array_set(array, index, value);
                let released_index = values.release(index);
                array = stored?;
                released_index?;
            }
            Ok(())
        })();
        // Both cells came from owned iterator/array reads. The destination
        // retains the value and borrows the key, even when its shape changes.
        let released = release_spread_entry_owners([(key, value)], values);
        if let Err(status) = stored.and(released) {
            let _ = release_spread_entry_owners(entries, values);
            return Err(status);
        }
    }
    Ok(array)
}

/// Drains every pair even if releasing one cell reports an error.
fn release_spread_entry_owners(
    entries: impl IntoIterator<Item = (RuntimeCellHandle, RuntimeCellHandle)>,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut first_error = None;
    for (key, value) in entries {
        for cell in [key, value] {
            if let Err(status) = values.release(cell) {
                first_error.get_or_insert(status);
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// Reads one array or Traversable into owned key/value pairs.
///
/// Shared by the array-literal spread and `iterator_to_array()`, which need the same three
/// traversable shapes: an eval generator, an `IteratorAggregate`, and an `Iterator` driven
/// through its methods.
///
/// PHP accepts an array or ANY Traversable here and refuses everything else with the fatal
/// `Only arrays and Traversables can be unpacked` -- an error `catch (\Throwable)` does not
/// catch, so it is a refusal rather than an exception. The three traversable shapes are the same
/// three `foreach` distinguishes: an eval generator, an `IteratorAggregate` that hands over
/// another traversable, and an `Iterator` driven through `rewind`/`valid`/`current`/`key`/`next`.
pub(in crate::interpreter) fn eval_spread_source_entries(
    source: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<(RuntimeCellHandle, RuntimeCellHandle)>, EvalStatus> {
    match values.type_tag(source)? {
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            let len = values.array_len(source)?;
            let mut entries = Vec::with_capacity(len);
            for position in 0..len {
                let key = values.array_iter_key(source, position)?;
                let value = values.array_get(source, key)?;
                entries.push((key, value));
            }
            Ok(entries)
        }
        EVAL_TAG_OBJECT => eval_spread_object_entries(source, 0, context, values),
        _ => {
            note_eval_runtime_failure(
                String::from("only arrays and Traversables can be unpacked"),
                context,
            );
            Err(EvalStatus::RuntimeFatal)
        }
    }
}

/// Reads one traversable OBJECT spread operand into owned key/value pairs.
///
/// `depth` bounds the `IteratorAggregate` hand-off: PHP lets one aggregate return another, and
/// a cycle would otherwise recurse forever. Ten is far past any real chain and is a refusal, not
/// a silent truncation.
fn eval_spread_object_entries(
    object: RuntimeCellHandle,
    depth: usize,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<(RuntimeCellHandle, RuntimeCellHandle)>, EvalStatus> {
    if depth > 10 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let identity = values.object_identity(object)?;
    if context.has_eval_generator(identity) {
        return eval_generator_collect_entries(identity, context, values);
    }
    if eval_foreach_object_is_a(object, "IteratorAggregate", context, values)? {
        let inner = eval_method_call_result(object, "getIterator", Vec::new(), context, values)?;
        let entries = match values.type_tag(inner)? {
            EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => eval_spread_source_entries(inner, context, values),
            EVAL_TAG_OBJECT => eval_spread_object_entries(inner, depth + 1, context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        };
        values.release(inner)?;
        return entries;
    }
    if !eval_foreach_object_is_a(object, "Iterator", context, values)? {
        note_eval_runtime_failure(
            String::from("only arrays and Traversables can be unpacked"),
            context,
        );
        return Err(EvalStatus::RuntimeFatal);
    }
    let result = eval_method_call_result(object, "rewind", Vec::new(), context, values)?;
    values.release(result)?;
    let mut entries = Vec::new();
    loop {
        let valid = eval_method_call_result(object, "valid", Vec::new(), context, values)?;
        let is_valid = values.truthy(valid)?;
        values.release(valid)?;
        if !is_valid {
            return Ok(entries);
        }
        let value = eval_method_call_result(object, "current", Vec::new(), context, values)?;
        let key = eval_method_call_result(object, "key", Vec::new(), context, values)?;
        entries.push((key, value));
        let result = eval_method_call_result(object, "next", Vec::new(), context, values)?;
        values.release(result)?;
    }
}

/// Materializes a spread `Traversable` as the array a call-site `...$iterable` unpacks.
///
/// `f(...$generator)` and `new C(...$iterator)` are valid php: integer keys are positional in
/// iteration order whatever their value, string keys become named arguments. The call machinery
/// only understood arrays, and read an object as an empty one -- `new Substitutions(...
/// Inflectible::getIrregular())` built an inflector with no rules, so `Str::plural()` and
/// `Str::singular()` died on the first call.
pub(in crate::interpreter) fn eval_spread_traversable_array(
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let entries = eval_spread_object_entries(object, 0, context, values)?;
    let mut array = values.assoc_new(entries.len())?;
    let mut next = 0_i64;
    for (key, value) in entries {
        let key = if values.type_tag(key)? == EVAL_TAG_STRING {
            key
        } else {
            let positional = values.int(next)?;
            next += 1;
            positional
        };
        array = values.array_set(array, key, value)?;
    }
    Ok(array)
}

/// Emits the associative-array element stage that failed under opt-in runtime tracing.
fn trace_array_literal_error(
    stage: &str,
    index: usize,
    status: EvalStatus,
    context: &ElephcEvalContext,
) -> EvalStatus {
    if crate::eval_trace::enabled() {
        let call_site = context.call_site();
        eprintln!(
            "[elephc-eval-trace] phase=array_literal_error stage={stage} index={index} status={status:?} file={:?} line={}",
            call_site.0,
            call_site.2,
        );
    }
    status
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

/// Materializes a stable eval array-reference key as a temporary runtime cell.
pub(in crate::interpreter) fn eval_array_reference_key_value(
    key: &EvalArrayReferenceKey,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match key {
        EvalArrayReferenceKey::Int(value) => values.int(*value),
        EvalArrayReferenceKey::String(bytes) => values.string_bytes_value(bytes),
    }
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
