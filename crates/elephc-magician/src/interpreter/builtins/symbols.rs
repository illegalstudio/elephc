//! Purpose:
//! Symbol, constant, class, and language-construct builtin probes.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by core call dispatch.
//!
//! Key details:
//! - Helpers stay scoped to the eval interpreter and preserve PHP-visible runtime
//!   behavior through `RuntimeValueOps`.

use super::super::*;
use super::*;

/// Evaluates `function_exists()` and `is_callable()` inside an eval fragment.
pub(in crate::interpreter) fn eval_builtin_function_probe(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_function_probe_result(name, value, context, values)
}

/// Evaluates `function_exists()` and `is_callable()` from materialized arguments.
pub(in crate::interpreter) fn eval_function_probe_result(
    name: &str,
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exists = match name {
        "function_exists" => {
            let name = eval_function_probe_name(value, values)?;
            eval_function_probe_exists(context, &name)
        }
        "is_callable" => eval_is_callable_value(value, context, values)?,
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    values.bool_value(exists)
}

/// Evaluates `define(name, value)` for eval dynamic constant-name registration.
pub(in crate::interpreter) fn eval_builtin_define(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name, value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let name = eval_expr(name, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    let defined = eval_define_name(name, value, context, values)?;
    values.bool_value(defined)
}

/// Evaluates `defined(name)` against eval dynamic constant names.
pub(in crate::interpreter) fn eval_builtin_defined(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let name = eval_expr(name, context, scope, values)?;
    let exists = eval_defined_name(name, context, values)?;
    values.bool_value(exists)
}

/// Evaluates `define(...)` from already materialized call arguments.
pub(in crate::interpreter) fn eval_define_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name, value] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let defined = eval_define_name(*name, *value, context, values)?;
    values.bool_value(defined)
}

/// Evaluates `defined(...)` from already materialized call arguments.
pub(in crate::interpreter) fn eval_defined_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let exists = eval_defined_name(*name, context, values)?;
    values.bool_value(exists)
}

/// Normalizes and registers one eval dynamic constant name.
pub(in crate::interpreter) fn eval_define_name(
    name: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = eval_constant_name(name, values)?;
    if name.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if eval_predefined_constant_value(&name).is_some() || context.has_constant(&name) {
        values.warning(DEFINE_ALREADY_DEFINED_WARNING)?;
        return Ok(false);
    }
    let value = values.retain(value)?;
    if context.define_constant(&name, value) {
        Ok(true)
    } else {
        values.release(value)?;
        Ok(false)
    }
}

/// Normalizes and probes one eval dynamic constant name.
pub(in crate::interpreter) fn eval_defined_name(
    name: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = eval_constant_name(name, values)?;
    Ok(eval_predefined_constant_value(&name).is_some() || context.has_constant(&name))
}

