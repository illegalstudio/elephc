//! Purpose:
//! Routes by-value dynamic builtin dispatch to focused builtin-family dispatchers.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` re-exports.
//!
//! Key details:
//! - Each child dispatcher handles already evaluated runtime-cell arguments for one
//!   builtin family and returns `Ok(None)` when the name is outside its domain.

mod arrays;
mod core;
mod filesystem;
mod formatting;
mod json;
mod network_env;
mod regex;
mod scalars;
mod strings;
mod symbols;
mod time;

use super::super::super::*;

use arrays::*;
use core::*;
use filesystem::*;
use formatting::*;
use json::*;
use network_env::*;
use regex::*;
use scalars::*;
use strings::*;
use symbols::*;
use time::*;

/// Evaluates PHP-visible builtins when they are invoked through a dynamic callable name.
pub(in crate::interpreter) fn eval_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if let Some(result) = eval_arrays_builtin_with_values(name, evaluated_args, context, values)? {
        return Ok(Some(result));
    }
    if let Some(result) =
        eval_filesystem_builtin_with_values(name, evaluated_args, context, values)?
    {
        return Ok(Some(result));
    }
    if let Some(result) =
        eval_formatting_builtin_with_values(name, evaluated_args, context, values)?
    {
        return Ok(Some(result));
    }
    if let Some(result) = eval_regex_builtin_with_values(name, evaluated_args, context, values)? {
        return Ok(Some(result));
    }
    if let Some(result) = eval_strings_builtin_with_values(name, evaluated_args, context, values)? {
        return Ok(Some(result));
    }
    if let Some(result) = eval_scalars_builtin_with_values(name, evaluated_args, context, values)? {
        return Ok(Some(result));
    }
    if let Some(result) = eval_time_builtin_with_values(name, evaluated_args, context, values)? {
        return Ok(Some(result));
    }
    if let Some(result) =
        eval_network_env_builtin_with_values(name, evaluated_args, context, values)?
    {
        return Ok(Some(result));
    }
    if let Some(result) = eval_symbols_builtin_with_values(name, evaluated_args, context, values)? {
        return Ok(Some(result));
    }
    if let Some(result) = eval_core_builtin_with_values(name, evaluated_args, context, values)? {
        return Ok(Some(result));
    }
    if let Some(result) = eval_json_builtin_with_values(name, evaluated_args, context, values)? {
        return Ok(Some(result));
    }
    Ok(None)
}
