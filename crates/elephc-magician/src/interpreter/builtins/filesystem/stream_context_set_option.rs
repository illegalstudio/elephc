//! Purpose:
//! Declarative eval registry entry for `stream_context_set_option`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Owns both `stream_context_set_option($context, $options)` and the
//!   four-argument nested option form.

eval_builtin! {
    contract: "stream_context_set_option",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// php-src's `E_DEPRECATED` text for the two-argument `stream_context_set_option()` form.
///
/// The notice counts ARGUMENTS, not types: `stream_context_set_option($c, $array, null)` is a
/// three-argument call and stays quiet, MEASURED on `php -n` 8.5.6. It arrived in php 8.3, so
/// the older profiles this eval can be asked to imitate must not print it.
const STREAM_CONTEXT_SET_OPTION_TWO_ARG_DEPRECATION: &str =
    "Deprecated: Calling stream_context_set_option() with 2 arguments is deprecated, \
     use stream_context_set_options() instead\n";

/// The first `PHP_VERSION_ID` that deprecates the two-argument form.
const STREAM_CONTEXT_SET_OPTION_DEPRECATION_SINCE: u32 = 80300;

/// php-src's `ValueError` when a STRING wrapper reaches the three-argument form.
const STREAM_CONTEXT_SET_OPTION_VALUE_REQUIRED: &str =
    "stream_context_set_option(): Argument #4 ($value) must be provided when argument #2 \
     ($wrapper_or_options) is a string";

/// php-src's `ValueError` when a STRING wrapper is paired with a null `$option_name`.
const STREAM_CONTEXT_SET_OPTION_NAME_CANNOT_BE_NULL: &str =
    "stream_context_set_option(): Argument #3 ($option_name) cannot be null when argument #2 \
     ($wrapper_or_options) is a string";

/// php-src's `ValueError` when an ARRAY wrapper is paired with a non-null `$option_name`.
const STREAM_CONTEXT_SET_OPTION_NAME_MUST_BE_NULL: &str =
    "stream_context_set_option(): Argument #3 ($option_name) must be null when argument #2 \
     ($wrapper_or_options) is an array";

/// php-src's `ValueError` when an ARRAY wrapper is paired with a fourth `$value` argument.
const STREAM_CONTEXT_SET_OPTION_VALUE_FORBIDDEN: &str =
    "stream_context_set_option(): Argument #4 ($value) cannot be provided when argument #2 \
     ($wrapper_or_options) is an array";

/// Evaluates `stream_context_set_option($context, ...)`.
pub(in crate::interpreter) fn eval_stream_context_set_option_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=4).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_stream_context_set_option_declared_values_result(&evaluated, context, values)
}

