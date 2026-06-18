//! Purpose:
//! Implements directory listing helpers and indexed byte-array insertion.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::ops` and file-reading helpers.
//!
//! Key details:
//! - Byte-array insertion is reused by `file()` and glob/listing builtins to
//!   produce sequential PHP arrays.

use super::super::super::super::*;
use super::super::*;

/// Evaluates PHP `scandir($directory)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_scandir(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [directory] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let directory = eval_expr(directory, context, scope, values)?;
    eval_scandir_result(directory, values)
}

/// Lists one local directory into an indexed string array, or an empty array on failure.
pub(in crate::interpreter) fn eval_scandir_result(
    directory: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(directory, values)?;
    let Ok(entries) = std::fs::read_dir(path) else {
        return values.array_new(0);
    };
    let mut names = vec![".".to_string(), "..".to_string()];
    for entry in entries {
        let entry = entry.map_err(|_| EvalStatus::RuntimeFatal)?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    let mut result = values.array_new(names.len())?;
    for (index, name) in names.iter().enumerate() {
        result = eval_array_set_indexed_bytes(result, index, name.as_bytes(), values)?;
    }
    Ok(result)
}

/// Writes one byte-string value into an indexed runtime array at a zero-based position.
pub(in crate::interpreter) fn eval_array_set_indexed_bytes(
    array: RuntimeCellHandle,
    index: usize,
    value: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
    let value = values.string_bytes_value(value)?;
    values.array_set(array, key, value)
}
