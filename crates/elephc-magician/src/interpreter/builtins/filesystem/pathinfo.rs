//! Purpose:
//! Declarative eval registry entry for `pathinfo`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the pathinfo helper.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "pathinfo",
    area: Filesystem,
    params: [path, flags = EvalBuiltinDefaultValue::Int(15)],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `pathinfo` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_pathinfo_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_pathinfo(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `pathinfo` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_pathinfo_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [path] => eval_pathinfo_result(*path, None, values),
        [path, flags] => eval_pathinfo_result(*path, Some(*flags), values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
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
        super::dirname::eval_dirname_once(path)
    }
}

/// Splits pathinfo basename, extension, and filename components.
pub(in crate::interpreter) fn eval_pathinfo_parts(path: &[u8]) -> EvalPathInfoParts {
    let basename = super::basename::eval_basename_bytes(path, None);
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
