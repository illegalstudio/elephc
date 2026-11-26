//! Purpose:
//! Executes static-property, class-constant, and static reference operations.
//!
//! Called from:
//! - Expression and statement dispatch for class-member access.
//!
//! Key details:
//! - Receiver resolution, hooks, enum/reflection constants, and reference aliases share one path.
//! - A static-property access initializes an eval class first (instance and static defaults
//!   evaluated again for their deprecation notices, once), and a class constant's first read
//!   evaluates its initializer again, as php-src evaluates those initializers lazily.

use super::*;
use crate::interpreter::builtins::collection_builder::with_collection_cells;
use crate::runtime_hooks::owner_frames::{
    RuntimeOwnerSlot, StagedRuntimeOwners, with_runtime_owners,
};
use elephc_memoir::runtime_owners::release_staged_cells;

/// Raises native initialization failures after access checks, without invoking a throwing getter.
/// A static-property access initializes the class, so its deprecation notices come first.
fn validate_native_static_class_defaults(
    class: &str, declaring: &str, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_raise_class_initialization_deprecations(class, context, values)?;
    let message = context.native_class_default_error(class)
        .or_else(|| context.native_class_default_error(declaring)).map(str::to_owned)
        .or_else(|| context.class_native_parent_name(class).and_then(|parent|
            context.native_class_default_error(&parent).map(str::to_owned)));
    if let Some(message) = message { return throwables::eval_throw_error(&message, context, values); }
    Ok(())
}

/// Stores a borrowed reflection argument under an independent static-property owner.
/// Same-cell writes preserve the existing owner; replacements release the displaced value.
pub(in crate::interpreter) fn store_borrowed_static_property(
    class_name: &str,
    property_name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_reject_null_object_static_property(class_name, property_name, value, context, values)?;
    let value = match context.static_property_alias(class_name, property_name) {
        Some(EvalReferenceTarget::Cell { cell }) if values.is_reference(*cell)? => *cell,
        _ => value,
    };
    if context.static_property(class_name, property_name) == Some(value) {
        return Ok(());
    }
    let retained = values.retain(value)?;
    if let Some(previous) = context.set_static_property(class_name, property_name, retained) {
        eval_release_value(context, values, previous)?;
    }
    Ok(())
}

/// Transfers a fresh value to static storage or acquires an independent owner for a storage borrow.
/// In-place mutation results retain the existing slot owner when their cell identity is unchanged.
pub(in crate::interpreter) fn store_static_property_value(
    class_name: &str,
    property_name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if value.is_borrowed() {
        return store_borrowed_static_property(class_name, property_name, value, context, values);
    }
    if let Some(previous) = context.set_static_property(class_name, property_name, value) {
        eval_release_value(context, values, previous)?;
    }
    Ok(())
}

/// Reads one eval-declared static property after resolving the class-like receiver.
pub(in crate::interpreter) fn eval_static_property_get_result(
    class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_static_property_get_result_with_ownership(class_name, property_name, context, values, None)
}

