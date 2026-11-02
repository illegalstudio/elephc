//! Purpose:
//! Registers the eval implementation of PHP `debug_backtrace()` and keeps the frame-based helpers
//! that build the trace a COMPILED Throwable records at its `new`.
//!
//! A WRITTEN call is answered here, from the interpreter's live call frames. A dynamic-callable
//! call goes through the registry to the shared implementation in `core::runtime_introspection`
//! (`core::backtrace_runtime`), whose argument spelling the listing below borrows.
//!
//! Called from:
//! - `crate::interpreter::expressions::calls` for a written call.
//! - `crate::ffi::throwable_trace` for the trace a compiled Throwable records.
//! - `crate::interpreter::builtins::symbols::function_exists` for the existence probe.
//!
//! Key details:
//! - A stub returning `[]` would be worse than no function at all. Symfony's
//!   `ClassExistenceResource::throwOnRequiredClass` reads the second frame and returns silently
//!   when that frame names a class probe with no `class` key; given an empty trace it throws a
//!   `ReflectionException` instead, turning a successful `class_exists()` into a fatal.
//! - Measured against `php -n` 8.5.6: the keys are `file, line, function, class, object, type,
//!   args` in that order, innermost frame first. `class`, `object` and `type` are omitted for a
//!   plain function; `object` is omitted for a static call, whose `type` is `::`.
//! - `debug_backtrace()` never appears in its own result, and `debug_print_backtrace()` prints
//!   `#N FILE(LINE): Class->method(args)` per frame with no trailing `{main}` line.
//! - These two are dispatched as plain interpreter handlers rather than through the PHP-visible
//!   builtin registry, exactly like the OPcache handlers next door: a shared-contract entry would
//!   also claim an AOT implementation, and a compiled binary has no interpreter frames to report.

use super::super::super::*;
use super::backtrace_runtime::render_backtrace_argument;

/// PHP's `DEBUG_BACKTRACE_PROVIDE_OBJECT`, which is also the default `$options`.
const EVAL_BACKTRACE_PROVIDE_OBJECT: i64 = 1;
/// PHP's `DEBUG_BACKTRACE_IGNORE_ARGS`, which drops the `args` key from every frame.
const EVAL_BACKTRACE_IGNORE_ARGS: i64 = 2;

/// Returns whether `name` (already lowercased and unqualified) is one of the two backtrace
/// functions, so `function_exists()` reports it even though it is not a catalog builtin.
pub(in crate::interpreter) fn eval_debug_backtrace_function_exists(name: &str) -> bool {
    matches!(name, "debug_backtrace" | "debug_print_backtrace")
}

/// Evaluates a written `debug_backtrace(...)` or `debug_print_backtrace(...)` call.
pub(in crate::interpreter) fn eval_debug_backtrace_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let args = positional_call_arg_exprs(args)?;
    if args.len() > 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in &args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_debug_backtrace_values_result(name, &evaluated_args, context, values)
}

