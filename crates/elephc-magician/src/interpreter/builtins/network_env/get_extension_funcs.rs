//! Purpose:
//! Eval registry entry and implementation for `get_extension_funcs`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - The date extension inventory preserves php-src declaration order and casing.
//! - Extension names are matched case-insensitively; unsupported names return `false`.

use super::*;

/// php-src declaration-order inventory returned for the date extension.
const DATE_EXTENSION_FUNCTIONS: &[&str] = &[
    "strtotime",
    "date",
    "idate",
    "gmdate",
    "mktime",
    "gmmktime",
    "checkdate",
    "strftime",
    "gmstrftime",
    "time",
    "localtime",
    "getdate",
    "date_create",
    "date_create_immutable",
    "date_create_from_format",
    "date_create_immutable_from_format",
    "date_parse",
    "date_parse_from_format",
    "date_get_last_errors",
    "date_format",
    "date_modify",
    "date_add",
    "date_sub",
    "date_timezone_get",
    "date_timezone_set",
    "date_offset_get",
    "date_diff",
    "date_time_set",
    "date_date_set",
    "date_isodate_set",
    "date_timestamp_set",
    "date_timestamp_get",
    "timezone_open",
    "timezone_name_get",
    "timezone_name_from_abbr",
    "timezone_offset_get",
    "timezone_transitions_get",
    "timezone_location_get",
    "timezone_identifiers_list",
    "timezone_abbreviations_list",
    "timezone_version_get",
    "date_interval_create_from_date_string",
    "date_interval_format",
    "date_default_timezone_set",
    "date_default_timezone_get",
    "date_sunrise",
    "date_sunset",
    "date_sun_info",
];

eval_builtin! {
    contract: "get_extension_funcs",
    area: NetworkEnv,
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `get_extension_funcs($extension)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_get_extension_funcs(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [extension] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let extension = eval_expr(extension, context, scope, values)?;
    eval_get_extension_funcs_result(extension, context, values)
}

/// Returns the ordered function inventory for an evaluated extension name.
pub(in crate::interpreter) fn eval_get_extension_funcs_result(
    extension: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = eval_get_extension_funcs_name(extension, context, values)?;
    if !String::from_utf8_lossy(&name).eq_ignore_ascii_case("date") {
        return values.bool_value(false);
    }
    let mut functions = values.string_array_new(DATE_EXTENSION_FUNCTIONS.len())?;
    for name in DATE_EXTENSION_FUNCTIONS {
        functions = values.string_array_push(functions, name)?;
    }
    Ok(functions)
}

/// Applies PHP's weak string-parameter binding before extension lookup.
fn eval_get_extension_funcs_name(
    extension: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    let tag = values.type_tag(extension)?;
    if context.strict_types() && tag != EVAL_TAG_STRING {
        let actual = if tag == EVAL_TAG_OBJECT {
            runtime_object_class_name(extension, values)?
        } else {
            eval_get_extension_funcs_type_name(tag).to_string()
        };
        return eval_throw_type_error(
            &format!(
                "get_extension_funcs(): Argument #1 ($extension) must be of type string, {actual} given"
            ),
            context,
            values,
        );
    }
    if tag == EVAL_TAG_NULL {
        values.deprecated(
            "\nDeprecated: get_extension_funcs(): Passing null to parameter #1 ($extension) of type string is deprecated",
        )?;
        return Ok(Vec::new());
    }
    if matches!(tag, EVAL_TAG_INT | EVAL_TAG_FLOAT | EVAL_TAG_BOOL) {
        let coerced = values.cast_string(extension)?;
        let bytes = values.string_bytes(coerced)?;
        values.release(coerced)?;
        return Ok(bytes);
    }
    if tag == EVAL_TAG_STRING {
        return values.string_bytes(extension);
    }
    if tag == EVAL_TAG_OBJECT {
        let actual = runtime_object_class_name(extension, values)?;
        let stringable = context.class_is_a(&actual, "Stringable", false)
            || values.object_is_a(extension, "Stringable", false)?;
        if stringable {
            let coerced = eval_string_context_value(extension, context, values)?;
            let bytes = values.string_bytes(coerced)?;
            if coerced != extension {
                values.release(coerced)?;
            }
            return Ok(bytes);
        }
        return eval_throw_type_error(
            &format!(
                "get_extension_funcs(): Argument #1 ($extension) must be of type string, {actual} given"
            ),
            context,
            values,
        );
    }
    let actual = eval_get_extension_funcs_type_name(tag);
    eval_throw_type_error(
        &format!(
            "get_extension_funcs(): Argument #1 ($extension) must be of type string, {actual} given"
        ),
        context,
        values,
    )
}

/// Returns the PHP type spelling used by the builtin's argument `TypeError`.
fn eval_get_extension_funcs_type_name(tag: u64) -> &'static str {
    match tag {
        EVAL_TAG_INT => "int",
        EVAL_TAG_STRING => "string",
        EVAL_TAG_FLOAT => "float",
        EVAL_TAG_BOOL => "bool",
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => "array",
        EVAL_TAG_NULL => "null",
        EVAL_TAG_RESOURCE => "resource",
        EVAL_TAG_OBJECT => "object",
        _ => "unknown",
    }
}
