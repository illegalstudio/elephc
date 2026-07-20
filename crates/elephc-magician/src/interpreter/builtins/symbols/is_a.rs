//! Purpose:
//! Eval registry entry and implementation for `is_a`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Eval-created classes are checked first, then generated/AOT object and
//!   interface metadata fills inherited relationships.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "is_a",
    area: Symbols,
    params: [object_or_class, r#class, allow_string = EvalBuiltinDefaultValue::Bool(false)],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `is_a(...)` calls over eval boxed object cells and class strings.
pub(in crate::interpreter) fn eval_is_a_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_is_a_relation("is_a", args, context, scope, values)
}

/// Evaluates materialized `is_a(...)` arguments.
pub(in crate::interpreter) fn eval_is_a_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_is_a_relation_result("is_a", evaluated_args, context, values)
}

/// Evaluates `is_a(...)` and `is_subclass_of(...)` over eval boxed object cells.
pub(in crate::interpreter) fn eval_builtin_is_a_relation(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_is_a_relation_result(name, &evaluated_args, context, values)
}

/// Evaluates materialized `is_a(...)` or `is_subclass_of(...)` builtin arguments.
pub(in crate::interpreter) fn eval_is_a_relation_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (object_or_class, target_class, allow_string) = match evaluated_args {
        [object_or_class, target_class] => {
            (*object_or_class, *target_class, name == "is_subclass_of")
        }
        [object_or_class, target_class, allow_string] => (
            *object_or_class,
            *target_class,
            values.truthy(*allow_string)?,
        ),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let target_class = values.string_bytes(target_class)?;
    let target_class = String::from_utf8(target_class).map_err(|_| EvalStatus::RuntimeFatal)?;
    let target_class = target_class.trim_start_matches('\\');
    let resolved_target_class = context
        .resolve_class_like_name(target_class)
        .unwrap_or_else(|| target_class.to_string());
    let is_object = values.type_tag(object_or_class)? == 6;
    let exclude_self = name == "is_subclass_of";
    let result = if is_object {
        dynamic_object_is_a(
            object_or_class,
            &resolved_target_class,
            exclude_self,
            context,
            values,
        )?
        .map_or_else(
            || values.object_is_a(object_or_class, &resolved_target_class, exclude_self),
            Ok,
        )?
    } else if allow_string && values.type_tag(object_or_class)? == EVAL_TAG_STRING {
        let source_class = values.string_bytes(object_or_class)?;
        let source_class = String::from_utf8(source_class).map_err(|_| EvalStatus::RuntimeFatal)?;
        let resolved_source_class = context
            .resolve_class_like_name(&source_class)
            .unwrap_or_else(|| source_class.trim_start_matches('\\').to_string());
        if context.class(&resolved_source_class).is_some() {
            eval_class_string_is_a(
                &resolved_source_class,
                &resolved_target_class,
                exclude_self,
                context,
                values,
            )?
        } else if context.interface(&resolved_source_class).is_some() {
            eval_interface_string_is_a(
                &resolved_source_class,
                &resolved_target_class,
                exclude_self,
                context,
                values,
            )?
        } else if context.trait_decl(&resolved_source_class).is_some() {
            !exclude_self
                && eval_class_like_name_matches(&resolved_source_class, &resolved_target_class)
        } else {
            values.object_is_a(object_or_class, &resolved_target_class, exclude_self)?
        }
    } else if allow_string {
        values.object_is_a(object_or_class, &resolved_target_class, exclude_self)?
    } else {
        false
    };
    values.bool_value(result)
}

/// Returns whether an interface string source satisfies a class-like target.
fn eval_interface_string_is_a(
    source_class: &str,
    target_class: &str,
    exclude_self: bool,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if !exclude_self && eval_class_like_name_matches(source_class, target_class) {
        return Ok(true);
    }
    Ok(eval_interface_runtime_parent_names(source_class, context, values)?
        .iter()
        .any(|parent| eval_class_like_name_matches(parent, target_class)))
}

/// Returns whether an eval class-string source satisfies a class-like target.
fn eval_class_string_is_a(
    source_class: &str,
    target_class: &str,
    exclude_self: bool,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if context.class_is_a(source_class, target_class, exclude_self) {
        return Ok(true);
    }
    Ok(eval_class_runtime_interface_names(source_class, context, values)?
        .iter()
        .any(|interface| eval_class_like_name_matches(interface, target_class)))
}

/// Returns eval class interfaces plus generated/AOT inherited interface names.
fn eval_class_runtime_interface_names(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(parent) = context.class_native_parent_name(class_name) {
        for name in eval_runtime_class_interface_names(&parent, values)? {
            eval_push_unique_class_name(name, &mut names, &mut seen);
        }
    }
    for name in context.class_interface_names(class_name) {
        eval_push_unique_class_name(name.clone(), &mut names, &mut seen);
        if !context.has_interface(&name) && eval_runtime_interface_exists(&name, values)? {
            for parent in eval_runtime_class_interface_names(&name, values)? {
                eval_push_unique_class_name(parent, &mut names, &mut seen);
            }
        }
    }
    Ok(names)
}

/// Returns eval interface parents plus generated/AOT inherited interface names.
fn eval_interface_runtime_parent_names(
    interface_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for name in context.interface_parent_names(interface_name) {
        eval_push_unique_class_name(name.clone(), &mut names, &mut seen);
        if !context.has_interface(&name) && eval_runtime_interface_exists(&name, values)? {
            for parent in eval_runtime_class_interface_names(&name, values)? {
                eval_push_unique_class_name(parent, &mut names, &mut seen);
            }
        }
    }
    Ok(names)
}

/// Returns generated/AOT interface names visible for one class-like symbol.
fn eval_runtime_class_interface_names(
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let names_array = values.reflection_class_interface_names(class_name)?;
    let names = eval_string_array_to_vec(names_array, values)?;
    values.release(names_array)?;
    Ok(names)
}

/// Copies a runtime string array into Rust-owned names.
fn eval_string_array_to_vec(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.int(position as i64)?;
        let value = values.array_get(array, key)?;
        let bytes = values.string_bytes(value)?;
        result.push(String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?);
    }
    Ok(result)
}

/// Appends one class-like name while preserving PHP's case-insensitive uniqueness.
fn eval_push_unique_class_name(
    name: String,
    names: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    if seen.insert(name.to_ascii_lowercase()) {
        names.push(name);
    }
}

/// Returns whether two class-like names match PHP's case-insensitive class-name rules.
fn eval_class_like_name_matches(left: &str, right: &str) -> bool {
    left.trim_start_matches('\\')
        .eq_ignore_ascii_case(right.trim_start_matches('\\'))
}

/// Returns whether an eval-created object matches a dynamic class/interface target.
pub(in crate::interpreter) fn dynamic_object_is_a(
    object: RuntimeCellHandle,
    target_class: &str,
    exclude_self: bool,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<bool>, EvalStatus> {
    let identity = values.object_identity(object)?;
    if context.dynamic_object_is_class(identity, "Closure") {
        return Ok(Some(
            !exclude_self && eval_class_like_name_matches("Closure", target_class),
        ));
    }
    let Some(class) = context.dynamic_object_class(identity) else {
        return Ok(None);
    };
    if eval_class_string_is_a(class.name(), target_class, exclude_self, context, values)? {
        return Ok(Some(true));
    }
    if context.class_native_parent_name(class.name()).is_some() {
        return values
            .object_is_a(object, target_class, exclude_self)
            .map(Some);
    }
    Ok(Some(false))
}
