//! Purpose:
//! Trim, case conversion, ucwords, and wordwrap helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::scalars` re-exports.
//!
//! Key details:
//! - Runtime cells remain opaque and all PHP coercions flow through `RuntimeValueOps`.

use super::super::super::*;
use super::*;

pub(in crate::interpreter) const PHP_DEFAULT_TRIM_MASK: &[u8] = b" \n\r\t\x0B\x0C\0";

/// Evaluates PHP trim-like string builtins over one eval expression and optional mask.
pub(in crate::interpreter) fn eval_builtin_trim_like(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_trim_like_result(name, value, None, values)
        }
        [value, mask] => {
            let value = eval_expr(value, context, scope, values)?;
            let mask = eval_expr(mask, context, scope, values)?;
            eval_trim_like_result(name, value, Some(mask), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Trims one converted string using PHP's default mask or a caller-provided byte mask.
pub(in crate::interpreter) fn eval_trim_like_result(
    name: &str,
    value: RuntimeCellHandle,
    mask: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let explicit_mask;
    let trim_mask = if let Some(mask) = mask {
        explicit_mask = values.string_bytes(mask)?;
        explicit_mask.as_slice()
    } else {
        PHP_DEFAULT_TRIM_MASK
    };

    let mut start = 0;
    let mut end = bytes.len();
    if matches!(name, "trim" | "ltrim") {
        while start < end && trim_mask.contains(&bytes[start]) {
            start += 1;
        }
    }
    if matches!(name, "trim" | "rtrim" | "chop") {
        while end > start && trim_mask.contains(&bytes[end - 1]) {
            end -= 1;
        }
    }
    if !matches!(name, "trim" | "ltrim" | "rtrim" | "chop") {
        return Err(EvalStatus::UnsupportedConstruct);
    }

    let value =
        String::from_utf8(bytes[start..end].to_vec()).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string(&value)
}

/// Evaluates PHP ASCII case-conversion string builtins over one eval expression.
pub(in crate::interpreter) fn eval_builtin_string_case(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_string_case_result(name, value, values)
}

/// Converts one eval value through PHP string conversion and ASCII case mapping.
pub(in crate::interpreter) fn eval_string_case_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut bytes = values.string_bytes(value)?;
    match name {
        "strtolower" => {
            for byte in &mut bytes {
                if byte.is_ascii_uppercase() {
                    *byte += b'a' - b'A';
                }
            }
        }
        "strtoupper" => {
            for byte in &mut bytes {
                if byte.is_ascii_lowercase() {
                    *byte -= b'a' - b'A';
                }
            }
        }
        "ucfirst" => {
            if bytes.first().is_some_and(|byte| byte.is_ascii_lowercase()) {
                bytes[0] -= b'a' - b'A';
            }
        }
        "lcfirst" => {
            if bytes.first().is_some_and(|byte| byte.is_ascii_uppercase()) {
                bytes[0] += b'a' - b'A';
            }
        }
        _ => return Err(EvalStatus::UnsupportedConstruct),
    }
    let value = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string(&value)
}

/// Evaluates PHP `ucwords(...)` over one string and optional separator expression.
pub(in crate::interpreter) fn eval_builtin_ucwords(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_ucwords_result(value, None, values)
        }
        [value, separators] => {
            let value = eval_expr(value, context, scope, values)?;
            let separators = eval_expr(separators, context, scope, values)?;
            eval_ucwords_result(value, Some(separators), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Uppercases ASCII lowercase bytes at the start of words separated by PHP delimiters.
pub(in crate::interpreter) fn eval_ucwords_result(
    value: RuntimeCellHandle,
    separators: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut bytes = values.string_bytes(value)?;
    let separators = match separators {
        Some(separators) => values.string_bytes(separators)?,
        None => b" \t\r\n\x0c\x0b".to_vec(),
    };
    let mut word_start = true;
    for byte in &mut bytes {
        if separators.contains(byte) {
            word_start = true;
        } else if word_start {
            if byte.is_ascii_lowercase() {
                *byte -= b'a' - b'A';
            }
            word_start = false;
        }
    }
    values.string_bytes_value(&bytes)
}

/// Evaluates PHP `wordwrap(...)` over one string and optional wrapping controls.
pub(in crate::interpreter) fn eval_builtin_wordwrap(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_wordwrap_result(value, None, None, None, values)
        }
        [value, width] => {
            let value = eval_expr(value, context, scope, values)?;
            let width = eval_expr(width, context, scope, values)?;
            eval_wordwrap_result(value, Some(width), None, None, values)
        }
        [value, width, break_string] => {
            let value = eval_expr(value, context, scope, values)?;
            let width = eval_expr(width, context, scope, values)?;
            let break_string = eval_expr(break_string, context, scope, values)?;
            eval_wordwrap_result(value, Some(width), Some(break_string), None, values)
        }
        [value, width, break_string, cut] => {
            let value = eval_expr(value, context, scope, values)?;
            let width = eval_expr(width, context, scope, values)?;
            let break_string = eval_expr(break_string, context, scope, values)?;
            let cut = eval_expr(cut, context, scope, values)?;
            eval_wordwrap_result(value, Some(width), Some(break_string), Some(cut), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Wraps a byte string at PHP word boundaries and preserves existing newlines.
pub(in crate::interpreter) fn eval_wordwrap_result(
    value: RuntimeCellHandle,
    width: Option<RuntimeCellHandle>,
    break_string: Option<RuntimeCellHandle>,
    cut: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let width = match width {
        Some(width) => eval_int_value(width, values)?,
        None => 75,
    };
    let break_string = match break_string {
        Some(break_string) => values.string_bytes(break_string)?,
        None => b"\n".to_vec(),
    };
    if break_string.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let cut = match cut {
        Some(cut) => values.truthy(cut)?,
        None => false,
    };
    if width == 0 && cut {
        return Err(EvalStatus::RuntimeFatal);
    }
    if bytes.is_empty() {
        return values.string_bytes_value(&bytes);
    }
    let output = eval_wordwrap_bytes(&bytes, width, &break_string, cut);
    values.string_bytes_value(&output)
}

/// Applies the core PHP word-wrap scan over already converted byte slices.
pub(in crate::interpreter) fn eval_wordwrap_bytes(
    bytes: &[u8],
    width: i64,
    break_string: &[u8],
    cut: bool,
) -> Vec<u8> {
    if width < 0 && cut {
        let mut output = Vec::with_capacity(bytes.len() + (bytes.len() * break_string.len()));
        for byte in bytes {
            output.extend_from_slice(break_string);
            output.push(*byte);
        }
        return output;
    }

    let width = width.max(0) as usize;
    let mut output = Vec::with_capacity(bytes.len());
    let mut line_start = 0;
    let mut last_space = None;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                output.extend_from_slice(&bytes[line_start..=index]);
                index += 1;
                line_start = index;
                last_space = None;
            }
            b' ' => {
                if index.saturating_sub(line_start) >= width {
                    output.extend_from_slice(&bytes[line_start..index]);
                    output.extend_from_slice(break_string);
                    index += 1;
                    line_start = index;
                    last_space = None;
                } else {
                    last_space = Some(index);
                    index += 1;
                }
            }
            _ if index.saturating_sub(line_start) >= width => {
                if let Some(space) = last_space {
                    output.extend_from_slice(&bytes[line_start..space]);
                    output.extend_from_slice(break_string);
                    line_start = space + 1;
                    last_space = None;
                } else if cut && width > 0 {
                    output.extend_from_slice(&bytes[line_start..index]);
                    output.extend_from_slice(break_string);
                    line_start = index;
                } else {
                    index += 1;
                }
            }
            _ => {
                index += 1;
            }
        }
    }
    output.extend_from_slice(&bytes[line_start..]);
    output
}
