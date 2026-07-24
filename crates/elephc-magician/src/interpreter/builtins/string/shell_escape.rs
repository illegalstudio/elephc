//! Purpose:
//! Implements eval-time PHP shell escaping for argument and command fragments.
//!
//! Called from:
//! - The builtin hook dispatch for `escapeshellarg()` and `escapeshellcmd()`.
//!
//! Key details:
//! - NUL is rejected before shell syntax is produced.
//! - Windows and POSIX use deliberately distinct quoting rules.

use super::super::super::*;

/// Evaluates a direct `escapeshellarg(arg)` call before platform-specific escaping.
pub(in crate::interpreter) fn eval_builtin_escapeshellarg(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_escapeshellarg_result(value, values)
}

/// Evaluates a direct `escapeshellcmd(command)` call before platform-specific escaping.
pub(in crate::interpreter) fn eval_builtin_escapeshellcmd(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_escapeshellcmd_result(value, values)
}

/// Escapes one string as a single shell argument using the host platform's PHP rules.
pub(in crate::interpreter) fn eval_escapeshellarg_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    if bytes.contains(&0) {
        return Err(EvalStatus::RuntimeFatal);
    }
    #[cfg(windows)]
    let escaped = eval_windows_shell_arg(&bytes);
    #[cfg(not(windows))]
    let escaped = eval_posix_shell_arg(&bytes);
    values.string_bytes_value(&escaped)
}

/// Escapes one string for interpolation into a shell command using host-specific metacharacters.
pub(in crate::interpreter) fn eval_escapeshellcmd_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    if bytes.contains(&0) {
        return Err(EvalStatus::RuntimeFatal);
    }
    #[cfg(windows)]
    let escaped = eval_windows_shell_cmd(&bytes);
    #[cfg(not(windows))]
    let escaped = eval_posix_shell_cmd(&bytes);
    values.string_bytes_value(&escaped)
}

/// Quotes one POSIX shell argument while preserving embedded apostrophes.
#[cfg(not(windows))]
fn eval_posix_shell_arg(bytes: &[u8]) -> Vec<u8> {
    let mut escaped = Vec::with_capacity(bytes.len() + 2);
    escaped.push(b'\'');
    for &byte in bytes {
        if byte == 0xff {
            continue;
        }
        if byte == b'\'' {
            escaped.extend_from_slice(b"'\\''");
        } else {
            escaped.push(byte);
        }
    }
    escaped.push(b'\'');
    escaped
}

/// Quotes one Windows command-shell argument, neutralizing delayed-expansion syntax.
#[cfg(windows)]
fn eval_windows_shell_arg(bytes: &[u8]) -> Vec<u8> {
    let mut escaped = Vec::with_capacity(bytes.len() + 2);
    let mut trailing_backslashes = 0_usize;
    escaped.push(b'"');
    for &byte in bytes {
        if byte == 0xff {
            continue;
        }
        if byte == b'\\' {
            trailing_backslashes += 1;
        } else {
            trailing_backslashes = 0;
        }
        if matches!(byte, b'"' | b'%' | b'!') {
            escaped.push(b' ');
        }
        escaped.push(byte);
    }
    if trailing_backslashes % 2 == 1 {
        escaped.push(b'\\');
    }
    escaped.push(b'"');
    escaped
}

/// Backslash-escapes POSIX shell metacharacters without adding argument quotes.
#[cfg(not(windows))]
fn eval_posix_shell_cmd(bytes: &[u8]) -> Vec<u8> {
    eval_posix_shell_cmd_with_quote_pairs(bytes)
}

/// Caret-escapes Windows command-shell metacharacters without adding argument quotes.
#[cfg(windows)]
fn eval_windows_shell_cmd(bytes: &[u8]) -> Vec<u8> {
    eval_windows_shell_cmd_with_escape(bytes, b'^')
}

/// Applies Windows command-shell metacharacter escaping with a caret prefix.
#[cfg(windows)]
fn eval_windows_shell_cmd_with_escape(bytes: &[u8], escape: u8) -> Vec<u8> {
    let mut escaped = Vec::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        if byte == 0xff {
            continue;
        }
        let special = matches!(byte, b'#' | b'&' | b';' | b'`' | b'|' | b'*' | b'?' | b'~' | b'<' | b'>' | b'^' | b'(' | b')' | b'[' | b']' | b'{' | b'}' | b'$' | b'\\' | b'\n');
        if special || matches!(byte, b'%' | b'!' | b'"' | b'\'') {
            escaped.push(escape);
        }
        escaped.push(byte);
    }
    escaped
}

/// Backslash-escapes POSIX command metacharacters while retaining matched quote pairs.
#[cfg(not(windows))]
fn eval_posix_shell_cmd_with_quote_pairs(bytes: &[u8]) -> Vec<u8> {
    let mut escaped = Vec::with_capacity(bytes.len() * 2);
    let mut paired_quote_end = None;
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == 0xff {
            continue;
        }
        if matches!(byte, b'\'' | b'"') {
            if paired_quote_end == Some(index) {
                paired_quote_end = None;
                escaped.push(byte);
                continue;
            }
            if paired_quote_end.is_none()
                && bytes[index + 1..].iter().position(|candidate| *candidate == byte).is_some()
            {
                paired_quote_end = bytes[index + 1..]
                    .iter()
                    .position(|candidate| *candidate == byte)
                    .map(|offset| index + offset + 1);
                escaped.push(byte);
                continue;
            }
            escaped.push(b'\\');
            escaped.push(byte);
            continue;
        }
        let special = matches!(byte, b'#' | b'&' | b';' | b'`' | b'|' | b'*' | b'?' | b'~' | b'<' | b'>' | b'^' | b'(' | b')' | b'[' | b']' | b'{' | b'}' | b'$' | b'\\' | b'\n');
        if special {
            escaped.push(b'\\');
        }
        escaped.push(byte);
    }
    escaped
}
