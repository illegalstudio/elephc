//! Purpose:
//! Eval registry entry and implementation for `method_exists`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Shared member-existence logic for `property_exists()` lives here.

eval_builtin! {
    name: "method_exists",
    area: Symbols,
    params: [object_or_class, method],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;
use super::super::{
    eval_class_metadata_is_a, eval_class_metadata_name, eval_class_relation_name_exists,
    eval_object_class_metadata_name, eval_object_public_property_exists,
    eval_resolved_class_metadata_name, eval_runtime_property_access_metadata,
    eval_same_class_metadata_name,
};

/// Dispatches direct eval calls for the `method_exists` symbol builtin.
pub(in crate::interpreter) fn eval_method_exists_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_member_exists("method_exists", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `method_exists` symbol builtin.
pub(in crate::interpreter) fn eval_method_exists_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_member_exists_result("method_exists", evaluated_args, context, values)
}

/// Evaluates `method_exists()` or `property_exists()` from eval expressions.
pub(in crate::interpreter) fn eval_builtin_member_exists(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [target, member] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let target = eval_expr(target, context, scope, values)?;
    let member = eval_expr(member, context, scope, values)?;
    eval_member_exists_result(name, &[target, member], context, values)
}

/// Evaluates materialized `method_exists()` or `property_exists()` arguments.
pub(in crate::interpreter) fn eval_member_exists_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [target, member] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let member = eval_class_metadata_name(*member, values)?;
    let exists = match name {
        "method_exists" => eval_method_exists_target(*target, &member, context, values)?,
        "property_exists" => eval_property_exists_target(*target, &member, context, values)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(exists)
}

/// Resolves a `method_exists()` target and applies PHP object-vs-string lookup rules.
fn eval_method_exists_target(
    target: RuntimeCellHandle,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    match values.type_tag(target)? {
        EVAL_TAG_OBJECT => {
            let class_name = eval_object_class_metadata_name(target, context, values)?;
            eval_method_exists_on_class(&class_name, method_name, true, context, values)
        }
        EVAL_TAG_STRING => {
            let class_name = eval_resolved_class_metadata_name(target, context, values)?;
            eval_method_exists_on_class(&class_name, method_name, false, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Resolves a `property_exists()` target and applies declared and dynamic-property lookup rules.
fn eval_property_exists_target(
    target: RuntimeCellHandle,
    property_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    match values.type_tag(target)? {
        EVAL_TAG_OBJECT => {
            let class_name = eval_object_class_metadata_name(target, context, values)?;
            if eval_property_exists_on_class(&class_name, property_name, context, values)? {
                return Ok(true);
            }
            if eval_current_scope_private_property_exists_on_object(
                &class_name,
                property_name,
                context,
                values,
            )? {
                return Ok(true);
            }
            eval_object_public_property_exists(target, property_name, values)
        }
        EVAL_TAG_STRING => {
            let class_name = eval_resolved_class_metadata_name(target, context, values)?;
            eval_property_exists_on_class(&class_name, property_name, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Checks method metadata for one resolved class-like name.
fn eval_method_exists_on_class(
    class_name: &str,
    method_name: &str,
    target_is_object: bool,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if context.has_class(class_name) || context.has_enum(class_name) {
        if target_is_object {
            if context
                .class_method_names(class_name)
                .iter()
                .any(|name| name.eq_ignore_ascii_case(method_name))
            {
                return Ok(true);
            }
            return eval_native_parent_method_exists_on_class(
                class_name,
                method_name,
                target_is_object,
                context,
                values,
            );
        }
        if context
            .class_method_names(class_name)
            .iter()
            .any(|name| name.eq_ignore_ascii_case(method_name))
        {
            let Some((declaring_class, method)) = context.class_method(class_name, method_name)
            else {
                return Ok(true);
            };
            return Ok(method.visibility() != EvalVisibility::Private
                || declaring_class
                    .trim_start_matches('\\')
                    .eq_ignore_ascii_case(class_name.trim_start_matches('\\')));
        }
        return eval_native_parent_method_exists_on_class(
            class_name,
            method_name,
            target_is_object,
            context,
            values,
        );
    }
    if context.has_interface(class_name) {
        return Ok(context
            .interface_method_names(class_name)
            .iter()
            .any(|name| name.eq_ignore_ascii_case(method_name)));
    }
    if context.has_trait(class_name) {
        return Ok(context
            .trait_method_names(class_name)
            .iter()
            .any(|name| name.eq_ignore_ascii_case(method_name)));
    }
    if target_is_object {
        return eval_native_method_exists_on_class(
            class_name,
            class_name,
            method_name,
            target_is_object,
            context,
            values,
        );
    }
    eval_native_method_exists_on_class(
        class_name,
        class_name,
        method_name,
        target_is_object,
        context,
        values,
    )
}

/// Checks generated/AOT parent method metadata inherited by one eval class.
fn eval_native_parent_method_exists_on_class(
    class_name: &str,
    method_name: &str,
    target_is_object: bool,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some(parent) = context.class_native_parent_name(class_name) else {
        return Ok(false);
    };
    eval_native_method_exists_on_class(
        class_name,
        &parent,
        method_name,
        target_is_object,
        context,
        values,
    )
}

/// Checks generated/AOT method metadata for method_exists() semantics.
fn eval_native_method_exists_on_class(
    reflected_class_name: &str,
    lookup_class_name: &str,
    method_name: &str,
    target_is_object: bool,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some((declaring_class, visibility, _, _)) =
        eval_aot_method_dispatch_metadata_in_hierarchy(
            lookup_class_name,
            method_name,
            context,
            values,
        )?
    else {
        return Ok(false);
    };
    if target_is_object || visibility != EvalVisibility::Private {
        return Ok(true);
    }
    Ok(declaring_class
        .trim_start_matches('\\')
        .eq_ignore_ascii_case(reflected_class_name.trim_start_matches('\\')))
}

/// Checks property metadata for one resolved class-like name.
fn eval_property_exists_on_class(
    class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if context.has_class(class_name) || context.has_enum(class_name) {
        if context
            .class_property_names(class_name)
            .iter()
            .any(|name| name == property_name)
        {
            return Ok(true);
        }
        return eval_native_parent_property_exists_on_class(
            class_name,
            property_name,
            context,
            values,
        );
    }
    if context.has_interface(class_name) {
        return Ok(context
            .interface_property_names(class_name)
            .iter()
            .any(|name| name == property_name));
    }
    if context.has_trait(class_name) {
        return Ok(context
            .trait_property_names(class_name)
            .iter()
            .any(|name| name == property_name));
    }
    eval_native_property_exists_on_class(class_name, class_name, property_name, values)
}

/// Checks generated/AOT parent property metadata inherited by one eval class.
fn eval_native_parent_property_exists_on_class(
    class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some(parent) = context.class_native_parent_name(class_name) else {
        return Ok(false);
    };
    eval_native_property_exists_on_class(class_name, &parent, property_name, values)
}

/// Checks generated/AOT property metadata for property_exists() semantics.
fn eval_native_property_exists_on_class(
    reflected_class_name: &str,
    lookup_class_name: &str,
    property_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some((declaring_class, visibility, _)) =
        eval_runtime_property_access_metadata(lookup_class_name, property_name, values)?
    else {
        return Ok(false);
    };
    if visibility != EvalVisibility::Private {
        return Ok(true);
    }
    Ok(declaring_class
        .trim_start_matches('\\')
        .eq_ignore_ascii_case(reflected_class_name.trim_start_matches('\\')))
}

/// Checks private instance properties declared by the current scope for object targets.
fn eval_current_scope_private_property_exists_on_object(
    class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some(current_class) = context.current_class_scope() else {
        return Ok(false);
    };
    if !eval_class_metadata_is_a(class_name, current_class, context) {
        return Ok(false);
    }
    if let Some(class) = context.class(current_class) {
        return Ok(class.properties().iter().any(|property| {
            property.name() == property_name
                && property.visibility() == EvalVisibility::Private
                && !property.is_static()
        }));
    }
    if context.has_interface(current_class) || context.has_trait(current_class) {
        return Ok(false);
    }
    if !eval_class_relation_name_exists(current_class, context, values)? {
        return Ok(false);
    }
    let Some((declaring_class, visibility, is_static)) =
        eval_runtime_property_access_metadata(current_class, property_name, values)?
    else {
        return Ok(false);
    };
    Ok(visibility == EvalVisibility::Private
        && !is_static
        && eval_same_class_metadata_name(&declaring_class, current_class))
}
