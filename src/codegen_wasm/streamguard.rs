//! Purpose:
//! The argument checks php performs BEFORE a stream builtin runs: a closed stream resource is a
//! `TypeError`, and a few length arguments are `ValueError`s. The runtime helpers behind the
//! builtins work on a raw fd and answer a failure value for anything unusable, so without these
//! guards a read from a closed handle answered `""` and a negative length read anyway.
//!
//! Called from:
//! - `inst::lower_runtime_call` runs `emit_guards` before it dispatches a runtime call.
//! - `plan::plan_module` lays out `layout_strings` so every message is addressable.
//!
//! Key details:
//! - One table (`checks_for`) drives both the planner and the emitter, so a message that is
//!   emitted is always laid out.
//! - The messages are php 8.5's own, measured: `fread(): Argument #1 ($stream) must be an open
//!   stream resource`, with the function and parameter names varying and `fscanf` (which php
//!   words differently) special-cased.
//! - The raise is catchable when the module declares the exception tag and can build the class
//!   inline; otherwise it is php's uncaught-fatal line followed by exit status 255. Messages
//!   differ per function, so they are composed from a laid-out string, not from the fixed
//!   failure-code table `inst::emit_runtime_failure` uses for `fclose`.

use super::context::{FnCtx, Result};
use super::inst::operand;
use crate::ir::{
    Immediate, Instruction, IrHeapKind, IrType, Module, RuntimeCallTarget, RuntimeFnId,
};

/// What one guard tests, and the class it raises.
enum CheckKind {
    /// The operand is a stream/directory handle that must still be open: a `TypeError`.
    OpenHandle,
    /// The `i64` operand must be at least `bound`: a `ValueError`.
    AtLeast(i64),
}

/// One argument check of a stream builtin.
struct Check {
    /// Position of the operand in the call.
    operand: usize,
    /// What it must satisfy.
    kind: CheckKind,
    /// php's complete message.
    message: String,
}

impl Check {
    /// The PHP class the check raises.
    fn class_name(&self) -> &'static str {
        match self.kind {
            CheckKind::OpenHandle => "TypeError",
            CheckKind::AtLeast(_) => "ValueError",
        }
    }
}

/// The open-stream `TypeError` text for argument `position` of `function`.
fn open_stream_message(function: &str, position: usize, parameter: &str) -> String {
    format!("{function}(): Argument #{position} (${parameter}) must be an open stream resource")
}

/// Returns the checks php runs for `target`, in argument order; empty for any other builtin.
///
/// `fclose` is deliberately absent: its closed-stream `TypeError` predates this table and is
/// raised by `builtins::lower_file_builtin` through its fixed failure code.
fn checks_for(target: RuntimeFnId) -> Vec<Check> {
    let handle = |function: &str, position: usize, parameter: &str| Check {
        operand: position - 1,
        kind: CheckKind::OpenHandle,
        message: open_stream_message(function, position, parameter),
    };
    let at_least = |function: &str, bound: i64, text: &str| Check {
        operand: 1,
        kind: CheckKind::AtLeast(bound),
        message: format!("{function}(): Argument #2 ($length) must be {text}"),
    };
    match target {
        RuntimeFnId::Fwrite => vec![handle("fwrite", 1, "stream")],
        RuntimeFnId::Fread => vec![
            handle("fread", 1, "stream"),
            at_least("fread", 1, "greater than 0"),
        ],
        RuntimeFnId::Fgetc => vec![handle("fgetc", 1, "stream")],
        RuntimeFnId::Feof => vec![handle("feof", 1, "stream")],
        RuntimeFnId::Ftell => vec![handle("ftell", 1, "stream")],
        RuntimeFnId::Rewind => vec![handle("rewind", 1, "stream")],
        RuntimeFnId::Fseek => vec![handle("fseek", 1, "stream")],
        RuntimeFnId::Flock => vec![handle("flock", 1, "stream")],
        RuntimeFnId::Fprintf => vec![handle("fprintf", 1, "stream")],
        RuntimeFnId::Fscanf => vec![Check {
            operand: 0,
            kind: CheckKind::OpenHandle,
            message: "fscanf(): supplied resource is not a valid File-Handle resource".to_string(),
        }],
        RuntimeFnId::StreamGetContents => vec![
            handle("stream_get_contents", 1, "stream"),
            at_least("stream_get_contents", -1, "greater than or equal to -1"),
        ],
        RuntimeFnId::StreamGetLine => vec![
            handle("stream_get_line", 1, "stream"),
            at_least("stream_get_line", 0, "greater than or equal to 0"),
        ],
        RuntimeFnId::StreamGetMetaData => vec![handle("stream_get_meta_data", 1, "stream")],
        RuntimeFnId::StreamCopyToStream => vec![
            handle("stream_copy_to_stream", 1, "from"),
            handle("stream_copy_to_stream", 2, "to"),
        ],
        RuntimeFnId::StreamSetChunkSize => vec![handle("stream_set_chunk_size", 1, "stream")],
        RuntimeFnId::StreamSetReadBuffer => vec![handle("stream_set_read_buffer", 1, "stream")],
        RuntimeFnId::StreamSetWriteBuffer => vec![handle("stream_set_write_buffer", 1, "stream")],
        RuntimeFnId::Readdir => vec![handle("readdir", 1, "dir_handle")],
        RuntimeFnId::Rewinddir => vec![handle("rewinddir", 1, "dir_handle")],
        RuntimeFnId::Closedir => vec![handle("closedir", 1, "dir_handle")],
        _ => Vec::new(),
    }
}

