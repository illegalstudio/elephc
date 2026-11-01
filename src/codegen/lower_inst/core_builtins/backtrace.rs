//! Purpose:
//! Materializes the active AOT PHP frame for Core backtrace builtins.
//!
//! Called from:
//! - `super::lower_core_builtin()` for `debug_backtrace()` and `debug_print_backtrace()`.
//!
//! Key details:
//! - The process entry frame is not PHP-visible, matching php-src backtraces.
//! - Arguments are read from their current local slots and copied into fresh Mixed storage.
//! - Negative limits produce no frames, while zero means no explicit limit.

use crate::codegen::platform::Arch;
use crate::codegen::{
    abi, emit_array_value_type_stamp,
    emit_box_current_value_as_mixed, Result,
};
use crate::ir::Instruction;
use crate::ir::Op;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::expect_operand;

const DEBUG_BACKTRACE_IGNORE_ARGS: i64 = 2;
const DEBUG_BACKTRACE_PROVIDE_OBJECT: i64 = 1;
const TRACE_STATE_BYTES: usize = 48;
const TRACE_CURSOR_OFFSET: usize = 0;
const TRACE_LIMIT_OFFSET: usize = 8;
const TRACE_COUNT_OFFSET: usize = 16;
const TRACE_OPTIONS_OFFSET: usize = 24;
const TRACE_ARRAY_OFFSET: usize = 32;
const PRINT_TRACE_STATE_BYTES: usize = 32;
const CALLBACK_LINE_OFFSET: usize = 0;
const CALLBACK_OPTIONS_OFFSET: usize = 8;
const CALLBACK_ARG_COUNT_OFFSET: usize = 16;
/// Reuses the argument-count word once the argument list is trimmed: the boxed `$this`, or zero.
const CALLBACK_THIS_OFFSET: usize = 16;
const CALLBACK_PRINT_INDEX_OFFSET: usize = 24;
/// The boxed-Mixed argument array the frame reader built for this frame, or zero.
const CALLBACK_ARGS_OFFSET: usize = 32;
/// The activation's caller-file descriptor (`{ptr, len}`, or zero for the entry file).
const CALLBACK_FILE_OFFSET: usize = 40;

/// Reads the nearest suspended source call site without materializing a PHP backtrace array.
pub(super) fn lower_diagnostic_caller_line(ctx: &mut FunctionContext<'_>) {
    let arm = ctx.emitter.target.arch == Arch::AArch64;
    let result = abi::int_result_reg(ctx.emitter);
    let cursor = abi::secondary_scratch_reg(ctx.emitter);
    let limit = abi::tertiary_scratch_reg(ctx.emitter);
    let scan = ctx.next_label("diagnostic_caller_scan");
    let next = ctx.next_label("diagnostic_caller_next");
    let empty = ctx.next_label("diagnostic_caller_empty");
    let done = ctx.next_label("diagnostic_caller_done");
    // -- initialize the activation cursor and source-coordinate bound --
    abi::emit_load_symbol_to_reg(ctx.emitter, cursor, "_exc_call_frame_top", 0);
    abi::emit_load_int_immediate(ctx.emitter, limit, i64::from(crate::span::SYNTHETIC_LINE_BASE));
    // -- scan suspended frames for the nearest real PHP call site --
    ctx.emitter.label(&scan);
    ctx.emitter.instruction(&if arm { format!("cmp {cursor}, #0") } else { format!("test {cursor}, {cursor}") }); // detect the end of the activation chain
    ctx.emitter.instruction(&format!("{} {empty}", if arm { "b.eq" } else { "jz" })); // fall back when no source caller remains
    abi::emit_load_from_address(ctx.emitter, result, cursor, 16);
    ctx.emitter.instruction(&format!("cmp {result}, {cursor}"));                // ownership guards are 32-byte records without a source-line field
    ctx.emitter.instruction(&format!("{} {next}", if arm { "b.eq" } else { "je" })); // skip guards before reading the larger PHP activation layout
    abi::emit_load_from_address(ctx.emitter, result, cursor, 32);
    ctx.emitter.instruction(&if arm { format!("cmp {result}, #0") } else { format!("test {result}, {result}") }); // skip frames with no source coordinate
    ctx.emitter.instruction(&format!("{} {next}", if arm { "b.eq" } else { "jz" })); // continue past generated frame entries
    ctx.emitter.instruction(&format!("cmp {result}, {limit}"));                 // distinguish real PHP lines from synthetic AST identities
    ctx.emitter.instruction(&format!("{} {done}", if arm { "b.lo" } else { "jb" })); // retain the nearest real call-site line
    ctx.emitter.label(&next);
    abi::emit_load_from_address(ctx.emitter, cursor, cursor, 0);
    abi::emit_jump(ctx.emitter, &scan);
    // -- return an unknown line only when no source frame survives --
    ctx.emitter.label(&empty);
    abi::emit_load_int_immediate(ctx.emitter, result, 0);
    ctx.emitter.label(&done);
}

/// Publishes the caller's source line before a nested PHP call so the callee's activation can record it.
pub(super) fn prepare_call_site(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    // Local retirement cannot emit its own PHP warning. A nested destructor publishes its
    // source location from its own frame, so eagerly updating diagnostics here only burdens
    // scalar Mixed rebinds whose fast path cannot invoke PHP code.
    let warns_at_call_site = inst.effects.contains(crate::ir::Effects::MAY_WARN)
        && !matches!(inst.op, Op::ReleaseLocalRefCell | Op::ReleaseLocalSlot);
    // The file this instruction's code came from: an included or autoloaded file's spans carry
    // its source identity. `None` is the entry file, `_script_source_file`.
    let span_file = ctx.span_source_file(inst.span).map(str::to_string);
    // A node with no place in the program (a prelude body, a compiler-built node) publishes
    // nothing, so the site its caller published stays current: php's internal functions have no
    // file or line of their own, and a warning raised inside one names the user call that reached
    // it. Publishing line 0 here made `highlight_file()`'s warnings say "on line 0".
    let from_source = inst.span.is_some_and(|span| span.is_from_source());
    if from_source && (instruction_may_enter_php_frame(inst) || warns_at_call_site) {
        // ONE pointer store names this site: a static `{file descriptor, line}` record, shared by
        // every site with the same file and line. It is both the location a warning raised here
        // names and the caller `file`/`line` a callee's prologue copies into its activation.
        // Publishing the five words separately (file, length, line, next file, next line) cost
        // about twenty instructions per call site, a few million in a Symfony build.
        let scratch = abi::int_result_reg(ctx.emitter);
        // Only source spans reach this point (see `from_source` above). A compiler-built node's
        // synthetic line (1 000 000 and up) was also numbered in a run-dependent order, which
        // made the records, and so two builds of one program, differ.
        let line = u64::from(
            inst.span
                .filter(|span| span.is_from_source())
                .map_or(0, |span| span.line),
        );
        let descriptor = match &span_file {
            Some(path) => {
                let (label, len) = ctx.data.add_string(path.as_bytes());
                let descriptor = ctx.data.add_words(vec![
                    crate::codegen_support::data_section::DataWord::Symbol(label),
                    crate::codegen_support::data_section::DataWord::U64(len as u64),
                ]);
                crate::codegen_support::data_section::DataWord::Symbol(descriptor)
            }
            None => crate::codegen_support::data_section::DataWord::U64(0),
        };
        let record = ctx.data.add_words(vec![
            descriptor,
            crate::codegen_support::data_section::DataWord::U64(line),
        ]);
        abi::emit_symbol_address(ctx.emitter, scratch, &record);
        abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_php_site", 0);
    }
    // The callee's prologue copies this line into its own activation (`[32]`) and installs its
    // function-level frame reader (`[24]`); a call site must never overwrite either word, or every
    // frame would report the line of its own next call instead of the line it was called from.
    Ok(())
}

