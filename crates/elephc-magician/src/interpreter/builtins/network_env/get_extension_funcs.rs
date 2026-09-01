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
    eval_get_extension_funcs_result(extension, values)
}

/// Returns the ordered function inventory for an evaluated extension name.
pub(in crate::interpreter) fn eval_get_extension_funcs_result(
    extension: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = values.string_bytes(extension)?;
    if !String::from_utf8_lossy(&name).eq_ignore_ascii_case("date") {
        return values.bool_value(false);
    }
    let mut functions = values.string_array_new(DATE_EXTENSION_FUNCTIONS.len())?;
    for name in DATE_EXTENSION_FUNCTIONS {
        functions = values.string_array_push(functions, name)?;
    }
    Ok(functions)
}