/// Reads a property while optionally retaining reference-backed values for a native caller.
pub(in crate::interpreter) fn eval_static_property_get_result_with_ownership(
    class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    owned: Option<&StagedRuntimeOwners>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    if let Some((declaring_class, property)) = context.class_property(&class_name, property_name) {
        if !property.is_static() {
            return eval_throw_undeclared_static_property_error(
                &class_name,
                property_name,
                context,
                values,
            );
        }
        if validate_eval_member_access(&declaring_class, property.visibility(), context).is_err() {
            return eval_throw_property_access_error(
                &declaring_class,
                property.name(),
                property.visibility(),
                context,
                values,
            );
        }
        validate_native_static_class_defaults(&class_name, &declaring_class, context, values)?;
        if let Some(target) = context
            .static_property_alias(&declaring_class, property.name())
            .cloned()
        {
            return match owned {
                Some(owners) => eval_owned_reference_target_value(&target, context, values, owners),
                None => eval_reference_target_value(&target, context, values),
            };
        }
        if let Some(value) = context.static_property(&declaring_class, property.name()) {
            return if owned.is_some() { values.retain(value) } else { Ok(value) };
        }
        return eval_throw_uninitialized_static_property_error(
            &declaring_class,
            property.name(),
            context,
            values,
        );
    }
    if eval_static_member_context_owns_class(&class_name, context) {
        if let Some(parent) = context.class_native_parent_name(&class_name) {
            if let Some((declaring_class, visibility, _, is_static)) =
                eval_reflection_aot_static_property_access_metadata(
                    &parent,
                    property_name,
                    context,
                    values,
                )?
            {
                if !is_static {
                    return eval_throw_undeclared_static_property_error(
                        &class_name,
                        property_name,
                        context,
                        values,
                    );
                }
                if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
                    return eval_throw_property_access_error(
                        &declaring_class,
                        property_name,
                        visibility,
                        context,
                        values,
                    );
                }
                validate_native_static_class_defaults(&class_name, &declaring_class, context, values)?;
                if let Some(target) = context
                    .static_property_alias(&declaring_class, property_name)
                    .cloned()
                {
                    return match owned {
                        Some(owners) => eval_owned_reference_target_value(&target, context, values, owners),
                        None => eval_reference_target_value(&target, context, values),
                    };
                }
                if !eval_with_native_bridge_scope(&declaring_class, context, values, |values| {
                    values.static_property_is_initialized(&declaring_class, property_name)
                })? {
                    return eval_throw_uninitialized_static_property_error(
                        &declaring_class,
                        property_name,
                        context,
                        values,
                    );
                }
                if let Some(value) = eval_with_native_bridge_scope(
                    &declaring_class,
                    context,
                    values, |values| values.static_property_get(&declaring_class, property_name),
                )? {
                    return Ok(value);
                }
            }
        }
        return eval_throw_undeclared_static_property_error(
            &class_name,
            property_name,
            context,
            values,
        );
    }
    if let Some((declaring_class, visibility, _, is_static)) =
        eval_reflection_aot_static_property_access_metadata(
            &class_name,
            property_name,
            context,
            values,
        )?
    {
        if is_static {
            if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
                return eval_throw_property_access_error(
                    &declaring_class,
                    property_name,
                    visibility,
                    context,
                    values,
                );
            }
            validate_native_static_class_defaults(&class_name, &declaring_class, context, values)?;
            if let Some(target) = context
                .static_property_alias(&declaring_class, property_name)
                .cloned()
            {
                return match owned {
                    Some(owners) => eval_owned_reference_target_value(&target, context, values, owners),
                    None => eval_reference_target_value(&target, context, values),
                };
            }
            if !eval_with_native_bridge_scope(&declaring_class, context, values, |values| {
                values.static_property_is_initialized(&declaring_class, property_name)
            })? {
                return eval_throw_uninitialized_static_property_error(
                    &declaring_class,
                    property_name,
                    context,
                    values,
                );
            }
            if let Some(value) = eval_with_native_bridge_scope(&declaring_class, context, values,
                |values| values.static_property_get(&class_name, property_name))?
            {
                return Ok(value);
            }
            return eval_throw_undeclared_static_property_error(&class_name, property_name, context, values);
        }
    }
    if let Some(value) = values.static_property_get(&class_name, property_name)? {
        return Ok(value);
    }
    if eval_runtime_class_like_exists(&class_name, context, values)? {
        eval_throw_undeclared_static_property_error(&class_name, property_name, context, values)
    } else {
        eval_throw_class_not_found_error(&class_name, context, values)
    }
}

/// Returns whether a static property is PHP-`isset()` without throwing for missing properties.
pub(in crate::interpreter) fn eval_static_property_isset_result(
    class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    if let Some((declaring_class, property)) = context.class_property(&class_name, property_name) {
        if !property.is_static() {
            return Ok(false);
        }
        if validate_eval_member_access(&declaring_class, property.visibility(), context).is_err() {
            return Ok(false);
        }
        validate_native_static_class_defaults(&class_name, &declaring_class, context, values)?;
        let value = if let Some(target) = context
            .static_property_alias(&declaring_class, property.name())
            .cloned()
        {
            eval_reference_target_value(&target, context, values)?
        } else {
            let Some(value) = context.static_property(&declaring_class, property.name()) else {
                return Ok(false);
            };
            value
        };
        return eval_static_isset_value(value, context, values);
    }
    if eval_static_member_context_owns_class(&class_name, context) {
        if let Some(parent) = context.class_native_parent_name(&class_name) {
            if let Some((declaring_class, visibility, _, is_static)) =
                eval_reflection_aot_static_property_access_metadata(
                    &parent,
                    property_name,
                    context,
                    values,
                )?
            {
                if !is_static {
                    return Ok(false);
                }
                if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
                    return Ok(false);
                }
                validate_native_static_class_defaults(&class_name, &declaring_class, context, values)?;
                if let Some(target) = context
                    .static_property_alias(&declaring_class, property_name)
                    .cloned()
                {
                    let value = eval_reference_target_value(&target, context, values)?;
                    return eval_static_isset_value(value, context, values);
                }
                if !eval_with_native_bridge_scope(&declaring_class, context, values, |values| {
                    values.static_property_is_initialized(&declaring_class, property_name)
                })? {
                    return Ok(false);
                }
                if let Some(value) = eval_with_native_bridge_scope(
                    &declaring_class,
                    context,
                    values, |values| values.static_property_get(&declaring_class, property_name),
                )? {
                    return eval_static_isset_value(value, context, values);
                }
            }
        }
        return Ok(false);
    }
    if let Some((declaring_class, visibility, _, is_static)) =
        eval_reflection_aot_static_property_access_metadata(
            &class_name,
            property_name,
            context,
            values,
        )?
    {
        if !is_static {
            return Ok(false);
        }
        if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
            return Ok(false);
        }
        validate_native_static_class_defaults(&class_name, &declaring_class, context, values)?;
        if let Some(target) = context
            .static_property_alias(&declaring_class, property_name)
            .cloned()
        {
            let value = eval_reference_target_value(&target, context, values)?;
            return eval_static_isset_value(value, context, values);
        }
        if !eval_with_native_bridge_scope(&declaring_class, context, values, |values| {
            values.static_property_is_initialized(&declaring_class, property_name)
        })? {
            return Ok(false);
        }
        let value = eval_with_native_bridge_scope(&declaring_class, context, values,
            |values| values.static_property_get(&class_name, property_name))?;
        return match value { Some(value) => eval_static_isset_value(value, context, values), None => Ok(false) };
    } else if !eval_runtime_class_like_exists(&class_name, context, values)? {
        return eval_throw_class_not_found_error(&class_name, context, values);
    }
    if let Some(value) = values.static_property_get(&class_name, property_name)? {
        return eval_static_isset_value(value, context, values);
    }
    Ok(false)
}

