//! Purpose:
//! Declarative eval registry entry and implementation for `sys_get_temp_dir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Returns the same temporary directory literal as the native static builtin.

eval_builtin! {
    name: "sys_get_temp_dir",
    area: Filesystem,
    params: [],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `sys_get_temp_dir()` with no arguments.
pub(in crate::interpreter) fn eval_sys_get_temp_dir_declared_call(
    args: &[EvalExpr],
    _context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_sys_get_temp_dir(args, values)
}

/// Evaluates `sys_get_temp_dir()` from already evaluated arguments.
pub(in crate::interpreter) fn eval_sys_get_temp_dir_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_sys_get_temp_dir_result(values)
}

/// Evaluates PHP `sys_get_temp_dir()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_sys_get_temp_dir(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_sys_get_temp_dir_result(values)
}

/// Returns the process temporary directory, matching the compiled builtin.
///
/// On Windows the compiled path calls `__rt_sys_get_temp_dir` (GetTempPathW-backed),
/// so returning the literal `/tmp` here made an eval fragment contradict the rest of
/// its own binary with a path that is not even valid on that platform.
///
/// The POSIX arm resolves `TMPDIR` exactly as the compiled runtime helper
/// `__rt_php_temp_dir` does, so an eval fragment and the binary around it always
/// name the same directory.
pub(in crate::interpreter) fn eval_sys_get_temp_dir_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.string(&eval_temp_dir())
}

/// Resolves the temporary directory for the platform this interpreter targets.
pub(in crate::interpreter) fn eval_temp_dir() -> String {
    if cfg!(target_os = "windows") {
        // php strips the trailing separator GetTempPath() always appends; the TMP /
        // TEMP / USERPROFILE order mirrors what the Win32 call itself consults.
        for key in ["TMP", "TEMP", "USERPROFILE"] {
            if let Ok(dir) = std::env::var(key) {
                let trimmed = dir.trim_end_matches(['\\', '/']);
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }
        return "C:\\Windows\\Temp".to_string();
    }

    // php reads TMPDIR first and drops **exactly one** trailing slash, so
    // "/var/tmp/probe///" resolves to "/var/tmp/probe//" and a bare "/" resolves to
    // the empty string. Only an unset or empty TMPDIR falls back to P_tmpdir, which
    // php returns verbatim -- trailing slash included on macOS.
    if let Ok(dir) = std::env::var("TMPDIR") {
        if !dir.is_empty() {
            return dir.strip_suffix('/').unwrap_or(&dir).to_string();
        }
    }
    if cfg!(target_os = "macos") {
        return "/var/tmp/".to_string();
    }
    "/tmp".to_string()
}
