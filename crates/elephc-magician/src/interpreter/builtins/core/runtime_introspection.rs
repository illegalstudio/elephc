//! Purpose:
//! Implements PHP Core call-stack, handler, declaration, object, and resource introspection.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and evaluated-argument dispatch.
//!
//! Key details:
//! - Handler stacks own retained callback cells and return independent retained values.
//! - Introspection results are materialized from the active eval context and scope.

use super::super::super::*;
use super::backtrace_runtime::{eval_debug_backtrace, eval_debug_print_backtrace};
use crate::context::EvalErrorHandlerState;
use std::collections::HashSet;

const E_USER_ERROR: i64 = 256;
const E_USER_WARNING: i64 = 512;
const E_USER_NOTICE: i64 = 1_024;
const E_USER_DEPRECATED: i64 = 16_384;
const INVALID_RESOURCE_TYPE_MESSAGE: &str =
    "get_resources(): Argument #1 ($type) must be a valid resource type";
use elephc_builtin_contract::CORE_FUNCTION_NAMES;

/// Evaluates one direct PHP Core introspection or handler call in source order.
pub(in crate::interpreter) fn eval_builtin_runtime_introspection_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_runtime_introspection_result(name, &evaluated, context, Some(scope), values)
}

/// Evaluates one PHP Core introspection or handler call from materialized arguments.
pub(in crate::interpreter) fn eval_runtime_introspection_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_runtime_introspection_result(name, evaluated_args, context, None, values)
}

