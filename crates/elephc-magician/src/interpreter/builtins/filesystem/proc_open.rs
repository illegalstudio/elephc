//! Purpose:
//! Eval registry entry and implementation for `proc_open`, plus the helpers the other `proc_*`
//! builtins share: process-handle resolution and PHP-visible status arrays.
//!
//! Called from:
//! - `crate::interpreter::expressions::calls::eval_call()`, which dispatches `proc_open` from its
//!   own hard-coded ladder BEFORE the registry gate, because `$pipes` is a mandatory by-reference
//!   out-parameter and `EvalValuesHook` structurally cannot write one.
//! - The filesystem by-value dispatcher, for `call_user_func('proc_open', ...)`.
//!
//! Key details:
//! - The child is started by the shared `elephc_proc` crate, the same engine the compiled
//!   `proc_open()` prelude drives, so descriptor handling, cwd/env rules, and failure reporting
//!   agree between the two sides.
//! - Pipes become ordinary eval file streams; the process handle is a resource of type `process`
//!   whose payload is the key of the `elephc_proc` status table.
//! - Spec validation mirrors php-src: structural mistakes throw `ValueError`; an unknown
//!   descriptor kind or redirect target warns and returns `false`.

eval_builtin! {
    contract: "proc_open",
    area: Filesystem,
    direct: none,
    values: Filesystem,
}

use std::ffi::CString;

use elephc_proc::spawn::{spawn, Request, Source};

use super::super::super::*;
use crate::interpreter::collection_builder::EvalArrayBuilder;

/// Declared parameter names of `proc_open()`, in call order.
const PROC_OPEN_PARAMS: [&str; 6] =
    ["command", "descriptor_spec", "pipes", "cwd", "env_vars", "options"];

/// Evaluates `proc_open()` over full call metadata so `$pipes` can be written.
pub(in crate::interpreter) fn eval_builtin_proc_open_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(args, context, scope, values, |evaluated, context, _, values| {
        if evaluated.len() < 3 {
            return eval_throw_argument_count_error(
                &format!("proc_open() expects at least 3 arguments, {} given", evaluated.len()),
                context,
                values,
            );
        }
        let (bound, _) = bind_evaluated_ref_builtin_args(&PROC_OPEN_PARAMS, &evaluated, false)?;
        let command = required_evaluated_ref_arg(&bound, 0)?.value;
        let descriptor_spec = required_evaluated_ref_arg(&bound, 1)?.value;
        let pipes = required_evaluated_ref_arg(&bound, 2)?.ref_target.clone();
        let cwd = optional_evaluated_ref_arg(&bound, 3).map(|argument| argument.value);
        let env_vars = optional_evaluated_ref_arg(&bound, 4).map(|argument| argument.value);
        eval_proc_open_result(command, descriptor_spec, pipes.as_ref(), cwd, env_vars, context, values)
    })
}

/// Evaluates a by-value `proc_open()` call: `$pipes` cannot be written, so PHP warns.
pub(in crate::interpreter) fn eval_proc_open_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(3..=6).contains(&evaluated_args.len()) {
        return eval_throw_argument_count_error(
            &format!(
                "proc_open() expects at least 3 arguments, {} given",
                evaluated_args.len()
            ),
            context,
            values,
        );
    }
    values.warning("proc_open(): Argument #3 ($pipes) must be passed by reference, value given")?;
    eval_proc_open_result(
        evaluated_args[0],
        evaluated_args[1],
        None,
        evaluated_args.get(3).copied(),
        evaluated_args.get(4).copied(),
        context,
        values,
    )
}

/// What one `$descriptor_spec` entry holds, decoded to plain bytes.
enum EvalSpecEntry {
    /// A stream resource payload.
    Stream(i64),
    /// An array spec: its first three elements, `None` where missing or null.
    Array([Option<Vec<u8>>; 3]),
    /// Neither an array nor a stream.
    Invalid,
}

/// Converts one spec element to bytes with PHP's string-context rules.
fn eval_spec_element_bytes(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<Vec<u8>>, EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_NULL => Ok(None),
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => Ok(Some(b"Array".to_vec())),
        _ => {
            let text = eval_string_context_value(value, context, values)?;
            Ok(Some(values.string_bytes(text)?))
        }
    }
}

/// Decodes one spec entry, releasing every temporary cell it reads.
fn eval_decode_spec_entry(
    entry: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalSpecEntry, EvalStatus> {
    match values.type_tag(entry)? {
        EVAL_TAG_RESOURCE => Ok(EvalSpecEntry::Stream(eval_resource_payload(entry, values)?)),
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            let mut elements: [Option<Vec<u8>>; 3] = [None, None, None];
            for (index, slot) in elements.iter_mut().enumerate() {
                let key = values.int(index as i64)?;
                let element = values.array_get(entry, key);
                let released = values.release(key);
                let element = element?;
                released?;
                let decoded = eval_spec_element_bytes(element, context, values);
                let released = values.release(element);
                *slot = decoded?;
                released?;
            }
            Ok(EvalSpecEntry::Array(elements))
        }
        _ => Ok(EvalSpecEntry::Invalid),
    }
}