/// Probes an acquired static value and retires its read lease before returning the boolean.
fn eval_static_isset_value(
    value: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let present = values.is_null(value).map(|null| !null);
    let released = release_expr_result(value, context, values);
    present.and_then(|present| released.map(|()| present))
}

/// Throws PHP's catchable error for attempts to unset an existing static property target.
pub(in crate::interpreter) fn eval_static_property_unset_result(
    class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    if !eval_runtime_class_like_exists(&class_name, context, values)? {
        return eval_throw_class_not_found_error(&class_name, context, values);
    }
    eval_throw_error(
        &format!(
            "Attempt to unset static property {}::${}",
            class_name.trim_start_matches('\\'),
            property_name
        ),
        context,
        values,
    )
}

/// Initializes an eval-declared class (and its ancestors first, from the root, each once) the
/// way php-src does on the first instantiation, static-property access, or reflection use:
/// the constants of its table not evaluated yet, then its flattened instance defaults
/// (inherited slots included, a non-private redeclaration replacing its parent's slot), then
/// the static defaults it declares itself, own declarations before trait imports. The values
/// are stored: statics in their cells, instance defaults in the class's table that each new
/// object copies. An instance slot the class copied already evaluated when it was declared is
/// evaluated silently. If an initializer throws, the class stays uninitialized, so the next
/// use retries, as in php-src.
pub(in crate::interpreter) fn eval_raise_class_initialization_deprecations(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if context.deprecations_silenced() {
        return Ok(());
    }
    let chain = context.class_chain(class_name);
    for (index, class) in chain.iter().enumerate() {
        if !context.begin_class_initialization(class.name()) {
            continue;
        }
        let result = eval_initialize_class(&chain[..=index], class, context, values);
        if result.is_err() {
            context.abort_class_initialization(class.name());
        }
        result?;
    }
    Ok(())
}

/// Evaluates and stores one class's constants, instance defaults, and static defaults.
fn eval_initialize_class(
    chain: &[EvalClass],
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_class_constant_table_evaluation(class.name(), context, values)?;
    for (declaring, property) in eval_flattened_property_slots(chain, false, true) {
        let Some(default) = property.default() else { continue };
        let precopied = context.slot_precopied(class.name(), declaring.name(), property.name());
        if precopied {
            context.begin_deprecation_silence();
        }
        let mut default_scope = ElephcEvalScope::new();
        let value = eval_class_like_member_default(
            declaring.name(),
            property.trait_origin(),
            default,
            context,
            &mut default_scope,
            values,
        );
        if precopied {
            context.end_deprecation_silence();
        }
        let value = value?;
        let value = if value.is_borrowed() { values.retain(value)? } else { value };
        let storage_name = eval_instance_property_storage_name(declaring.name(), property);
        if let Some(replaced) = context.set_instance_default_cell(class.name(), storage_name, value) {
            eval_release_value(context, values, replaced)?;
        }
    }
    for property in eval_initialization_ordered_properties(class)
        .filter(|property| property.is_static() && !property.is_abstract())
    {
        let Some(default) = property.default() else { continue };
        let mut default_scope = ElephcEvalScope::new();
        let value = eval_class_like_member_default(
            class.name(),
            property.trait_origin(),
            default,
            context,
            &mut default_scope,
            values,
        )?;
        // A static already stored (a reference bound to it, a write through a path that
        // initialized the class) keeps its value.
        if context.static_property(class.name(), property.name()).is_some()
            || context.static_property_alias(class.name(), property.name()).is_some()
        {
            if !value.is_borrowed() {
                values.release(value)?;
            }
            continue;
        }
        store_static_property_value(class.name(), property.name(), value, context, values)?;
    }
    Ok(())
}

/// Returns the property slots the last class of `chain` (root first) exposes, in php-src's
/// slot order: an ancestor's slots first, a non-private redeclaration replacing the slot it
/// overrides in place, and any other property appended. `statics` selects static or instance
/// properties; `ancestor_private` keeps an ancestor's private properties (instance defaults
/// keep their separate private slots, while statics visible on the class do not include them).
fn eval_flattened_property_slots(
    chain: &[EvalClass],
    statics: bool,
    ancestor_private: bool,
) -> Vec<(&EvalClass, &EvalClassProperty)> {
    let mut slots: Vec<(&EvalClass, &EvalClassProperty)> = Vec::new();
    let last = chain.len().saturating_sub(1);
    for (position, class) in chain.iter().enumerate() {
        for property in eval_initialization_ordered_properties(class)
            .filter(|property| property.is_static() == statics && !property.is_abstract())
        {
            let private = property.visibility() == EvalVisibility::Private;
            if private && position != last && !ancestor_private {
                continue;
            }
            let replaced = (!private)
                .then(|| {
                    slots.iter().position(|(_, existing)| {
                        existing.visibility() != EvalVisibility::Private
                            && existing.name() == property.name()
                    })
                })
                .flatten();
            match replaced {
                Some(index) => slots[index] = (class, property),
                None => slots.push((class, property)),
            }
        }
    }
    slots
}

