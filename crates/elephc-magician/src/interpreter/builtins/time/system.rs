//! Purpose:
//! Implements shared system information helpers still used by network/env builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` until those builtins are co-located.
//!
//! Key details:
//! - `phpversion()` reads the workspace package version at compile time and
//!   `php_uname()` formats libc `uname` fields.

use super::super::super::*;

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

/// Evaluates PHP `php_uname($mode = "a")` over zero or one eval expression.
pub(in crate::interpreter) fn eval_builtin_php_uname(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_php_uname_result(None, values),
        [mode] => {
            let mode = eval_expr(mode, context, scope, values)?;
            eval_php_uname_result(Some(mode), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Reads the local uname fields and formats the PHP `php_uname()` mode result.
pub(in crate::interpreter) fn eval_php_uname_result(
    mode: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mode = match mode {
        Some(mode) => {
            let bytes = values.string_bytes(mode)?;
            let [mode] = bytes.as_slice() else {
                return Err(EvalStatus::RuntimeFatal);
            };
            *mode
        }
        None => b'a',
    };

    let mut utsname = std::mem::MaybeUninit::<libc::utsname>::zeroed();
    let status = unsafe {
        // libc writes all uname fields into the stack-owned utsname buffer.
        libc::uname(utsname.as_mut_ptr())
    };
    if status != 0 {
        return values.string("");
    }
    let utsname = unsafe {
        // `uname` succeeded, so libc initialized the full `utsname` structure.
        utsname.assume_init()
    };
    let sysname = eval_uname_field_bytes(&utsname.sysname);
    let nodename = eval_uname_field_bytes(&utsname.nodename);
    let release = eval_uname_field_bytes(&utsname.release);
    let version = eval_uname_field_bytes(&utsname.version);
    let machine = eval_uname_field_bytes(&utsname.machine);

    match mode {
        b'a' => {
            let mut output = Vec::new();
            for field in [&sysname, &nodename, &release, &version, &machine] {
                if !output.is_empty() {
                    output.push(b' ');
                }
                output.extend_from_slice(field);
            }
            values.string_bytes_value(&output)
        }
        b's' => values.string_bytes_value(&sysname),
        b'n' => values.string_bytes_value(&nodename),
        b'r' => values.string_bytes_value(&release),
        b'v' => values.string_bytes_value(&version),
        b'm' => values.string_bytes_value(&machine),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Copies one NUL-terminated `utsname` field into raw PHP string bytes.
pub(in crate::interpreter) fn eval_uname_field_bytes(field: &[libc::c_char]) -> Vec<u8> {
    let length = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    field[..length].iter().map(|byte| *byte as u8).collect()
}