/// Returns whether an instruction can synchronously enter another PHP-visible frame.
fn instruction_may_enter_php_frame(inst: &Instruction) -> bool {
    matches!(
        inst.op,
        Op::Call
            | Op::FunctionVariantCall
            | Op::ClosureBind
            | Op::LanguageConstructCall
            | Op::EvalLiteralCall
            | Op::EvalFunctionCall
            | Op::EvalFunctionCallArray
            | Op::EvalObjectNew
            | Op::RuntimeCall
            | Op::ExternCall
            | Op::ObjectNew
            | Op::DynamicObjectNew
            | Op::DynamicObjectNewMixed
            | Op::DynamicObjectNewWithoutConstructorMixed
            | Op::MethodCall
            | Op::NullsafeMethodCall
            | Op::StaticMethodCall
            | Op::EvalStaticMethodCall
            | Op::ClosureCall
            | Op::ExprCall
            | Op::CallableDescriptorInvoke
            | Op::PipeCall
            | Op::IteratorMethodCall
            | Op::SplRuntimeCall
            | Op::FiberRuntimeCall
            | Op::CoreBuiltin
    )
}

/// Builds the PHP backtrace array visible at the current AOT call site.
pub(super) fn lower_debug_backtrace(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let limit = expect_operand(inst, 1)?;
    let empty = ctx.next_label("debug_backtrace_empty");
    let done = ctx.next_label("debug_backtrace_done");
    ctx.load_value_to_result(limit)?;
    emit_branch_if_negative(ctx, &empty);
    emit_active_trace(ctx, inst)?;
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&empty);
    emit_empty_trace(ctx)?;
    ctx.emitter.label(&done);
    Ok(())
}

/// Walks the active activation chain and asks each PHP frame reader to materialize one frame.
fn emit_active_trace(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let options = expect_operand(inst, 0)?;
    let limit = expect_operand(inst, 1)?;
    allocate_mixed_array(ctx, 4);
    abi::emit_reserve_temporary_stack(ctx.emitter, TRACE_STATE_BYTES);
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        TRACE_ARRAY_OFFSET,
    );
    ctx.load_value_to_result(options)?;
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        TRACE_OPTIONS_OFFSET,
    );
    ctx.load_value_to_result(limit)?;
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        TRACE_LIMIT_OFFSET,
    );
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        TRACE_COUNT_OFFSET,
    );
    abi::emit_load_symbol_to_reg(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        "_exc_call_frame_top",
        0,
    );
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        TRACE_CURSOR_OFFSET,
    );

    let loop_label = ctx.next_label("debug_backtrace_walk");
    let next_label = ctx.next_label("debug_backtrace_next_frame");
    let finish_label = ctx.next_label("debug_backtrace_walk_done");
    ctx.emitter.label(&loop_label);
    emit_trace_limit_guard(ctx, &finish_label);
    let cursor_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, cursor_reg, TRACE_CURSOR_OFFSET);
    emit_branch_if_zero(ctx, cursor_reg, &finish_label);
    emit_skip_owned_guard(ctx, cursor_reg, &next_label);
    let callback_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r10",
    };
    abi::emit_load_from_address(ctx.emitter, callback_reg, cursor_reg, 24);
    emit_branch_if_zero(ctx, callback_reg, &next_label);
    emit_frame_reader_call(ctx, callback_reg);
    append_trace_frame(ctx);
    increment_trace_count(ctx);
    ctx.emitter.label(&next_label);
    advance_trace_cursor(ctx);
    abi::emit_jump(ctx.emitter, &loop_label);
    ctx.emitter.label(&finish_label);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        TRACE_ARRAY_OFFSET,
    );
    abi::emit_release_temporary_stack(ctx.emitter, TRACE_STATE_BYTES);
    Ok(())
}

/// Stops a trace walk after the requested positive number of visible frames.
fn emit_trace_limit_guard(ctx: &mut FunctionContext<'_>, done: &str) {
    let unlimited = ctx.next_label("debug_backtrace_unlimited");
    let limit_reg = abi::int_result_reg(ctx.emitter);
    let count_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, limit_reg, TRACE_LIMIT_OFFSET);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cbz x0, {unlimited}"));
            abi::emit_load_temporary_stack_slot(ctx.emitter, count_reg, TRACE_COUNT_OFFSET);
            ctx.emitter.instruction(&format!("cmp {count_reg}, x0"));           // compare emitted-frame count with the requested limit
            ctx.emitter.instruction(&format!("b.ge {done}"));                   // stop once the positive limit is reached
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // determine whether zero selected an unlimited trace
            ctx.emitter.instruction(&format!("jz {unlimited}"));                // skip the count guard for an unlimited trace
            abi::emit_load_temporary_stack_slot(ctx.emitter, count_reg, TRACE_COUNT_OFFSET);
            ctx.emitter.instruction(&format!("cmp {count_reg}, rax"));          // compare emitted-frame count with the requested limit
            ctx.emitter.instruction(&format!("jge {done}"));                    // stop once the positive limit is reached
        }
    }
    ctx.emitter.label(&unlimited);
}

/// Skips ownership guards whose offset 24 holds an owner instead of a frame reader.
fn emit_skip_owned_guard(ctx: &mut FunctionContext<'_>, cursor: &str, next: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr x11, [{cursor}, #16]"));      // load the activation self-pointer or an owned-value payload
            ctx.emitter.instruction(&format!("cmp x11, {cursor}"));             // distinguish activation records from ownership guards
            ctx.emitter.instruction(&format!("b.eq {next}"));                   // continue only when this record exposes a frame reader
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp QWORD PTR [{cursor} + 16], {cursor}")); // distinguish activation records from ownership guards
            ctx.emitter.instruction(&format!("je {next}"));                     // continue only when this record exposes a frame reader
        }
    }
}