/// Returns a class's own property declarations in php-src's initialization order: its own
/// declarations in source order, then the ones imported from traits.
fn eval_initialization_ordered_properties(
    class: &EvalClass,
) -> impl Iterator<Item = &EvalClassProperty> {
    let own = class.properties().iter().filter(|property| property.trait_origin().is_none());
    let imported = class.properties().iter().filter(|property| property.trait_origin().is_some());
    own.chain(imported)
}

/// Returns the static slots `ReflectionClass::getDefaultProperties()` re-evaluates on every
/// call: walking from the reflected class (last in `chain`) toward the root, each class's own
/// statics in initialization order, a name a descendant already covered skipped, and an
/// ancestor's private static left out.
fn eval_reflected_static_slots(chain: &[EvalClass]) -> Vec<(&EvalClass, &EvalClassProperty)> {
    let mut slots: Vec<(&EvalClass, &EvalClassProperty)> = Vec::new();
    for (position, class) in chain.iter().enumerate().rev() {
        let reflected = position + 1 == chain.len();
        for property in eval_initialization_ordered_properties(class)
            .filter(|property| property.is_static() && !property.is_abstract())
        {
            let private = property.visibility() == EvalVisibility::Private;
            if (private && !reflected)
                || slots.iter().any(|(_, existing)| existing.name() == property.name())
            {
                continue;
            }
            slots.push((class, property));
        }
    }
    slots
}

/// Evaluates each slot's default and releases an owned value, for the notices it raises.
fn eval_property_slot_defaults(
    slots: &[(&EvalClass, &EvalClassProperty)],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for (class, property) in slots {
        let Some(default) = property.default() else { continue };
        let mut default_scope = ElephcEvalScope::new();
        let value = eval_class_like_member_default(
            class.name(),
            property.trait_origin(),
            default,
            context,
            &mut default_scope,
            values,
        )?;
        // A constant fetch lends the stored cell; only an owned result is released.
        if !value.is_borrowed() {
            values.release(value)?;
        }
    }
    Ok(())
}

/// Raises, on every `ReflectionClass::getDefaultProperties()` call, the notices of the static
/// property defaults visible on the reflected eval class (its own, and the non-private ones it
/// inherits without redeclaring): php-src evaluates each of them again for the returned array,
/// besides the class initialization that call also performs.
pub(in crate::interpreter) fn eval_raise_reflected_static_default_deprecations(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if context.deprecations_silenced() {
        return Ok(());
    }
    let chain = context.class_chain(class_name);
    let slots = eval_reflected_static_slots(&chain);
    eval_property_slot_defaults(&slots, context, values)
}

/// Raises, on every `get_class_vars()` call, the notices of the static property defaults it
/// returns for an eval class: the statics visible on the class from the calling scope, the
/// class's own first, each evaluated again as php-src copies it into the result.
pub(in crate::interpreter) fn eval_raise_visible_static_default_deprecations(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if context.deprecations_silenced() {
        return Ok(());
    }
    let chain = context.class_chain(class_name);
    let slots: Vec<(&EvalClass, &EvalClassProperty)> = eval_reflected_static_slots(&chain)
        .into_iter()
        .filter(|(class, property)| validate_eval_member_access(class.name(), property.visibility(), context).is_ok())
        .collect();
    eval_property_slot_defaults(&slots, context, values)
}

/// Raises PHP's `E_DEPRECATED` for a read of a deprecated builtin class constant
/// (`DateTimeInterface::RFC7231`) under the active eval profile, whichever receiver reaches
/// it: the builtin classes themselves, a user subclass declared in eval or compiled ahead of
/// time, `self::`/`static::` (already resolved to the called class here), or an object. The
/// message names the declaring class whichever receiver was written, and a subclass that
/// redeclares the constant reads its own, silent value.
fn eval_report_class_constant_deprecation(
    class_name: &str,
    constant_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !elephc_builtin_contract::is_deprecated_class_constant_name(constant_name) {
        return Ok(());
    }
    // Only a fetch that reaches the builtin declaration deprecates: a constant eval code
    // declares (a class reusing a builtin's name included) is the user's, and a class the
    // program does not have (an extension prelude it never loaded) only fails to resolve.
    let from_declaring = |context: &mut ElephcEvalContext, values: &mut _| {
        eval_class_constant_declaring_class(class_name, constant_name, context, values)
            .and_then(|declaring| elephc_builtin_contract::class_constant_deprecation(&declaring, constant_name))
    };
    let deprecation = if context.class_constant(class_name, constant_name).is_some() {
        None
    } else if eval_static_member_context_owns_class(class_name, context) {
        from_declaring(context, values)
    } else if values.class_exists(class_name)? || eval_runtime_interface_exists(class_name, values)? {
        elephc_builtin_contract::class_constant_deprecation(class_name, constant_name)
            .or_else(|| from_declaring(context, values))
    } else {
        None
    };
    let Some(deprecation) = deprecation else {
        return Ok(());
    };
    if deprecation.applies_to_version_id(crate::eval_php_profile::eval_php_version_id())
        && !context.capture_deprecation(deprecation.message)
    {
        eval_dispatch_php_error(deprecation.message, E_DEPRECATED, context, values)?;
    }
    Ok(())
}

