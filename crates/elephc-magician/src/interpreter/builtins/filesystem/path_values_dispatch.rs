//! Purpose:
//! Evaluated-argument dispatch for declarative path, file, directory, and stat builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::values_dispatch`.
//!
//! Key details:
//! - This module owns by-value dispatch for filesystem helpers that do not work
//!   with stream resource cursor state.

use super::super::super::*;
use super::*;

/// Attempts evaluated-argument dispatch for path and file builtins.
pub(in crate::interpreter::builtins::filesystem) fn eval_filesystem_path_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "fnmatch" => match evaluated_args {
            [pattern, filename] => eval_fnmatch_result(*pattern, *filename, None, values)?,
            [pattern, filename, flags] => {
                eval_fnmatch_result(*pattern, *filename, Some(*flags), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "realpath_cache_get" => match evaluated_args {
            [] => eval_realpath_cache_get_result(values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "realpath_cache_size" => match evaluated_args {
            [] => eval_realpath_cache_size_result(values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "sys_get_temp_dir" => match evaluated_args {
            [] => eval_sys_get_temp_dir_result(values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        _ => return Ok(None),
    };
    Ok(Some(result))
}
