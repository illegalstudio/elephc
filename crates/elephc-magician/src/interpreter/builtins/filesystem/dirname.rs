//! Purpose:
//! Declarative eval registry entry for `dirname`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the path helper.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "dirname",
    area: Filesystem,
    params: [path, levels = EvalBuiltinDefaultValue::Int(1)],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `dirname` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_dirname_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_dirname(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `dirname` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_dirname_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [path] => eval_dirname_result(*path, None, values),
        [path, levels] => eval_dirname_result(*path, Some(*levels), values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
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
    while end > 0 && eval_dirname_separator(path[end - 1]) {
        end -= 1;
    }
    if end == 0 {
        return b"/".to_vec();
    }
    let mut cursor = end;
    while cursor > 0 {
        cursor -= 1;
        if eval_dirname_separator(path[cursor]) {
            let mut parent_end = cursor;
            while parent_end > 0 && eval_dirname_separator(path[parent_end - 1]) {
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

/// Returns whether one byte separates path components on the current host platform.
fn eval_dirname_separator(byte: u8) -> bool {
    byte == b'/' || cfg!(windows) && byte == b'\\'
}