/// Calls one activation's frame reader with its record and the original options mask.
fn emit_frame_reader_call(ctx: &mut FunctionContext<'_>, callback_reg: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", TRACE_CURSOR_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", TRACE_OPTIONS_OFFSET);
            abi::emit_load_int_immediate(ctx.emitter, "x2", -1);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", TRACE_CURSOR_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", TRACE_OPTIONS_OFFSET);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", -1);
        }
    }
    abi::emit_call_reg(ctx.emitter, callback_reg);
}

/// Appends and releases one owned boxed frame returned by a frame reader.
fn append_trace_frame(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_push_reg(ctx.emitter, &result_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                "x0",
                TRACE_ARRAY_OFFSET + 16,
            );
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                "rdi",
                TRACE_ARRAY_OFFSET + 16,
            );
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 0);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
    abi::emit_store_to_sp(ctx.emitter, &result_reg, TRACE_ARRAY_OFFSET + 16);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &result_reg, 0);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Increments the number of visible frames already appended to the result.
fn increment_trace_count(ctx: &mut FunctionContext<'_>) {
    let count_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, count_reg, TRACE_COUNT_OFFSET);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("add x0, x0, #1"),             // account for one appended trace frame
        Arch::X86_64 => ctx.emitter.instruction("add rax, 1"),                  // account for one appended trace frame
    }
    abi::emit_store_to_sp(ctx.emitter, count_reg, TRACE_COUNT_OFFSET);
}

/// Advances the walk cursor to the preceding activation record.
fn advance_trace_cursor(ctx: &mut FunctionContext<'_>) {
    let cursor_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, cursor_reg, TRACE_CURSOR_OFFSET);
    abi::emit_load_from_address(ctx.emitter, cursor_reg, cursor_reg, 0);
    abi::emit_store_to_sp(ctx.emitter, cursor_reg, TRACE_CURSOR_OFFSET);
}

/// Emits the function-level callback that reads the suspended PHP frame's live locals.
///
/// The prologue publishes `label` in the activation record (`[24]`), so every frame on the chain
/// can be walked, including one that never calls anything before it throws. With `materialize`
/// the callback can also build a `debug_backtrace()` frame hash; without it only the print mode
/// used by the uncaught-exception report is emitted, which keeps the per-function size small for
/// programs that never call `debug_backtrace()`.
pub(in crate::codegen) fn emit_function_frame_reader(
    ctx: &mut FunctionContext<'_>,
    label: &str,
    materialize: bool,
) -> Result<()> {
    ctx.emitter.blank();
    ctx.emitter.comment("PHP frame reader for backtraces and uncaught-exception traces");
    // A real symbol, so Mach-O never folds it into its function's addend, but file-local:
    // only the prologue in this object takes its address.
    ctx.emitter.label(label);
    emit_frame_reader_callback(ctx, materialize)
}

/// Emits one callback body that reads the suspended PHP frame's live locals.
///
/// The reader only does what depends on THIS function: it reads its parameters from their frame
/// slots into a boxed argument array. Printing (`debug_print_backtrace()`) is one call into
/// `__rt_backtrace_print_frame` with the finished array, so no function carries its own copy of
/// the "#n file(line): name(args)" renderer. Without `materialize` only that print mode exists.
fn emit_frame_reader_callback(ctx: &mut FunctionContext<'_>, materialize: bool) -> Result<()> {
    // The reader switches x29 to the suspended frame; x28 is not that frame's base.
    let frame_slot_base = ctx.emitter.frame_slot_base.take();
    emit_frame_reader_prologue(ctx);
    capture_callback_argument_count(ctx)?;
    emit_build_frame_arguments(ctx)?;
    if !materialize {
        emit_print_frame_call(ctx);
        emit_frame_reader_epilogue(ctx);
        ctx.emitter.frame_slot_base = frame_slot_base;
        return Ok(());
    }
    let materialize_label = ctx.next_label("backtrace_frame_materialize");
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        CALLBACK_PRINT_INDEX_OFFSET,
    );
    emit_branch_if_negative(ctx, &materialize_label);
    emit_print_frame_call(ctx);
    emit_frame_reader_epilogue(ctx);
    ctx.emitter.label(&materialize_label);
    emit_frame_hash_call(ctx)?;
    emit_frame_reader_epilogue(ctx);
    ctx.emitter.frame_slot_base = frame_slot_base;
    Ok(())
}

/// Loads the frame's `{ptr, len}` file record into `reg`: the descriptor the call site published
/// in the activation, or the entry file's static record when it published none.
fn load_frame_file_record(ctx: &mut FunctionContext<'_>, reg: &str) {
    let file = ctx.module.source_path.clone().unwrap_or_else(|| "Unknown".to_string());
    let (file_label, file_len) = ctx.data.add_string(file.as_bytes());
    let entry_record = ctx.data.add_words(vec![
        crate::codegen_support::data_section::DataWord::Symbol(file_label),
        crate::codegen_support::data_section::DataWord::U64(file_len as u64),
    ]);
    let have_file = ctx.next_label("backtrace_file_record_loaded");
    abi::emit_load_temporary_stack_slot(ctx.emitter, reg, CALLBACK_FILE_OFFSET);
    emit_branch_if_nonzero(ctx, reg, &have_file);
    abi::emit_symbol_address(ctx.emitter, reg, &entry_record);
    ctx.emitter.label(&have_file);
}

