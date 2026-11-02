//! Purpose:
//! Executes increment, unset, append, and indexed array mutation statements.
//!
//! Called from:
//! - `crate::interpreter::statements::execute_stmt()`.
//!
//! Key details:
//! - Object ArrayAccess and plain runtime arrays preserve reference and release semantics.

use super::*;

/// Applies member increment/decrement to a runtime value using PHP numeric semantics.
pub(in crate::interpreter) fn eval_inc_dec_value(
    current: RuntimeCellHandle,
    increment: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let one = values.int(1)?;
    let result = if increment {
        values.add(current, one)
    } else {
        values.sub(current, one)
    };
    let released = values.release(one);
    match (result, released) {
        (Err(status), _) => Err(status),
        (Ok(value), Err(status)) => {
            let _ = values.release(value);
            Err(status)
        }
        (Ok(value), Ok(())) => Ok(value),
    }
}

/// Keeps the old property value alive through the RHS and balances every compound-assignment operand.
pub(super) fn eval_property_compound_assign_result(
    object: RuntimeCellHandle,
    property: &str,
    op: EvalBinOp,
    right: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let current = eval_property_get_result(object, property, context, values)?;
    with_eval_value_lease(current, context, values, |current, context, values| {
        with_eval_void_operands(&[right], context, scope, values, |args, context, _, values| {
            let value = eval_binary_result(op, current, args[0], context, values)?;
            with_eval_value_lease(value, context, values, |value, context, values| {
                eval_property_set_result(object, property, value, context, values)
            })
        })
    })
}

/// Reads, updates, and writes one object property after the receiver/name are evaluated.
pub(super) fn eval_property_inc_dec_result(
    object: RuntimeCellHandle,
    property: &str,
    increment: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let current = eval_property_get_result(object, property, context, values)?;
    with_eval_value_lease(current, context, values, |current, context, values| {
        let value = eval_inc_dec_value(current, increment, values)?;
        with_eval_value_lease(value, context, values, |value, context, values| {
            eval_property_set_result(object, property, value, context, values)
        })
    })
}

/// Reads, updates, and writes one static property after the receiver/name are resolved.
pub(super) fn eval_static_property_inc_dec_result(
    class_name: &str,
    property: &str,
    increment: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let current = eval_static_property_get_result(class_name, property, context, values)?;
    let value = eval_inc_dec_value(current, increment, values)?;
    eval_static_property_set_result(class_name, property, value, context, values)
}

/// Consumes an eval owner even when its dynamic destructor throws, preserving the pending exception.
pub(in crate::interpreter) fn eval_release_value(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    value: RuntimeCellHandle,
) -> Result<(), EvalStatus> {
    let destructor = values.final_object_identity_for_release(value).and_then(|identity| {
        match identity {
            Some(identity) => eval_dynamic_destructor_for_release(identity, value, context, values),
            None => Ok(()),
        }
    });
    // Native release collects any further child exceptions with the context's pending throw.
    // Do not return early: the destructor consumes its receiver lease, not this final owner.
    let released = values.release(value);
    destructor.and(released)
}

/// Calls a dynamic eval `__destruct()` hook immediately before the runtime frees the object.
pub(super) fn eval_dynamic_destructor_for_release(
    identity: u64,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_dynamic_destructor_for_object_cell(identity, object, context, values).map(|_| ())
}

/// Runs whatever a released eval object owes before the runtime frees it.
///
/// A Generator owes the `finally` blocks of every `try` it is still suspended inside, which is
/// how PHP answers an abandoned `foreach`; an ordinary dynamic object owes its `__destruct()`.
pub(crate) fn eval_dynamic_destructor_for_object_cell(
    identity: u64,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if context.has_eval_generator(identity) {
        eval_generator_finalize(identity, context, values)?;
        return Ok(true);
    }
    eval_dynamic_destruct_method_for_object_cell(identity, object, context, values)
}

