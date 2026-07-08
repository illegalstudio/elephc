//! Purpose:
//! Declarative eval registry entry for `wordwrap`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the wordwrap hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "wordwrap",
    area: String,
    params: [
        string,
        width = EvalBuiltinDefaultValue::Int(75),
        r#break = EvalBuiltinDefaultValue::String("\n"),
        cut_long_words = EvalBuiltinDefaultValue::Bool(false),
    ],
    direct: Wordwrap,
    values: Wordwrap,
}

use super::super::super::*;

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