/// Builds this frame's `debug_backtrace()` entry with one call into
/// `__rt_backtrace_frame_hash(file record, line, info record, this, args)`.
///
/// `this` is boxed here (it is a local of THIS function) and only when PROVIDE_OBJECT asks for
/// it; the static info record names the function, and the class and call type for a method.
fn emit_frame_hash_call(ctx: &mut FunctionContext<'_>) -> Result<()> {
    use crate::codegen_support::data_section::DataWord;
    // The argument-count slot is free once the argument list is trimmed: it carries `this`.
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result, 0);
    abi::emit_store_to_sp(ctx.emitter, result, CALLBACK_THIS_OFFSET);
    if let Some(slot) = ctx.local_slot_by_name("this") {
        let skip = ctx.next_label("debug_backtrace_skip_object");
        abi::emit_load_temporary_stack_slot(ctx.emitter, result, CALLBACK_OPTIONS_OFFSET);
        let provide = ctx.next_label("debug_backtrace_provide_object");
        emit_branch_if_bit_set(ctx, DEBUG_BACKTRACE_PROVIDE_OBJECT, &provide);
        abi::emit_jump(ctx.emitter, &skip);
        ctx.emitter.label(&provide);
        ctx.load_local_to_result(slot)?;
        let ty = ctx.local_php_type(slot)?;
        // The trace's frame hash takes ownership of this box. An already-boxed Mixed `$this`
        // (a closure bound to a receiver is stored as a Mixed capture) is a BORROWED cell, and
        // `emit_box_current_value_as_mixed` deliberately does nothing for Mixed, so passing it
        // through made the trace free the descriptor's own capture: the bound `$this` box lost a
        // reference on every exception whose trace materialized this frame, and the next
        // `$this` read in the closure used freed memory. Clone into a fresh owned cell.
        if matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_clone");
        } else {
            emit_box_current_value_as_mixed(ctx.emitter, &ty);
        }
        abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), CALLBACK_THIS_OFFSET);
        ctx.emitter.label(&skip);
    }
    let (class, function, call_type) = frame_function_parts(ctx);
    let (function_label, function_len) = ctx.data.add_string(function.as_bytes());
    let mut words = vec![DataWord::Symbol(function_label), DataWord::U64(function_len as u64)];
    match class {
        Some(class) => {
            let (class_label, class_len) = ctx.data.add_string(class.as_bytes());
            let (type_label, type_len) = ctx.data.add_string(call_type.as_bytes());
            words.extend([
                DataWord::Symbol(class_label),
                DataWord::U64(class_len as u64),
                DataWord::Symbol(type_label),
                DataWord::U64(type_len as u64),
            ]);
        }
        None => words.extend((0..4).map(|_| DataWord::U64(0))),
    }
    let info = ctx.data.add_words(words);
    let (file_reg, line, info_reg, this, args) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x0", "x1", "x2", "x3", "x4"),
        Arch::X86_64 => ("rdi", "rsi", "rdx", "rcx", "r8"),
    };
    load_frame_file_record(ctx, file_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, line, CALLBACK_LINE_OFFSET);
    abi::emit_symbol_address(ctx.emitter, info_reg, &info);
    abi::emit_load_temporary_stack_slot(ctx.emitter, this, CALLBACK_THIS_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, args, CALLBACK_ARGS_OFFSET);
    abi::emit_call_label(ctx.emitter, "__rt_backtrace_frame_hash");
    Ok(())
}

/// Builds the frame's argument array into `CALLBACK_ARGS_OFFSET` (zero when arguments are
/// suppressed), before anything else is pushed on the reader's frame.
fn emit_build_frame_arguments(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result, 0);
    abi::emit_store_to_sp(ctx.emitter, result, CALLBACK_ARGS_OFFSET);
    let skip = ctx.next_label("debug_backtrace_skip_args");
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, CALLBACK_OPTIONS_OFFSET);
    emit_branch_if_bit_set(ctx, DEBUG_BACKTRACE_IGNORE_ARGS, &skip);
    emit_argument_array_from_live_locals(ctx)?;
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), CALLBACK_ARGS_OFFSET);
    ctx.emitter.label(&skip);
    Ok(())
}

/// Prints this frame through the shared renderer and releases the argument array.
///
/// `__rt_backtrace_print_frame(index, file descriptor, line, name descriptor, args)`; the file
/// descriptor is the activation's `{ptr, len}` record, or the entry file's static record when the
/// activation carries none.
fn emit_print_frame_call(ctx: &mut FunctionContext<'_>) {
    let name = display_function_name(ctx);
    let (name_label, name_len) = ctx.data.add_string(name.as_bytes());
    let name_record = ctx.data.add_words(vec![
        crate::codegen_support::data_section::DataWord::Symbol(name_label),
        crate::codegen_support::data_section::DataWord::U64(name_len as u64),
    ]);
    let (index, file_reg, line, name_reg, args) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x0", "x1", "x2", "x3", "x4"),
        Arch::X86_64 => ("rdi", "rsi", "rdx", "rcx", "r8"),
    };
    abi::emit_load_temporary_stack_slot(ctx.emitter, index, CALLBACK_PRINT_INDEX_OFFSET);
    load_frame_file_record(ctx, file_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, line, CALLBACK_LINE_OFFSET);
    abi::emit_symbol_address(ctx.emitter, name_reg, &name_record);
    abi::emit_load_temporary_stack_slot(ctx.emitter, args, CALLBACK_ARGS_OFFSET);
    abi::emit_call_label(ctx.emitter, "__rt_backtrace_print_frame");
    let release = ctx.next_label("backtrace_print_args_released");
    let array = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, array, CALLBACK_ARGS_OFFSET);
    emit_branch_if_zero(ctx, array, &release);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    ctx.emitter.label(&release);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
}

/// Saves callback state and selects the suspended PHP frame pointer for local reads.
fn emit_frame_reader_prologue(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #64");                         // reserve callback state and saved frame linkage
            ctx.emitter.instruction("stp x29, x30, [sp, #48]");                 // preserve the callback caller frame
            ctx.emitter.instruction("ldr x9, [x0, #32]");                       // load the source frame's caller line
            ctx.emitter.instruction("str x9, [sp]");                            // save the caller line for frame materialization
            ctx.emitter.instruction("ldr x9, [x0, #40]");                       // load the source frame's caller-file descriptor
            ctx.emitter.instruction("str x9, [sp, #40]");                       // save it for the frame's `file`
            ctx.emitter.instruction("str x1, [sp, #8]");                        // save the original debug options mask
            ctx.emitter.instruction("str x2, [sp, #24]");                       // save the materialize or print mode selector
            ctx.emitter.instruction("ldr x29, [x0, #16]");                      // switch local reads to the suspended PHP frame
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("push rbp");                                // preserve the callback caller frame pointer
            ctx.emitter.instruction("sub rsp, 48");                             // reserve callback state with SysV alignment
            ctx.emitter.instruction("mov rax, QWORD PTR [rdi + 32]");           // load the source frame's caller line
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // save the caller line for frame materialization
            ctx.emitter.instruction("mov rax, QWORD PTR [rdi + 40]");           // load the source frame's caller-file descriptor
            ctx.emitter.instruction("mov QWORD PTR [rsp + 40], rax");           // save it for the frame's `file`
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rsi");            // save the original debug options mask
            ctx.emitter.instruction("mov QWORD PTR [rsp + 24], rdx");           // save the materialize or print mode selector
            ctx.emitter.instruction("mov rbp, QWORD PTR [rdi + 16]");           // switch local reads to the suspended PHP frame
        }
    }
}

