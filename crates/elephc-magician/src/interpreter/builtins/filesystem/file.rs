//! Purpose:
//! Declarative eval registry entry for `file`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the file-lines helper.
//! - The parameter list mirrors PHP's `file(string $filename, int $flags = 0, $context = null)`
//!   and must stay shape-identical to the static registry declaration, which the builtin
//!   parity gate asserts. `$context` is accepted and ignored here: eval has no stream-context
//!   plumbing, and dropping the parameter would put the two registries out of shape.

eval_builtin! {
    contract: "file",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `file` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_file_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_file(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `file` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_file_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [filename] => eval_file_result(*filename, 0, context, values),
        [filename, flags] | [filename, flags, _] => {
            let flags = eval_int_value(*flags, values)?;
            eval_file_result(*filename, flags, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `file($filename, $flags)` over its eval expressions.
pub(in crate::interpreter) fn eval_builtin_file(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [filename] => {
            let filename = eval_expr(filename, context, scope, values)?;
            eval_file_result(filename, 0, context, values)
        }
        [filename, flags] | [filename, flags, _] => {
            let filename = eval_expr(filename, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            let flags = eval_int_value(flags, values)?;
            eval_file_result(filename, flags, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// PHP's `FILE_IGNORE_NEW_LINES`: drop each line's trailing `\n`, and a `\r` before it.
const EVAL_FILE_IGNORE_NEW_LINES: i64 = 2;

/// PHP's `FILE_SKIP_EMPTY_LINES`: drop lines that are empty after the newline handling above.
const EVAL_FILE_SKIP_EMPTY_LINES: i64 = 4;

/// Reads one local file or supported wrapper and returns indexed line byte strings.
pub(in crate::interpreter) fn eval_file_result(
    filename: RuntimeCellHandle,
    flags: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    if let Some(result) = eval_user_wrapper_file_get_contents_result(&path, context, values)? {
        if values.type_tag(result)? == EVAL_TAG_STRING {
            let bytes = values.string_bytes(result)?;
            return eval_file_lines_array(&bytes, flags, values);
        }
        values.warning(&format!(
            "Warning: file({path}): Failed to open stream: {}\n",
            crate::stream_resources::EVAL_OPEN_DEFAULT_REASON
        ))?;
        // php answers false for a file it cannot read, and the compiled runtime agrees.
        return values.bool_value(false);
    }
    let bytes = match super::file_get_contents::eval_read_path_or_wrapper_bytes(&path) {
        Ok(bytes) => bytes,
        Err(reason) => {
            values.warning(&format!(
                "Warning: file({path}): Failed to open stream: {reason}\n"
            ))?;
            // php answers false for a file it cannot read, and the compiled runtime agrees.
        return values.bool_value(false);
        }
    };
    eval_file_lines_array(&bytes, flags, values)
}

/// Splits file payload bytes into runtime array entries, honoring PHP's `file()` flags.
///
/// Trailing newlines are preserved unless `FILE_IGNORE_NEW_LINES` is set, and `FILE_SKIP_EMPTY_LINES`
/// is applied AFTER that trimming — which is why it alone keeps a bare `"\n"` line, exactly like
/// php-src. Result keys are always renumbered from zero over the lines that survive.
fn eval_file_lines_array(
    bytes: &[u8],
    flags: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let ignore_new_lines = flags & EVAL_FILE_IGNORE_NEW_LINES != 0;
    let skip_empty_lines = flags & EVAL_FILE_SKIP_EMPTY_LINES != 0;
    let mut result = values.array_new(0)?;
    let mut line_start = 0;
    let mut line_index = 0;
    let push = |line: &[u8],
                    result: RuntimeCellHandle,
                    line_index: &mut usize,
                    values: &mut _|
     -> Result<RuntimeCellHandle, EvalStatus> {
        let mut line = line;
        if ignore_new_lines {
            if let Some(trimmed) = line.strip_suffix(b"\n") {
                line = trimmed.strip_suffix(b"\r").unwrap_or(trimmed);
            }
        }
        if skip_empty_lines && line.is_empty() {
            return Ok(result);
        }
        let result = super::scandir::eval_array_set_indexed_bytes(result, *line_index, line, values)?;
        *line_index += 1;
        Ok(result)
    };
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'\n' {
            continue;
        }
        result = push(&bytes[line_start..=index], result, &mut line_index, values)?;
        line_start = index + 1;
    }
    if line_start < bytes.len() {
        result = push(&bytes[line_start..], result, &mut line_index, values)?;
    }
    Ok(result)
}