/// Evaluates materialized backtrace arguments for either function.
pub(in crate::interpreter) fn eval_debug_backtrace_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "debug_backtrace" => eval_debug_backtrace_result(evaluated_args, context, values),
        "debug_print_backtrace" => {
            eval_debug_print_backtrace_result(evaluated_args, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds PHP's `debug_backtrace()` array from the interpreter's live frames.
fn eval_debug_backtrace_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (options, limit) = eval_backtrace_options(evaluated_args, values)?;
    let frames = eval_backtrace_frames(context, limit);
    let mut result = values.array_new(frames.len())?;
    for (index, frame) in frames.iter().enumerate() {
        let entry = eval_backtrace_frame_array(frame, options, values)?;
        let key = values.int(index as i64)?;
        result = eval_backtrace_store(result, key, entry, values)?;
    }
    // The COMPILED frames below the interpreter follow, outermost last: the compiled function
    // that ran `eval()` is a frame php lists (with its arguments). The dynamic-callable path in
    // `core::backtrace_runtime` already appends them the same way; a written call answered from
    // the interpreter's frames alone and lost every one of them.
    let mut position = frames.len();
    while limit >= 0 && (limit == 0 || position < limit as usize) {
        let Some(entry) = values.runtime_backtrace_entry(position - frames.len(), options)? else {
            break;
        };
        let key = values.int(position as i64)?;
        result = eval_backtrace_store(result, key, entry, values)?;
        position += 1;
    }
    Ok(result)
}

/// Builds the `getTrace()` array a Throwable created by COMPILED code records at its `new`.
///
/// PHP captures a Throwable's trace when the object is created, from the frames live at that
/// moment, with the `file`, `line`, `function`, `class` and `type` keys `debug_backtrace()` uses
/// and never an `object`. The interpreter's frames come first, then the suspended compiled
/// activations the generated runtime materializes, exactly as `debug_backtrace()` composes them.
/// `args` is recorded, as php does by default
/// (`zend.exception_ignore_args=Off`, also `php -n`): `getTraceAsString()` prints them, and
/// leaving them out made every listed call read `f()`.
///
/// Returns `None` when no interpreter frame is live, so the caller keeps the Throwable's
/// declared empty trace without building anything.
#[cfg(not(test))]
pub(crate) fn eval_throwable_trace_value(
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let frames = eval_backtrace_frames(context, 0);
    if frames.is_empty() {
        return Ok(None);
    }
    if crate::eval_trace::enabled() {
        let listing = frames
            .iter()
            .map(|frame| {
                format!(
                    "{}{}{}@{}:{}",
                    frame.class.as_deref().unwrap_or(""),
                    if frame.class.is_some() { "::" } else { "" },
                    frame.function,
                    frame.file,
                    frame.line
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        eprintln!("[elephc-eval-trace] phase=throwable_trace frames={listing}");
    }
    let mut result = values.array_new(frames.len())?;
    for (index, frame) in frames.iter().enumerate() {
        let entry = eval_backtrace_frame_array(frame, 0, values)?;
        let key = values.int(index as i64)?;
        result = eval_backtrace_store(result, key, entry, values)?;
    }
    // The interpreter only sees the frames it pushed. Compiled callers above the outermost eval
    // entry live in the activation chain, and php reports ONE stack: append those frames through
    // the same suspended-activation reader `debug_backtrace()` uses. Without this, a `new` inside
    // eval started the trace at the eval boundary and the dev error page lost every kernel frame.
    let mut position = frames.len();
    while let Some(entry) = values.runtime_backtrace_entry(position - frames.len(), 0)? {
        let key = values.int(position as i64)?;
        result = eval_backtrace_store(result, key, entry, values)?;
        position += 1;
    }
    Ok(Some(result))
}

/// Renders a Throwable the way php's builtin `getTraceAsString()` (`trace_only`) or
/// `__toString()` does, through the object's own accessors.
///
/// For an eval-declared class extending a builtin Throwable: the compiled `Exception` bodies
/// call a prelude function the interpreter has no direct way to reach, and the bridge's
/// synthetic accessor used to answer `""`. The format is php's: one
/// `#N FILE(LINE): Class->method(args)` line per `getTrace()` frame ending in `#N {main}`, and for
/// `__toString()` the previous exception first, `\n\nNext `, then
/// `Class: message in FILE:LINE\nStack trace:\n` and the listing.
pub(in crate::interpreter) fn eval_throwable_string(
    object: RuntimeCellHandle,
    class_name: &str,
    trace_only: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let trace = eval_method_call_result(object, "getTrace", Vec::new(), context, values)?;
    let count = values.array_len(trace)?;
    let mut listing = String::new();
    for index in 0..count {
        let key = values.int(index as i64)?;
        let frame = values.array_get(trace, key)?;
        values.release(key)?;
        let rendered =
            super::backtrace_runtime::render_native_frame(index, frame, 0, context, values);
        values.release(frame)?;
        listing.push_str(&rendered?);
    }
    values.release(trace)?;
    listing.push_str(&format!("#{count} {{main}}"));
    if trace_only {
        return Ok(listing);
    }
    let mut text = String::new();
    let previous = eval_method_call_result(object, "getPrevious", Vec::new(), context, values)?;
    if !values.is_null(previous)? {
        let rendered = eval_method_call_result(previous, "__toString", Vec::new(), context, values);
        values.release(previous)?;
        let rendered = rendered?;
        text.push_str(&String::from_utf8_lossy(&values.string_bytes(rendered)?));
        values.release(rendered)?;
        text.push_str("\n\nNext ");
    } else {
        values.release(previous)?;
    }
    let message = eval_method_call_result(object, "getMessage", Vec::new(), context, values)?;
    let message_text = String::from_utf8_lossy(&values.string_bytes(message)?).into_owned();
    values.release(message)?;
    let file = eval_method_call_result(object, "getFile", Vec::new(), context, values)?;
    let file_text = String::from_utf8_lossy(&values.string_bytes(file)?).into_owned();
    values.release(file)?;
    let line = eval_method_call_result(object, "getLine", Vec::new(), context, values)?;
    let line_number = eval_int_value(line, values)?;
    values.release(line)?;
    text.push_str(class_name.trim_start_matches('\\'));
    if !message_text.is_empty() {
        text.push_str(": ");
        text.push_str(&message_text);
    }
    text.push_str(&format!(" in {file_text}:{line_number}\nStack trace:\n{listing}"));
    Ok(text)
}

/// Prints PHP's `debug_print_backtrace()` listing and returns null.
///
/// Arguments are printed unless `DEBUG_BACKTRACE_IGNORE_ARGS` is set, in php's compact spelling:
/// `php -n` 8.5 prints `f('a\\b\nc\t\x01\xC3\xA9')` for a string holding a backslash, a
/// newline, a tab and three non-printable bytes. The listing used to print every call as `name()`.
fn eval_debug_print_backtrace_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (options, limit) = eval_backtrace_options(evaluated_args, values)?;
    let frames = eval_backtrace_frames(context, limit);
    let mut output = String::new();
    for (index, frame) in frames.iter().enumerate() {
        let call = match (&frame.class, frame.kind) {
            (Some(class), EvalCallFrameKind::Method) => format!("{class}->{}", frame.function),
            (Some(class), EvalCallFrameKind::StaticMethod) => format!("{class}::{}", frame.function),
            _ => frame.function.clone(),
        };
        let mut rendered_args = Vec::new();
        if options & EVAL_BACKTRACE_IGNORE_ARGS == 0 {
            for arg in frame.args.iter().flatten() {
                rendered_args.push(render_backtrace_argument(*arg, context, values)?);
            }
        }
        output.push_str(&format!(
            "#{index} {}({}): {call}({})\n",
            frame.file,
            frame.line,
            rendered_args.join(", ")
        ));
    }
    // The compiled frames below the interpreter, as `eval_debug_backtrace_result` appends them.
    let mut position = frames.len();
    while limit >= 0 && (limit == 0 || position < limit as usize) {
        let Some(frame) = values.runtime_backtrace_entry(position - frames.len(), options)? else {
            break;
        };
        let rendered =
            super::backtrace_runtime::render_native_frame(position, frame, options, context, values);
        values.release(frame)?;
        output.push_str(&rendered?);
        position += 1;
    }
    let output = values.string_bytes_value(output.as_bytes())?;
    values.echo(output)?;
    values.null()
}

/// One frame copied out of the context so the borrow ends before runtime values are built.
struct EvalBacktraceFrame {
    function: String,
    class: Option<String>,
    kind: EvalCallFrameKind,
    object: Option<RuntimeCellHandle>,
    args: Option<Vec<RuntimeCellHandle>>,
    file: String,
    line: i64,
}

/// Collects the live frames innermost-first, honouring `$limit`.
///
/// PHP's convention: `0` is unlimited and a NEGATIVE limit selects no frame at all
/// (`count(debug_backtrace(0, -1))` is `0` under `php -n` 8.5).
fn eval_backtrace_frames(context: &ElephcEvalContext, limit: i64) -> Vec<EvalBacktraceFrame> {
    if limit < 0 {
        return Vec::new();
    }
    let mut frames: Vec<EvalBacktraceFrame> = context.with_call_frames(|frames| {
        frames
            .iter()
            .rev()
            .map(|frame| EvalBacktraceFrame {
                function: frame.function.clone(),
                class: frame.class.clone(),
                kind: frame.kind,
                object: frame.object,
                args: frame.args.clone(),
                file: frame.file.clone(),
                line: frame.line,
            })
            .collect()
    });
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    if limit > 0 && frames.len() > limit {
        frames.truncate(limit);
    }
    frames
}

/// Reads the `$options` and `$limit` arguments PHP accepts.
fn eval_backtrace_options(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<(i64, i64), EvalStatus> {
    if evaluated_args.len() > 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let options = match evaluated_args.first() {
        Some(options) => eval_int_value(*options, values)?,
        None => EVAL_BACKTRACE_PROVIDE_OBJECT,
    };
    let limit = match evaluated_args.get(1) {
        Some(limit) => eval_int_value(*limit, values)?,
        None => 0,
    };
    Ok((options, limit))
}

/// Builds one frame's associative array with PHP's keys, in PHP's order.
fn eval_backtrace_frame_array(
    frame: &EvalBacktraceFrame,
    options: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut entry = values.assoc_new(7)?;
    entry = eval_backtrace_set_str(entry, "file", &frame.file, values)?;
    entry = eval_backtrace_set_int(entry, "line", frame.line, values)?;
    entry = eval_backtrace_set_str(entry, "function", &frame.function, values)?;
    if let Some(class) = &frame.class {
        entry = eval_backtrace_set_str(entry, "class", class, values)?;
        // PHP carries `object` only for an instance call, and only while the caller keeps
        // `DEBUG_BACKTRACE_PROVIDE_OBJECT` set.
        if frame.kind == EvalCallFrameKind::Method && options & EVAL_BACKTRACE_PROVIDE_OBJECT != 0 {
            if let Some(object) = frame.object {
                let object = values.retain(object)?;
                entry = eval_backtrace_set_handle(entry, "object", object, values)?;
            }
        }
        let type_name = match frame.kind {
            EvalCallFrameKind::StaticMethod => "::",
            _ => "->",
        };
        entry = eval_backtrace_set_str(entry, "type", type_name, values)?;
    }
    if options & EVAL_BACKTRACE_IGNORE_ARGS == 0 {
        if let Some(frame_args) = &frame.args {
            let mut args = values.array_new(frame_args.len())?;
            for (index, arg) in frame_args.iter().enumerate() {
                let key = values.int(index as i64)?;
                let value = values.retain(*arg)?;
                args = eval_backtrace_store(args, key, value, values)?;
            }
            entry = eval_backtrace_set_handle(entry, "args", args, values)?;
        }
    }
    Ok(entry)
}

/// Stores one string-keyed string entry in a frame array.
fn eval_backtrace_set_str(
    entry: RuntimeCellHandle,
    key: &str,
    value: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = values.string(value)?;
    eval_backtrace_set_handle(entry, key, value, values)
}

/// Stores one string-keyed integer entry in a frame array.
fn eval_backtrace_set_int(
    entry: RuntimeCellHandle,
    key: &str,
    value: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = values.int(value)?;
    eval_backtrace_set_handle(entry, key, value, values)
}

/// Stores one string-keyed runtime handle in a frame array.
fn eval_backtrace_set_handle(
    entry: RuntimeCellHandle,
    key: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(key)?;
    eval_backtrace_store(entry, key, value, values)
}

/// Writes an owned key and value into `array` and drops both owners.
///
/// `array_set` borrows its key and value and the array retains what it stores, so every frame
/// built here leaked its keys, its values and, in the outer array, the frame itself -- one hash
/// per frame of every eval-created Throwable's trace.
fn eval_backtrace_store(
    array: RuntimeCellHandle,
    key: RuntimeCellHandle,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let stored = values.array_set(array, key, value);
    let key_release = values.release(key);
    let value_release = values.release(value);
    let stored = stored?;
    key_release?;
    value_release?;
    Ok(stored)
}

eval_builtin! { contract: "debug_backtrace", area: Core, direct: Core, values: Core }