/// Restores callback frame linkage while preserving the boxed frame result.
fn emit_frame_reader_epilogue(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldp x29, x30, [sp, #48]");                 // restore the callback caller frame
            ctx.emitter.instruction("add sp, sp, #64");                         // release callback scratch storage
            ctx.emitter.instruction("ret");                                     // return the frame result to the trace walker
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("add rsp, 48");                             // release callback scratch storage
            ctx.emitter.instruction("pop rbp");                                 // restore the callback caller frame pointer
            ctx.emitter.instruction("ret");                                     // return the frame result to the trace walker
        }
    }
}

/// Captures the caller-supplied argument count, or `-1` when every fixed parameter is present.
fn capture_callback_argument_count(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let count_slot = ctx.local_slot_by_name(crate::func_args::HIDDEN_ARGC_PARAM);
    if let Some(count_slot) = count_slot {
        ctx.load_local_to_result(count_slot)?;
        abi::emit_store_to_sp(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            CALLBACK_ARG_COUNT_OFFSET,
        );
        return Ok(());
    }

    let signature = ctx.function.signature.clone();
    let hidden_count = signature
        .as_ref()
        .is_some_and(crate::func_args::sig_collects_optional_arg_count);
    if hidden_count {
        let collector = ctx
            .local_slot_by_name(crate::func_args::HIDDEN_ARGS_PARAM)
            .expect("optional argument-count metadata requires a hidden collector");
        ctx.load_local_to_result(collector)?;
        emit_load_collector_leading_count(ctx);
        match ctx.emitter.target.arch {
            Arch::AArch64 => abi::emit_store_to_sp(ctx.emitter, "x1", CALLBACK_ARG_COUNT_OFFSET),
            Arch::X86_64 => {
                abi::emit_store_to_sp(ctx.emitter, "rdi", CALLBACK_ARG_COUNT_OFFSET)
            }
        }
        return Ok(());
    }

    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), -1);
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        CALLBACK_ARG_COUNT_OFFSET,
    );
    Ok(())
}

/// Appends a boxed parameter slot, or a boxed `null` when the slot holds no box yet.
///
/// An omitted optional parameter of a frame entered without an argument count -- Symfony's
/// `Kernel::handle($request)`, called from the runtime runner -- can still hold a null pointer
/// when an exception's trace reads it. Pushed as a cell, it left a hole the trace renderer
/// dereferenced and the 404 page killed the worker.
fn emit_append_boxed_parameter_or_null(ctx: &mut FunctionContext<'_>) {
    let null_box = ctx.next_label("debug_backtrace_param_null_box");
    let appended = ctx.next_label("debug_backtrace_param_appended");
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("cbz x0, {null_box}")), // an empty slot has no box to borrow
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // inspect whether the slot holds a box
            ctx.emitter.instruction(&format!("jz {null_box}"));                 // an empty slot has no box to borrow
        }
    }
    append_box_to_saved_array(ctx, false);
    abi::emit_jump(ctx.emitter, &appended);
    ctx.emitter.label(&null_box);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #8");                              // runtime value tag 8 is null
            ctx.emitter.instruction("mov x1, xzr");                             // null carries no payload
            ctx.emitter.instruction("mov x2, xzr");                             // null carries no second word
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, 8");                              // runtime value tag 8 is null
            ctx.emitter.instruction("xor edi, edi");                            // null carries no payload
            ctx.emitter.instruction("xor esi, esi");                            // null carries no second word
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    append_box_to_saved_array(ctx, true);
    ctx.emitter.label(&appended);
}

/// Reads the caller argument count a collector carries as its FIRST entry into `x1` / `rdi`.
///
/// Input: the collector pointer in the result register. A packed collector keeps the count in
/// its first boxed slot. A spread call hands the callee a HASH collector (Symfony's lazy event
/// listeners: `$listener(...$args)`), where `[collector + 24]` is hash metadata: the count read
/// from it was nonsense, the frame's argument list was sliced to that length, and the trace held
/// unfilled slots that `getTraceAsString()` dereferenced -- the 404 page killed the worker. The
/// hash case reads its first entry by insertion order instead.
fn emit_load_collector_leading_count(ctx: &mut FunctionContext<'_>) {
    let hash = ctx.next_label("debug_backtrace_count_hash");
    let native = ctx.next_label("debug_backtrace_count_native");
    let done = ctx.next_label("debug_backtrace_count_done");
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_push_reg(ctx.emitter, &result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #3");                              // is the collector hash storage?
            ctx.emitter.instruction(&format!("b.eq {hash}"));                   // read the first hash entry by insertion order
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 0);
            ctx.emitter.instruction("ldr x0, [x0, #24]");                       // load the hidden caller argument count box
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction(&format!("b {done}"));                      // the count is in x1
            ctx.emitter.label(&hash);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 0);
            ctx.emitter.instruction("mov x1, #0");                              // start a fresh insertion-order walk
            abi::emit_call_label(ctx.emitter, "__rt_hash_iter_next");
            ctx.emitter.instruction("cmp x5, #7");                              // is the count entry a boxed Mixed cell?
            ctx.emitter.instruction(&format!("b.ne {native}"));                 // a native int is the count itself
            ctx.emitter.instruction("mov x0, x3");                              // unbox the borrowed count cell
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction(&format!("b {done}"));                      // the count is in x1
            ctx.emitter.label(&native);
            ctx.emitter.instruction("mov x1, x3");                              // the entry's low word is the count
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp eax, 3");                              // is the collector hash storage?
            ctx.emitter.instruction(&format!("je {hash}"));                     // read the first hash entry by insertion order
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 0);
            ctx.emitter.instruction("mov rax, QWORD PTR [rax + 24]");           // load the hidden caller argument count box
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction(&format!("jmp {done}"));                    // the count is in rdi
            ctx.emitter.label(&hash);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
            ctx.emitter.instruction("xor esi, esi");                            // start a fresh insertion-order walk
            abi::emit_call_label(ctx.emitter, "__rt_hash_iter_next");
            ctx.emitter.instruction("cmp r9, 7");                               // is the count entry a boxed Mixed cell?
            ctx.emitter.instruction(&format!("jne {native}"));                  // a native int is the count itself
            ctx.emitter.instruction("mov rax, rcx");                            // unbox the borrowed count cell
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction(&format!("jmp {done}"));                    // the count is in rdi
            ctx.emitter.label(&native);
            ctx.emitter.instruction("mov rdi, rcx");                            // the entry's low word is the count
        }
    }
    ctx.emitter.label(&done);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Appends the hidden argument collector in the result register, from index `start`, to the