/// Calls a dynamic eval `__destruct()` hook for an already-boxed object cell.
fn eval_dynamic_destruct_method_for_object_cell(
    identity: u64,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some(class_name) = context
        .dynamic_object_class(identity)
        .map(|class| class.name().to_string())
    else {
        return Ok(false);
    };
    if context.dynamic_object_destructor_is_finished(identity) {
        return Ok(true);
    }
    let Some((declaring_class, method)) = context.class_method(&class_name, "__destruct") else {
        return Ok(false);
    };
    if !context.begin_dynamic_object_destructor(identity) {
        return Ok(true);
    }
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        &class_name,
        &method,
        object,
        Vec::new(),
        context,
        values,
    );
    let release_result = match result {
        Ok(result) => values.release(result),
        Err(status) => Err(status),
    };
    context.finish_dynamic_object_destructor(identity);
    release_result.map(|_| true)
}

/// Executes `unset($object[$key])` through `ArrayAccess::offsetUnset()`.
pub(super) fn eval_array_unset_element_stmt(
    array: &EvalExpr,
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if is_globals_array(array) {
        let name = eval_global_name(index, context, scope, values)?;
        return unset_global_value(&name, context, scope, values);
    }
    match array {
        EvalExpr::LoadVar(name) => {
            let existing = scope_entry(context, scope, name)
                .filter(|entry| entry.flags().is_visible())
                .map(|entry| (entry.cell(), entry.flags().ownership));
            let Some((array, ownership)) = existing else {
                return Ok(());
            };
            if let Some(array) =
                eval_array_unset_target_result(array, index, context, scope, values)?
            {
                for replaced in set_scope_cell(context, scope, name.clone(), array, ownership, values)? {
                    values.release(replaced)?;
                }
            }
            return Ok(());
        }
        EvalExpr::PropertyGet { object, property } => {
            return with_eval_void_operands(&[object], context, scope, values, |args, context, scope, values| {
                eval_property_array_unset_result(args[0], property, index, context, scope, values)
            });
        }
        EvalExpr::DynamicPropertyGet { object, property } => {
            return with_eval_void_operands(&[object], context, scope, values, |args, context, scope, values| {
                let property = eval_dynamic_member_name(property, context, scope, values)?;
                eval_property_array_unset_result(args[0], &property, index, context, scope, values)
            });
        }
        EvalExpr::StaticPropertyGet {
            class_name,
            property,
        } => {
            let array = eval_static_property_get_result(class_name, property, context, values)?;
            if let Some(array) =
                eval_array_unset_target_result(array, index, context, scope, values)?
            {
                eval_static_property_set_result(class_name, property, array, context, values)?;
            }
            return Ok(());
        }
        EvalExpr::DynamicStaticPropertyGet {
            class_name,
            property,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let array = eval_static_property_get_result(&class_name, property, context, values)?;
            if let Some(array) =
                eval_array_unset_target_result(array, index, context, scope, values)?
            {
                eval_static_property_set_result(&class_name, property, array, context, values)?;
            }
            return Ok(());
        }
        EvalExpr::DynamicStaticPropertyNameGet {
            class_name,
            property,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            let array = eval_static_property_get_result(&class_name, &property, context, values)?;
            if let Some(array) =
                eval_array_unset_target_result(array, index, context, scope, values)?
            {
                eval_static_property_set_result(&class_name, &property, array, context, values)?;
            }
            return Ok(());
        }
        EvalExpr::ArrayGet { .. } => {
            return eval_nested_array_unset_element(array, index, context, scope, values);
        }
        _ => {}
    }
    with_eval_void_operands(&[array], context, scope, values, |args, context, scope, values| {
        eval_array_access_unset_result(args[0], index, context, scope, values)
    })
}

/// Releases the old property read and the rebuilt array after an indexed unset writes back.
fn eval_property_array_unset_result(
    object: RuntimeCellHandle,
    property: &str,
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let array = eval_property_get_result(object, property, context, values)?;
    with_eval_value_lease(array, context, values, |array, context, values| {
        if let Some(replacement) = eval_array_unset_target_result(array, index, context, scope, values)? {
            with_eval_value_lease(replacement, context, values, |replacement, context, values| {
                eval_property_set_result(object, property, replacement, context, values)
            })?;
        }
        Ok(())
    })
}