/// Returns the class or interface that declares `class_name::constant_name`, looking through
/// eval-declared metadata first and then the generated AOT reflection metadata. A lookup the
/// metadata cannot answer yields `None`; the fetch itself reports a missing constant.
fn eval_class_constant_declaring_class(
    class_name: &str,
    constant_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Option<String> {
    if let Some((declaring_class, _)) = context.class_constant(class_name, constant_name) {
        return Some(declaring_class);
    }
    if eval_static_member_context_owns_class(class_name, context) {
        return eval_dynamic_class_native_constant_metadata(class_name, constant_name, context, values)
            .ok()
            .flatten()
            .map(|(declaring_class, _)| declaring_class);
    }
    values
        .reflection_constant_declaring_class(class_name, constant_name)
        .ok()
        .flatten()
}

/// Reads one eval-declared class constant after resolving the class-like receiver.
pub(in crate::interpreter) fn eval_class_constant_fetch_result(
    class_name: &str,
    constant_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(value) = eval_builtin_reflection_class_constant(class_name, constant_name, values)?
    {
        return Ok(value);
    }
    if let Some(value) =
        eval_builtin_property_hook_type_case(class_name, constant_name, context, values)?
    {
        return Ok(value);
    }
    // Inside a constant expression php-src names the class as this fetch spells it (a
    // self-referencing, undefined, or trait constant); in ordinary code by its declared name.
    let written_class = class_name;
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    let display_class = if context.in_constant_expression() {
        written_class.trim_start_matches('\\').to_string()
    } else {
        eval_class_like_display_name(&class_name, context)
    };
    if context
        .trait_decl(&class_name)
        .is_some_and(|trait_decl| trait_decl.constants().iter().any(|constant| constant.name() == constant_name))
    {
        return eval_throw_error(
            &format!("Cannot access trait constant {display_class}::{constant_name} directly"),
            context,
            values,
        );
    }
    eval_report_class_constant_deprecation(&class_name, constant_name, context, values)?;
    // A backed enum is initialized by the first fetch of any of its constants; a pure enum's
    // case is created on its own first fetch.
    if eval_is_backed_enum(&class_name, context) {
        eval_raise_enum_initialization_deprecations(&class_name, context, values)?;
    }
    if eval_is_enum_case(&class_name, constant_name, context) {
        return eval_fetch_class_constant(written_class, &class_name, constant_name, context, values)?
            .ok_or(EvalStatus::RuntimeFatal);
    }
    if let Some((declaring_class, constant)) = context.class_constant(&class_name, constant_name) {
        if validate_eval_member_access(&declaring_class, constant.visibility(), context).is_err() {
            return eval_throw_constant_access_error(
                &declaring_class,
                constant.name(),
                constant.visibility(),
                context,
                values,
            );
        }
        // php-src evaluates the initializer on the first read and caches the value.
        return eval_fetch_class_constant(written_class, &declaring_class, constant.name(), context, values)?
            .ok_or(EvalStatus::RuntimeFatal);
    }
    if eval_static_member_context_owns_class(&class_name, context) {
        if let Some((declaring_class, visibility)) =
            eval_dynamic_class_native_constant_metadata(
                &class_name,
                constant_name,
                context,
                values,
            )?
        {
            if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
                return eval_throw_constant_access_error(
                    &declaring_class,
                    constant_name,
                    visibility,
                    context,
                    values,
                );
            }
            if let Some(value) = eval_with_native_bridge_scope(
                &declaring_class,
                context,
                values, |values| values.class_constant_get(&declaring_class, constant_name),
            )? {
                return Ok(value);
            }
        }
        return eval_throw_error(&format!("Undefined constant {display_class}::{constant_name}"), context, values);
    }
    if let Some(value) = values.class_constant_get(&class_name, constant_name)? {
        return Ok(value);
    }
    if !eval_runtime_class_like_exists(&class_name, context, values)? {
        return eval_throw_error(&format!("Class \"{}\" not found", class_name.trim_start_matches('\\')), context, values);
    }
    eval_throw_error(&format!("Undefined constant {display_class}::{constant_name}"), context, values)
}

/// Returns the declared spelling of class-like `class_name` for a diagnostic, as php-src prints
/// `ce->name`.
fn eval_class_like_display_name(class_name: &str, context: &ElephcEvalContext) -> String {
    context
        .class(class_name)
        .map(|class| class.name().to_string())
        .or_else(|| context.enum_decl(class_name).map(|enum_decl| enum_decl.name().to_string()))
        .or_else(|| context.interface(class_name).map(|interface| interface.name().to_string()))
        .or_else(|| context.trait_decl(class_name).map(|trait_decl| trait_decl.name().to_string()))
        .unwrap_or_else(|| class_name.to_string())
        .trim_start_matches('\\')
        .to_string()
}