/// boxed-Mixed destination array on top of the stack.
///
/// A `mixed` collector -- every function with a hidden argument collector has one -- goes through
/// the shared `__rt_backtrace_append_tail`; a typed one keeps its inline loop, whose element
/// loads depend on the static element type.
fn append_argument_tail(ctx: &mut FunctionContext<'_>, start: usize, element_ty: &PhpType) {
    if matches!(element_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x1, x0");                          // pass the collector
                ctx.emitter.instruction("ldr x0, [sp]");                        // pass the destination array saved on the stack top
                abi::emit_load_int_immediate(ctx.emitter, "x2", start as i64);
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rsi, rax");                        // pass the collector
                ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");            // pass the destination array saved on the stack top
                abi::emit_load_int_immediate(ctx.emitter, "rdx", start as i64);
            }
        }
        abi::emit_call_label(ctx.emitter, "__rt_backtrace_append_tail");
        abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        return;
    }
    let base = ctx.next_label("debug_backtrace_tail");
    let mut counter = 0usize;
    let mut labels = |prefix: &str| {
        counter += 1;
        format!("{base}_{prefix}_{counter}")
    };
    crate::codegen_support::backtrace_tail::append_argument_array_range(
        ctx.emitter,
        &mut labels,
        crate::codegen_support::backtrace_tail::TailStart::Immediate(start),
        element_ty,
    );
}

/// Drops defaulted parameters beyond the caller-supplied argument count (see
/// `__rt_backtrace_trim_args`); the array is in the result register.
fn trim_frame_arguments(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", CALLBACK_ARG_COUNT_OFFSET);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the argument array
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", CALLBACK_ARG_COUNT_OFFSET);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_backtrace_trim_args");
}

/// Builds a fresh indexed Mixed array from the current values of source-visible parameters.
fn emit_argument_array_from_live_locals(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let signature = ctx.function.signature.clone();
    let source_variadic = signature
        .as_ref()
        .and_then(|signature| signature.variadic.clone())
        .filter(|name| name != crate::func_args::HIDDEN_ARGS_PARAM);
    let parameters = signature
        .as_ref()
        .map(|signature| {
            signature
                .params
                .iter()
                .filter(|(name, _)| {
                    name != crate::func_args::HIDDEN_ARGS_PARAM
                        && name != crate::func_args::HIDDEN_ARGC_PARAM
                        && source_variadic.as_deref() != Some(name.as_str())
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    allocate_mixed_array(ctx, parameters.len());
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    for (name, _) in parameters {
        let Some(slot) = ctx.local_slot_by_name(&name) else {
            continue;
        };
        let ty = ctx.load_local_to_result(slot)?;
        let borrowed_box = matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_));
        // A `?ptr<T>` parameter keeps its union, so address 0 is PHP null.
        let box_ty = ctx.local_mixed_box_type(slot)?;
        if super::backtrace_sensitive::is_sensitive(ctx, &name) {
            if super::backtrace_sensitive::wrapper_class_is_emitted(ctx) {
                super::backtrace_sensitive::wrap_value(ctx, &box_ty)?;
            } else {
                // A program that never builds the wrapper class still must not list the value.
                emit_sensitive_placeholder_box(ctx);
            }
            append_box_to_saved_array(ctx, true);
            continue;
        }
        if !borrowed_box {
            emit_box_current_value_as_mixed(ctx.emitter, &box_ty);
            append_box_to_saved_array(ctx, true);
            continue;
        }
        emit_append_boxed_parameter_or_null(ctx);
    }
    // php lists a variadic's surplus arguments as ordinary entries after the fixed ones, so a
    // frame `f($a, ...$tail)` called `f(1, $x, $y)` reports `[1, $x, $y]`. Leaving them out also
    // let the trace's retained copy go missing: the objects passed through `...$tail` were
    // released before a same-frame catch, where php keeps them alive until the exception dies.
    // A frame that also keeps the argument snapshot (`crate::func_args`, every frame once the
    // program calls `debug_backtrace()`) already holds those positional values in it, read
    // below; appending the variadic too listed each of them twice.
    let variadic_sensitive = source_variadic
        .as_deref()
        .is_some_and(|name| super::backtrace_sensitive::is_sensitive(ctx, name));
    if let Some(variadic_slot) = source_variadic
        .as_deref()
        .filter(|_| ctx.local_slot_by_name(crate::func_args::HIDDEN_ARGS_PARAM).is_none())
        .and_then(|name| ctx.local_slot_by_name(name))
    {
        if let PhpType::Array(element_ty) = ctx.local_php_type(variadic_slot)?.codegen_repr() {
            ctx.load_local_to_result(variadic_slot)?;
            append_argument_tail_checked(ctx, 0, &element_ty, variadic_sensitive)?;
        }
    }
    if let Some(hidden_args) = ctx.local_slot_by_name(crate::func_args::HIDDEN_ARGS_PARAM) {
        let hidden_ty = ctx.local_php_type(hidden_args)?;
        let element_ty = match hidden_ty {
            PhpType::Array(element_ty) => *element_ty,
            other => {
                return Err(crate::codegen::CodegenIrError::unsupported(format!(
                    "backtrace hidden argument collector PHP type {other:?}"
                )));
            }
        };
        ctx.load_local_to_result(hidden_args)?;
        let starts_with_count = ctx
            .local_slot_by_name(crate::func_args::HIDDEN_ARGC_PARAM)
            .is_some()
            || signature
                .as_ref()
                .is_some_and(crate::func_args::sig_collects_optional_arg_count);
        append_argument_tail_checked(
            ctx,
            usize::from(starts_with_count),
            &element_ty,
            source_variadic.as_ref().is_some_and(|name| super::backtrace_sensitive::is_sensitive(ctx, name)),
        )?;
    }
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    trim_frame_arguments(ctx);
    Ok(())
}

/// Appends the hidden/variadic argument collector in the result register from index `start`,
/// wrapping every element when the variadic parameter is `#[\SensitiveParameter]`.
fn append_argument_tail_checked(
    ctx: &mut FunctionContext<'_>,
    start: usize,
    element_ty: &PhpType,
    sensitive: bool,
) -> Result<()> {
    if !sensitive {
        append_argument_tail(ctx, start, element_ty);
        return Ok(());
    }
    if !super::backtrace_sensitive::wrapper_class_is_emitted(ctx) {
        // Nothing may list the secret values: the collector is simply not appended.
        return Ok(());
    }
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_push_reg(ctx.emitter, &result_reg);
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    abi::emit_load_int_immediate(ctx.emitter, &result_reg, start as i64);
    abi::emit_store_to_sp(ctx.emitter, &result_reg, 0);
    let source_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, source_reg, 32);
    abi::emit_load_from_address(ctx.emitter, source_reg, source_reg, 0);
    abi::emit_store_to_sp(ctx.emitter, source_reg, 8);

    let loop_label = ctx.next_label("debug_backtrace_arg_tail");
    let done = ctx.next_label("debug_backtrace_arg_tail_done");
    ctx.emitter.label(&loop_label);
    emit_argument_tail_done_guard(ctx, &done);
    emit_load_argument_tail_value(ctx, &element_ty.codegen_repr());
    super::backtrace_sensitive::wrap_value(ctx, element_ty)?;
    append_argument_tail_box(ctx, true);
    increment_argument_tail_index(ctx);
    abi::emit_jump(ctx.emitter, &loop_label);
    ctx.emitter.label(&done);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    let discard = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, discard);
    Ok(())
}

