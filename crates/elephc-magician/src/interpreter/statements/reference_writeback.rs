//! Purpose:
//! Binds and writes back method by-reference targets across runtime storage shapes.
//!
//! Called from:
//! - Eval and native method invocation after argument binding.
//!
//! Key details:
//! - Invoker slots, arrays, nested arrays, object properties, and static properties preserve aliases.
//! - Array writeback clones preserve element aliases and cursors before replacing shared storage.

use super::*;

/// Returns the calling-scope class when PHP's private-method shadowing rule
/// selects it as the dispatch scope: `$obj->m()` from class scope S calls S's
/// own private instance method `m` — never the receiver's override — when S
/// declares one and the receiver is an instance of S. The returned scope
/// string drives the native bridge's hidden private shadow slot directly.
pub(in crate::interpreter) fn eval_private_scope_shadow_bridge_scope(
    receiver_class: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let Some(scope_class) = context.current_class_scope() else {
        return Ok(None);
    };
    let scope_class = scope_class.to_string();
    if !same_eval_class_name(receiver_class, &scope_class)
        && !native_class_is_a(receiver_class, &scope_class, context)
    {
        return Ok(None);
    }
    let Some((declaring_class, visibility, is_static, is_abstract)) =
        eval_aot_method_dispatch_metadata(&scope_class, method_name, values)?
    else {
        return Ok(None);
    };
    if visibility == EvalVisibility::Private
        && !is_static
        && !is_abstract
        && same_eval_class_name(&declaring_class, &scope_class)
    {
        Ok(Some(declaring_class))
    } else {
        Ok(None)
    }
}

