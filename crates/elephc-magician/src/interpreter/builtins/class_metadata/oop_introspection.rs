//! Purpose:
//! Implements eval OOP introspection builtins for class/object members and
//! visible object variables.
//!
//! Called from:
//! - `crate::interpreter::builtins::class_metadata` re-exports.
//!
//! Key details:
//! - `method_exists()` distinguishes object targets from class-string targets
//!   because PHP exposes inherited private methods only on object targets.
//! - `get_class_vars()` materializes declarative defaults, not current runtime
//!   static property state.
//! - `get_object_vars()` filters declared storage slots so inaccessible
//!   protected/private eval properties do not leak as dynamic properties.

use super::super::super::*;
use super::{eval_class_metadata_name, eval_class_relation_name_exists};
use std::collections::HashSet;

const EVAL_CLASS_METADATA_FLAG_STATIC: u64 = 1;
const EVAL_CLASS_METADATA_FLAG_PROTECTED: u64 = 4;
const EVAL_CLASS_METADATA_FLAG_PRIVATE: u64 = 8;

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

/// Evaluates `get_class_methods()` from eval expressions.
pub(in crate::interpreter) fn eval_builtin_get_class_methods(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [target] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let target = eval_expr(target, context, scope, values)?;
    eval_get_class_methods_result(&[target], context, values)
}

/// Evaluates materialized `get_class_methods()` arguments.
pub(in crate::interpreter) fn eval_get_class_methods_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [target] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let (class_name, target_is_object) = eval_class_metadata_target_name(*target, context, values)?;
    if !target_is_object && !eval_class_relation_name_exists(&class_name, context, values)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let names = eval_class_method_names_for_scope(&class_name, context, values)?;
    eval_indexed_string_array_result(&names, values)
}

/// Evaluates `get_class_vars()` from eval expressions.
pub(in crate::interpreter) fn eval_builtin_get_class_vars(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [target] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let target = eval_expr(target, context, scope, values)?;
    eval_get_class_vars_result(&[target], context, values)
}

/// Evaluates materialized `get_class_vars()` arguments.
pub(in crate::interpreter) fn eval_get_class_vars_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [target] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let class_name = eval_resolved_class_metadata_name(*target, context, values)?;
    if context.has_class(&class_name) || context.has_enum(&class_name) {
        return eval_dynamic_class_vars_result(&class_name, context, values);
    }
    if context.has_trait(&class_name) {
        return eval_dynamic_trait_vars_result(&class_name, context, values);
    }
    if context.has_interface(&class_name) {
        return values.assoc_new(0);
    }
    if eval_class_relation_name_exists(&class_name, context, values)? {
        return eval_runtime_class_vars_result(&class_name, context, values);
    }
    Err(EvalStatus::RuntimeFatal)
}

/// Evaluates `get_object_vars()` from eval expressions.
pub(in crate::interpreter) fn eval_builtin_get_object_vars(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [object] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let object = eval_expr(object, context, scope, values)?;
    eval_get_object_vars_result(&[object], context, values)
}

/// Evaluates materialized `get_object_vars()` arguments.
pub(in crate::interpreter) fn eval_get_object_vars_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [object] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if values.type_tag(*object)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    let Ok(identity) = values.object_identity(*object) else {
        return eval_public_object_vars_result(*object, values);
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        let class_name = eval_object_class_metadata_name(*object, context, values)?;
        return eval_runtime_object_vars_result(*object, &class_name, context, values);
    };
    let class_name = class.name().to_string();
    eval_dynamic_object_vars_result(*object, &class_name, context, values)
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
            return Ok(context
                .class_method_names(class_name)
                .iter()
                .any(|name| name.eq_ignore_ascii_case(method_name)));
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
        return Ok(false);
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
        return Ok(eval_aot_method_dispatch_metadata_in_hierarchy(
            class_name,
            method_name,
            context,
            values,
        )?
        .is_some());
    }
    values
        .reflection_method_flags(class_name, method_name)
        .map(|flags| flags.is_some())
}

