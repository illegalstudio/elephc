//! Purpose:
//! Executes static-property, class-constant, and static reference operations.
//!
//! Called from:
//! - Expression and statement dispatch for class-member access.
//!
//! Key details:
//! - Receiver resolution, hooks, enum/reflection constants, and reference aliases share one path.

use super::*;

/// Raises native initialization failures after access checks, without invoking a throwing getter.
fn validate_native_static_class_defaults(
    class: &str, declaring: &str, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let message = context.native_class_default_error(class)
        .or_else(|| context.native_class_default_error(declaring)).map(str::to_owned)
        .or_else(|| context.class_native_parent_name(class).and_then(|parent|
            context.native_class_default_error(&parent).map(str::to_owned)));
    if let Some(message) = message { return throwables::eval_throw_error(&message, context, values); }
    Ok(())
}

/// Answers a read of an uninitialized typed static property.
///
/// A plain read throws PHP's `Error`. A QUIET read -- the left side of `??`, the target of `??=`,
/// `isset()` -- finds the property absent and gets `null`: `self::$sentinel ??= new \stdClass()`
/// is how `DeepClone` initializes its sentinel, and it died on the first call.
fn eval_uninitialized_static_property_read(
    declaring_class: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if context.quiet_property_fetch() {
        return values.null();
    }
    eval_throw_uninitialized_static_property_error(declaring_class, property_name, context, values)
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
    let value = match context.static_property_alias(class_name, property_name) {
        Some(EvalReferenceTarget::Cell { cell }) if values.is_reference(cell)? => cell,
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

/// Gives the registered SPL autoloaders their chance at a static-property receiver nobody has
/// declared yet, as php does for `Foo::$prop` (read or write) before it reports the class
/// missing. Constants and static calls already did; the property forms threw
/// `Class "Foo" not found` without asking, which broke Laravel's
/// `Serializers\Native::$transformUseVariables = ...` under artisan.
fn eval_autoload_static_property_class(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if matches!(
        class_name.to_ascii_lowercase().as_str(),
        "self" | "parent" | "static"
    ) || eval_class_like_is_known(class_name, context, values)?
    {
        return Ok(());
    }
    let _ = crate::interpreter::eval_spl_autoload_class(class_name, context, values)?;
    #[cfg(not(test))]
    context.sync_global_eval_classes();
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
    owned: Option<&mut Vec<RuntimeCellHandle>>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    eval_autoload_static_property_class(&class_name, context, values)?;
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
        {
            return match owned {
                Some(owners) => eval_owned_reference_target_value(&target, context, values, owners),
                None => eval_reference_target_value(&target, context, values),
            };
        }
        if let Some(value) =
            eval_static_property_value_or_default(&declaring_class, &property, context, values)?
        {
            return if owned.is_some() { values.retain(value) } else { Ok(value) };
        }
        if property.property_type().is_none() {
            return values.null();
        }
        if crate::eval_trace::enabled() {
            eprintln!(
                "[elephc-eval-trace] phase=static_property_uninitialized class={:?} declaring={declaring_class:?} property={:?}",
                class_name,
                property.name(),
            );
        }
        return eval_uninitialized_static_property_read(
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
                {
                    return match owned {
                        Some(owners) => eval_owned_reference_target_value(&target, context, values, owners),
                        None => eval_reference_target_value(&target, context, values),
                    };
                }
                if !eval_with_native_bridge_scope(&declaring_class, context, || {
                    values.static_property_is_initialized(&declaring_class, property_name)
                })? {
                    return eval_uninitialized_static_property_read(
                        &declaring_class,
                        property_name,
                        context,
                        values,
                    );
                }
                if let Some(value) = eval_with_native_bridge_scope(
                    &declaring_class,
                    context,
                    || values.static_property_get(&declaring_class, property_name),
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
            {
                return match owned {
                    Some(owners) => eval_owned_reference_target_value(&target, context, values, owners),
                    None => eval_reference_target_value(&target, context, values),
                };
            }
            if !values.static_property_is_initialized(&declaring_class, property_name)? {
                return eval_uninitialized_static_property_read(
                    &declaring_class,
                    property_name,
                    context,
                    values,
                );
            }
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

/// Reads an eval-declared static property, materializing its declaration default on first use.
///
/// Class METADATA is process-global — `sync_global_eval_classes` imports a class another eval
/// context declared — but static property VALUES are per-context, and only the context that
/// executed the declaration seeded them. Every other context then finds the declaration, finds
/// no cell, and reports PHP's "must not be accessed before initialization" for a property that
/// HAS a default. That is what took Symfony's `LinkStub::$composerRoots` down: the class is
/// declared by the autoloader's context and read from the constructor's, and `= []` never
/// crossed. A declaration default is a constant expression, so evaluating it on first access
/// here yields exactly what the declaring context stored, only later.
///
/// This does NOT make the two contexts share one static: a value one context has since written
/// still stays invisible to the other. It makes an unwritten default visible, which is the half
/// that was reporting an error instead of a value.
fn eval_static_property_value_or_default(
    declaring_class: &str,
    property: &EvalClassProperty,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if let Some(value) = context.static_property(declaring_class, property.name()) {
        return Ok(Some(value));
    }
    let Some(default) = property.default() else {
        return Ok(None);
    };
    let mut scope = ElephcEvalScope::new();
    let value = eval_class_like_member_default(
        declaring_class,
        property.trait_origin(),
        default,
        context,
        &mut scope,
        values,
    )?;
    if let Some(replaced) = context.set_static_property(declaring_class, property.name(), value) {
        values.release(replaced)?;
    }
    Ok(Some(value))
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
        {
            eval_reference_target_value(&target, context, values)?
        } else {
            let Some(value) =
                eval_static_property_value_or_default(&declaring_class, &property, context, values)?
            else {
                return Ok(false);
            };
            value
        };
        return Ok(!values.is_null(value)?);
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
                {
                    let value = eval_reference_target_value(&target, context, values)?;
                    return Ok(!values.is_null(value)?);
                }
                if !eval_with_native_bridge_scope(&declaring_class, context, || {
                    values.static_property_is_initialized(&declaring_class, property_name)
                })? {
                    return Ok(false);
                }
                if let Some(value) = eval_with_native_bridge_scope(
                    &declaring_class,
                    context,
                    || values.static_property_get(&declaring_class, property_name),
                )? {
                    return Ok(!values.is_null(value)?);
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
        {
            let value = eval_reference_target_value(&target, context, values)?;
            return Ok(!values.is_null(value)?);
        }
        if !values.static_property_is_initialized(&declaring_class, property_name)? {
            return Ok(false);
        }
    } else if !eval_runtime_class_like_exists(&class_name, context, values)? {
        return eval_throw_class_not_found_error(&class_name, context, values);
    }
    if let Some(value) = values.static_property_get(&class_name, property_name)? {
        return Ok(!values.is_null(value)?);
    }
    Ok(false)
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

/// Reads an array-valued compiled class constant, which the generated scalar helper cannot box.
///
/// The value is rebuilt on every read from the registered description, which is what a
/// constant array is to php: a value, never a shared reference. Visibility is checked against
/// the declaring class the way the scalar helper checks it.
fn eval_native_class_constant_default_result(
    class_name: &str,
    constant_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let mut current = Some(class_name.trim_start_matches('\\').to_string());
    let mut seen = 0usize;
    let constant = loop {
        let Some(name) = current else {
            return Ok(None);
        };
        if let Some(constant) = context.native_class_constant_default(&name, constant_name) {
            break constant;
        }
        // An interpreted subclass inherits its compiled ancestor's constants.
        seen += 1;
        if seen > 64 {
            return Ok(None);
        }
        current = context
            .class(&name)
            .and_then(|class| class.parent().map(str::to_string))
            .or_else(|| context.native_class_parent(&name).map(str::to_string));
    };
    if validate_eval_member_access(&constant.declaring_class, constant.visibility, context).is_err()
    {
        // php names the class the constant was read through, not the one declaring it.
        return eval_throw_constant_access_error(
            class_name.trim_start_matches('\\'),
            constant_name,
            constant.visibility,
            context,
            values,
        )
        .map(Some);
    }
    materialize_native_callable_default(&constant.default, context, values).map(Some)
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
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    if let Some(case) = context.enum_case(&class_name, constant_name) {
        return Ok(case);
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
        return eval_class_like_constant_cell(&declaring_class, &constant, context, values);
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
                || values.class_constant_get(&declaring_class, constant_name),
            )? {
                return Ok(value);
            }
        }
        if let Some(value) =
            eval_native_class_constant_default_result(&class_name, constant_name, context, values)?
        {
            return Ok(value);
        }
        // A constant of a COMPILED interface the eval class implements -- directly, through an
        // eval parent, or through an interface's own parents. Nothing above consulted them:
        // Carbon's `static::YEARS_PER_MILLENNIUM` (declared by the compiled `CarbonInterface`)
        // failed as an unsupported constant fetch inside `addUnit()`.
        for interface in context.class_interface_names(&class_name) {
            if context.has_interface(&interface) {
                continue;
            }
            if let Some(value) = values.class_constant_get(&interface, constant_name)? {
                return Ok(value);
            }
        }
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Some(value) = values.class_constant_get(&class_name, constant_name)? {
        return Ok(value);
    }
    if let Some(value) =
        eval_native_class_constant_default_result(&class_name, constant_name, context, values)?
    {
        return Ok(value);
    }
    // A class-like nobody declared yet is loaded first, as php does for `Foo::BAR`: the
    // `int $referenceType = UrlGeneratorInterface::ABSOLUTE_PATH` default of symfony's
    // `AbstractController::generateUrl()` names an interface only that default ever touches.
    if !eval_class_like_is_known(&class_name, context, values)? {
        let _ = crate::interpreter::eval_spl_autoload_class(&class_name, context, values)?;
        #[cfg(not(test))]
        context.sync_global_eval_classes();
        if eval_class_like_is_known(&class_name, context, values)? {
            return eval_class_constant_fetch_result(&class_name, constant_name, context, values);
        }
    }
    eval_throw_error(
        &format!(
            "Undefined constant {}::{}",
            class_name.trim_start_matches('\\'),
            constant_name
        ),
        context,
        values,
    )
}

/// Whether any context or the compiled program already declares this class-like name.
pub(in crate::interpreter) fn eval_class_like_is_known(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(context.class(class_name).is_some()
        || context.has_interface(class_name)
        || context.has_trait(class_name)
        || context.has_enum(class_name)
        || values.class_exists(class_name)?
        || values.interface_exists(class_name)?
        || values.enum_exists(class_name)?)
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
            // PHP 8.4 lazy-object option flags; see the checker's `reflection_class_constants`.
            "SKIP_INITIALIZATION_ON_SERIALIZE" => Some(8),
            "SKIP_DESTRUCTOR" => Some(16),
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
    } else if class_name.eq_ignore_ascii_case("ReflectionAttribute") {
        match constant_name {
            "IS_INSTANCEOF" => Some(2),
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
    let object = values.new_object("stdClass")?;
    let identity = values.object_identity(object)?;
    context.register_dynamic_object(identity, "PropertyHookType");
    let name = values.string(case_name)?;
    values.property_set(object, "name", name)?;
    let value = values.string(case_value)?;
    values.property_set(object, "value", value)?;
    if let Some(replaced) = context.set_enum_case_value("PropertyHookType", case_name, value) {
        values.release(replaced)?;
    }
    if let Some(replaced) = context.set_enum_case("PropertyHookType", case_name, object) {
        values.release(replaced)?;
    }
    Ok(Some(object.borrowed()))
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
    eval_autoload_static_property_class(&class_name, context, values)?;
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
        let target = eval_static_property_reference_target(
            &declaring_class,
            property.name(),
            source_name,
            context,
            scope,
            values,
        )?;
        // Bound to another variable, the property's value lives in THAT storage, and every read
        // goes through the alias first. Recording the variable's current cell as the property's
        // own value -- with no reference of its own -- left a dangling handle the moment the
        // variable was reassigned, which the next write through the alias then released.
        if !matches!(target, EvalReferenceTarget::Cell { .. }) {
            context.bind_static_property_alias(&declaring_class, property.name(), target);
            if let Some(previous) = context.take_static_property(&declaring_class, property.name()) {
                values.release(previous)?;
            }
            return Ok(());
        }
        let value = eval_reference_target_value(&target, context, values)?;
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
                if eval_with_native_bridge_scope(&declaring_class, context, || {
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
        if values.static_property_set(&class_name, property_name, value)? {
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
    for replaced in set_reference_alias(context, scope, &alias_name, source_name, values)? {
        values.release(replaced)?;
    }
    Ok(EvalReferenceTarget::Variable {
        scope: crate::scope::EvalScopeRef::new(scope),
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
    eval_autoload_static_property_class(&class_name, context, values)?;
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
        if let Some(target) = context
            .static_property_alias(&declaring_class, property.name())
        {
            let writes_another_variable = !matches!(target, EvalReferenceTarget::Cell { .. });
            eval_static_reference_target_write(
                &declaring_class,
                property.name(),
                target,
                value,
                context,
                values,
            )?;
            // The aliased variable now holds the value; the property keeps no copy of its own.
            if writes_another_variable {
                return Ok(());
            }
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
                if eval_with_native_bridge_scope(&declaring_class, context, || {
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
