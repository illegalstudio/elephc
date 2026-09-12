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
use super::super::collection_builder::EvalArrayBuilder;
use super::object_inventory::eval_get_mangled_object_vars;
use crate::context::{EvalErrorHandlerState, EvalNativeUserConstant};

const E_USER_ERROR: i64 = 256;
const E_USER_WARNING: i64 = 512;
const E_USER_NOTICE: i64 = 1_024;
const E_USER_DEPRECATED: i64 = 16_384;
const INVALID_RESOURCE_TYPE_MESSAGE: &str =
    "get_resources(): Argument #1 ($type) must be a valid resource type";
use elephc_builtin_contract::{
    constants, eval_constant_support, BackendSupport, CORE_FUNCTION_NAMES,
};

/// Evaluates one direct PHP Core introspection or handler call in source order.
pub(in crate::interpreter) fn eval_builtin_runtime_introspection_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let args = args.iter().collect::<Vec<_>>();
    with_eval_operands(&args, context, scope, values, |args, context, scope, values| {
        eval_runtime_introspection_result(name, args, context, Some(scope), values)
    })
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
            values.warning_unhandled(&format_user_error(&message, level, context))?;
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
    let suspended = context.suspend_error_handler().expect("active handler was just checked");
    let result = eval_evaluated_callable_with_values(&callback, callback_args, context, values);
    if let Some(discarded) = context.resume_error_handler(suspended) {
        values.release(discarded.callback)?;
    }
    let result = result?;
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
    let native_user = context.native_user_constant_entries();
    let entries = context.defined_constant_entries();
    if !categorize {
        let core = core_constant_array(context, values)?;
        let mut result = EvalArrayBuilder::from_owned(values, core);
        for (name, value) in &native_user {
            result.string(name, |values| eval_native_user_constant(value, values))?;
        }
        for (name, value) in entries {
            result.string(&name, |values| values.retain(value))?;
        }
        return Ok(result.finish());
    }
    let mut result = EvalArrayBuilder::assoc(values, 2)?;
    result.string("Core", |values| core_constant_array(context, values))?;
    result.string("user", |values| {
        user_constant_array(&native_user, &entries, values)
    })?;
    Ok(result.finish())
}

/// Builds PHP's `user` constant category from seeded AOT user constants plus eval `define()`s.
///
/// Seeded names come first, matching AOT's own categorized inventory, which emits the module's
/// user constants before appending the eval-context inventory. Both groups are name-sorted, and
/// the seeded registry never holds an eval-defined name, so no constant appears twice.
fn user_constant_array(
    native_user: &[(String, EvalNativeUserConstant)],
    entries: &[(String, RuntimeCellHandle)],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = EvalArrayBuilder::assoc(values, native_user.len() + entries.len())?;
    for (name, value) in native_user {
        result.string(name, |values| eval_native_user_constant(value, values))?;
    }
    for (name, value) in entries {
        result.string(name, |values| values.retain(*value))?;
    }
    Ok(result.finish())
}

/// Builds the eval-visible non-user constant category exposed under PHP's `Core` key.
///
/// This intentionally mirrors AOT's coarse `Core` versus `user` split, so the fallback may
/// contain eval-supported catalog constants whose owning `PhpModule` is not Core.
fn core_constant_array(
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let registered = context.native_global_constant_entries();
    if !registered.is_empty() {
        let mut result = EvalArrayBuilder::assoc(values, registered.len())?;
        for (name, value) in registered {
            result.string(&name, |values| eval_native_global_constant(&value, values))?;
        }
        return Ok(result.finish());
    }
    let supported = constants()
        .iter()
        .filter(|constant| {
            matches!(
                eval_constant_support(constant),
                BackendSupport::Implemented(_)
            )
        })
        .collect::<Vec<_>>();
    let mut result = EvalArrayBuilder::assoc(values, supported.len())?;
    for constant in supported {
        result.string(constant.name, |values| {
            eval_predefined_constant(constant.name, values)?.ok_or(EvalStatus::RuntimeFatal)
        })?;
    }
    Ok(result.finish())
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
    let user_names = context.defined_user_function_names();
    let mut result = EvalArrayBuilder::assoc(values, 2)?;
    result.string("internal", |values| string_array_from_iter(internal_names.iter().copied(), values))?;
    result.string("user", |values| string_array_from_iter(user_names.iter().map(String::as_str), values))?;
    Ok(result.finish())
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
    let selector = match filter.as_deref() {
        None => -1,
        Some("stream") => 0,
        Some("stream-context") => 1,
        Some("stream filter") => 2,
        Some("Unknown") => 3,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    if let Some(inventory) = values.runtime_resource_inventory(selector)? {
        return Ok(inventory);
    }
    let include_context = !entries.is_empty();
    let mut visible = vec![(0_i64, "stream"), (1, "stream"), (2, "stream")];
    if include_context {
        visible.push((
            crate::stream_resources::EVAL_DEFAULT_CONTEXT_PAYLOAD,
            "stream-context",
        ));
    }
    visible.extend(entries);
    let mut result = EvalArrayBuilder::assoc(values, visible.len())?;
    for (payload, resource_type) in visible {
        if filter.as_deref().is_some_and(|filter| filter != resource_type) {
            continue;
        }
        result.entry(|values| values.resource(payload), |values, resource| values.cast_int(resource))?;
    }
    Ok(result.finish())
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
    let mut result = EvalArrayBuilder::assoc(values, entries.len())?;
    for (name, value) in entries {
        result.string(name, |values| values.retain(*value))?;
    }
    Ok(result.finish())
}

/// Builds an indexed string array from a stable name iterator.
fn string_array_from_iter<'a>(
    names: impl IntoIterator<Item = &'a str>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let names = names.into_iter().collect::<Vec<_>>();
    let mut result = EvalArrayBuilder::indexed(values, names.len())?;
    for (position, name) in names.into_iter().enumerate() {
        result.index(position, |values| values.string(name))?;
    }
    Ok(result.finish())
}