/// Resolves eval-visible built-in Reflection class constants.
pub(super) fn eval_builtin_reflection_class_constant(
    class_name: &str,
    constant_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let class_name = class_name.trim_start_matches('\\');
    let value = if class_name.eq_ignore_ascii_case("ReflectionClass") {
        match constant_name {
            "IS_IMPLICIT_ABSTRACT" => Some(16),
            "IS_FINAL" => Some(32),
            "IS_EXPLICIT_ABSTRACT" => Some(64),
            "IS_READONLY" => Some(65_536),
            _ => None,
        }
    } else if class_name.eq_ignore_ascii_case("ReflectionMethod") {
        match constant_name {
            "IS_PUBLIC" => Some(1),
            "IS_PROTECTED" => Some(2),
            "IS_PRIVATE" => Some(4),
            "IS_STATIC" => Some(16),
            "IS_FINAL" => Some(32),
            "IS_ABSTRACT" => Some(64),
            _ => None,
        }
    } else if class_name.eq_ignore_ascii_case("ReflectionProperty") {
        match constant_name {
            "IS_STATIC" => Some(16),
            "IS_READONLY" => Some(128),
            "IS_PUBLIC" => Some(1),
            "IS_PROTECTED" => Some(2),
            "IS_PRIVATE" => Some(4),
            "IS_ABSTRACT" => Some(64),
            "IS_PROTECTED_SET" => Some(2048),
            "IS_PRIVATE_SET" => Some(4096),
            "IS_VIRTUAL" => Some(512),
            "IS_FINAL" => Some(32),
            _ => None,
        }
    } else if class_name.eq_ignore_ascii_case("ReflectionClassConstant")
        || class_name.eq_ignore_ascii_case("ReflectionEnumUnitCase")
        || class_name.eq_ignore_ascii_case("ReflectionEnumBackedCase")
    {
        match constant_name {
            "IS_PUBLIC" => Some(1),
            "IS_PROTECTED" => Some(2),
            "IS_PRIVATE" => Some(4),
            "IS_FINAL" => Some(32),
            _ => None,
        }
    } else {
        None
    };
    value.map(|value| values.int(value)).transpose()
}

/// Resolves eval-visible `PropertyHookType` builtin enum cases.
pub(super) fn eval_builtin_property_hook_type_case(
    class_name: &str,
    constant_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("PropertyHookType")
    {
        return Ok(None);
    }
    let Some((case_name, case_value)) = eval_property_hook_type_case_parts(constant_name) else {
        return Ok(None);
    };
    if let Some(case) = context.enum_case("PropertyHookType", case_name) {
        return Ok(Some(case));
    }
    let object = RuntimeOwnerSlot::new(values.new_object("stdClass")?);
    let value = RuntimeOwnerSlot::empty();
    with_runtime_owners(&(&object, &value), || {
        let initialized = (|| {
            let identity = values.object_identity(object.borrowed())?;
            context.register_dynamic_object(identity, "PropertyHookType");
            with_collection_cells(values, |values, owners| {
                let name = owners.stage(values.string(case_name)?);
                values.property_set(object.borrowed(), "name", name.borrowed())
            })?;
            value.replace(Some(values.string(case_value)?));
            values.property_set(object.borrowed(), "value", value.borrowed())?;
            if let Some(replaced) =
                context.set_enum_case_value("PropertyHookType", case_name, value.take())
            {
                eval_release_value(context, values, replaced)?;
            }
            if let Some(replaced) =
                context.set_enum_case("PropertyHookType", case_name, object.take())
            {
                eval_release_value(context, values, replaced)?;
            }
            Ok(())
        })();
        let remaining = [&object, &value].into_iter().map(RuntimeOwnerSlot::take);
        release_staged_cells(initialized, remaining, |cell| values.release(cell))?;
        Ok(context
            .enum_case("PropertyHookType", case_name)
            .map(RuntimeCellHandle::borrowed))
    })
}

/// Returns the PHP case name and backed value for a builtin property-hook case.
pub(super) fn eval_property_hook_type_case_parts(constant_name: &str) -> Option<(&'static str, &'static str)> {
    match constant_name {
        "Get" => Some(("Get", "get")),
        "Set" => Some(("Set", "set")),
        _ => None,
    }
}

/// Returns the PHP class-name literal for `ClassName::class`-style eval expressions.
pub(in crate::interpreter) fn eval_class_name_fetch_result(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let class_name = resolve_eval_class_name_literal(class_name, context)?;
    values.string(&class_name)
}