/// Dispatches the shared result implementation for the supported Core operation.
fn eval_runtime_introspection_result(
    name: &str,
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    scope: Option<&ElephcEvalScope>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "debug_backtrace" => eval_debug_backtrace(args, context, values),
        "debug_print_backtrace" => eval_debug_print_backtrace(args, context, values),
        "error_reporting" => eval_error_reporting(args, context, values),
        "restore_error_handler" => eval_restore_error_handler(args, context, values),
        "restore_exception_handler" => eval_restore_exception_handler(args, context, values),
        "set_error_handler" => eval_set_error_handler(args, context, scope, values),
        "set_exception_handler" => eval_set_exception_handler(args, context, scope, values),
        "trigger_error" | "user_error" => eval_trigger_error(args, context, values),
        "get_defined_constants" => eval_get_defined_constants(args, context, values),
        "get_defined_functions" => eval_get_defined_functions(args, context, values),
        "get_defined_vars" => eval_get_defined_vars(args, context, scope, values),
        "get_extension_funcs" => eval_get_extension_funcs(args, values),
        "get_included_files" | "get_required_files" => {
            eval_get_included_files(args, context, values)
        }
        "get_mangled_object_vars" => eval_get_mangled_object_vars(args, context, values),
        "get_resources" => eval_get_resources(args, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Gets or replaces the active runtime error reporting mask.
fn eval_error_reporting(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 1 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let replacement = match args.first().copied() {
        None => None,
        Some(value) if values.is_null(value)? => None,
        Some(value) => Some(eval_int_value(value, values)?),
    };
    let previous = match values.runtime_error_reporting(replacement) {
        Ok(previous) => previous,
        Err(EvalStatus::UnsupportedConstruct) => context.update_error_reporting(replacement),
        Err(status) => return Err(status),
    };
    values.int(previous)
}

/// Installs a user error handler and returns the previously active callback.
fn eval_set_error_handler(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    scope: Option<&ElephcEvalScope>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(1..=2).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let levels = optional_int_arg(
        args.get(1).copied(),
        crate::eval_php_profile::eval_all_error_mask(),
        values,
    )?;
    let replacement = if values.is_null(args[0])? {
        None
    } else {
        normalize_handler(args[0], context, scope, values)?;
        Some(args[0])
    };
    match values.runtime_error_handler_set(replacement, levels) {
        Ok(Some(previous)) => Ok(previous),
        Ok(None) => values.null(),
        Err(EvalStatus::UnsupportedConstruct) => {
            let replacement = match replacement {
                Some(callback) => Some(EvalErrorHandlerState {
                    callback: values.retain(callback)?,
                    levels,
                }),
                None => None,
            };
            let previous = context.push_error_handler(replacement);
            return_previous_error_handler(previous, values)
        }
        Err(status) => Err(status),
    }
}

/// Installs an uncaught-exception handler and returns the previous callback.
fn eval_set_exception_handler(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    scope: Option<&ElephcEvalScope>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [callback] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let replacement = if values.is_null(*callback)? {
        None
    } else {
        normalize_handler(*callback, context, scope, values)?;
        Some(*callback)
    };
    match values.runtime_exception_handler_set(replacement) {
        Ok(Some(previous)) => Ok(previous),
        Ok(None) => values.null(),
        Err(EvalStatus::UnsupportedConstruct) => {
            let replacement = match replacement {
                Some(callback) => Some(values.retain(callback)?),
                None => None,
            };
            let previous = context.push_exception_handler(replacement);
            match previous {
                Some(previous) => values.retain(previous),
                None => values.null(),
            }
        }
        Err(status) => Err(status),
    }
}

/// Restores the previous user error handler and releases the discarded callback.
fn eval_restore_error_handler(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    match values.runtime_error_handler_restore() {
        Ok(()) => {}
        Err(EvalStatus::UnsupportedConstruct) => {
            if let Some(discarded) = context.restore_error_handler_state() {
                values.release(discarded.callback)?;
            }
        }
        Err(status) => return Err(status),
    }
    values.bool_value(true)
}

/// Restores the previous exception handler and releases the discarded callback.
fn eval_restore_exception_handler(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    match values.runtime_exception_handler_restore() {
        Ok(()) => {}
        Err(EvalStatus::UnsupportedConstruct) => {
            if let Some(discarded) = context.restore_exception_handler_state() {
                values.release(discarded)?;
            }
        }
        Err(status) => return Err(status),
    }
    values.bool_value(true)
}

/// Validates one handler callback using the direct call's lexical scope when available.
fn normalize_handler(
    callback: RuntimeCellHandle,
    context: &ElephcEvalContext,
    scope: Option<&ElephcEvalScope>,
    values: &mut impl RuntimeValueOps,
) -> Result<EvaluatedCallable, EvalStatus> {
    match scope {
        Some(scope) => eval_callable_from_scope(callback, context, scope, values),
        None => eval_callable(callback, context, values),
    }
}

/// Returns an independent copy of a previous error handler or PHP null.
fn return_previous_error_handler(
    previous: Option<EvalErrorHandlerState>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match previous {
        Some(previous) => values.retain(previous.callback),
        None => values.null(),
    }
}

/// Dispatches one PHP user-level diagnostic through the active handler or warning path.
fn eval_trigger_error(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(1..=2).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let message = String::from_utf8(values.string_bytes(args[0])?)
        .map_err(|_| EvalStatus::RuntimeFatal)?;
    let level = optional_int_arg(args.get(1).copied(), E_USER_NOTICE, values)?;
    if !matches!(level, E_USER_ERROR | E_USER_WARNING | E_USER_NOTICE | E_USER_DEPRECATED) {
        return eval_throw_builtin_value_error(
            "trigger_error(): Argument #2 ($error_level) must be one of E_USER_ERROR, E_USER_WARNING, E_USER_NOTICE, or E_USER_DEPRECATED",
            context,
            values,
        );
    }
    let handled = dispatch_user_error_handler(&message, level, context, values)?;
    let reporting_mask = match values.runtime_error_reporting(None) {
        Ok(mask) => mask,
        Err(EvalStatus::UnsupportedConstruct) => context.error_reporting_mask(),
        Err(status) => return Err(status),
    };
    if !handled {
        if reporting_mask & level != 0 {
            values.warning(&format_user_error(&message, level, context))?;
        }
        if level == E_USER_ERROR {
            return Err(EvalStatus::UserFatal);
        }
    }
    values.bool_value(true)
}

/// Formats PHP's default user diagnostic with its category and source location.
fn format_user_error(message: &str, level: i64, context: &ElephcEvalContext) -> String {
    let category = match level {
        E_USER_ERROR => "Fatal error",
        E_USER_WARNING => "Warning",
        E_USER_DEPRECATED => "Deprecated",
        _ => "Notice",
    };
    let (file, _, line, _) = context.call_site();
    format!("{category}: {message} in {file} on line {line}\n")
}

/// Invokes the active handler and returns whether it suppressed the default diagnostic.
fn dispatch_user_error_handler(
    message: &str,
    level: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let (file, _, line, _) = context.call_site();
    let callback_args = vec![
        values.int(level)?,
        values.string(message)?,
        values.string(&file)?,
        values.int(line)?,
    ];
    match values.runtime_error_handler_dispatch(level, &callback_args) {
        Ok(result) => {
            for argument in callback_args {
                values.release(argument)?;
            }
            let Some(result) = result else {
                return Ok(false);
            };
            let falls_through =
                values.type_tag(result)? == EVAL_TAG_BOOL && !values.truthy(result)?;
            values.release(result)?;
            return Ok(!falls_through);
        }
        Err(EvalStatus::UnsupportedConstruct) => {}
        Err(status) => {
            for argument in callback_args {
                values.release(argument)?;
            }
            return Err(status);
        }
    }
    let Some(handler) = context.error_handler_state() else {
        for argument in callback_args {
            values.release(argument)?;
        }
        return Ok(false);
    };
    if handler.levels & level == 0 {
        for argument in callback_args {
            values.release(argument)?;
        }
        return Ok(false);
    }
    let callback = eval_callable(handler.callback, context, values)?;
    let result = eval_evaluated_callable_with_values(&callback, callback_args, context, values)?;
    let falls_through = values.type_tag(result)? == EVAL_TAG_BOOL && !values.truthy(result)?;
    values.release(result)?;
    Ok(!falls_through)
}

/// Returns eval-visible user constants, optionally nested under PHP's `user` category.
fn eval_get_defined_constants(
    args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 1 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let categorize = match args.first().copied() {
        Some(value) => values.truthy(value)?,
        None => false,
    };
    let entries = context.defined_constant_entries();
    let user = assoc_from_entries(&entries, values)?;
    let core = core_constant_array(values)?;
    if !categorize {
        let mut result = core;
        for (name, value) in entries {
            let key = values.string(&name)?;
            let value = values.retain(value)?;
            result = values.array_set(result, key, value)?;
        }
        return Ok(result);
    }
    let mut result = values.assoc_new(2)?;
    result = set_assoc_cell(result, "Core", core, values)?;
    result = set_assoc_cell(result, "user", user, values)?;
    Ok(result)
}

/// Builds the eval-visible PHP Core predefined constant category.
fn core_constant_array(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let integer_constants = [
        ("E_ERROR", 1),
        ("E_WARNING", 2),
        ("E_PARSE", 4),
        ("E_NOTICE", 8),
        ("E_CORE_ERROR", 16),
        ("E_CORE_WARNING", 32),
        ("E_COMPILE_ERROR", 64),
        ("E_COMPILE_WARNING", 128),
        ("E_USER_ERROR", 256),
        ("E_USER_WARNING", 512),
        ("E_USER_NOTICE", 1_024),
        ("E_STRICT", 2_048),
        ("E_RECOVERABLE_ERROR", 4_096),
        ("E_DEPRECATED", 8_192),
        ("E_USER_DEPRECATED", 16_384),
        ("E_ALL", crate::eval_php_profile::eval_all_error_mask()),
        ("DEBUG_BACKTRACE_PROVIDE_OBJECT", 1),
        ("DEBUG_BACKTRACE_IGNORE_ARGS", 2),
        ("PHP_VERSION_ID", i64::from(crate::eval_php_profile::eval_php_version_id())),
        ("PHP_MAJOR_VERSION", 8),
        ("PHP_MINOR_VERSION", i64::from(crate::eval_php_profile::eval_php_minor_version())),
        ("PHP_RELEASE_VERSION", 0),
        ("PHP_INT_MAX", i64::MAX),
        ("PHP_INT_MIN", i64::MIN),
        ("PHP_INT_SIZE", std::mem::size_of::<i64>() as i64),
        ("PHP_MAXPATHLEN", 4_096),
    ];
    let mut result = values.assoc_new(integer_constants.len() + 7)?;
    for (name, value) in integer_constants {
        result = set_assoc_int(result, name, value, values)?;
    }
    result = set_assoc_string(
        result,
        "PHP_VERSION",
        crate::eval_php_profile::eval_php_version_string(),
        values,
    )?;
    result = set_assoc_string(result, "PHP_EXTRA_VERSION", "", values)?;
    result = set_assoc_string(result, "PHP_SAPI", "cli", values)?;
    result = set_assoc_string(result, "PHP_EOL", "\n", values)?;
    result = set_assoc_string(result, "DIRECTORY_SEPARATOR", "/", values)?;
    result = set_assoc_string(result, "PATH_SEPARATOR", ":", values)?;
    let os = if cfg!(target_os = "macos") { "Darwin" } else { "Linux" };
    set_assoc_string(result, "PHP_OS", os, values)
}

/// Returns internal registry names and user-declared eval function names.
fn eval_get_defined_functions(
    args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 1 {
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Some(exclude_disabled) = args.first().copied() {
        let _ = values.truthy(exclude_disabled)?;
    }
    let internal_names = eval_php_visible_builtin_function_names();
    let internal = string_array_from_iter(internal_names.iter().copied(), values)?;
    let user_names = context.defined_user_function_names();
    let user = string_array_from_iter(user_names.iter().map(String::as_str), values)?;
    let mut result = values.assoc_new(2)?;
    result = set_assoc_cell(result, "internal", internal, values)?;
    result = set_assoc_cell(result, "user", user, values)?;
    Ok(result)
}

/// Returns variables visible in the direct caller scope.
fn eval_get_defined_vars(
    args: &[RuntimeCellHandle],
    _context: &ElephcEvalContext,
    scope: Option<&ElephcEvalScope>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let entries = scope.map(ElephcEvalScope::visible_entries).unwrap_or_default();
    assoc_from_entries(&entries, values)
}

/// Returns variables visible to an explicit `call_user_func*` invocation.
pub(in crate::interpreter) fn eval_get_defined_vars_from_scope(
    args: &[RuntimeCellHandle],
    scope: &ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let entries = scope.visible_entries();
    assoc_from_entries(&entries, values)
}

/// Returns the Core extension's supported function list or false for unknown extensions.
fn eval_get_extension_funcs(
    args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [extension] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let extension = String::from_utf8(values.string_bytes(*extension)?)
        .map_err(|_| EvalStatus::RuntimeFatal)?;
    if !extension.eq_ignore_ascii_case("core") {
        return values.bool_value(false);
    }
    string_array_from_iter(CORE_FUNCTION_NAMES.iter().copied(), values)
}

/// Returns the active main file and successfully included eval files.
fn eval_get_included_files(
    args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let names = context.included_file_names();
    string_array_from_iter(names.iter().map(String::as_str), values)
}

/// Returns all initialized object properties under visibility-mangled PHP keys.
fn eval_get_mangled_object_vars(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [object] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if values.type_tag(*object)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    let identity = values.object_identity(*object)?;
    let Some(class_name) = context.dynamic_object_class_name(identity) else {
        return eval_get_object_vars_result(args, context, values);
    };
    let initial_capacity = values.object_property_len(*object)?;
    let mut result = values.assoc_new(initial_capacity)?;
    let mut storage_names = HashSet::new();
    for class in context.class_chain(&class_name) {
        for property in class.properties() {
            if property.is_static() {
                continue;
            }
            let storage = eval_instance_property_storage_name(class.name(), property);
            storage_names.insert(storage.clone());
            if !values.property_is_initialized(*object, &storage)? {
                continue;
            }
            let key_name = match property.visibility() {
                EvalVisibility::Public => property.name().to_string(),
                EvalVisibility::Protected => format!("\0*\0{}", property.name()),
                EvalVisibility::Private => format!(
                    "\0{}\0{}",
                    class.name().trim_start_matches('\\'),
                    property.name()
                ),
            };
            let key = values.string(&key_name)?;
            let value = values.property_get(*object, &storage)?;
            result = values.array_set(result, key, value)?;
        }
    }
    let property_count = values.object_property_len(*object)?;
    for position in 0..property_count {
        let key = values.object_property_iter_key(*object, position)?;
        let key_bytes = values.string_bytes(key)?;
        values.release(key)?;
        let key_name = String::from_utf8(key_bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
        if storage_names.contains(&key_name) {
            continue;
        }
        let key = values.string(&key_name)?;
        let value = values.property_get(*object, &key_name)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Returns all live eval resources, optionally restricted to one type name.
fn eval_get_resources(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 1 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let filter = match args.first().copied() {
        None => None,
        Some(value) if values.is_null(value)? => None,
        Some(value) => Some(
            String::from_utf8(values.string_bytes(value)?).map_err(|_| EvalStatus::RuntimeFatal)?,
        ),
    };
    if filter.as_deref().is_some_and(|filter| {
        !matches!(filter, "stream" | "stream-context" | "stream filter" | "Unknown")
    }) {
        return eval_throw_builtin_value_error(
            INVALID_RESOURCE_TYPE_MESSAGE,
            context,
            values,
        );
    }
    let entries = context.stream_resources().resource_entries();
    let include_context = !entries.is_empty();
    let mut visible = vec![(0_i64, "stream"), (1, "stream"), (2, "stream")];
    if include_context {
        visible.push((
            crate::stream_resources::EVAL_DEFAULT_CONTEXT_PAYLOAD,
            "stream-context",
        ));
    }
    visible.extend(entries);
    let mut result = values.assoc_new(visible.len())?;
    for (payload, resource_type) in visible {
        if filter.as_deref().is_some_and(|filter| filter != resource_type) {
            continue;
        }
        let resource = values.resource(payload)?;
        let key = values.cast_int(resource)?;
        result = values.array_set(result, key, resource)?;
    }
    Ok(result)
}

/// Reads an optional integer argument, applying the PHP default when absent.
fn optional_int_arg(
    value: Option<RuntimeCellHandle>,
    default: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    value.map_or(Ok(default), |value| eval_int_value(value, values))
}

/// Builds a string-keyed associative array while retaining source values.
fn assoc_from_entries(
    entries: &[(String, RuntimeCellHandle)],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.assoc_new(entries.len())?;
    for (name, value) in entries {
        let key = values.string(name)?;
        let value = values.retain(*value)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Builds an indexed string array from a stable name iterator.
fn string_array_from_iter<'a>(
    names: impl IntoIterator<Item = &'a str>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let names = names.into_iter().collect::<Vec<_>>();
    let mut result = values.array_new(names.len())?;
    for (position, name) in names.into_iter().enumerate() {
        let key = values.int(position as i64)?;
        let value = values.string(name)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Inserts one string value under a string key.
fn set_assoc_string(
    result: RuntimeCellHandle,
    key: &str,
    value: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = values.string(value)?;
    set_assoc_cell(result, key, value, values)
}

/// Inserts one integer value under a string key.
fn set_assoc_int(
    result: RuntimeCellHandle,
    key: &str,
    value: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = values.int(value)?;
    set_assoc_cell(result, key, value, values)
}

/// Inserts one materialized value under a string key.
fn set_assoc_cell(
    result: RuntimeCellHandle,
    key: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(key)?;
    values.array_set(result, key, value)
}
