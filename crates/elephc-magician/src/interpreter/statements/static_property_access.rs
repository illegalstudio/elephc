//! Purpose:
//! Executes static-property, class-constant, and static reference operations.
//!
//! Called from:
//! - Expression and statement dispatch for class-member access.
//!
//! Key details:
//! - Receiver resolution, hooks, enum/reflection constants, and reference aliases share one path.

use super::*;

/// Reads one eval-declared static property after resolving the class-like receiver.
pub(in crate::interpreter) fn eval_static_property_get_result(
    class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
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
        if let Some(target) = context
            .static_property_alias(&declaring_class, property.name())
            .cloned()
        {
            return eval_reference_target_value(&target, context, values);
        }
        if let Some(value) = context.static_property(&declaring_class, property.name()) {
            return Ok(value);
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
                if let Some(target) = context
                    .static_property_alias(&declaring_class, property_name)
                    .cloned()
                {
                    return eval_reference_target_value(&target, context, values);
                }
                if !eval_with_native_bridge_scope(&declaring_class, context, || {
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
            if let Some(target) = context
                .static_property_alias(&declaring_class, property_name)
                .cloned()
            {
                return eval_reference_target_value(&target, context, values);
            }
            if !values.static_property_is_initialized(&declaring_class, property_name)? {
                return eval_throw_uninitialized_static_property_error(
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
                if let Some(target) = context
                    .static_property_alias(&declaring_class, property_name)
                    .cloned()
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
        if let Some(target) = context
            .static_property_alias(&declaring_class, property_name)
            .cloned()
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
        return context
            .class_constant_cell(&declaring_class, constant.name())
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
                || values.class_constant_get(&declaring_class, constant_name),
            )? {
                return Ok(value);
            }
        }
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Some(value) = values.class_constant_get(&class_name, constant_name)? {
        return Ok(value);
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
    Ok(Some(object))
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
        if validate_eval_readonly_property_write(&declaring_class, &property, context).is_err() {
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
        let value = eval_reference_target_value(&target, context, values)?;
        context.bind_static_property_alias(&declaring_class, property.name(), target);
        if let Some(replaced) =
            context.set_static_property(&declaring_class, property.name(), value)
        {
            values.release(replaced)?;
        }
        return Ok(());
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
    context: &ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalReferenceTarget, EvalStatus> {
    if let Some(target) = scope.reference_target(source_name).cloned() {
        return Ok(target);
    }
    if context.current_function().is_some() {
        let cell =
            visible_scope_cell(context, scope, source_name).map_or_else(|| values.null(), Ok)?;
        return Ok(EvalReferenceTarget::Cell { cell });
    }
    let alias_name = eval_static_property_reference_alias_name(class_name, property_name);
    for replaced in set_reference_alias(context, scope, &alias_name, source_name, values)? {
        values.release(replaced)?;
    }
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
    if matches!(target, EvalReferenceTarget::Cell { .. }) {
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
        if validate_eval_readonly_property_write(&declaring_class, &property, context).is_err() {
            return eval_throw_readonly_property_modification_error(
                &declaring_class,
                property.name(),
                context,
                values,
            );
        }
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
        if let Some(replaced) =
            context.set_static_property(&declaring_class, property.name(), value)
        {
            values.release(replaced)?;
        }
        return Ok(());
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