/// Boxes a placeholder string standing for a sensitive argument when the wrapper class is absent.
fn emit_sensitive_placeholder_box(ctx: &mut FunctionContext<'_>) {
    let (label, len) = ctx.data.add_string(b"Object(SensitiveParameterValue)");
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
}

/// Branches after every requested hidden argument has been appended.
fn emit_argument_tail_done_guard(ctx: &mut FunctionContext<'_>, done: &str) {
    let index_reg = abi::int_result_reg(ctx.emitter);
    let length_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, index_reg, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, length_reg, 8);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {index_reg}, {length_reg}"));
            ctx.emitter.instruction(&format!("b.ge {done}"));                   // stop after every hidden argument is appended
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {index_reg}, {length_reg}"));
            ctx.emitter.instruction(&format!("jge {done}"));                    // stop after every hidden argument is appended
        }
    }
}

/// Loads one typed hidden-argument array slot into the canonical result registers.
fn emit_load_argument_tail_value(ctx: &mut FunctionContext<'_>, element_ty: &PhpType) {
    let stride = element_ty.stack_size().max(8);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", 32);
            if stride == 16 {
                ctx.emitter.instruction("lsl x9, x9, #4");                      // scale the index for a two-word typed slot
                ctx.emitter.instruction("add x10, x10, x9");                    // select the indexed typed slot
                ctx.emitter.instruction("add x10, x10, #24");                   // advance past array metadata
            } else {
                ctx.emitter.instruction("add x10, x10, #24");                   // advance past array metadata
                ctx.emitter.instruction("add x10, x10, x9, lsl #3");            // select the indexed one-word slot
            }
            match element_ty {
                PhpType::Float => ctx.emitter.instruction("ldr d0, [x10]"),     // load a typed floating-point argument
                PhpType::Str => ctx.emitter.instruction("ldp x1, x2, [x10]"),   // load a typed string pointer and length
                PhpType::TaggedScalar => ctx.emitter.instruction("ldp x0, x1, [x10]"), // load a typed tagged-scalar pair
                _ => ctx.emitter.instruction("ldr x0, [x10]"),                  // load a one-word typed argument
            }
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", 32);
            if stride == 16 {
                ctx.emitter.instruction("shl r10, 4");                          // scale the index for a two-word typed slot
                ctx.emitter.instruction("lea r11, [r11 + r10 + 24]");           // select the indexed typed slot after metadata
            } else {
                ctx.emitter.instruction("lea r11, [r11 + r10 * 8 + 24]");       // select the indexed one-word slot after metadata
            }
            match element_ty {
                PhpType::Float => ctx.emitter.instruction("movsd xmm0, QWORD PTR [r11]"), // load a typed floating-point argument
                PhpType::Str => {
                    ctx.emitter.instruction("mov rax, QWORD PTR [r11]");        // load the typed string pointer
                    ctx.emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");    // load the typed string length
                }
                PhpType::TaggedScalar => {
                    ctx.emitter.instruction("mov rax, QWORD PTR [r11]");        // load the tagged-scalar payload
                    ctx.emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");    // load the tagged-scalar tag
                }
                _ => ctx.emitter.instruction("mov rax, QWORD PTR [r11]"),       // load a one-word typed argument
            }
        }
    }
}

/// Appends a boxed hidden argument and optionally releases the temporary cell afterward.
fn append_argument_tail_box(ctx: &mut FunctionContext<'_>, release_temporary: bool) {
    let box_reg = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_push_reg(ctx.emitter, &box_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 64);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 64);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 0);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
    abi::emit_store_to_sp(ctx.emitter, &box_reg, 64);
    if release_temporary {
        abi::emit_load_temporary_stack_slot(ctx.emitter, &box_reg, 0);
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    }
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Advances the hidden-argument array index after one retained append.
fn increment_argument_tail_index(ctx: &mut FunctionContext<'_>) {
    let index_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, index_reg, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("add x0, x0, #1"),             // advance the hidden-argument index
        Arch::X86_64 => ctx.emitter.instruction("add rax, 1"),                  // advance the hidden-argument index
    }
    abi::emit_store_to_sp(ctx.emitter, index_reg, 0);
}

/// Prints every selected AOT activation in PHP's numbered compact backtrace form.
pub(super) fn lower_debug_print_backtrace(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let options = expect_operand(inst, 0)?;
    let limit = expect_operand(inst, 1)?;
    let done = ctx.next_label("debug_print_backtrace_done");
    ctx.load_value_to_result(limit)?;
    emit_branch_if_negative(ctx, &done);
    abi::emit_reserve_temporary_stack(ctx.emitter, PRINT_TRACE_STATE_BYTES);
    ctx.load_value_to_result(options)?;
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        TRACE_OPTIONS_OFFSET,
    );
    ctx.load_value_to_result(limit)?;
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        TRACE_LIMIT_OFFSET,
    );
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        TRACE_COUNT_OFFSET,
    );
    abi::emit_load_symbol_to_reg(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        "_exc_call_frame_top",
        0,
    );
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        TRACE_CURSOR_OFFSET,
    );

    let loop_label = ctx.next_label("debug_print_backtrace_walk");
    let next_label = ctx.next_label("debug_print_backtrace_next_frame");
    let finish_label = ctx.next_label("debug_print_backtrace_walk_done");
    ctx.emitter.label(&loop_label);
    emit_trace_limit_guard(ctx, &finish_label);
    let cursor_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, cursor_reg, TRACE_CURSOR_OFFSET);
    emit_branch_if_zero(ctx, cursor_reg, &finish_label);
    emit_skip_owned_guard(ctx, cursor_reg, &next_label);
    let callback_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r10",
    };
    abi::emit_load_from_address(ctx.emitter, callback_reg, cursor_reg, 24);
    emit_branch_if_zero(ctx, callback_reg, &next_label);
    emit_frame_printer_call(ctx, callback_reg);
    increment_trace_count(ctx);
    ctx.emitter.label(&next_label);
    advance_trace_cursor(ctx);
    abi::emit_jump(ctx.emitter, &loop_label);
    ctx.emitter.label(&finish_label);
    abi::emit_release_temporary_stack(ctx.emitter, PRINT_TRACE_STATE_BYTES);
    ctx.emitter.label(&done);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    Ok(())
}