/// Stores context options from already evaluated arguments.
///
/// php refuses most of the three- and four-argument shapes; the arity alone does not decide,
/// because the refusal depends on whether `$wrapper_or_options` is an ARRAY or a STRING.
/// MEASURED on `php -n` 8.5.6:
///
/// ```text
/// stream_context_set_option($c, ['http' => [...]])        E_DEPRECATED, then bool(true)
/// stream_context_set_option($c, ['http' => [...]], null)  bool(true), and NO deprecation
/// stream_context_set_option($c, ['http' => [...]], 'x')   ValueError: Argument #3 ($option_name) must be null when argument #2 ($wrapper_or_options) is an array
/// stream_context_set_option($c, ['http' => [...]], null, 5) ValueError: Argument #4 ($value) cannot be provided when argument #2 ($wrapper_or_options) is an array
/// stream_context_set_option($c, 'http', 'header')         ValueError: Argument #4 ($value) must be provided when argument #2 ($wrapper_or_options) is a string
/// stream_context_set_option($c, 'http', null)             ValueError: Argument #3 ($option_name) cannot be null when argument #2 ($wrapper_or_options) is a string
/// stream_context_set_option($c, 'http', null, 'v')        ValueError: Argument #3 ($option_name) cannot be null when argument #2 ($wrapper_or_options) is a string
/// ```
///
/// The three-argument form used to be an uncatchable `RuntimeFatal`, and the two-argument
/// form printed nothing.
pub(in crate::interpreter) fn eval_stream_context_set_option_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (Some(&stream_context), Some(&wrapper)) =
        (evaluated_args.first(), evaluated_args.get(1))
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let wrapper_is_array = values.is_array_like(wrapper)?;
    let option_is_null = match evaluated_args.get(2) {
        Some(option) => values.type_tag(*option)? == EVAL_TAG_NULL,
        None => true,
    };
    // The deprecation counts arguments and fires BEFORE the shape is judged: a two-argument
    // call with a STRING wrapper prints it and THEN raises the ValueError, measured.
    if evaluated_args.len() == 2
        && crate::eval_php_profile::eval_php_version_id()
            >= STREAM_CONTEXT_SET_OPTION_DEPRECATION_SINCE
    {
        values.warning(STREAM_CONTEXT_SET_OPTION_TWO_ARG_DEPRECATION)?;
    }
    match (evaluated_args.len(), wrapper_is_array) {
        (2, true) => {
            eval_stream_context_set_options_result(stream_context, wrapper, context, values)
        }
        (3, true) if option_is_null => {
            eval_stream_context_set_options_result(stream_context, wrapper, context, values)
        }
        (3, true) => eval_stream_value_error(
            STREAM_CONTEXT_SET_OPTION_NAME_MUST_BE_NULL,
            context,
            values,
        ),
        (4, true) => eval_stream_value_error(
            STREAM_CONTEXT_SET_OPTION_VALUE_FORBIDDEN,
            context,
            values,
        ),
        (_, false) if option_is_null => eval_stream_value_error(
            STREAM_CONTEXT_SET_OPTION_NAME_CANNOT_BE_NULL,
            context,
            values,
        ),
        (3, false) => eval_stream_value_error(
            STREAM_CONTEXT_SET_OPTION_VALUE_REQUIRED,
            context,
            values,
        ),
        (4, false) => eval_stream_context_set_option_result(
            stream_context,
            wrapper,
            evaluated_args[2],
            evaluated_args[3],
            context,
            values,
        ),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Stores a materialized options array on a stream context resource.
pub(in crate::interpreter) fn eval_stream_context_set_options_result(
    stream_context: RuntimeCellHandle,
    options: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_context_resource_id(stream_context, values)?;
    let options = super::stream_context_create::eval_stream_context_options_arg(Some(options), values)?;
    values.bool_value(
        context
            .stream_resources_mut()
            .set_stream_context_options(id, options),
    )
}

/// Stores one nested `options[wrapper][option] = value` entry on a stream context.
pub(in crate::interpreter) fn eval_stream_context_set_option_result(
    stream_context: RuntimeCellHandle,
    wrapper: RuntimeCellHandle,
    option: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_context_resource_id(stream_context, values)?;
    let wrapper = values.cast_string(wrapper)?;
    let option = values.cast_string(option)?;
    let options = match context.stream_resources().stream_context_options(id) {
        Some(options) => options,
        None => values.assoc_new(1)?,
    };
    let wrapper_options = eval_stream_context_wrapper_options(options, wrapper, values)?;
    let wrapper_options = values.array_set(wrapper_options, option, value)?;
    let options = values.array_set(options, wrapper, wrapper_options)?;
    values.bool_value(
        context
            .stream_resources_mut()
            .set_stream_context_options(id, Some(options)),
    )
}

/// Converts a runtime resource cell into eval's zero-based stream context id.
pub(in crate::interpreter) fn eval_stream_context_resource_id(
    stream_context: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    eval_resource_payload(stream_context, values)
}

/// Returns the nested wrapper options array, creating one when missing or scalar.
fn eval_stream_context_wrapper_options(
    options: RuntimeCellHandle,
    wrapper: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exists = values.array_key_exists(wrapper, options)?;
    if values.truthy(exists)? {
        let wrapper_options = values.array_get(options, wrapper)?;
        if values.is_array_like(wrapper_options)? {
            return Ok(wrapper_options);
        }
    }
    values.assoc_new(1)
}
