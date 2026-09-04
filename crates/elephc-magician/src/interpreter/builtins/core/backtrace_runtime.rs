//! Purpose:
//! Materializes and formats active eval callable frames for PHP backtrace builtins.
//!
//! Called from:
//! - `super::runtime_introspection` for `debug_backtrace()` and `debug_print_backtrace()`.
//!
//! Key details:
//! - Frame arguments are read from live activation scopes so parameter writes remain visible.
//! - Object metadata obeys `DEBUG_BACKTRACE_PROVIDE_OBJECT`, while bit two suppresses arguments.

use super::super::super::*;
use super::func_args::eval_current_function_arg;
use crate::context::{EvalBacktraceFrame, EvalFunctionArgsFrame};

const DEBUG_BACKTRACE_PROVIDE_OBJECT: i64 = 1;
const DEBUG_BACKTRACE_IGNORE_ARGS: i64 = 2;

/// Materializes the active call stack as PHP backtrace frame arrays.
pub(super) fn eval_debug_backtrace(
    args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let options = optional_int_arg(args.first().copied(), 1, values)?;
    let limit = optional_int_arg(args.get(1).copied(), 0, values)?;
    let frames = context.backtrace_frames();
    let frame_limit = backtrace_frame_limit(limit, frames.len());
    let mut result = values.array_new(frame_limit)?;
    for frame in frames.into_iter().take(frame_limit) {
        let frame = build_backtrace_frame(&frame, options, values)?;
        let position = values.array_len(result)? as i64;
        let key = values.int(position)?;
        result = values.array_set(result, key, frame)?;
    }
    Ok(result)
}

/// Prints every selected active frame in PHP's compact numbered form.
pub(super) fn eval_debug_print_backtrace(
    args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let options = optional_int_arg(args.first().copied(), 0, values)?;
    let limit = optional_int_arg(args.get(1).copied(), 0, values)?;
    let frames = context.backtrace_frames();
    let frame_limit = backtrace_frame_limit(limit, frames.len());
    for (index, frame) in frames.into_iter().take(frame_limit).enumerate() {
        let rendered = render_backtrace_frame(index, &frame, options, context, values)?;
        let rendered = values.string(&rendered)?;
        values.echo(rendered)?;
        values.release(rendered)?;
    }
    values.null()
}

/// Applies PHP's limit convention where zero is unlimited and a negative value selects no frames.
fn backtrace_frame_limit(limit: i64, available: usize) -> usize {
    if limit < 0 {
        0
    } else if limit == 0 {
        available
    } else {
        usize::try_from(limit).unwrap_or(usize::MAX).min(available)
    }
}

/// Builds one associative PHP backtrace frame from active eval metadata.
fn build_backtrace_frame(
    frame: &EvalBacktraceFrame,
    options: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.assoc_new(7)?;
    result = set_assoc_string(result, "file", frame.file(), values)?;
    result = set_assoc_int(result, "line", frame.line(), values)?;
    result = set_assoc_string(result, "function", frame.function(), values)?;
    if let Some(class_name) = frame.class_name() {
        result = set_assoc_string(result, "class", class_name, values)?;
    }
    if let Some(call_type) = frame.call_type() {
        result = set_assoc_string(result, "type", call_type, values)?;
    }
    if options & DEBUG_BACKTRACE_PROVIDE_OBJECT != 0 {
        if let Some(object) = frame.object() {
            result = set_assoc_cell(result, "object", values.retain(object)?, values)?;
        }
    }
    if options & DEBUG_BACKTRACE_IGNORE_ARGS == 0 {
        let arguments = build_backtrace_args(frame.arguments(), values)?;
        result = set_assoc_cell(result, "args", arguments, values)?;
    }
    Ok(result)
}