/// Binds one eval-declared static property to a by-reference source variable.
pub(super) fn eval_static_property_reference_bind_result(
    class_name: &str,
    property_name: &str,
    source_name: &str,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    // Binding a static by reference fetches it, which initializes the class first, as every
    // other static-property access does.
    eval_raise_class_initialization_deprecations(&class_name, context, values)?;
    if let Some((declaring_class, property)) = context.class_property(&class_name, property_name) {
        if !property.is_static() {
            return eval_throw_undeclared_static_property_error(
                &class_name,
                property_name,
                context,
                values,
            );
        }
        if validate_eval_property_write_access(&declaring_class, &property, context).is_err() {
            return eval_throw_property_write_access_error(
                &declaring_class,
                &property,
                context,
                values,
            );
        }
        if validate_eval_readonly_property_write(&declaring_class, &property, None, context).is_err() {
            return eval_throw_readonly_property_modification_error(
                &declaring_class,
                property.name(),
                context,
                values,
            );
        }
        eval_preflight_property_reference_source(&declaring_class, &property, source_name, context, scope, values)?;
        let target = eval_static_property_reference_target(
            &declaring_class,
            property.name(),
            source_name,
            context,
            scope,
            values,
        )?;
        let value = eval_reference_target_value(&target, context, values)?;
        eval_reject_null_object_property(&declaring_class, &property, value, context, values)?;
        context.bind_static_property_alias(&declaring_class, property.name(), target);
        return store_borrowed_static_property(&declaring_class, property.name(), value, context, values);
    }
    if eval_static_member_context_owns_class(&class_name, context) {
        if let Some(parent) = context.class_native_parent_name(&class_name) {
            if let Some((declaring_class, _, write_visibility, is_static)) =
                eval_reflection_aot_static_property_access_metadata(
                    &parent,
                    property_name,
                    context,
                    values,
                )?
            {
                if !is_static {
                    return eval_throw_undeclared_static_property_error(
                        &class_name,
                        property_name,
                        context,
                        values,
                    );
                }
                if validate_eval_member_access(&declaring_class, write_visibility, context)
                    .is_err()
                {
                    return eval_throw_property_access_error(
                        &declaring_class,
                        property_name,
                        write_visibility,
                        context,
                        values,
                    );
                }
                let target = eval_static_property_reference_target(
                    &declaring_class,
                    property_name,
                    source_name,
                    context,
                    scope,
                    values,
                )?;
                let value = eval_reference_target_value(&target, context, values)?;
                context.bind_static_property_alias(&declaring_class, property_name, target);
                if eval_with_native_bridge_scope(&declaring_class, context, values, |values| {
                    values.static_property_set(&declaring_class, property_name, value)
                })? {
                    return Ok(());
                }
            }
        }
        return eval_throw_undeclared_static_property_error(
            &class_name,
            property_name,
            context,
            values,
        );
    }
    if let Some((declaring_class, _, write_visibility, is_static)) =
        eval_reflection_aot_static_property_access_metadata(
            &class_name,
            property_name,
            context,
            values,
        )?
    {
        if !is_static {
            return eval_throw_undeclared_static_property_error(
                &class_name,
                property_name,
                context,
                values,
            );
        }
        if validate_eval_member_access(&declaring_class, write_visibility, context).is_err() {
            return eval_throw_property_access_error(
                &declaring_class,
                property_name,
                write_visibility,
                context,
                values,
            );
        }
        let target = eval_static_property_reference_target(
            &declaring_class,
            property_name,
            source_name,
            context,
            scope,
            values,
        )?;
        let value = eval_reference_target_value(&target, context, values)?;
        context.bind_static_property_alias(&declaring_class, property_name, target);
        if eval_with_native_bridge_scope(&declaring_class, context, values, |values| {
            values.static_property_set(&class_name, property_name, value)
        })? {
            return Ok(());
        }
        return eval_throw_undeclared_static_property_error(
            &class_name,
            property_name,
            context,
            values,
        );
    }
    if eval_runtime_class_like_exists(&class_name, context, values)? {
        eval_throw_undeclared_static_property_error(&class_name, property_name, context, values)
    } else {
        eval_throw_class_not_found_error(&class_name, context, values)
    }
}

