//! Purpose:
//! Declarative eval registry entry for `disk_free_space`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the disk-space helper.

eval_builtin! {
    name: "disk_free_space",
    area: Filesystem,
    params: [directory],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `disk_free_space` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_disk_free_space_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_disk_space("disk_free_space", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `disk_free_space` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_disk_free_space_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [directory] => eval_disk_space_result("disk_free_space", *directory, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `disk_free_space($directory)` or `disk_total_space($directory)`.
pub(in crate::interpreter) fn eval_builtin_disk_space(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [directory] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let directory = eval_expr(directory, context, scope, values)?;
    eval_disk_space_result(name, directory, values)
}

/// Reports available or total filesystem bytes as a PHP float, or 0.0 on failure.
pub(in crate::interpreter) fn eval_disk_space_result(
    name: &str,
    directory: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(directory)?;
    let Ok(path) = CString::new(bytes) else {
        return values.float(0.0);
    };
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::zeroed();
    let status = unsafe {
        // libc writes the statvfs fields for this NUL-terminated local path.
        libc::statvfs(path.as_ptr(), stats.as_mut_ptr())
    };
    if status != 0 {
        return values.float(0.0);
    }
    let stats = unsafe {
        // `statvfs` succeeded, so libc initialized the full stat buffer.
        stats.assume_init()
    };
    let block_size = if stats.f_frsize > 0 {
        stats.f_frsize
    } else {
        stats.f_bsize
    };
    let blocks = match name {
        "disk_free_space" => stats.f_bavail,
        "disk_total_space" => stats.f_blocks,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.float((block_size as f64) * (blocks as f64))
}