/// Checks property metadata for one resolved class-like name.
fn eval_property_exists_on_class(
    class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if context.has_class(class_name) || context.has_enum(class_name) {
        return Ok(context
            .class_property_names(class_name)
            .iter()
            .any(|name| name == property_name));
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
    let Some(flags) = values.reflection_property_flags(class_name, property_name)? else {
        return Ok(false);
    };
    if flags & EVAL_CLASS_METADATA_FLAG_PRIVATE == 0 {
        return Ok(true);
    }
    let Some(declaring_class) = values.reflection_property_declaring_class(
        class_name,
        property_name,
    )? else {
        return Ok(true);
    };
    Ok(declaring_class
        .trim_start_matches('\\')
        .eq_ignore_ascii_case(class_name.trim_start_matches('\\')))
}

/// Resolves an object-or-class argument to a PHP class name and records whether it was an object.
fn eval_class_metadata_target_name(
    target: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(String, bool), EvalStatus> {
    match values.type_tag(target)? {
        EVAL_TAG_OBJECT => Ok((
            eval_object_class_metadata_name(target, context, values)?,
            true,
        )),
        EVAL_TAG_STRING => Ok((
            eval_resolved_class_metadata_name(target, context, values)?,
            false,
        )),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Resolves an object cell to its eval or runtime class name.
fn eval_object_class_metadata_name(
    object: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let identity = values.object_identity(object)?;
    if let Some(class) = context.dynamic_object_class(identity) {
        return Ok(class.name().trim_start_matches('\\').to_string());
    }
    let class_name = values.object_class_name(object)?;
    let class_name_bytes = values.string_bytes(class_name);
    values.release(class_name)?;
    let class_name = String::from_utf8(class_name_bytes?).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(class_name.trim_start_matches('\\').to_string())
}

/// Reads a class-name cell and applies eval alias resolution.
fn eval_resolved_class_metadata_name(
    name: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let name = eval_class_metadata_name(name, values)?;
    Ok(context.resolve_class_name(&name).unwrap_or(name))
}

/// Collects PHP-visible methods for `get_class_methods()` in the current eval scope.
fn eval_class_method_names_for_scope(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    if context.has_class(class_name) || context.has_enum(class_name) {
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        for name in context.class_method_names(class_name) {
            let Some((declaring_class, method)) = context.class_method(class_name, &name) else {
                eval_push_unique_method_name(&mut names, &mut seen, name);
                continue;
            };
            if validate_eval_member_access(&declaring_class, method.visibility(), context).is_ok() {
                eval_push_unique_method_name(&mut names, &mut seen, name);
            }
        }
        eval_add_current_scope_private_method_names(
            &mut names, &mut seen, class_name, context, values,
        )?;
        return Ok(names);
    }
    if context.has_interface(class_name) {
        return Ok(context.interface_method_names(class_name));
    }
    if let Some(trait_decl) = context.trait_decl(class_name) {
        return Ok(trait_decl
            .methods()
            .iter()
            .filter(|method| method.visibility() == EvalVisibility::Public)
            .map(|method| method.name().to_string())
            .collect());
    }
    let method_names = values.reflection_method_names(class_name)?;
    let names = eval_runtime_string_array_to_vec(method_names, values)?;
    values.release(method_names)?;
    let mut names = eval_visible_runtime_method_names(class_name, names, context, values)?;
    let mut seen = names
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    eval_add_current_scope_private_method_names(&mut names, &mut seen, class_name, context, values)?;
    Ok(names)
}

/// Filters generated runtime methods to the surface visible from the current eval scope.
fn eval_visible_runtime_method_names(
    class_name: &str,
    names: Vec<String>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let mut result = Vec::new();
    for name in names {
        let Some((declaring_class, visibility)) =
            eval_runtime_method_access_metadata(class_name, &name, values)?
        else {
            continue;
        };
        if validate_eval_member_access(&declaring_class, visibility, context).is_ok() {
            result.push(name);
        }
    }
    Ok(result)
}

/// Adds private methods declared by the current eval scope when PHP would expose them.
fn eval_add_current_scope_private_method_names(
    names: &mut Vec<String>,
    seen: &mut HashSet<String>,
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(current_class) = context.current_class_scope() else {
        return Ok(());
    };
    if !eval_class_metadata_is_a(class_name, current_class, context) {
        return Ok(());
    }
    if let Some(class) = context.class(current_class) {
        for method in class.methods() {
            if method.visibility() == EvalVisibility::Private {
                eval_push_unique_method_name(names, seen, method.name().to_string());
            }
        }
        return Ok(());
    }
    if context.has_interface(current_class) || context.has_trait(current_class) {
        return Ok(());
    }
    if !eval_class_relation_name_exists(current_class, context, values)? {
        return Ok(());
    }
    let method_names = values.reflection_method_names(current_class)?;
    let current_names = eval_runtime_string_array_to_vec(method_names, values)?;
    values.release(method_names)?;
    for name in current_names {
        let Some((declaring_class, visibility)) =
            eval_runtime_method_access_metadata(current_class, &name, values)?
        else {
            continue;
        };
        if visibility == EvalVisibility::Private
            && eval_same_class_metadata_name(&declaring_class, current_class)
        {
            eval_push_unique_method_name(names, seen, name);
        }
    }
    Ok(())
}

/// Returns whether one eval or generated/AOT class name is the same as or extends another.
fn eval_class_metadata_is_a(
    class_name: &str,
    target: &str,
    context: &ElephcEvalContext,
) -> bool {
    eval_same_class_metadata_name(class_name, target)
        || context.class_is_a(class_name, target, false)
        || eval_native_class_metadata_is_a(class_name, target, context)
}

/// Returns whether generated/AOT parent metadata proves one class extends another.
fn eval_native_class_metadata_is_a(
    class_name: &str,
    target: &str,
    context: &ElephcEvalContext,
) -> bool {
    let target = target.trim_start_matches('\\');
    let mut current = class_name.trim_start_matches('\\').to_string();
    let mut seen = HashSet::new();
    loop {
        if !seen.insert(current.to_ascii_lowercase()) {
            return false;
        }
        if eval_same_class_metadata_name(&current, target) {
            return true;
        }
        let Some(parent) = context.native_class_parent(&current) else {
            return false;
        };
        current = parent.to_string();
    }
}

/// Returns whether two PHP class names refer to the same normalized metadata name.
fn eval_same_class_metadata_name(left: &str, right: &str) -> bool {
    left.trim_start_matches('\\')
        .eq_ignore_ascii_case(right.trim_start_matches('\\'))
}

/// Appends one method name while preserving PHP's case-insensitive uniqueness rule.
fn eval_push_unique_method_name(
    names: &mut Vec<String>,
    seen: &mut HashSet<String>,
    name: String,
) {
    if seen.insert(name.to_ascii_lowercase()) {
        names.push(name);
    }
}

/// Returns access metadata for one generated/AOT method name, if reflection exposes it.
fn eval_runtime_method_access_metadata(
    class_name: &str,
    method_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, EvalVisibility)>, EvalStatus> {
    let Some(flags) = values.reflection_method_flags(class_name, method_name)? else {
        return Ok(None);
    };
    let declaring_class = values
        .reflection_method_declaring_class(class_name, method_name)?
        .unwrap_or_else(|| class_name.to_string());
    Ok(Some((
        declaring_class,
        eval_runtime_member_visibility(flags),
    )))
}

/// Builds `get_class_vars()` for an eval-declared class or enum.
fn eval_dynamic_class_vars_result(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.assoc_new(0)?;
    let mut emitted_keys = HashSet::new();
    if let Some(enum_decl) = context.enum_decl(class_name) {
        let name_value = values.null()?;
        result = eval_add_class_var_entry(result, "name", name_value, values)?;
        emitted_keys.insert(String::from("name"));
        if enum_decl.backing_type().is_some() {
            let value_value = values.null()?;
            result = eval_add_class_var_entry(result, "value", value_value, values)?;
            emitted_keys.insert(String::from("value"));
        }
    }
    for class in context.class_chain(class_name).into_iter().rev() {
        for property in class.properties() {
            if emitted_keys.contains(property.name())
                || validate_eval_member_access(class.name(), property.visibility(), context)
                    .is_err()
            {
                continue;
            }
            let value =
                eval_class_vars_property_default_value(class.name(), property, context, values)?;
            result = eval_add_class_var_entry(result, property.name(), value, values)?;
            emitted_keys.insert(property.name().to_string());
        }
    }
    Ok(result)
}

/// Builds `get_class_vars()` for an eval-declared trait.
fn eval_dynamic_trait_vars_result(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(trait_decl) = context.trait_decl(class_name) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let trait_name = trait_decl.name().to_string();
    let properties = trait_decl.properties().to_vec();
    let mut result = values.assoc_new(properties.len())?;
    let mut emitted_keys = HashSet::new();
    for property in properties {
        if emitted_keys.contains(property.name())
            || validate_eval_member_access(&trait_name, property.visibility(), context).is_err()
        {
            continue;
        }
        let value =
            eval_class_vars_property_default_value(&trait_name, &property, context, values)?;
        result = eval_add_class_var_entry(result, property.name(), value, values)?;
        emitted_keys.insert(property.name().to_string());
    }
    Ok(result)
}

/// Builds `get_class_vars()` data for generated/AOT class metadata.
fn eval_runtime_class_vars_result(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let property_names = values.reflection_property_names(class_name)?;
    let declared_names = eval_runtime_string_array_to_vec(property_names, values)?;
    values.release(property_names)?;
    let mut result = values.assoc_new(declared_names.len())?;
    let mut emitted_keys = HashSet::new();
    for property_name in declared_names {
        if emitted_keys.contains(&property_name) {
            continue;
        }
        let Some((declaring_class, visibility, _is_static)) =
            eval_runtime_property_access_metadata(class_name, &property_name, values)?
        else {
            continue;
        };
        if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
            continue;
        }
        let value = eval_runtime_class_var_default_value(
            class_name,
            &declaring_class,
            &property_name,
            context,
            values,
        )?;
        result = eval_add_class_var_entry(result, &property_name, value, values)?;
        emitted_keys.insert(property_name);
    }
    Ok(result)
}

/// Materializes one eval-declared property default for `get_class_vars()`.
fn eval_class_vars_property_default_value(
    declaring_class: &str,
    property: &EvalClassProperty,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(default) = property.default() else {
        return values.null();
    };
    context.push_class_scope(declaring_class.to_string());
    context.push_called_class_scope(declaring_class.to_string());
    context.push_class_like_member_magic_scope(declaring_class, property.trait_origin());
    let result = eval_method_parameter_default(default, context, values);
    context.pop_magic_scope();
    context.pop_called_class_scope();
    context.pop_class_scope();
    result
}

/// Materializes one generated/AOT property default for `get_class_vars()`.
fn eval_runtime_class_var_default_value(
    runtime_class: &str,
    declaring_class: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(default) = context
        .native_property_default(declaring_class, property_name)
        .or_else(|| context.native_property_default(runtime_class, property_name))
    {
        return materialize_native_callable_default(&default, context, values);
    }
    values.null()
}

/// Adds one string-keyed class variable value to an associative result array.
fn eval_add_class_var_entry(
    result: RuntimeCellHandle,
    property_name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(property_name)?;
    values.array_set(result, key, value)
}

/// Builds `get_object_vars()` for an eval-declared object.
fn eval_dynamic_object_vars_result(
    object: RuntimeCellHandle,
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let property_count = values.object_property_len(object)?;
    let mut result = values.assoc_new(property_count)?;
    let mut emitted_keys = HashSet::new();
    let storage_keys = eval_declared_object_storage_names(class_name, context);
    result = eval_add_enum_object_vars(
        result,
        object,
        class_name,
        &mut emitted_keys,
        context,
        values,
    )?;
    result = eval_add_declared_object_vars(
        result,
        object,
        class_name,
        &mut emitted_keys,
        context,
        values,
    )?;
    eval_add_dynamic_object_vars(result, object, &mut emitted_keys, &storage_keys, values)
}

/// Builds `get_object_vars()` for generated/AOT objects from reflection metadata.
fn eval_runtime_object_vars_result(
    object: RuntimeCellHandle,
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let property_names = values.reflection_property_names(class_name)?;
    let declared_names = eval_runtime_string_array_to_vec(property_names, values)?;
    values.release(property_names)?;
    let property_count = values.object_property_len(object)?;
    let mut result = values.assoc_new(declared_names.len() + property_count)?;
    let mut emitted_keys = HashSet::new();
    result = eval_add_runtime_scope_private_object_vars(
        result,
        object,
        &mut emitted_keys,
        context,
        values,
    )?;
    result = eval_add_runtime_declared_object_vars(
        result,
        object,
        class_name,
        &declared_names,
        &mut emitted_keys,
        context,
        values,
    )?;
    eval_add_dynamic_object_vars(result, object, &mut emitted_keys, &HashSet::new(), values)
}

/// Adds generated/AOT private properties declared by the current eval class scope.
fn eval_add_runtime_scope_private_object_vars(
    mut result: RuntimeCellHandle,
    object: RuntimeCellHandle,
    emitted_keys: &mut HashSet<String>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(current_class) = context.current_class_scope() else {
        return Ok(result);
    };
    if !values.object_is_a(object, current_class, false)? {
        return Ok(result);
    }
    let property_names = values.reflection_property_names(current_class)?;
    let declared_names = eval_runtime_string_array_to_vec(property_names, values)?;
    values.release(property_names)?;
    for property_name in declared_names {
        let Some((_, visibility, is_static)) =
            eval_runtime_property_access_metadata(current_class, &property_name, values)?
        else {
            continue;
        };
        if is_static
            || visibility != EvalVisibility::Private
            || emitted_keys.contains(&property_name)
            || !values.property_is_initialized(object, &property_name)?
        {
            continue;
        }
        emitted_keys.insert(property_name.clone());
        let key = values.string(&property_name)?;
        let value = values.property_get(object, &property_name)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Adds generated/AOT declared instance properties visible from the current eval scope.
fn eval_add_runtime_declared_object_vars(
    mut result: RuntimeCellHandle,
    object: RuntimeCellHandle,
    class_name: &str,
    property_names: &[String],
    emitted_keys: &mut HashSet<String>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    for property_name in property_names {
        let Some((declaring_class, visibility, is_static)) =
            eval_runtime_property_access_metadata(class_name, property_name, values)?
        else {
            continue;
        };
        if is_static
            || validate_eval_member_access(&declaring_class, visibility, context).is_err()
            || emitted_keys.contains(property_name)
            || !values.property_is_initialized(object, property_name)?
        {
            continue;
        }
        emitted_keys.insert(property_name.clone());
        let key = values.string(property_name)?;
        let value = values.property_get(object, property_name)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Returns access metadata for one generated/AOT property name, if reflection exposes it.
fn eval_runtime_property_access_metadata(
    class_name: &str,
    property_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, EvalVisibility, bool)>, EvalStatus> {
    let Some(flags) = values.reflection_property_flags(class_name, property_name)? else {
        return Ok(None);
    };
    let declaring_class = values
        .reflection_property_declaring_class(class_name, property_name)?
        .unwrap_or_else(|| class_name.to_string());
    Ok(Some((
        declaring_class,
        eval_runtime_member_visibility(flags),
        flags & EVAL_CLASS_METADATA_FLAG_STATIC != 0,
    )))
}

/// Converts generated/AOT reflection member flags into eval visibility metadata.
fn eval_runtime_member_visibility(flags: u64) -> EvalVisibility {
    if flags & EVAL_CLASS_METADATA_FLAG_PRIVATE != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_CLASS_METADATA_FLAG_PROTECTED != 0 {
        EvalVisibility::Protected
    } else {
        EvalVisibility::Public
    }
}

/// Adds synthetic enum properties exposed by PHP enum case objects.
fn eval_add_enum_object_vars(
    mut result: RuntimeCellHandle,
    object: RuntimeCellHandle,
    class_name: &str,
    emitted_keys: &mut HashSet<String>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(enum_decl) = context.enum_decl(class_name) else {
        return Ok(result);
    };
    let is_backed = enum_decl.backing_type().is_some();
    result = eval_add_object_var(result, object, "name", emitted_keys, context, values)?;
    if is_backed {
        result = eval_add_object_var(result, object, "value", emitted_keys, context, values)?;
    }
    Ok(result)
}

/// Adds declared instance properties visible from the current eval scope.
fn eval_add_declared_object_vars(
    mut result: RuntimeCellHandle,
    object: RuntimeCellHandle,
    class_name: &str,
    emitted_keys: &mut HashSet<String>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let identity = values.object_identity(object)?;
    for class in context.class_chain(class_name) {
        for property in class.properties() {
            if property.is_static()
                || validate_eval_member_access(class.name(), property.visibility(), context)
                    .is_err()
                || emitted_keys.contains(property.name())
            {
                continue;
            }
            let storage_property_name = eval_instance_property_storage_name(class.name(), property);
            if !property.is_virtual()
                && !context.dynamic_property_is_initialized(identity, &storage_property_name)
            {
                continue;
            }
            result = eval_add_object_var(
                result,
                object,
                property.name(),
                emitted_keys,
                context,
                values,
            )?;
        }
    }
    Ok(result)
}

/// Adds one visible object variable to an associative result array.
fn eval_add_object_var(
    result: RuntimeCellHandle,
    object: RuntimeCellHandle,
    property_name: &str,
    emitted_keys: &mut HashSet<String>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    emitted_keys.insert(property_name.to_string());
    let key = values.string(property_name)?;
    let value = eval_property_get_result(object, property_name, context, values)?;
    values.array_set(result, key, value)
}

/// Adds public dynamic properties that are not declared storage slots.
fn eval_add_dynamic_object_vars(
    mut result: RuntimeCellHandle,
    object: RuntimeCellHandle,
    emitted_keys: &mut HashSet<String>,
    storage_keys: &HashSet<String>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let property_count = values.object_property_len(object)?;
    for position in 0..property_count {
        let key = values.object_property_iter_key(object, position)?;
        let key_bytes = values.string_bytes(key);
        values.release(key)?;
        let key_name = String::from_utf8(key_bytes?).map_err(|_| EvalStatus::RuntimeFatal)?;
        if key_name.contains('\0')
            || storage_keys.contains(&key_name)
            || !emitted_keys.insert(key_name.clone())
        {
            continue;
        }
        let key = values.string(&key_name)?;
        let value = values.property_get(object, &key_name)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Returns physical storage names used by declared eval object properties.
fn eval_declared_object_storage_names(
    class_name: &str,
    context: &ElephcEvalContext,
) -> HashSet<String> {
    let mut names = HashSet::new();
    for class in context.class_chain(class_name) {
        for property in class.properties() {
            names.insert(eval_instance_property_storage_name(class.name(), property));
        }
    }
    names
}

/// Builds `get_object_vars()` for runtime objects with public bridge-visible properties.
fn eval_public_object_vars_result(
    object: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let property_count = values.object_property_len(object)?;
    let mut result = values.assoc_new(property_count)?;
    for position in 0..property_count {
        let key = values.object_property_iter_key(object, position)?;
        let key_bytes = values.string_bytes(key);
        values.release(key)?;
        let key_name = String::from_utf8(key_bytes?).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = values.string(&key_name)?;
        let value = values.property_get(object, &key_name)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Returns whether an object has a public bridge-visible property by exact name.
fn eval_object_public_property_exists(
    object: RuntimeCellHandle,
    property_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let property_count = values.object_property_len(object)?;
    for position in 0..property_count {
        let key = values.object_property_iter_key(object, position)?;
        let key_bytes = values.string_bytes(key);
        values.release(key)?;
        if key_bytes? == property_name.as_bytes() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Builds an indexed PHP array from owned Rust strings.
fn eval_indexed_string_array_result(
    names: &[String],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(names.len())?;
    for (index, name) in names.iter().enumerate() {
        let key = values.int(index as i64)?;
        let value = values.string(name)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Copies a runtime string array into Rust-owned strings for class metadata helpers.
fn eval_runtime_string_array_to_vec(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.int(position as i64)?;
        let value = values.array_get(array, key)?;
        result.push(eval_class_metadata_name(value, values)?);
    }
    Ok(result)
}