/// Executes `unset($container[$a][$b])`: the element one level down is itself the container.
///
/// php reads the nested container, removes the key from it and writes the container back; an
/// absent or null link anywhere in the chain makes the whole `unset` a silent no-op. Only the
/// ArrayAccess form was supported, so Doctrine's
/// `unset($this->initializedHashMapping[$event][$hash])` (`ContainerAwareEventManager`) failed as
/// an unsupported construct and took every database-backed page down with it.
fn eval_nested_array_unset_element(
    container: &EvalExpr,
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !crate::interpreter::builtins::eval_isset_arg(container, context, scope, values)? {
        return Ok(());
    }
    let element = eval_expr(container, context, scope, values)?;
    let tag = values.type_tag(element)?;
    if tag == EVAL_TAG_OBJECT {
        let result = eval_array_access_unset_result(element, index, context, scope, values);
        eval_release_value(context, values, element)?;
        return result;
    }
    if !matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        eval_release_value(context, values, element)?;
        return Err(EvalStatus::UnsupportedConstruct);
    }
    let updated = eval_array_unset_target_result(element, index, context, scope, values);
    eval_release_value(context, values, element)?;
    if let Some(updated) = updated? {
        eval_store_value_in_lvalue(container, updated, context, scope, values)?;
    }
    Ok(())
}

/// Unsets one offset from an already-resolved array-like target and returns a replacement array.
pub(super) fn eval_array_unset_target_result(
    array: RuntimeCellHandle,
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if values.type_tag(array)? == EVAL_TAG_OBJECT {
        eval_array_access_unset_result(array, index, context, scope, values)?;
        return Ok(None);
    }
    let tag = values.type_tag(array)?;
    if !matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    let index = eval_owned_array_set_index(index, context, scope, values)?;
    let result = (|| {
        let key = eval_array_reference_key(index, values)?;
        let replacement = eval_array_without_key_result(array, index, values)?;
        context.clone_array_element_aliases(array, replacement, key.as_ref());
        Ok(replacement)
    })();
    let released = eval_release_value(context, values, index);
    match (result, released) {
        (Err(status), _) => Err(status),
        (Ok(result), Err(status)) => {
            let _ = values.release(result);
            Err(status)
        }
        (Ok(result), Ok(())) => Ok(Some(result)),
    }
}

/// Executes `unset($object[$key])` through `ArrayAccess::offsetUnset()`.
pub(super) fn eval_array_access_unset_result(
    array: RuntimeCellHandle,
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    with_eval_void_operands(&[index], context, scope, values, |args, context, _, values| {
        if values.type_tag(array)? != EVAL_TAG_OBJECT {
            return Err(EvalStatus::UnsupportedConstruct);
        }
        if !eval_array_access_object_matches(array, context, values)? {
            return Err(EvalStatus::RuntimeFatal);
        }
        let result = eval_method_call_result(array, "offsetUnset", args.to_vec(), context, values)?;
        eval_release_value(context, values, result)
    })
}

/// Rebuilds an unset array with preserved keys, append history, and balanced temporary ownership.
pub(super) fn eval_array_without_key_result(
    array: RuntimeCellHandle,
    index: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    // Unset can leave a hole in a dense array, so preserve keys and append history in a hash.
    let mut result = builtins::collection_builder::EvalArrayBuilder::assoc(values, len.saturating_sub(1))?;
    for position in 0..len {
        let key = result.values().array_iter_key(array, position)?;
        let copied = (|| {
            let equal = result.values().compare(EvalBinOp::StrictEq, key, index)?;
            let matches = result.values().truthy(equal);
            let released = result.values().release(equal);
            let matches = matches?;
            released?;
            if !matches {
                result.entry(|values| values.array_get_preserving_references(array, key), |values, _| values.retain(key))?;
            }
            Ok(())
        })();
        let released = result.values().release(key);
        copied?;
        released?;
    }
    let rebuilt = result.finish();
    if let Err(status) = values.array_copy_index_history(array, rebuilt) {
        let _ = values.release(rebuilt);
        return Err(status);
    }
    Ok(rebuilt)
}

/// Executes short array destructuring and stores each selected positional element.
pub(in crate::interpreter) fn eval_array_destructure_stmt(
    targets: &[Option<EvalDestructureTarget>],
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let array = eval_owned_expr(value, context, scope, values)?;
    let result = eval_destructure_into(targets, array, context, scope, values);
    let released = eval_release_value(context, values, array);
    result.and(released)
}

