//! Purpose:
//! Eval registry entry and implementation for `phpversion`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - The compiler package version is read from the workspace manifest.

use super::*;

eval_builtin! {
    name: "phpversion",
    area: NetworkEnv,
    params: [],
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `phpversion()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_phpversion(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_phpversion_result(values)
}

/// Returns the root elephc package version as a boxed PHP string.
pub(in crate::interpreter) fn eval_phpversion_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.string(eval_compiler_php_version())
}

/// Reads the root package version from the workspace manifest used by native `phpversion()`.
pub(in crate::interpreter) fn eval_compiler_php_version() -> &'static str {
    let mut in_package = false;
    for line in EVAL_ROOT_CARGO_TOML.lines() {
        let line = line.trim();
        if line == "[package]" {
            in_package = true;
            continue;
        }
        if in_package && line.starts_with('[') {
            break;
        }
        if in_package {
            if let Some(value) = line.strip_prefix("version = ") {
                return value.trim_matches('"');
            }
        }
    }
    env!("CARGO_PKG_VERSION")
}