/// Calls one activation's frame reader in print mode with its zero-based output index.
fn emit_frame_printer_call(ctx: &mut FunctionContext<'_>, callback_reg: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", TRACE_CURSOR_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", TRACE_OPTIONS_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", TRACE_COUNT_OFFSET);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", TRACE_CURSOR_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", TRACE_OPTIONS_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", TRACE_COUNT_OFFSET);
        }
    }
    abi::emit_call_reg(ctx.emitter, callback_reg);
}

/// Returns class, function, and call-type strings for the current frame.
fn frame_function_parts(ctx: &FunctionContext<'_>) -> (Option<String>, String, &'static str) {
    // A closure carries the label PHP prints for it (`{closure:<context>:<line>}`), recorded when
    // it was lowered.
    if let Some(label) = &ctx.function.display_name {
        // A closure declared in a class keeps that class and the call type PHP prints for it:
        // `Class->{closure:...}()` when `$this` is bound, `Class::{closure:...}()` otherwise.
        let class = ctx
            .function
            .lexical_class
            .as_ref()
            .map(|class| class.trim_start_matches('\\').to_string());
        let call_type = if class.is_none() {
            ""
        } else if ctx.local_slot_by_name("this").is_some() {
            "->"
        } else {
            "::"
        };
        return (class, label.clone(), call_type);
    }
    // A compiled closure body is named after the compiler's internal symbol, but php names the
    // frame `{closure:SCOPE:LINE}` and keeps the enclosing class and call type when the closure
    // was declared inside a method. Reflection already resolves that name for the descriptor, so
    // the trace reuses the same resolver instead of leaking `__eir_closure_*` into a page.
    if let Some(source) = crate::codegen::lower_inst::core_closures_php_source(ctx, ctx.function) {
        let class = ctx
            .function
            .lexical_class
            .as_ref()
            .map(|class| class.trim_start_matches('\\').to_string());
        let call_type = if class.is_some() {
            if ctx.function.flags.is_static { "::" } else { "->" }
        } else {
            ""
        };
        return (class, source.name, call_type);
    }
    match ctx.function.display_name().rsplit_once("::") {
        Some((class, function)) => (
            Some(class.trim_start_matches('\\').to_string()),
            function.to_string(),
            if ctx.function.flags.is_static { "::" } else { "->" },
        ),
        None => (None, public_function_name(ctx), ""),
    }
}

/// Returns the PHP name a frame shows for this free function.
///
/// A function declared in an included file is compiled under an include-variant symbol
/// (`__elephc_include_variant_<hash>_<name>`), which a trace used to print verbatim. The public
/// name comes from the recorded dispatch groups, never from the symbol's spelling; the prefix
/// test only spares every other function the group scan. A conditionally declared function
/// (`if (!function_exists('f')) { function f() {} }`) is compiled under
/// `[Ns\]__elephc_conditional_fn_<hash>_<name>` and resolves the same way.
fn public_function_name(ctx: &FunctionContext<'_>) -> String {
    let name = ctx.function.display_name().trim_start_matches('\\');
    if name.starts_with("__elephc_include_variant_") || name.contains("__elephc_conditional_fn_") {
        if let Some(group) = crate::ir::function_variants::collect_dispatch_groups(ctx.module)
            .into_iter()
            .find(|group| group.variants.iter().any(|variant| variant.trim_start_matches('\\') == name))
        {
            return group.name.trim_start_matches('\\').to_string();
        }
    }
    name.to_string()
}

/// Returns the compact callable spelling used by `debug_print_backtrace()`.
fn display_function_name(ctx: &FunctionContext<'_>) -> String {
    let (class, function, call_type) = frame_function_parts(ctx);
    class.map_or(function.clone(), |class| format!("{class}{call_type}{function}"))
}

/// Allocates an indexed array whose elements are boxed Mixed cells.
fn allocate_mixed_array(ctx: &mut FunctionContext<'_>, capacity: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity.max(1) as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 8);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity.max(1) as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 8);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    emit_array_value_type_stamp(ctx.emitter, &result_reg, &PhpType::Mixed);
}

/// Appends the current boxed cell to the saved array and optionally drops its temporary owner.
fn append_box_to_saved_array(ctx: &mut FunctionContext<'_>, release_temporary: bool) {
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_push_reg(ctx.emitter, &result_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 0);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("str x0, [sp, #16]"),          // update the saved array after append
        Arch::X86_64 => ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax"), // update the saved array after append
    }
    if release_temporary {
        abi::emit_load_temporary_stack_slot(ctx.emitter, &result_reg, 0);
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    }
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Emits an empty indexed Mixed array.
fn emit_empty_trace(ctx: &mut FunctionContext<'_>) -> Result<()> {
    allocate_mixed_array(ctx, 0);
    Ok(())
}

/// Branches to `label` when the current signed integer result is negative.
fn emit_branch_if_negative(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // compare the signed result with zero
            ctx.emitter.instruction(&format!("b.lt {label}"));                  // branch when the result is negative
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // set flags from the signed result
            ctx.emitter.instruction(&format!("js {label}"));                    // branch when the result is negative
        }
    }
}

/// Branches to `label` when the requested register contains a nonzero value.
fn emit_branch_if_nonzero(ctx: &mut FunctionContext<'_>, reg: &str, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("cbnz {reg}, {label}")), // branch when the selected register is set
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {reg}, {reg}"));             // set flags from the selected register
            ctx.emitter.instruction(&format!("jnz {label}"));                   // branch when the selected register is set
        }
    }
}

/// Branches to `label` when the requested register contains zero.
fn emit_branch_if_zero(ctx: &mut FunctionContext<'_>, reg: &str, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("cbz {reg}, {label}")), // branch when the selected register is null
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {reg}, {reg}"));             // set flags from the selected register
            ctx.emitter.instruction(&format!("jz {label}"));                    // branch when the selected register is null
        }
    }
}

/// Branches to `label` when one bit is set in the current integer result.
fn emit_branch_if_bit_set(ctx: &mut FunctionContext<'_>, bit: i64, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("tst x0, #{bit}"));                // test the requested option bit
            ctx.emitter.instruction(&format!("b.ne {label}"));                  // branch when the option bit is set
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test rax, {bit}"));               // test the requested option bit
            ctx.emitter.instruction(&format!("jnz {label}"));                   // branch when the option bit is set
        }
    }
}