/// Writes one destructuring pattern's slots from an already-evaluated right-hand side.
///
/// A non-array right-hand side is NOT fatal: `php -n` 8.5.6 gives every target null and takes
/// the right-hand side as the expression's value, warning `Cannot use int as array` only for a
/// non-null scalar. Refusing here broke `[, , , $x] = $scopes[$name] ?? null;`, which is
/// `var-exporter`'s `LazyDecoratorTrait` and is written precisely so the null case is reachable.
pub(in crate::interpreter) fn eval_destructure_into(
    targets: &[Option<EvalDestructureTarget>],
    array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let tag = values.type_tag(array)?;
    // An `ArrayAccess` object destructures through `offsetGet()`: `[$a, $b] = $collection->partition(...)`
    // is how Blade's component attribute bag splits its attributes.
    let readable = matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC)
        || (tag == EVAL_TAG_OBJECT && eval_array_access_object_matches(array, context, values)?);
    if !readable && tag != EVAL_TAG_NULL {
        values.warning(&format!(
            "Cannot use {} as array",
            eval_array_destructure_type_name(tag)
        ))?;
    }
    for (position, target) in targets.iter().enumerate() {
        let Some(target) = target else {
            continue;
        };
        let element = if readable {
            let key = match target.key.as_ref() {
                Some(key) => eval_owned_expr(key, context, scope, values)?,
                None => values.int(position as i64)?,
            };
            let element = eval_array_get_result(array, key, context, values);
            let key_released = eval_release_value(context, values, key);
            let element = element?;
            key_released?;
            // A storage read answers a borrowed handle; the slot below takes an owner of its own.
            if element.is_borrowed() { values.retain(element)? } else { element }
        } else {
            values.null()?
        };
        match &target.slot {
            EvalDestructureSlot::Nested(inner) => {
                let result = eval_destructure_into(inner, element, context, scope, values);
                eval_release_value(context, values, element)?;
                result?;
            }
            EvalDestructureSlot::Lvalue(EvalExpr::LoadVar(name)) => {
                // Array reads own a reference but may return the very cell already in the
                // destination. Scope replacement treats identical cells as an in-place
                // write, so detach that assignment result before transferring its owner.
                let element = if visible_scope_cell(context, scope, name) == Some(element) {
                    let copied = values.copy_value(element);
                    eval_release_value(context, values, element)?;
                    copied?
                } else {
                    element
                };
                for replaced in set_scope_cell(
                    context,
                    scope,
                    name.clone(),
                    element,
                    ScopeCellOwnership::Owned,
                    values,
                )? {
                    eval_release_value(context, values, replaced)?;
                }
            }
            EvalDestructureSlot::Lvalue(lvalue) => {
                eval_store_value_in_lvalue(lvalue, element, context, scope, values)?;
            }
        }
    }
    Ok(())
}

/// Evaluates a destructuring assignment used as an EXPRESSION and returns its RHS value.
///
/// PHP's value here is the right-hand side itself, which is what makes
/// `if ([, , , $x] = $scopes[$name] ?? null)` a usable condition: the `if` tests the array, not
/// the last slot.
pub(in crate::interpreter) fn eval_array_destructure_assign(
    targets: &[Option<EvalDestructureTarget>],
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let assigned = eval_owned_expr(value, context, scope, values)?;
    if let Err(status) = eval_destructure_into(targets, assigned, context, scope, values) {
        let _ = eval_release_value(context, values, assigned);
        return Err(status);
    }
    Ok(assigned)
}

/// Maps runtime tags to PHP's array-destructuring warning spelling.
fn eval_array_destructure_type_name(tag: u64) -> &'static str {
    match tag {
        EVAL_TAG_INT => "int",
        EVAL_TAG_STRING => "string",
        EVAL_TAG_FLOAT => "float",
        EVAL_TAG_BOOL => "bool",
        EVAL_TAG_OBJECT => "object",
        EVAL_TAG_RESOURCE => "resource",
        EVAL_TAG_CALLABLE => "callable",
        _ => "unknown",
    }
}