/// Reads a command element as a C string, refusing an embedded NUL for an array element.
fn eval_command_cstring(bytes: &[u8]) -> Option<CString> {
    CString::new(bytes.to_vec()).ok()
}

/// Starts the process described by already evaluated `proc_open()` arguments.
pub(in crate::interpreter) fn eval_proc_open_result(
    command: RuntimeCellHandle,
    descriptor_spec: RuntimeCellHandle,
    pipes_target: Option<&EvalReferenceTarget>,
    cwd: Option<RuntimeCellHandle>,
    env_vars: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // Command: a shell line, or a program and its arguments.
    let (shell, args, command_text) = match values.type_tag(command)? {
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            let length = values.array_len(command)?;
            if length == 0 {
                return eval_throw_builtin_value_error(
                    "proc_open(): Argument #1 ($command) must not be empty",
                    context,
                    values,
                );
            }
            let mut args = Vec::with_capacity(length);
            for position in 0..length {
                let element = values.array_iter_value(command, position)?;
                let bytes = eval_spec_element_bytes(element, context, values);
                values.release(element)?;
                let bytes = bytes?.unwrap_or_default();
                let Some(argument) = eval_command_cstring(&bytes) else {
                    return eval_throw_builtin_value_error(
                        &format!("Command array element {} contains a null byte", position + 1),
                        context,
                        values,
                    );
                };
                args.push(argument);
            }
            let text = args[0].to_string_lossy().into_owned();
            (false, args, text)
        }
        EVAL_TAG_STRING => {
            let bytes = values.string_bytes(command)?;
            let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
            let text = String::from_utf8_lossy(&bytes[..end]).into_owned();
            let line = eval_command_cstring(&bytes[..end]).ok_or(EvalStatus::RuntimeFatal)?;
            (true, vec![line], text)
        }
        _ => {
            let given = eval_given_type_spelling(command, values)?;
            return eval_throw_type_error(
                &format!("proc_open(): Argument #1 ($command) must be of type array|string, {given} given"),
                context,
                values,
            );
        }
    };

    // Descriptor spec.
    if !matches!(values.type_tag(descriptor_spec)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        let given = eval_given_type_spelling(descriptor_spec, values)?;
        return eval_throw_type_error(
            &format!("proc_open(): Argument #2 ($descriptor_spec) must be of type array, {given} given"),
            context,
            values,
        );
    }
    let mut descriptors: Vec<(i32, Source)> = Vec::new();
    // For each pipe: the child descriptor and whether the PARENT end is the readable one.
    let mut pipe_directions: Vec<(i32, bool)> = Vec::new();
    let mut declared: Vec<i64> = Vec::new();
    for position in 0..values.array_len(descriptor_spec)? {
        let key = values.array_iter_key(descriptor_spec, position)?;
        let entry = values.array_get(descriptor_spec, key);
        let key_tag = values.type_tag(key);
        let fd = eval_int_value(key, values);
        let released_key = values.release(key);
        let entry = entry?;
        released_key?;
        let decoded = eval_decode_spec_entry(entry, context, values);
        values.release(entry)?;
        let decoded = decoded?;
        if key_tag? != EVAL_TAG_INT {
            return eval_throw_builtin_value_error(
                "proc_open(): Argument #2 ($descriptor_spec) must be an integer indexed array",
                context,
                values,
            );
        }
        let fd = fd?;
        declared.push(fd);
        let fd = i32::try_from(fd).map_err(|_| EvalStatus::RuntimeFatal)?;
        match decoded {
            EvalSpecEntry::Invalid => {
                return eval_throw_builtin_value_error(
                    "proc_open(): Argument #2 ($descriptor_spec) must only contain arrays and streams",
                    context,
                    values,
                );
            }
            EvalSpecEntry::Stream(payload) => {
                let Some(source) = context.stream_resources_mut().raw_descriptor(payload) else {
                    values.warning("proc_open(): Unable to cast the stream to a file descriptor")?;
                    return values.bool_value(false);
                };
                descriptors.push((fd, Source::Fd(source)));
            }
            EvalSpecEntry::Array(elements) => {
                let [kind, first, second] = elements;
                let Some(kind) = kind else {
                    return eval_throw_builtin_value_error("Missing handle qualifier", context, values);
                };
                match kind.as_slice() {
                    b"pipe" => {
                        let Some(mode) = first else {
                            return eval_throw_builtin_value_error(
                                "Missing mode parameter for 'pipe'",
                                context,
                                values,
                            );
                        };
                        let child_reads = mode.first() == Some(&b'r');
                        descriptors.push((fd, Source::Pipe { child_reads }));
                        pipe_directions.push((fd, child_reads));
                    }
                    b"file" => {
                        let Some(path) = first else {
                            return eval_throw_builtin_value_error(
                                "Missing file name parameter for 'file'",
                                context,
                                values,
                            );
                        };
                        let Some(mode) = second else {
                            return eval_throw_builtin_value_error(
                                "Missing mode parameter for 'file'",
                                context,
                                values,
                            );
                        };
                        let path = eval_command_cstring(&path).ok_or(EvalStatus::RuntimeFatal)?;
                        descriptors.push((fd, Source::File { path, mode }));
                    }
                    b"null" => descriptors.push((fd, Source::Null)),
                    b"redirect" => {
                        let Some(target) = first else {
                            return eval_throw_builtin_value_error(
                                "Missing redirection target",
                                context,
                                values,
                            );
                        };
                        let target = String::from_utf8_lossy(&target)
                            .trim()
                            .parse::<i64>()
                            .unwrap_or(0);
                        descriptors.push((fd, Source::Redirect(target as i32)));
                    }
                    other => {
                        values.warning(&format!(
                            "proc_open(): {} is not a valid descriptor spec/mode",
                            String::from_utf8_lossy(other)
                        ))?;
                        return values.bool_value(false);
                    }
                }
            }
        }
    }
    for (_, source) in &descriptors {
        if let Source::Redirect(target) = source {
            if *target > 2 && !declared.contains(&i64::from(*target)) {
                values.warning(&format!("proc_open(): Redirection target {target} not found"))?;
                return values.bool_value(false);
            }
        }
    }

    // Working directory and environment.
    let cwd = match cwd {
        Some(cell) if values.type_tag(cell)? != EVAL_TAG_NULL => {
            let bytes = values.string_bytes(cell)?;
            Some(eval_command_cstring(&bytes).ok_or(EvalStatus::RuntimeFatal)?)
        }
        _ => None,
    };
    let env = match env_vars {
        Some(cell) if values.type_tag(cell)? != EVAL_TAG_NULL => {
            let mut entries = Vec::new();
            for position in 0..values.array_len(cell)? {
                let key = values.array_iter_key(cell, position)?;
                let value = values.array_get(cell, key);
                let key_is_string = values.type_tag(key).map(|tag| tag == EVAL_TAG_STRING);
                let name = values.string_bytes(key);
                let released_key = values.release(key);
                let value = value?;
                released_key?;
                let text = eval_spec_element_bytes(value, context, values);
                values.release(value)?;
                let text = text?.unwrap_or_default();
                if !key_is_string? {
                    continue;
                }
                let mut entry = name?;
                entry.push(b'=');
                entry.extend_from_slice(&text);
                if let Some(entry) = eval_command_cstring(&entry) {
                    entries.push(entry);
                }
            }
            Some(entries)
        }
        _ => None,
    };

    let request = Request { shell, args, cwd, env, descriptors };
    let spawned = match spawn(&request) {
        Ok(spawned) => spawned,
        Err(code) => {
            let reason = std::io::Error::from_raw_os_error(code).to_string();
            let reason = reason.split(" (os error").next().unwrap_or("").to_string();
            values.warning(&format!("proc_open(): posix_spawn() failed: {reason}"))?;
            return values.bool_value(false);
        }
    };

    // Pipes become eval streams, in `$descriptor_spec` order.
    let mut pipe_ids = Vec::with_capacity(spawned.pipes.len());
    let mut pipe_keys = Vec::with_capacity(spawned.pipes.len());
    for (child_fd, parent_fd) in &spawned.pipes {
        let child_reads = pipe_directions
            .iter()
            .find(|(number, _)| number == child_fd)
            .is_some_and(|(_, reads)| *reads);
        let id = context.stream_resources_mut().open_pipe_stream(
            *parent_fd,
            &command_text,
            !child_reads,
        );
        pipe_ids.push(id);
        pipe_keys.push(*child_fd);
    }
    let process = context
        .stream_resources_mut()
        .open_process_handle(spawned.pid, &command_text, pipe_ids.clone());
    if let Some(target) = pipes_target {
        let mut result = EvalArrayBuilder::assoc(values, pipe_ids.len())?;
        for (key, id) in pipe_keys.iter().zip(&pipe_ids) {
            let id = *id;
            result.index(usize::try_from(*key).map_err(|_| EvalStatus::RuntimeFatal)?, |values| {
                values.resource(id)
            })?;
        }
        let array = result.finish();
        eval_write_direct_ref_target(target, array, context, values, Some(ScopeCellOwnership::Owned))?;
    }
    values.resource(process)
}

/// Resolves a `proc_*` first argument to its process-handle payload, or throws php's TypeError.
pub(in crate::interpreter) fn eval_process_handle_id(
    function: &str,
    handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    if values.type_tag(handle)? != EVAL_TAG_RESOURCE {
        let given = eval_given_type_spelling(handle, values)?;
        return eval_throw_type_error(
            &format!("{function}(): Argument #1 ($process) must be of type resource, {given} given"),
            context,
            values,
        );
    }
    let id = eval_resource_payload(handle, values)?;
    if !context.stream_resources_mut().is_process_handle(id) {
        return eval_throw_type_error(
            &format!("{function}(): supplied resource is not a valid process resource"),
            context,
            values,
        );
    }
    Ok(id)
}