/// Retains all PHP-visible arguments from one active frame into an indexed array.
fn build_backtrace_args(
    frame: &EvalFunctionArgsFrame,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(frame.actual_count())?;
    let Some(scope) = frame.scope() else {
        return Ok(result);
    };
    for position in 0..frame.actual_count() {
        let value = eval_current_function_arg(position, frame, scope, values)?;
        let key = values.int(position as i64)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Formats one selected frame using the compact form emitted by php-src.
fn render_backtrace_frame(
    index: usize,
    frame: &EvalBacktraceFrame,
    options: i64,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let class = frame.class_name().unwrap_or_default();
    let call_type = frame.call_type().unwrap_or_default();
    let mut rendered = format!(
        "#{index} {}({}): {class}{call_type}{}(",
        frame.file(),
        frame.line(),
        frame.function()
    );
    if options & DEBUG_BACKTRACE_IGNORE_ARGS == 0 {
        append_rendered_arguments(&mut rendered, frame.arguments(), context, values)?;
    }
    rendered.push_str(")\n");
    Ok(rendered)
}

/// Appends the current values of all arguments in one active frame.
fn append_rendered_arguments(
    rendered: &mut String,
    frame: &EvalFunctionArgsFrame,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(scope) = frame.scope() else {
        return Ok(());
    };
    for position in 0..frame.actual_count() {
        if position > 0 {
            rendered.push_str(", ");
        }
        let value = eval_current_function_arg(position, frame, scope, values)?;
        rendered.push_str(&render_backtrace_argument(value, context, values)?);
        values.release(value)?;
    }
    Ok(())
}

/// Formats one argument using the compact scalar and container spellings used by backtraces.
fn render_backtrace_argument(
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_INT => Ok((values.raw_value_word(value)? as i64).to_string()),
        EVAL_TAG_FLOAT => Ok(f64::from_bits(values.raw_value_word(value)?).to_string()),
        EVAL_TAG_STRING => {
            let bytes = values.string_bytes(value)?;
            let value = escape_backtrace_string(&bytes);
            Ok(format!("'{value}'"))
        }
        EVAL_TAG_BOOL => Ok(if values.raw_value_word(value)? == 0 {
            "false".to_string()
        } else {
            "true".to_string()
        }),
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => Ok("Array".to_string()),
        EVAL_TAG_OBJECT => {
            let identity = values.object_identity(value)?;
            let class_name = match context.dynamic_object_class_name(identity) {
                Some(class_name) => class_name,
                None => {
                    let class_name = values.object_class_name(value)?;
                    let bytes = values.string_bytes(class_name)?;
                    values.release(class_name)?;
                    String::from_utf8_lossy(&bytes).into_owned()
                }
            };
            Ok(format!("Object({class_name})"))
        }
        EVAL_TAG_RESOURCE => {
            let id = eval_int_value(value, values)?;
            Ok(format!("Resource id #{id}"))
        }
        EVAL_TAG_NULL => Ok("NULL".to_string()),
        EVAL_TAG_CALLABLE => Ok("Object(Closure)".to_string()),
        _ => Ok("Unknown".to_string()),
    }
}

/// Escapes string bytes with php-src's compact printable-ASCII backtrace notation.
fn escape_backtrace_string(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut escaped = String::with_capacity(bytes.len());
    for byte in bytes.iter().copied() {
        match byte {
            b'\\' => escaped.push_str("\\\\"),
            b'\n' => escaped.push_str("\\n"),
            b'\t' => escaped.push_str("\\t"),
            b'\r' => escaped.push_str("\\r"),
            0x0b => escaped.push_str("\\v"),
            0x0c => escaped.push_str("\\f"),
            0x1b => escaped.push_str("\\e"),
            0x20..=0x7e => escaped.push(char::from(byte)),
            _ => {
                escaped.push_str("\\x");
                escaped.push(char::from(HEX[usize::from(byte >> 4)]));
                escaped.push(char::from(HEX[usize::from(byte & 0x0f)]));
            }
        }
    }
    escaped
}

/// Reads an optional integer argument, applying the PHP default when absent.
fn optional_int_arg(
    value: Option<RuntimeCellHandle>,
    default: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    value.map_or(Ok(default), |value| eval_int_value(value, values))
}

/// Inserts one string value under a string key.
fn set_assoc_string(
    result: RuntimeCellHandle,
    key: &str,
    value: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = values.string(value)?;
    set_assoc_cell(result, key, value, values)
}

/// Inserts one integer value under a string key.
fn set_assoc_int(
    result: RuntimeCellHandle,
    key: &str,
    value: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = values.int(value)?;
    set_assoc_cell(result, key, value, values)
}

/// Inserts one materialized value under a string key.
fn set_assoc_cell(
    result: RuntimeCellHandle,
    key: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(key)?;
    values.array_set(result, key, value)
}