/// Binds method parameters into a fresh method scope and marks by-reference params as aliases.
pub(in crate::interpreter) fn bind_method_scope_args(
    method_scope: &mut ElephcEvalScope,
    params: &[String],
    parameter_is_by_ref: &[bool],
    bound_args: &[BoundMethodArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for (position, (name, bound_arg)) in params.iter().zip(bound_args.iter()).enumerate() {
        if parameter_is_by_ref.get(position).copied().unwrap_or(false) {
            method_scope.set_reference(
                name.clone(),
                name.clone(),
                bound_arg.value,
                ScopeCellOwnership::Borrowed,
            );
            if let Some(target) = bound_arg.ref_target.clone() {
                method_scope.set_reference_target(name.clone(), target);
            }
            if bound_arg.ref_target.is_some() && values.supports_persistent_references() && !values.is_reference(bound_arg.value)? {
                let reference = values.reference_new(bound_arg.value)?;
                context.copy_array_metadata(bound_arg.value, reference);
                for old in method_scope.set_respecting_references(name.clone(), reference, ScopeCellOwnership::Owned) {
                    eval_release_value(context, values, old)?;
                }
            }
            if let Some(target) = bound_arg.ref_target.as_ref() {
                if let Some(reference) = method_scope.visible_cell(name).filter(|_| values.supports_persistent_references()) {
                    context.bind_local_reference_types(reference, &generic_reference_contracts(target, context));
                }
            }
        } else {
            let value = copy_scope_value(bound_arg.value, context, values)?;
            method_scope.set(name.clone(), value, ScopeCellOwnership::Owned);
        }
    }
    alias_duplicate_method_ref_args(method_scope, params, bound_args, context, values)
}

/// Re-binds a GENERATOR's by-value parameters as cells the generator's own scope OWNS.
///
/// A generator body does not run during the call that creates it: PHP evaluates the arguments,
/// binds them, and hands back a `Generator` whose scope OUTLIVES this frame. But
/// [`bind_method_scope_args`] binds every by-value parameter `Borrowed`, and the caller releases
/// its own reference (`release_owned_bound_args`) as soon as the generator object exists. For an
/// argument the CALLER OWNS -- a literal, a concatenation, any temporary -- that release is the
/// last one, and the generator later reads a freed cell:
///
/// ```php
/// function gen($a, $b) { yield $a . '|' . $b; }
/// gen('L1', 'L2');        // yielded '|' -- both literals were gone
/// $x = 'V1'; gen($x, $b); // fine: the caller's VARIABLE kept it alive
/// ```
///
/// That asymmetry is why this only ever reproduced with literal arguments. Twig hits it on every
/// render (`$this->unwrap()->yieldBlock('title', $context, $blocks)` in every compiled template),
/// where the lost `$name` surfaced as `Block "" on template "base.html.twig" does not exist`.
///
/// Retaining here and marking the cell `Owned` is the same rule the `$this` binding at each
/// generator site already follows, for the same reason: net zero against the caller's release,
/// and the generator's own teardown frees it.
///
/// BY-REFERENCE parameters are skipped: they alias caller storage that the caller keeps alive,
/// exactly as `bind_method_scope_args` binds them.
///
/// `owned` is NOT consulted. It records who must release the incoming cell when this call ends,
/// which is a different question from whether the cell survives until the generator runs. A
/// COMPILED caller marks every argument it packs `owned: false` -- the interpreter must not
/// release them -- and then releases the boxed cells itself the instant the bridge returns
/// (`emit_release_eval_boxed_operands`). Skipping those left a compiled `$this->doDisplay($ctx)`
/// handing an eval generator a cell that was freed before the first `foreach` resumed it, which
/// read back as an empty array.
pub(in crate::interpreter) fn retain_generator_scope_args(
    generator_scope: &mut ElephcEvalScope,
    params: &[String],
    parameter_is_by_ref: &[bool],
    bound_args: &[BoundMethodArg],
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for (position, (name, bound_arg)) in params.iter().zip(bound_args.iter()).enumerate() {
        if parameter_is_by_ref.get(position).copied().unwrap_or(false) {
            continue;
        }
        // An array argument already got its own cell from `separate_array_scope_args`.
        if generator_scope
            .entry(name)
            .is_some_and(|entry| entry.flags().ownership == ScopeCellOwnership::Owned)
        {
            continue;
        }
        let retained = values.retain(bound_arg.value)?;
        generator_scope.set(name.clone(), retained, ScopeCellOwnership::Owned);
    }
    Ok(())
}

/// Gives every by-value ARRAY parameter its own cell, so writes in the body stay local.
///
/// [`bind_method_scope_args`] binds a by-value parameter as a `Borrowed` view of the caller's
/// cell, adding no reference. An array write then saw a payload nobody else seemed to share and
/// mutated it IN PLACE, so the caller's array changed under it -- PHP passes arrays by value:
///
/// ```php
/// function f(array $p, int $d) { $p[] = $d; if ($d < 2) f($p, $d + 1); return $p; }
/// f([], 0);   // php [0]   interpreter [0, 1, 2]
/// ```
///
/// Symfony's `PhpDumper::collectCircularReferences()` recurses with its `$path` exactly like
/// that, so every loop it found carried nodes from sibling branches: the dumped container lost
/// its circular-reference guards and inlined services cache:clear then asks for by id.
///
/// `copy_value` makes a detached cell that retains the payload, which is what an assignment
/// `$b = $a;` does: the first write finds the payload shared and copies it, and a body that only
/// reads pays one cell. BY-REFERENCE parameters are skipped: they alias the caller on purpose.
pub(in crate::interpreter) fn separate_array_scope_args(
    scope: &mut ElephcEvalScope,
    params: &[String],
    parameter_is_by_ref: &[bool],
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for (position, name) in params.iter().enumerate() {
        if parameter_is_by_ref.get(position).copied().unwrap_or(false) {
            continue;
        }
        let Some(entry) = scope.entry(name) else {
            continue;
        };
        if entry.flags().by_ref || !entry.flags().is_visible() {
            continue;
        }
        let cell = entry.cell();
        if !matches!(values.type_tag(cell)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
            continue;
        }
        let copied = values.copy_value(cell)?;
        if let Some(replaced) = scope.set(name.clone(), copied, ScopeCellOwnership::Owned) {
            values.release(replaced)?;
        }
    }
    Ok(())
}

/// Creates local aliases when two by-reference method parameters point at the same caller variable.
pub(super) fn alias_duplicate_method_ref_args(
    method_scope: &mut ElephcEvalScope,
    params: &[String],
    bound_args: &[BoundMethodArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for (position, bound_arg) in bound_args.iter().enumerate() {
        let Some(target) = bound_arg.ref_target.as_ref() else {
            continue;
        };
        let Some(param) = params.get(position) else {
            continue;
        };
        for previous_position in 0..position {
            let Some(previous_target) = bound_args[previous_position].ref_target.as_ref() else {
                continue;
            };
            if !same_method_ref_target(target, previous_target, values)? {
                continue;
            }
            if let Some(previous_param) = params.get(previous_position) {
                let replaced = method_scope.set_reference(
                    param.clone(),
                    previous_param.clone(),
                    bound_args[previous_position].value,
                    ScopeCellOwnership::Borrowed,
                );
                for old in replaced { eval_release_value(context, values, old)?; }
            }
            break;
        }
    }
    Ok(())
}

/// Returns true when two evaluated arguments target the same caller-side variable.
pub(super) fn same_method_ref_target(left: &EvalReferenceTarget, right: &EvalReferenceTarget, values: &mut impl RuntimeValueOps) -> Result<bool, EvalStatus> {
    Ok(match (left, right) {
        (
            EvalReferenceTarget::Variable {
                scope: left_scope,
                name: left_name,
            },
            EvalReferenceTarget::Variable {
                scope: right_scope,
                name: right_name,
            },
        ) => left_scope == right_scope && left_name == right_name,
        (
            EvalReferenceTarget::ArrayElement {
                scope: left_scope,
                array_name: left_name,
                index: left_index,
            },
            EvalReferenceTarget::ArrayElement {
                scope: right_scope,
                array_name: right_name,
                index: right_index,
            },
        ) => left_scope == right_scope && left_name == right_name && same_reference_index(left_index, right_index),
        (
            EvalReferenceTarget::NestedArrayElement {
                array_target: left_target,
                index: left_index,
            },
            EvalReferenceTarget::NestedArrayElement {
                array_target: right_target,
                index: right_index,
            },
        ) => same_reference_index(left_index, right_index) && same_method_ref_target(left_target, right_target, values)?,
        (
            EvalReferenceTarget::ObjectProperty {
                object: left_object,
                property: left_property,
                access_scope: left_access_scope,
            },
            EvalReferenceTarget::ObjectProperty {
                object: right_object,
                property: right_property,
                access_scope: right_access_scope,
            },
        ) => {
            left_object == right_object
                && left_property == right_property
                && left_access_scope == right_access_scope
        }
        (
            EvalReferenceTarget::Cell { cell: left_cell },
            EvalReferenceTarget::Cell { cell: right_cell },
        ) => left_cell == right_cell,
        (
            EvalReferenceTarget::InvokerSlot {
                slot: left_slot,
                source_tag: left_source_tag,
            },
            EvalReferenceTarget::InvokerSlot {
                slot: right_slot,
                source_tag: right_source_tag,
            },
        ) => left_slot == right_slot && left_source_tag == right_source_tag,
        (
            EvalReferenceTarget::StaticProperty {
                class_name: left_class_name,
                property: left_property,
                access_scope: left_access_scope,
            },
            EvalReferenceTarget::StaticProperty {
                class_name: right_class_name,
                property: right_property,
                access_scope: right_access_scope,
            },
        ) => {
            left_class_name == right_class_name
                && left_property == right_property
                && left_access_scope == right_access_scope
        }
        _ => false,
    })
}

/// Compares normalized PHP array keys instead of the identity of their temporary boxes.
fn same_reference_index(left: &EvalArrayReferenceKey, right: &EvalArrayReferenceKey) -> bool {
    // Keys are normalized at capture time, so equal PHP keys already compare equal here.
    left == right
}

/// Writes completed by-reference method parameter values back to their caller-side variables.
pub(in crate::interpreter) fn write_back_method_ref_args(
    params: &[String],
    bound_args: &[BoundMethodArg],
    method_scope: &ElephcEvalScope,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for (position, bound_arg) in bound_args.iter().enumerate() {
        let Some(param) = params.get(position) else {
            continue;
        };
        if let Some(original_target) = bound_arg.ref_target.as_ref() {
            let Some(entry) = method_scope
                .entry(param)
                .filter(|entry| entry.flags().is_visible() && entry.flags().by_ref)
            else {
                continue;
            };
            // A parameter rebound with `$param = &$other[...]` no longer aliases the caller's
            // variable: what it holds now belongs to whatever it was rebound to, and the
            // scope's CURRENT reference target names that storage. Writing it over the original
            // turned `Arr::set($config, 'a.b', 1)` into `$config = ['b' => 1]`.
            let target = method_scope.reference_target(param).unwrap_or(original_target);
            write_back_method_ref_target(target, entry.cell(), context, values)?;
        }
        write_back_method_variadic_ref_args(param, bound_arg, method_scope, context, values)?;
    }
    Ok(())
}

/// Writes element-level changes from a by-reference variadic method parameter back to callers.
pub(super) fn write_back_method_variadic_ref_args(
    param: &str,
    bound_arg: &BoundMethodArg,
    method_scope: &ElephcEvalScope,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if bound_arg.variadic_ref_targets.is_empty() {
        return Ok(());
    }
    let Some(entry) = method_scope
        .entry(param)
        .filter(|entry| entry.flags().is_visible() && entry.flags().by_ref)
    else {
        return Ok(());
    };
    if entry.cell() != bound_arg.value {
        return Ok(());
    }
    for (key, target) in &bound_arg.variadic_ref_targets {
        let value = values.array_get(entry.cell(), *key)?;
        write_back_method_ref_target(target, value, context, values)?;
    }
    Ok(())
}

/// Gives a persistent variable reference its own value owner before releasing displaced storage.
pub(super) fn write_back_owned_variable_ref_target(
    scope: &mut ElephcEvalScope,
    name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let retained = values.retain(value)?;
    let replaced = set_owned_scope_cell(context, scope, name.to_string(), retained, values)?;
    let mut result = Ok(());
    for replaced in replaced {
        let released = eval_release_value(context, values, replaced);
        if result.is_ok() { result = released; }
    }
    result
}

/// Stores one by-reference result in the original caller-side target.
pub(in crate::interpreter) fn write_back_method_ref_target(
    target: &EvalReferenceTarget,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match target {
        EvalReferenceTarget::Variable { scope, name } => {
            // The defining activation has returned -- a closure's `use (&$x)` outliving the
            // function that declared `$x`. No variable is left to update; the closure's own
            // binding is the only holder of the value now.
            let Some(scope) = (unsafe { scope.live_mut() }) else {
                return Ok(());
            };
            if visible_scope_cell(context, scope, name) == Some(value) {
                // Native writeback can mutate the existing cell in place. Do
                // not demote its scope owner to a borrow and release that owner.
                return Ok(());
            }
            // The named variable can itself BE a reference -- the outer loop variable of a
            // by-reference `foreach` aliasing an array element, a captured `use (&$x)` closure
            // parameter, or a property reference alias -- in which case this scope cell is not
            // what the write is really for. A write that stops here is invisible to whatever the
            // variable actually names, so it must keep following the chain first, exactly as a
            // plain `$name = value;` statement already does for its own target (`StoreVar` in
            // `crate::interpreter::statements::dispatch`). This is what a nested by-reference
            // `foreach` needed: the inner loop's subject resolves to a `Variable` target naming
            // the outer loop variable, and without this the inner write reached only that
            // variable's own local cell, never the array element it aliases.
            if let Some(nested_target) = scope.reference_target(name).cloned() {
                write_back_method_ref_target(&nested_target, value, context, values)?;
            }
            let stored = values.retain(value)?;
            let replaced = match set_scope_cell(
                context,
                scope,
                name.clone(),
                stored,
                ScopeCellOwnership::Owned,
                values,
            ) {
                Ok(replaced) => replaced,
                Err(status) => {
                    let _ = values.release(stored);
                    return Err(status);
                }
            };
            for replaced in replaced {
                values.release(replaced)?;
            }
            Ok(())
        }
        EvalReferenceTarget::ArrayElement {
            scope,
            array_name,
            index,
        } => {
            let Some(scope) = (unsafe { scope.live_mut() }) else {
                return Ok(());
            };
            write_back_method_array_element_ref_target(
                scope, array_name, index, value, context, values,
            )
        }
        EvalReferenceTarget::NestedArrayElement {
            array_target,
            index,
        } => write_back_method_nested_array_element_ref_target(
            array_target,
            index,
            value,
            context,
            values,
        ),
        EvalReferenceTarget::ObjectProperty {
            object,
            property,
            access_scope,
        } => write_back_method_object_property_ref_target(
            *object,
            property,
            access_scope.clone(),
            value,
            context,
            values,
        ),
        EvalReferenceTarget::StaticProperty {
            class_name,
            property,
            access_scope,
        } => write_back_method_static_property_ref_target(
            class_name,
            property,
            access_scope.clone(),
            value,
            context,
            values,
        ),
        EvalReferenceTarget::StaticLocal { function, name } => {
            // The store keeps this until the NEXT call, so it takes a reference of its own and
            // gives back the one it displaces -- the same rule `persist_static_locals` applies.
            let stored = values.retain(value)?;
            if let Some(replaced) =
                context.set_static_local(function.clone(), name.clone(), stored)
            {
                values.release(replaced)?;
            }
            Ok(())
        }
        EvalReferenceTarget::Cell { cell } => {
            for declared in context.local_reference_types(*cell) { validate_generic_local(&declared, "reference", value, context, values)?; }
            if *cell != value && values.is_reference(*cell)? {
                let previous = values.reference_replace(*cell, value)?;
                context.copy_array_metadata(value, *cell);
                eval_release_value(context, values, previous)?;
            }
            Ok(())
        },
        EvalReferenceTarget::InvokerSlot { slot, source_tag } => {
            write_back_invoker_slot_ref_target(*slot, *source_tag, value, values)
        }
    }
}

/// Reads a value from a native descriptor-invoker by-reference slot.
pub(in crate::interpreter) fn eval_invoker_slot_ref_target_value(
    slot: usize,
    source_tag: u64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match source_tag {
        EVAL_TAG_INT | EVAL_TAG_FLOAT | EVAL_TAG_BOOL | EVAL_TAG_RESOURCE => {
            let word = unsafe { *(slot as *const u64) };
            values.raw_word_value(source_tag, word)
        }
        EVAL_TAG_STRING => {
            let words = unsafe { *(slot as *const [u64; 2]) };
            values.raw_string_value(words[0], words[1])
        }
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_OBJECT | EVAL_TAG_CALLABLE => {
            let word = unsafe { *(slot as *const u64) };
            raw_slot_word_value(source_tag, word, values)
        }
        EVAL_TAG_MIXED => {
            let value = RuntimeCellHandle::from_raw(unsafe {
                *(slot as *const *mut crate::value::RuntimeCell)
            });
            values.retain(value)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Writes a value back into a native descriptor-invoker by-reference slot.
pub(super) fn write_back_invoker_slot_ref_target(
    slot: usize,
    source_tag: u64,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match source_tag {
        EVAL_TAG_INT | EVAL_TAG_FLOAT | EVAL_TAG_BOOL | EVAL_TAG_RESOURCE => {
            let word = values.raw_value_word(value)?;
            unsafe {
                *(slot as *mut u64) = word;
            }
            Ok(())
        }
        EVAL_TAG_STRING => write_back_invoker_string_slot(slot, value, values),
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_OBJECT | EVAL_TAG_CALLABLE => {
            write_back_invoker_heap_slot(slot, source_tag, value, values)
        }
        EVAL_TAG_MIXED => {
            let retained = values.retain(value)?;
            let replaced = unsafe {
                let slot = slot as *mut *mut crate::value::RuntimeCell;
                let replaced = RuntimeCellHandle::from_raw(*slot);
                *slot = retained.as_ptr();
                replaced
            };
            values.release(replaced)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Writes a boxed string value back into a native descriptor-invoker string slot.
pub(super) fn write_back_invoker_string_slot(
    slot: usize,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if values.type_tag(value)? != EVAL_TAG_STRING {
        return Err(EvalStatus::RuntimeFatal);
    }
    let ptr = values.raw_value_word(value)?;
    let len = values.raw_value_high_word(value)?;
    let retained = values.retain_raw_string_words(ptr, len)?;
    let replaced = unsafe {
        let slot = slot as *mut [u64; 2];
        let replaced = *slot;
        *slot = [retained.0, retained.1];
        replaced
    };
    values.release_raw_string_words(replaced[0], replaced[1])
}

/// Writes a boxed heap value back into a native descriptor-invoker raw heap slot.
pub(super) fn write_back_invoker_heap_slot(
    slot: usize,
    source_tag: u64,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let value_tag = values.type_tag(value)?;
    let array_shape_changed = matches!(source_tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC)
        && matches!(value_tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC);
    if value_tag != source_tag && !array_shape_changed {
        return Err(EvalStatus::RuntimeFatal);
    }
    let word = values.raw_value_word(value)?;
    let retained = values.retain_raw_heap_word(word)?;
    let replaced = unsafe {
        let slot = slot as *mut u64;
        let replaced = *slot;
        *slot = retained;
        replaced
    };
    values.release_raw_heap_word(replaced)
}

/// Stores one by-reference method result in a caller-side array element.
/// Preserves the receiver's array metadata before writing the clone back to shared storage.
pub(super) fn write_back_method_array_element_ref_target(
    scope: &mut ElephcEvalScope,
    array_name: &str,
    index: &EvalArrayReferenceKey,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let index = eval_array_reference_key_value(index, values)?;
    let result = write_back_method_array_element_ref_target_value(
        scope, array_name, index, value, context, values,
    );
    values.release(index)?;
    result
}

/// Stores one by-reference result using an already materialized array-key cell.
fn write_back_method_array_element_ref_target_value(
    scope: &mut ElephcEvalScope,
    array_name: &str,
    index: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let Some(current) = visible_scope_cell(context, scope, array_name) {
        for declared in generic_local_contracts(scope, array_name, context) {
            validate_generic_local_element(Some(&declared), current, index, value, context, values)?;
        }
    }
    // The rebuilt array is a fresh owner whatever the slot held before, a borrowed by-reference
    // parameter included: recording it `Borrowed` left nothing to release it.
    let ownership = ScopeCellOwnership::Owned;
    let array = if let Some(existing) =
        scope_entry(context, scope, array_name).filter(|entry| entry.flags().is_visible())
    {
        if values.is_array_like(existing.cell())? {
            let array = values.array_clone_shallow(existing.cell())?;
            context.copy_array_metadata(existing.cell(), array);
            array
        } else {
            eval_new_array_for_index(index, values)?
        }
    } else {
        eval_new_array_for_index(index, values)?
    };
    let array = values.array_set(array, index, value)?;
    for replaced in set_scope_cell(context, scope, array_name.to_string(), array, ownership, values)? {
        values.release(replaced)?;
    }
    Ok(())
}

/// Stores one by-reference method result in an element of a nested caller-side array target.
/// Copies the live receiver's metadata before replacing a nested persistent reference.
pub(super) fn write_back_method_nested_array_element_ref_target(
    array_target: &EvalReferenceTarget,
    index: &EvalArrayReferenceKey,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let index = eval_array_reference_key_value(index, values)?;
    let result = write_back_method_nested_array_element_ref_target_value(
        array_target, index, value, context, values,
    );
    values.release(index)?;
    result
}

/// Whether a reference target is rooted in an activation scope that no longer exists.
fn reference_target_root_scope_is_dead(target: &EvalReferenceTarget) -> bool {
    match target {
        EvalReferenceTarget::Variable { scope, .. }
        | EvalReferenceTarget::ArrayElement { scope, .. } => {
            unsafe { scope.live_mut() }.is_none()
        }
        EvalReferenceTarget::NestedArrayElement { array_target, .. } => {
            reference_target_root_scope_is_dead(array_target)
        }
        _ => false,
    }
}

/// Stores one by-reference result using an already materialized nested array key.
fn write_back_method_nested_array_element_ref_target_value(
    array_target: &EvalReferenceTarget,
    index: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    // The activation the chain is rooted in has returned (a closure's `use (&$x)` outliving the
    // method whose `foreach (... as &$x)` bound it): there is no variable left to update, exactly
    // as for the plain `Variable` and `ArrayElement` targets, and the reading below would turn
    // that into a fatal.
    if reference_target_root_scope_is_dead(array_target) {
        return Ok(());
    }
    let current = eval_reference_target_value(array_target, context, values)?;
    let array = if values.is_array_like(current)? {
        values.array_clone_shallow(current).map(|array| {
            context.copy_array_metadata(current, array);
            array
        })
    } else {
        eval_new_array_for_index(index, values)
    };
    // A property or element read hands back an owner of its own; a variable read only borrows.
    if !current.is_borrowed() {
        values.release(current)?;
    }
    let array = values.array_set(array?, index, value)?;
    let written = write_back_method_ref_target(array_target, array, context, values);
    // Every target but a static property keeps a reference of its own to the rebuilt array.
    if !matches!(array_target, EvalReferenceTarget::StaticProperty { .. }) {
        values.release(array)?;
    }
    written
}

/// Stores one by-reference method result in a caller-side object property.
pub(super) fn write_back_method_object_property_ref_target(
    object: RuntimeCellHandle,
    property: &str,
    access_scope: ElephcEvalExecutionScope,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let previous_scope = context.replace_execution_scope(access_scope);
    let result = eval_property_set_result(object, property, value, context, values);
    context.replace_execution_scope(previous_scope);
    result
}

/// Stores one by-reference method result in a caller-side static property.
pub(super) fn write_back_method_static_property_ref_target(
    class_name: &str,
    property: &str,
    access_scope: ElephcEvalExecutionScope,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let previous_scope = context.replace_execution_scope(access_scope);
    let result = eval_static_property_set_result(class_name, property, value, context, values);
    context.replace_execution_scope(previous_scope);
    result
}

/// Creates an indexed or associative array according to the first write key.
pub(super) fn eval_new_array_for_index(
    index: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(index)? == EVAL_TAG_STRING {
        values.assoc_new(1)
    } else {
        values.array_new(1)
    }
}