/// Reads a PHP constant name from a runtime cell without changing case.
pub(in crate::interpreter) fn eval_constant_name(
    name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let name = values.string_bytes(name)?;
    String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Evaluates `class_exists(...)` against dynamic and generated class-name tables.
pub(in crate::interpreter) fn eval_builtin_class_exists(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = match args {
        [name] => eval_expr(name, context, scope, values)?,
        [name, autoload] => {
            let name = eval_expr(name, context, scope, values)?;
            let _ = eval_expr(autoload, context, scope, values)?;
            name
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let exists = eval_class_exists_name(name, context, values)?;
    values.bool_value(exists)
}

/// Evaluates `class_exists(...)` from already materialized call arguments.
pub(in crate::interpreter) fn eval_class_exists_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exists = match evaluated_args {
        [name] => eval_class_exists_name(*name, context, values)?,
        [name, _autoload] => eval_class_exists_name(*name, context, values)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(exists)
}

/// Normalizes a PHP class-name cell and probes dynamic names before generated classes.
pub(in crate::interpreter) fn eval_class_exists_name(
    name: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = values.string_bytes(name)?;
    let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
    let name = name.trim_start_matches('\\');
    if context.has_class(name) {
        return Ok(true);
    }
    values.class_exists(name)
}

/// Evaluates `class_alias(class, alias, autoload?)` against eval and generated class tables.
pub(in crate::interpreter) fn eval_builtin_class_alias(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (class, alias) = match args {
        [class, alias] => (
            eval_expr(class, context, scope, values)?,
            eval_expr(alias, context, scope, values)?,
        ),
        [class, alias, autoload] => {
            let class = eval_expr(class, context, scope, values)?;
            let alias = eval_expr(alias, context, scope, values)?;
            let _ = eval_expr(autoload, context, scope, values)?;
            (class, alias)
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_class_alias_result(&[class, alias], context, values)
}

/// Evaluates `class_alias(...)` from already materialized call arguments.
pub(in crate::interpreter) fn eval_class_alias_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (class, alias) = match evaluated_args {
        [class, alias] => (*class, *alias),
        [class, alias, _autoload] => (*class, *alias),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let class = eval_class_alias_name(class, values)?;
    let alias = eval_class_alias_name(alias, values)?;
    if alias.is_empty()
        || context.resolve_class_like_name(&alias).is_some()
        || values.class_exists(&alias)?
        || eval_runtime_interface_exists(&alias, values)?
        || values.trait_exists(&alias)?
        || values.enum_exists(&alias)?
    {
        return values.bool_value(false);
    }
    let aliased = if context.resolve_class_like_name(&class).is_some() {
        context.define_class_alias(&class, &alias)
    } else if values.enum_exists(&class)? {
        context.define_external_enum_alias(&class, &alias)
    } else if values.class_exists(&class)? {
        context.define_external_class_alias(&class, &alias)
    } else if eval_runtime_interface_exists(&class, values)? {
        context.define_external_interface_alias(&class, &alias)
    } else if values.trait_exists(&class)? {
        context.define_external_trait_alias(&class, &alias)
    } else {
        false
    };
    values.bool_value(aliased)
}

/// Reads and normalizes one `class_alias()` class-name argument.
fn eval_class_alias_name(
    name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let name = values.string_bytes(name)?;
    let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(name.trim_start_matches('\\').to_string())
}

/// Evaluates `get_declared_classes/interfaces/traits()` for eval-visible declarations.
pub(in crate::interpreter) fn eval_builtin_get_declared_symbols(
    name: &str,
    args: &[EvalExpr],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_get_declared_symbols_result(name, context, values)
}

/// Builds an indexed array for eval-visible declared class-like names.
pub(in crate::interpreter) fn eval_get_declared_symbols_result(
    name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "get_declared_classes" => {
            eval_dynamic_string_array_result(context.declared_class_names(), values)
        }
        "get_declared_interfaces" => {
            eval_dynamic_string_array_result(context.declared_interface_names(), values)
        }
        "get_declared_traits" => {
            eval_dynamic_string_array_result(context.declared_trait_names(), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds one indexed PHP array from runtime-owned strings.
fn eval_dynamic_string_array_result(
    items: &[String],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(items.len())?;
    for (index, item) in items.iter().enumerate() {
        let index = i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = values.int(index)?;
        let value = values.string(item)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Evaluates `interface_exists(...)` against generated interface-name metadata.
pub(in crate::interpreter) fn eval_builtin_interface_exists(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = match args {
        [name] => eval_expr(name, context, scope, values)?,
        [name, autoload] => {
            let name = eval_expr(name, context, scope, values)?;
            let _ = eval_expr(autoload, context, scope, values)?;
            name
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let exists = eval_interface_exists_name(name, context, values)?;
    values.bool_value(exists)
}

/// Evaluates `interface_exists(...)` from already materialized call arguments.
pub(in crate::interpreter) fn eval_interface_exists_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exists = match evaluated_args {
        [name] => eval_interface_exists_name(*name, context, values)?,
        [name, _autoload] => eval_interface_exists_name(*name, context, values)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(exists)
}

/// Normalizes a PHP interface-name cell and probes eval and generated interface metadata.
pub(in crate::interpreter) fn eval_interface_exists_name(
    name: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = values.string_bytes(name)?;
    let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
    let name = name.trim_start_matches('\\');
    Ok(context.has_interface(name) || eval_runtime_interface_exists(name, values)?)
}

/// Evaluates `trait_exists(...)` and `enum_exists(...)` against generated metadata.
pub(in crate::interpreter) fn eval_builtin_class_like_exists(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let symbol = match args {
        [symbol] => eval_expr(symbol, context, scope, values)?,
        [symbol, autoload] => {
            let symbol = eval_expr(symbol, context, scope, values)?;
            let _ = eval_expr(autoload, context, scope, values)?;
            symbol
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let exists = eval_class_like_exists_name(name, symbol, context, values)?;
    values.bool_value(exists)
}

/// Evaluates materialized `trait_exists(...)` or `enum_exists(...)` arguments.
pub(in crate::interpreter) fn eval_class_like_exists_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exists = match evaluated_args {
        [symbol] => eval_class_like_exists_name(name, *symbol, context, values)?,
        [symbol, _autoload] => eval_class_like_exists_name(name, *symbol, context, values)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(exists)
}

/// Normalizes a PHP class-like name cell and probes generated trait or enum metadata.
pub(in crate::interpreter) fn eval_class_like_exists_name(
    name: &str,
    symbol: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let symbol = values.string_bytes(symbol)?;
    let symbol = String::from_utf8(symbol).map_err(|_| EvalStatus::RuntimeFatal)?;
    let symbol = symbol.trim_start_matches('\\');
    match name {
        "trait_exists" => Ok(context.has_trait(symbol) || values.trait_exists(symbol)?),
        "enum_exists" => Ok(context.has_enum(symbol) || values.enum_exists(symbol)?),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
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

/// Evaluates PHP's `isset(...)` language construct over eval-visible values.
pub(in crate::interpreter) fn eval_builtin_isset(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    for arg in args {
        if !eval_isset_arg(arg, context, scope, values)? {
            return values.bool_value(false);
        }
    }
    values.bool_value(true)
}

/// Evaluates PHP's `empty(...)` language construct over eval-visible values.
pub(in crate::interpreter) fn eval_builtin_empty(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [arg] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let empty = eval_empty_arg(arg, context, scope, values)?;
    values.bool_value(empty)
}

/// Evaluates direct `unset(...)` calls over eval-visible variables and object properties.
pub(in crate::interpreter) fn eval_builtin_unset(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    for arg in args {
        match arg {
            EvalExpr::LoadVar(name) => {
                if let Some(replaced) = unset_scope_cell(scope, name.clone()) {
                    values.release(replaced)?;
                }
            }
            EvalExpr::PropertyGet { object, property } => {
                let object = eval_expr(object, context, scope, values)?;
                eval_property_unset_result(object, property, context, values)?;
            }
            EvalExpr::DynamicPropertyGet { object, property } => {
                let object = eval_expr(object, context, scope, values)?;
                let property = eval_dynamic_member_name(property, context, scope, values)?;
                eval_property_unset_result(object, &property, context, values)?;
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }
    values.null()
}

/// Evaluates callable `isset(...)` over already materialized values.
pub(in crate::interpreter) fn eval_isset_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    for value in evaluated_args {
        if values.is_null(*value)? {
            return values.bool_value(false);
        }
    }
    values.bool_value(true)
}

/// Evaluates callable `empty(...)` over one already materialized value.
pub(in crate::interpreter) fn eval_empty_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let empty = !values.truthy(*value)?;
    values.bool_value(empty)
}

/// Evaluates callable `unset(...)` after values have already been materialized.
pub(in crate::interpreter) fn eval_unset_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    values.null()
}

/// Evaluates one `empty` operand without warning or failing on missing variables.
pub(in crate::interpreter) fn eval_empty_arg(
    arg: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if let EvalExpr::LoadVar(name) = arg {
        let Some(value) = visible_scope_cell(context, scope, name) else {
            return Ok(true);
        };
        return Ok(!values.truthy(value)?);
    }
    if let EvalExpr::PropertyGet { object, property } = arg {
        let object = eval_expr(object, context, scope, values)?;
        if !eval_property_isset_result(object, property, context, values)? {
            return Ok(true);
        }
        let value = eval_property_get_result(object, property, context, values)?;
        return Ok(!values.truthy(value)?);
    }
    if let EvalExpr::DynamicPropertyGet { object, property } = arg {
        let object = eval_expr(object, context, scope, values)?;
        let property = eval_dynamic_member_name(property, context, scope, values)?;
        if !eval_property_isset_result(object, &property, context, values)? {
            return Ok(true);
        }
        let value = eval_property_get_result(object, &property, context, values)?;
        return Ok(!values.truthy(value)?);
    }
    if let EvalExpr::NullsafePropertyGet { object, property } = arg {
        let object = eval_expr(object, context, scope, values)?;
        if values.is_null(object)? {
            return Ok(true);
        }
        if !eval_property_isset_result(object, property, context, values)? {
            return Ok(true);
        }
        let value = eval_property_get_result(object, property, context, values)?;
        return Ok(!values.truthy(value)?);
    }
    if let EvalExpr::NullsafeDynamicPropertyGet { object, property } = arg {
        let object = eval_expr(object, context, scope, values)?;
        if values.is_null(object)? {
            return Ok(true);
        }
        let property = eval_dynamic_member_name(property, context, scope, values)?;
        if !eval_property_isset_result(object, &property, context, values)? {
            return Ok(true);
        }
        let value = eval_property_get_result(object, &property, context, values)?;
        return Ok(!values.truthy(value)?);
    }
    if let EvalExpr::StaticPropertyGet {
        class_name,
        property,
    } = arg
    {
        if !eval_static_property_isset_result(class_name, property, context, values)? {
            return Ok(true);
        }
        let value = eval_static_property_get_result(class_name, property, context, values)?;
        return Ok(!values.truthy(value)?);
    }
    if let EvalExpr::DynamicStaticPropertyGet {
        class_name,
        property,
    } = arg
    {
        let class_name = eval_expr(class_name, context, scope, values)?;
        let class_name = eval_dynamic_class_name(class_name, context, values)?;
        if !eval_static_property_isset_result(&class_name, property, context, values)? {
            return Ok(true);
        }
        let value = eval_static_property_get_result(&class_name, property, context, values)?;
        return Ok(!values.truthy(value)?);
    }
    if let EvalExpr::DynamicStaticPropertyNameGet {
        class_name,
        property,
    } = arg
    {
        let class_name = eval_expr(class_name, context, scope, values)?;
        let class_name = eval_dynamic_class_name(class_name, context, values)?;
        let property = eval_dynamic_member_name(property, context, scope, values)?;
        if !eval_static_property_isset_result(&class_name, &property, context, values)? {
            return Ok(true);
        }
        let value = eval_static_property_get_result(&class_name, &property, context, values)?;
        return Ok(!values.truthy(value)?);
    }
    if let EvalExpr::ArrayGet { array, index } = arg {
        let array = eval_expr(array, context, scope, values)?;
        let index = eval_expr(index, context, scope, values)?;
        if values.type_tag(array)? == EVAL_TAG_OBJECT {
            return eval_array_access_empty_result(array, index, context, values);
        }
        let value = values.array_get(array, index)?;
        return Ok(!values.truthy(value)?);
    }
    let value = eval_expr(arg, context, scope, values)?;
    Ok(!values.truthy(value)?)
}

/// Evaluates one `isset` operand without allocating a null cell for missing variables.
pub(in crate::interpreter) fn eval_isset_arg(
    arg: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if let EvalExpr::LoadVar(name) = arg {
        let Some(value) = visible_scope_cell(context, scope, name) else {
            return Ok(false);
        };
        return Ok(!values.is_null(value)?);
    }
    if let EvalExpr::PropertyGet { object, property } = arg {
        let object = eval_expr(object, context, scope, values)?;
        return eval_property_isset_result(object, property, context, values);
    }
    if let EvalExpr::DynamicPropertyGet { object, property } = arg {
        let object = eval_expr(object, context, scope, values)?;
        let property = eval_dynamic_member_name(property, context, scope, values)?;
        return eval_property_isset_result(object, &property, context, values);
    }
    if let EvalExpr::NullsafePropertyGet { object, property } = arg {
        let object = eval_expr(object, context, scope, values)?;
        if values.is_null(object)? {
            return Ok(false);
        }
        return eval_property_isset_result(object, property, context, values);
    }
    if let EvalExpr::NullsafeDynamicPropertyGet { object, property } = arg {
        let object = eval_expr(object, context, scope, values)?;
        if values.is_null(object)? {
            return Ok(false);
        }
        let property = eval_dynamic_member_name(property, context, scope, values)?;
        return eval_property_isset_result(object, &property, context, values);
    }
    if let EvalExpr::StaticPropertyGet {
        class_name,
        property,
    } = arg
    {
        return eval_static_property_isset_result(class_name, property, context, values);
    }
    if let EvalExpr::DynamicStaticPropertyGet {
        class_name,
        property,
    } = arg
    {
        let class_name = eval_expr(class_name, context, scope, values)?;
        let class_name = eval_dynamic_class_name(class_name, context, values)?;
        return eval_static_property_isset_result(&class_name, property, context, values);
    }
    if let EvalExpr::DynamicStaticPropertyNameGet {
        class_name,
        property,
    } = arg
    {
        let class_name = eval_expr(class_name, context, scope, values)?;
        let class_name = eval_dynamic_class_name(class_name, context, values)?;
        let property = eval_dynamic_member_name(property, context, scope, values)?;
        return eval_static_property_isset_result(&class_name, &property, context, values);
    }
    if let EvalExpr::ArrayGet { array, index } = arg {
        let array = eval_expr(array, context, scope, values)?;
        let index = eval_expr(index, context, scope, values)?;
        if values.type_tag(array)? == EVAL_TAG_OBJECT {
            return eval_array_access_isset_result(array, index, context, values);
        }
        let value = values.array_get(array, index)?;
        return Ok(!values.is_null(value)?);
    }
    let value = eval_expr(arg, context, scope, values)?;
    Ok(!values.is_null(value)?)
}

/// Evaluates `empty($object[$key])` through `ArrayAccess::offsetExists()` and `offsetGet()`.
fn eval_array_access_empty_result(
    object: RuntimeCellHandle,
    index: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if !eval_array_access_isset_result(object, index, context, values)? {
        return Ok(true);
    }
    let value = eval_array_get_result(object, index, context, values)?;
    Ok(!values.truthy(value)?)
}

/// Evaluates `isset($object[$key])` through `ArrayAccess::offsetExists()`.
fn eval_array_access_isset_result(
    object: RuntimeCellHandle,
    index: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if !eval_array_access_object_matches(object, context, values)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let result = eval_method_call_result(object, "offsetExists", vec![index], context, values)?;
    let exists = values.truthy(result)?;
    values.release(result)?;
    Ok(exists)
}

/// Returns true when a PHP function name is visible to eval builtin probes.
pub(in crate::interpreter) fn eval_function_probe_exists(
    context: &ElephcEvalContext,
    name: &str,
) -> bool {
    !name.contains("::") && (context.has_function(name) || eval_php_visible_builtin_exists(name))
}

/// Reads and normalizes a function-probe string argument.
fn eval_function_probe_name(
    name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let name = values.string_bytes(name)?;
    let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(name.trim_start_matches('\\').to_ascii_lowercase())
}

/// Returns whether one runtime value is callable from the current eval scope.
fn eval_is_callable_value(
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Ok(callback) = eval_callable(value, context, values) else {
        return Ok(false);
    };
    eval_callable_probe_exists(&callback, context, values)
}

/// Returns whether a normalized eval callback has an invokable target.
fn eval_callable_probe_exists(
    callback: &EvaluatedCallable,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    match callback {
        EvaluatedCallable::Named(name) => {
            Ok(context.has_closure(name) || eval_function_probe_exists(context, name))
        }
        EvaluatedCallable::InvokableObject { object } => {
            eval_object_method_callable_probe(*object, "__invoke", context, values)
        }
        EvaluatedCallable::ObjectMethod {
            object,
            method,
            native_class,
            ..
        } => {
            if native_class.is_some() {
                Ok(true)
            } else {
                eval_object_method_callable_probe(*object, method, context, values)
            }
        }
        EvaluatedCallable::StaticMethod {
            class_name,
            method,
            native_class,
            ..
        } => {
            if native_class.is_some() {
                Ok(true)
            } else {
                eval_static_method_callable_probe(class_name, method, context, values)
            }
        }
    }
}

/// Returns whether one object method can be called from the current eval scope.
fn eval_object_method_callable_probe(
    object: RuntimeCellHandle,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return Ok(false);
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        return eval_aot_object_method_callable_probe(object, method_name, context, values);
    };
    if eval_enum_static_builtin_applies(class.name(), method_name, context).is_some() {
        return Ok(true);
    }
    let Some((declaring_class, method)) =
        eval_dynamic_method_for_call(class.name(), method_name, context)
    else {
        if eval_dynamic_class_native_method_callable_probe(
            class.name(),
            method_name,
            context,
            values,
        )? {
            return Ok(true);
        }
        return Ok(eval_instance_magic_method_callable_probe(
            class.name(),
            context,
        ));
    };
    if method.is_abstract() {
        return Ok(false);
    }
    if method_name.eq_ignore_ascii_case("__invoke") {
        return Ok(true);
    }
    Ok(validate_eval_member_access(&declaring_class, method.visibility(), context).is_ok()
        || eval_instance_magic_method_callable_probe(class.name(), context))
}

/// Returns whether one static method can be called from the current eval scope.
fn eval_static_method_callable_probe(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    if eval_enum_static_builtin_applies(&class_name, method_name, context).is_some() {
        return Ok(true);
    }
    if let Some((declaring_class, method)) = context.class_method(&class_name, method_name) {
        if !method.is_static() || method.is_abstract() {
            return Ok(false);
        }
        return Ok(validate_eval_member_access(&declaring_class, method.visibility(), context)
            .is_ok()
            || eval_static_magic_method_callable_probe(&class_name, context));
    }
    if context.has_class(&class_name)
        || context.has_interface(&class_name)
        || context.has_trait(&class_name)
        || context.has_enum(&class_name)
    {
        if eval_static_magic_method_callable_probe(&class_name, context) {
            return Ok(true);
        }
        if let Some(parent) = context.class_native_parent_name(&class_name) {
            return eval_aot_static_method_callable_probe(
                &parent,
                method_name,
                context,
                values,
            );
        }
        return Ok(false);
    }
    eval_aot_static_method_callable_probe(&class_name, method_name, context, values)
}

/// Returns whether a generated/AOT object method can be called from the current eval scope.
fn eval_aot_object_method_callable_probe(
    object: RuntimeCellHandle,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let class_name = runtime_object_class_name(object, values)?;
    eval_aot_class_method_callable_probe(&class_name, method_name, context, values)
}

/// Returns whether an eval class can call a generated/AOT parent instance method.
fn eval_dynamic_class_native_method_callable_probe(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some(parent) = context.class_native_parent_name(class_name) else {
        return Ok(false);
    };
    eval_aot_class_method_callable_probe(&parent, method_name, context, values)
}

/// Returns whether one generated/AOT class instance method can be called from eval.
fn eval_aot_class_method_callable_probe(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some((declaring_class, visibility, _, is_abstract)) =
        eval_aot_method_dispatch_metadata_in_hierarchy(&class_name, method_name, context, values)?
    else {
        return eval_aot_instance_magic_method_callable_probe(&class_name, context, values);
    };
    if is_abstract {
        return Ok(false);
    }
    Ok(validate_eval_member_access(&declaring_class, visibility, context).is_ok()
        || eval_aot_instance_magic_method_callable_probe(&class_name, context, values)?)
}

/// Returns whether a generated/AOT static method can be called from the current eval scope.
fn eval_aot_static_method_callable_probe(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some((declaring_class, visibility, is_static, is_abstract)) =
        eval_aot_method_dispatch_metadata_in_hierarchy(class_name, method_name, context, values)?
    else {
        return eval_aot_static_magic_method_callable_probe(class_name, context, values);
    };
    if !is_static || is_abstract {
        return Ok(false);
    }
    Ok(validate_eval_member_access(&declaring_class, visibility, context).is_ok()
        || eval_aot_static_magic_method_callable_probe(class_name, context, values)?)
}

/// Returns whether a generated/AOT class has a callable instance `__call()` fallback.
fn eval_aot_instance_magic_method_callable_probe(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(eval_aot_method_dispatch_metadata_in_hierarchy(class_name, "__call", context, values)?
        .is_some_and(|(_, _, is_static, is_abstract)| !is_static && !is_abstract))
}

/// Returns whether a generated/AOT class has a callable static `__callStatic()` fallback.
fn eval_aot_static_magic_method_callable_probe(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(
        eval_aot_method_dispatch_metadata_in_hierarchy(
            class_name,
            "__callStatic",
            context,
            values,
        )?
        .is_some_and(|(_, _, is_static, is_abstract)| is_static && !is_abstract),
    )
}

/// Returns whether an eval class has a callable instance `__call()` fallback.
fn eval_instance_magic_method_callable_probe(
    class_name: &str,
    context: &ElephcEvalContext,
) -> bool {
    context
        .class_method(class_name, "__call")
        .is_some_and(|(_, method)| !method.is_static() && !method.is_abstract())
}

/// Returns whether an eval class has a callable static `__callStatic()` fallback.
fn eval_static_magic_method_callable_probe(class_name: &str, context: &ElephcEvalContext) -> bool {
    context
        .class_method(class_name, "__callStatic")
        .is_some_and(|(_, method)| method.is_static() && !method.is_abstract())
}
