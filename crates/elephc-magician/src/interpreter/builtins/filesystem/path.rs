//! Purpose:
//! Path conversion and basename, dirname, realpath, and pathinfo helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem` re-exports.
//!
//! Key details:
//! - Helpers return PHP-compatible false/null/string/int cells via `RuntimeValueOps`.

use super::super::super::*;
use super::super::*;

/// Converts one eval value to a filesystem path string.
pub(in crate::interpreter) fn eval_path_string(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let filename = values.string_bytes(filename)?;
    Ok(String::from_utf8_lossy(&filename).into_owned())
}

/// Returns whether a path can be opened for reading by the current process.
pub(in crate::interpreter) fn eval_path_is_readable(path: &std::path::Path) -> bool {
    std::fs::File::open(path).is_ok() || std::fs::read_dir(path).is_ok()
}

/// Returns whether a path has any executable bit set in its Unix mode.
pub(in crate::interpreter) fn eval_path_is_executable(path: &std::path::Path) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Returns whether a path can be written by the current process.
pub(in crate::interpreter) fn eval_path_is_writable(path: &std::path::Path) -> bool {
    if path.is_file() {
        return std::fs::OpenOptions::new().write(true).open(path).is_ok();
    }
    if !path.is_dir() {
        return false;
    }
    let probe = path.join(format!(
        ".elephc_magician_writable_probe_{}",
        std::process::id()
    ));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

/// Evaluates PHP `basename($path, $suffix = "")` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_basename(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [path] => {
            let path = eval_expr(path, context, scope, values)?;
            eval_basename_result(path, None, values)
        }
        [path, suffix] => {
            let path = eval_expr(path, context, scope, values)?;
            let suffix = eval_expr(suffix, context, scope, values)?;
            eval_basename_result(path, Some(suffix), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Computes PHP `basename()` bytes and returns them as a runtime string.
pub(in crate::interpreter) fn eval_basename_result(
    path: RuntimeCellHandle,
    suffix: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = values.string_bytes(path)?;
    let suffix = suffix
        .map(|suffix| values.string_bytes(suffix))
        .transpose()?;
    let result = eval_basename_bytes(&path, suffix.as_deref());
    values.string_bytes_value(&result)
}

/// Extracts a PHP basename from one path byte string.
pub(in crate::interpreter) fn eval_basename_bytes(path: &[u8], suffix: Option<&[u8]>) -> Vec<u8> {
    let mut end = path.len();
    while end > 0 && path[end - 1] == b'/' {
        end -= 1;
    }
    if end == 0 {
        return Vec::new();
    }
    let mut start = end;
    while start > 0 && path[start - 1] != b'/' {
        start -= 1;
    }
    let mut result = path[start..end].to_vec();
    if let Some(suffix) = suffix {
        if !suffix.is_empty() && suffix.len() < result.len() && result.ends_with(suffix) {
            result.truncate(result.len() - suffix.len());
        }
    }
    result
}

/// Evaluates PHP `dirname($path, $levels = 1)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_dirname(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [path] => {
            let path = eval_expr(path, context, scope, values)?;
            eval_dirname_result(path, None, values)
        }
        [path, levels] => {
            let path = eval_expr(path, context, scope, values)?;
            let levels = eval_expr(levels, context, scope, values)?;
            eval_dirname_result(path, Some(levels), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Computes PHP `dirname()` bytes and returns them as a runtime string.
pub(in crate::interpreter) fn eval_dirname_result(
    path: RuntimeCellHandle,
    levels: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = values.string_bytes(path)?;
    let levels = match levels {
        Some(levels) => eval_int_value(levels, values)?,
        None => 1,
    };
    if levels < 1 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut current = path;
    for _ in 0..levels {
        current = eval_dirname_once(&current);
    }
    values.string_bytes_value(&current)
}

/// Applies one PHP `dirname()` parent traversal to a path byte string.
pub(in crate::interpreter) fn eval_dirname_once(path: &[u8]) -> Vec<u8> {
    if path.is_empty() {
        return b".".to_vec();
    }
    let mut end = path.len();
    while end > 0 && path[end - 1] == b'/' {
        end -= 1;
    }
    if end == 0 {
        return b"/".to_vec();
    }
    let mut cursor = end;
    while cursor > 0 {
        cursor -= 1;
        if path[cursor] == b'/' {
            let mut parent_end = cursor;
            while parent_end > 0 && path[parent_end - 1] == b'/' {
                parent_end -= 1;
            }
            return if parent_end == 0 {
                b"/".to_vec()
            } else {
                path[..parent_end].to_vec()
            };
        }
    }
    b".".to_vec()
}

/// Evaluates PHP `realpath($path)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_realpath(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [path] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let path = eval_expr(path, context, scope, values)?;
    eval_realpath_result(path, values)
}

/// Canonicalizes one path or returns PHP false when the path cannot be resolved.
pub(in crate::interpreter) fn eval_realpath_result(
    path: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = values.string_bytes(path)?;
    let path = String::from_utf8_lossy(&path);
    let Ok(canonical) = std::fs::canonicalize(path.as_ref()) else {
        return values.bool_value(false);
    };
    let canonical = canonical.to_string_lossy();
    values.string(canonical.as_ref())
}

/// Evaluates PHP `stream_resolve_include_path($filename)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_stream_resolve_include_path(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_stream_resolve_include_path_result(filename, values)
}

/// Resolves one filename using elephc's realpath-equivalent include-path semantics.
pub(in crate::interpreter) fn eval_stream_resolve_include_path_result(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_realpath_result(filename, values)
}

/// Evaluates PHP `pathinfo($path, $flags = PATHINFO_ALL)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_pathinfo(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [path] => {
            let path = eval_expr(path, context, scope, values)?;
            eval_pathinfo_result(path, None, values)
        }
        [path, flags] => {
            let path = eval_expr(path, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            eval_pathinfo_result(path, Some(flags), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Computes PHP `pathinfo()` as either an associative array or one component string.
pub(in crate::interpreter) fn eval_pathinfo_result(
    path: RuntimeCellHandle,
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = values.string_bytes(path)?;
    let Some(flags) = flags else {
        return eval_pathinfo_array_result(&path, values);
    };
    let flags = eval_int_value(flags, values)?;
    if flags == EVAL_PATHINFO_ALL {
        return eval_pathinfo_array_result(&path, values);
    }
    let component = eval_pathinfo_component_bytes(&path, flags);
    values.string_bytes_value(&component)
}

/// Builds the PHP `pathinfo()` associative-array result for all components.
pub(in crate::interpreter) fn eval_pathinfo_array_result(
    path: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.assoc_new(4)?;
    if !path.is_empty() {
        let dirname = eval_pathinfo_dirname_bytes(path);
        result = eval_pathinfo_array_set(result, "dirname", &dirname, values)?;
    }
    let parts = eval_pathinfo_parts(path);
    result = eval_pathinfo_array_set(result, "basename", &parts.basename, values)?;
    if parts.has_extension {
        result = eval_pathinfo_array_set(result, "extension", &parts.extension, values)?;
    }
    eval_pathinfo_array_set(result, "filename", &parts.filename, values)
}

/// Inserts one string component into a PHP `pathinfo()` associative result.
pub(in crate::interpreter) fn eval_pathinfo_array_set(
    array: RuntimeCellHandle,
    key: &str,
    value: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(key)?;
    let value = values.string_bytes_value(value)?;
    values.array_set(array, key, value)
}

/// Returns one PHP `pathinfo()` component for a non-all bitmask.
pub(in crate::interpreter) fn eval_pathinfo_component_bytes(path: &[u8], flags: i64) -> Vec<u8> {
    if flags & EVAL_PATHINFO_DIRNAME != 0 {
        return eval_pathinfo_dirname_bytes(path);
    }
    let parts = eval_pathinfo_parts(path);
    if flags & EVAL_PATHINFO_BASENAME != 0 {
        return parts.basename;
    }
    if flags & EVAL_PATHINFO_EXTENSION != 0 {
        return parts.extension;
    }
    if flags & EVAL_PATHINFO_FILENAME != 0 {
        return parts.filename;
    }
    Vec::new()
}

/// Computes the dirname component with `pathinfo("")`'s empty-string exception.
pub(in crate::interpreter) fn eval_pathinfo_dirname_bytes(path: &[u8]) -> Vec<u8> {
    if path.is_empty() {
        Vec::new()
    } else {
        eval_dirname_once(path)
    }
}

/// Splits pathinfo basename, extension, and filename components.
pub(in crate::interpreter) fn eval_pathinfo_parts(path: &[u8]) -> EvalPathInfoParts {
    let basename = eval_basename_bytes(path, None);
    let Some(dot) = basename.iter().rposition(|byte| *byte == b'.') else {
        return EvalPathInfoParts {
            filename: basename.clone(),
            basename,
            extension: Vec::new(),
            has_extension: false,
        };
    };
    EvalPathInfoParts {
        filename: basename[..dot].to_vec(),
        extension: basename[dot + 1..].to_vec(),
        basename,
        has_extension: true,
    }
}

/// Pathinfo components derived from a basename.
pub(in crate::interpreter) struct EvalPathInfoParts {
    /// Full basename component.
    basename: Vec<u8>,
    /// Extension component after the final dot, possibly empty for trailing-dot names.
    extension: Vec<u8>,
    /// Filename component before the final dot.
    filename: Vec<u8>,
    /// Whether the basename contained a dot and therefore has an extension key.
    has_extension: bool,
}