/// Resolves a local by-reference source into a persistent static-property alias target.
pub(super) fn eval_static_property_reference_target(
    class_name: &str,
    property_name: &str,
    source_name: &str,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalReferenceTarget, EvalStatus> {
    if let Some(target) = scope.reference_target(source_name).cloned() {
        let resolved = persistent_reference_target(target.clone())?;
        if reference_target_has_object_root(&resolved) {
            if let Some(cell) = eval_persistent_variable_reference(source_name, context, scope, values)? {
                return Ok(EvalReferenceTarget::Cell { cell });
            }
        }
        return Ok(target);
    }
    if context.current_function().is_some() {
        if !generic_local_contracts(scope, source_name, context).is_empty() {
            if let Some(cell) = eval_persistent_variable_reference(source_name, context, scope, values)? {
                return Ok(EvalReferenceTarget::Cell { cell });
            }
        }
        let cell =
            visible_scope_cell(context, scope, source_name).map_or_else(|| values.null(), Ok)?;
        return Ok(EvalReferenceTarget::Cell { cell });
    }
    let alias_name = eval_static_property_reference_alias_name(class_name, property_name);
    let replaced = set_reference_alias(context, scope, &alias_name, source_name, values)?;
    crate::interpreter::scope_cells::retire_replaced_scope_cells(replaced, context, values)?;
    Ok(EvalReferenceTarget::Variable {
        scope: scope as *mut ElephcEvalScope,
        name: alias_name,
    })
}

/// Builds the hidden scope variable name backing one static-property reference alias.
pub(super) fn eval_static_property_reference_alias_name(class_name: &str, property_name: &str) -> String {
    format!("\0elephc_static_property_ref:{class_name}:{property_name}")
}

/// Writes one eval static-property assignment through its persistent reference target.
pub(super) fn eval_static_reference_target_write(
    class_name: &str,
    property_name: &str,
    target: EvalReferenceTarget,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let EvalReferenceTarget::Cell { cell } = target {
        if values.is_reference(cell)? { return write_back_method_ref_target(&EvalReferenceTarget::Cell { cell }, value, context, values); }
        context.bind_static_property_alias(
            class_name,
            property_name,
            EvalReferenceTarget::Cell { cell: value },
        );
        return Ok(());
    }
    write_back_method_ref_target(&target, value, context, values)
}

/// Writes one eval-declared static property after resolving the class-like receiver.
pub(in crate::interpreter) fn eval_static_property_set_result(
    class_name: &str,
    property_name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    if let Some((declaring_class, property)) = context.class_property(&class_name, property_name) {
        if !property.is_static() {
            return eval_throw_undeclared_static_property_error(
                &class_name,
                property_name,
                context,
                values,
            );
        }
        if validate_eval_property_write_access(&declaring_class, &property, context).is_err() {
            return eval_throw_property_write_access_error(
                &declaring_class,
                &property,
                context,
                values,
            );
        }
        if validate_eval_readonly_property_write(&declaring_class, &property, None, context).is_err() {
            return eval_throw_readonly_property_modification_error(
                &declaring_class,
                property.name(),
                context,
                values,
            );
        }
        validate_native_static_class_defaults(&class_name, &declaring_class, context, values)?;
        eval_reject_null_object_property(&declaring_class, &property, value, context, values)?;
        if let Some(target) = context
            .static_property_alias(&declaring_class, property.name())
            .cloned()
        {
            eval_static_reference_target_write(
                &declaring_class,
                property.name(),
                target,
                value,
                context,
                values,
            )?;
        }
        return store_borrowed_static_property(&declaring_class, property.name(), value, context, values);
    }
    if eval_static_member_context_owns_class(&class_name, context) {
        if let Some(parent) = context.class_native_parent_name(&class_name) {
            if let Some((declaring_class, _, write_visibility, is_static)) =
                eval_reflection_aot_static_property_access_metadata(
                    &parent,
                    property_name,
                    context,
                    values,
                )?
            {
                if !is_static {
                    return eval_throw_undeclared_static_property_error(
                        &class_name,
                        property_name,
                        context,
                        values,
                    );
                }
                if validate_eval_member_access(&declaring_class, write_visibility, context)
                    .is_err()
                {
                    return eval_throw_property_access_error(
                        &declaring_class,
                        property_name,
                        write_visibility,
                        context,
                        values,
                    );
                }
                validate_native_static_class_defaults(&class_name, &declaring_class, context, values)?;
                super::instance_property_access::validate_eval_native_array_property_assignment(
                    &declaring_class,
                    property_name,
                    value,
                    context,
                    values,
                )?;
                if let Some(target) = context
                    .static_property_alias(&declaring_class, property_name)
                    .cloned()
                {
                    eval_static_reference_target_write(
                        &declaring_class,
                        property_name,
                        target,
                        value,
                        context,
                        values,
                    )?;
                }
                if eval_with_native_bridge_scope(&declaring_class, context, values, |values| {
                    values.static_property_set(&declaring_class, property_name, value)
                })? {
                    return Ok(());
                }
            }
        }
        return eval_throw_undeclared_static_property_error(
            &class_name,
            property_name,
            context,
            values,
        );
    }
    if let Some((declaring_class, _, write_visibility, is_static)) =
        eval_reflection_aot_static_property_access_metadata(
            &class_name,
            property_name,
            context,
            values,
        )?
    {
        if is_static
            && validate_eval_member_access(&declaring_class, write_visibility, context).is_err()
        {
            return eval_throw_property_access_error(
                &declaring_class,
                property_name,
                write_visibility,
                context,
                values,
            );
        }
        if is_static {
            validate_native_static_class_defaults(&class_name, &declaring_class, context, values)?;
            super::instance_property_access::validate_eval_native_array_property_assignment(
                &declaring_class,
                property_name,
                value,
                context,
                values,
            )?;
            if let Some(target) = context
                .static_property_alias(&declaring_class, property_name)
                .cloned()
            {
                eval_static_reference_target_write(
                    &declaring_class,
                    property_name,
                    target,
                    value,
                    context,
                    values,
                )?;
            }
            if eval_with_native_bridge_scope(&declaring_class, context, values, |values| {
                values.static_property_set(&class_name, property_name, value)
            })? {
                return Ok(());
            }
            return eval_throw_undeclared_static_property_error(&class_name, property_name, context, values);
        }
    }
    if values.static_property_set(&class_name, property_name, value)? {
        return Ok(());
    }
    if eval_runtime_class_like_exists(&class_name, context, values)? {
        eval_throw_undeclared_static_property_error(&class_name, property_name, context, values)
    } else {
        eval_throw_class_not_found_error(&class_name, context, values)
    }
}