/// Propagates an array-element reference write without borrowing the array's value owner.
pub(super) fn eval_array_element_reference_write(
    array: RuntimeCellHandle,
    index: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(key) = eval_array_reference_key(index, values)? else { return Ok(()); };
    let Some(target) = context.array_element_alias(array, &key).cloned() else { return Ok(()); };
    match target {
        EvalReferenceTarget::Variable { scope, name } => {
            // The activation that bound the element has returned (a closure's `use (&$x)` from
            // a by-reference `foreach` outliving its method): nothing is left to update, exactly
            // as `write_back_method_ref_target` treats the same target.
            let Some(scope) = (unsafe { scope.live_mut() }) else {
                return Ok(());
            };
            write_back_owned_variable_ref_target(scope, &name, value, context, values)
        }
        EvalReferenceTarget::Cell { cell } if !context.local_reference_types(cell).is_empty() || values.reference_is_shared(cell)? => {
            write_back_method_ref_target(&target, value, context, values)
        }
        EvalReferenceTarget::Cell { .. } => {
            context.bind_array_element_alias(array, key, EvalReferenceTarget::Cell { cell: value });
            Ok(())
        }
        _ => write_back_method_ref_target(&target, value, context, values),
    }
}

/// Writes an indexed property or appends to it while preserving COW and every operand's owner.
pub(super) fn eval_property_array_write_result(
    object: RuntimeCellHandle,
    property: &str,
    index: Option<&EvalExpr>,
    op: Option<EvalBinOp>,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut operands = Vec::new();
    let mut result = (|| {
        // The write TARGET auto-initializes: `$o->p['k'] = 1` on an uninitialized typed property
        // makes an array, where the same property READ raises. See
        // `eval_property_array_target_get_result`.
        let current = eval_property_array_target_get_result(object, property, context, values)?;
        let current = if current.is_borrowed() { values.retain(current)? } else { current };
        operands.push(current);
        if values.type_tag(current)? == EVAL_TAG_OBJECT {
            if !eval_array_access_object_matches(current, context, values)? {
                return Err(EvalStatus::RuntimeFatal);
            }
            let append = index.is_none();
            let index = match index {
                Some(index) => eval_owned_expr(index, context, scope, values)?,
                None => values.null()?,
            };
            operands.push(index);
            let value = eval_property_array_set_value(current, index, op, value, context, scope, values)?;
            let value = if value.is_borrowed() { values.retain(value)? } else { value };
            operands.push(value);
            if append { reject_weak_map_append(current, context, values)?; }
            let returned = eval_method_call_result(current, "offsetSet", vec![index, value], context, values)?;
            operands.push(returned);
            return Ok(());
        }
        let index = match index {
            Some(index) => eval_owned_array_set_index(index, context, scope, values)?,
            None if values.is_array_like(current)? => eval_array_append_key(current, values)?,
            None => values.int(0)?,
        };
        operands.push(index);
        let array = if values.is_array_like(current)? {
            values.array_clone_shallow(current)?
        } else {
            values.array_new(1)?
        };
        operands.push(array);
        context.clone_array_element_aliases(current, array, None);
        let value = eval_property_array_set_value(array, index, op, value, context, scope, values)?;
        let value = if value.is_borrowed() { values.retain(value)? } else { value };
        operands.push(value);
        eval_array_element_reference_write(array, index, value, context, values)?;
        // Runtime setters mutate the receiver cell in place, retaining only the inserted value.
        values.array_set(array, index, value)?;
        eval_property_set_result(object, property, array, context, values)
    })();
    for operand in operands.into_iter().rev() {
        let released = eval_release_value(context, values, operand);
        if result.is_ok() { result = released; }
    }
    result
}

/// Computes the value written by a simple or compound property-array assignment.
pub(super) fn eval_property_array_set_value(
    array: RuntimeCellHandle,
    index: RuntimeCellHandle,
    op: Option<EvalBinOp>,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(op) = op else {
        return eval_expr_value_for_consuming_store(value, context, scope, values);
    };
    let current = eval_array_get_result(array, index, context, values)?;
    let current = if current.is_borrowed() { values.retain(current)? } else { current };
    let result = with_eval_operands(&[value], context, scope, values, |args, context, _, values| {
        eval_binary_result(op, current, args[0], context, values)
    });
    let released = eval_release_value(context, values, current);
    match (result, released) {
        (Err(status), _) => Err(status),
        (Ok(result), Err(status)) => {
            let _ = release_expr_result(result, context, values);
            Err(status)
        }
        (Ok(result), Ok(())) => Ok(result),
    }
}