/// The runtime-call target of `inst`, when it is a plain builtin call.
fn call_target(inst: &Instruction) -> Option<RuntimeFnId> {
    match inst.immediate {
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(target) | RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => Some(target),
        _ => None,
    }
}

/// The fatal line php prints for an uncaught `class: message`, laid out for the no-`catch` path.
fn fatal_line(class_name: &str, message: &str) -> String {
    format!("PHP Fatal error: Uncaught {class_name}: {message}\n")
}

/// Every string the guards of `module` need in static data: each message, and each fatal line.
///
/// Empty for a module that calls no guarded builtin, so programs that never touch a stream pay
/// nothing.
pub(super) fn layout_strings(module: &Module) -> Vec<String> {
    let mut strings = Vec::new();
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
    {
        for inst in &function.instructions {
            let Some(target) = call_target(inst) else {
                continue;
            };
            for check in checks_for(target) {
                for text in [
                    check.message.clone(),
                    fatal_line(check.class_name(), &check.message),
                ] {
                    if !strings.contains(&text) {
                        strings.push(text);
                    }
                }
            }
        }
    }
    strings
}

/// Emits `inst`'s argument checks, each raising php's error when it fails.
///
/// Runs before any operand of the call is pushed, so the failing arm leaves nothing on the
/// operand stack.
pub(super) fn emit_guards(ctx: &mut FnCtx, inst: &Instruction, target: RuntimeFnId) -> Result<()> {
    for check in checks_for(target) {
        if check.operand >= inst.operands.len() {
            continue; // an omitted defaulted argument carries its own (valid) default
        }
        let value = operand(inst, check.operand)?;
        match check.kind {
            CheckKind::OpenHandle => {
                let boxed = ctx
                    .function
                    .value(value)
                    .is_some_and(|value| value.ir_type == IrType::Heap(IrHeapKind::Mixed));
                ctx.emit_load_value(value)?;
                if boxed {
                    ctx.fb.ins("call $__rt_box_closed", "is this stream/dir handle closed?");
                } else {
                    ctx.fb
                        .ins("call $__rt_handle_closed", "is this predefined handle closed?");
                }
            }
            CheckKind::AtLeast(bound) => {
                ctx.emit_load_value(value)?;
                ctx.fb.ins(&format!("i64.const {bound}"), "the smallest accepted length");
                ctx.fb.ins("i64.lt_s", "below it?");
            }
        }
        ctx.fb.ins("if", &check.message);
        emit_raise(ctx, check.class_name(), &check.message)?;
        ctx.fb.ins("end", "end argument guard");
    }
    Ok(())
}

/// Raises `class_name: message` as a catchable error, or reports php's uncaught fatal and exits.
///
/// Catchable needs the module to declare the exception tag and to be able to build `class_name`
/// inline; anything else keeps the deterministic fatal, the same fallback
/// `inst::emit_runtime_failure` takes.
fn emit_raise(ctx: &mut FnCtx, class_name: &str, message: &str) -> Result<()> {
    let catchable = ctx.module.functions.iter().any(|f| f.flags.is_main)
        && super::function::module_uses_exceptions(ctx.module)
        && super::objects::runtime_error_is_constructible(ctx.module, class_name);
    if catchable {
        let (pointer, length) = ctx.default_str_literal(message)?;
        ctx.fb.ins(&format!("i32.const {pointer}"), "the laid-out message");
        ctx.fb.ins(&format!("i64.const {length}"), "its length");
        ctx.fb
            .ins("call $__rt_str_persist", "an owned copy for the exception's message slot");
        return super::objects::emit_runtime_error_throw_of_computed_message(
            ctx,
            class_name,
            super::function::UNCAUGHT_EXCEPTION_FAILURE_CODE,
        );
    }
    let line = fatal_line(class_name, message);
    let (pointer, length) = ctx.default_str_literal(&line)?;
    ctx.fb.ins("i32.const 2", "stderr");
    ctx.fb.ins(&format!("i32.const {pointer}"), "the laid-out fatal line");
    ctx.fb.ins(&format!("i32.const {length}"), "its length");
    ctx.fb.ins("call $__rt_wasi_write_all", "report php's uncaught error");
    ctx.fb.ins("drop", "the byte count");
    ctx.fb.ins("i32.const 255", "php's fatal exit status");
    ctx.fb.ins("call $wasi_proc_exit", "stop the process");
    ctx.fb.ins(
        "unreachable",
        "elephc-trap:post-noreturn:stream-guard-fatal proc_exit does not return",
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Pins the guard table against php's measured wording.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.

    use super::*;

    /// Verifies the table names the messages php 8.5 prints, argument by argument.
    #[test]
    fn messages_are_phps_own() {
        let fread = checks_for(RuntimeFnId::Fread);
        assert_eq!(
            fread[0].message,
            "fread(): Argument #1 ($stream) must be an open stream resource"
        );
        assert_eq!(fread[1].message, "fread(): Argument #2 ($length) must be greater than 0");
        let copy = checks_for(RuntimeFnId::StreamCopyToStream);
        assert_eq!(
            copy[1].message,
            "stream_copy_to_stream(): Argument #2 ($to) must be an open stream resource"
        );
        assert_eq!(
            checks_for(RuntimeFnId::Closedir)[0].message,
            "closedir(): Argument #1 ($dir_handle) must be an open stream resource"
        );
    }

    /// Verifies `fclose` stays out of the table: its own raise predates and would duplicate it.
    #[test]
    fn fclose_keeps_its_own_guard() {
        assert!(checks_for(RuntimeFnId::Fclose).is_empty());
    }
}