/// Executes `Class::$property[] = value`, including ArrayAccess static-property values.
pub(super) fn eval_static_property_array_append_result(
    class_name: &str,
    property: &str,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let array = eval_static_property_get_result(class_name, property, context, values)?;
    if values.type_tag(array)? == EVAL_TAG_OBJECT {
        if !eval_array_access_object_matches(array, context, values)? {
            return Err(EvalStatus::RuntimeFatal);
        }
        let array = if array.is_borrowed() { values.retain(array)? } else { array };
        let mut operands = vec![array];
        let mut result = (|| {
            let value = eval_owned_expr(value, context, scope, values)?;
            operands.push(value);
            reject_weak_map_append(array, context, values)?;
            let offset = values.null()?;
            operands.push(offset);
            let returned = eval_method_call_result(array, "offsetSet", vec![offset, value], context, values)?;
            operands.push(returned);
            Ok(())
        })();
        for operand in operands.into_iter().rev() {
            let released = eval_release_value(context, values, operand);
            if result.is_ok() { result = released; }
        }
        return result;
    }
    let array = if values.is_array_like(array)? {
        let tag = values.type_tag(array)?;
        if !matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
            return Err(EvalStatus::UnsupportedConstruct);
        }
        array
    } else {
        values.array_new(1)?
    };
    let array = eval_array_append_value(array, value, context, scope, values)?;
    eval_static_property_set_result(class_name, property, array, context, values)
}

/// Executes `Class::$property[index] = value` and compound indexed static-property writes.
pub(super) fn eval_static_property_array_set_result(
    class_name: &str,
    property: &str,
    index: &EvalExpr,
    op: Option<EvalBinOp>,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let array = eval_static_property_get_result(class_name, property, context, values)?;
    if values.type_tag(array)? == EVAL_TAG_OBJECT {
        if !eval_array_access_object_matches(array, context, values)? {
            return Err(EvalStatus::RuntimeFatal);
        }
        let index = eval_expr(index, context, scope, values)?;
        let value = eval_property_array_set_value(array, index, op, value, context, scope, values)?;
        let result =
            eval_method_call_result(array, "offsetSet", vec![index, value], context, values)?;
        values.release(result)?;
        return Ok(());
    }
    let index = eval_array_set_index(index, context, scope, values)?;
    let array = if values.is_array_like(array)? {
        array
    } else {
        values.array_new(1)?
    };
    let array = eval_array_set_target_for_index(array, index, values)?;
    let value = eval_property_array_set_value(array, index, op, value, context, scope, values)?;
    for declared in context.local_reference_types(array) {
        validate_generic_local_element(Some(&declared), array, index, value, context, values)?;
    }
    let array = values.array_set(array, index, value)?;
    eval_static_property_set_result(class_name, property, array, context, values)
}

/// Evaluates an array-set index and normalizes PHP integer-string keys.
pub(super) fn eval_array_set_index(
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let index = eval_expr(index, context, scope, values)?;
    if values.type_tag(index)? != EVAL_TAG_STRING {
        return Ok(index);
    }
    let bytes = values.string_bytes(index)?;
    match eval_numeric_string_array_key(&bytes) {
        Some(key) => values.int(key),
        None => Ok(index),
    }
}

/// Normalizes an array mutation key into an owned cell, consuming temporary string inputs.
fn eval_owned_array_set_index(
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_operands(&[index], context, scope, values, |args, _, _, values| {
        let index = args[0];
        if values.type_tag(index)? == EVAL_TAG_STRING {
            let bytes = values.string_bytes(index)?;
            if let Some(key) = eval_numeric_string_array_key(&bytes) {
                return values.int(key);
            }
        }
        values.retain(index)
    })
}

/// Converts indexed arrays to associative arrays before writing a non-numeric string key.
pub(in crate::interpreter) fn eval_array_set_target_for_index(
    array: RuntimeCellHandle,
    index: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(array)? != EVAL_TAG_ARRAY || values.type_tag(index)? != EVAL_TAG_STRING {
        return Ok(array);
    }
    let len = values.array_len(array)?;
    let mut assoc = builtins::collection_builder::EvalArrayBuilder::assoc(values, len + 1)?;
    for position in 0..len {
        let key = assoc.values().array_iter_key(array, position)?;
        let inserted = assoc.entry(
            |values| values.array_get_preserving_references(array, key),
            |values, _| values.retain(key),
        );
        let released = assoc.values().release(key);
        inserted?;
        released?;
    }
    Ok(assoc.finish())
}
