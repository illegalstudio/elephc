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
    abi, emit_array_value_type_stamp, emit_box_current_owned_value_as_mixed,
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
const CALLBACK_PRINT_INDEX_OFFSET: usize = 24;
const CALLBACK_PRINT_ARG_COUNT_OFFSET: usize = 32;

/// Publishes the source line and a call-site-specific frame reader before a nested PHP call.
pub(super) fn prepare_call_site(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if !ctx.backtrace_enabled || !instruction_may_enter_php_frame(inst) {
        return Ok(());
    }
    let line = i64::from(inst.span.map_or(0, |span| span.line));
    let scratch = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, scratch, line);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_php_backtrace_next_line", 0);

    if !ctx.backtrace_activation {
        return Ok(());
    }
    let activation_offset = ctx
        .exception_activation_offset
        .expect("backtrace activation requires an activation frame slot");
    abi::store_at_offset(ctx.emitter, scratch, activation_offset - 32);
    let callback = ctx.next_label("backtrace_frame_reader");
    abi::emit_symbol_address(ctx.emitter, scratch, &callback);
    abi::store_at_offset(ctx.emitter, scratch, activation_offset - 24);

    let resume = ctx.next_label("backtrace_call_site");
    abi::emit_jump(ctx.emitter, &resume);
    ctx.emitter.label(&callback);
    emit_frame_reader_callback(ctx)?;
    ctx.emitter.label(&resume);
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

/// Emits one call-site-specific callback that reads the suspended PHP frame's live locals.
fn emit_frame_reader_callback(ctx: &mut FunctionContext<'_>) -> Result<()> {
    emit_frame_reader_prologue(ctx);
    capture_callback_argument_count(ctx)?;
    let materialize = ctx.next_label("backtrace_frame_materialize");
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        CALLBACK_PRINT_INDEX_OFFSET,
    );
    emit_branch_if_negative(ctx, &materialize);
    emit_printed_frame(ctx)?;
    emit_frame_reader_epilogue(ctx);
    ctx.emitter.label(&materialize);
    allocate_mixed_hash(ctx, 8);
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let file = ctx.module.source_path.clone().unwrap_or_else(|| "Unknown".to_string());
    insert_boxed_string(ctx, "file", &file);
    insert_callback_line(ctx);
    let (class, function, call_type) = frame_function_parts(ctx);
    insert_boxed_string(ctx, "function", &function);
    if let Some(class) = class {
        insert_boxed_string(ctx, "class", &class);
        insert_boxed_string(ctx, "type", call_type);
    }
    insert_callback_object(ctx)?;
    insert_callback_arguments(ctx)?;
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_box_current_owned_value_as_mixed(
        ctx.emitter,
        &PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Mixed),
        },
    );
    emit_frame_reader_epilogue(ctx);
    Ok(())
}

/// Saves callback state and selects the suspended PHP frame pointer for local reads.
fn emit_frame_reader_prologue(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #64");                         // reserve callback state and saved frame linkage
            ctx.emitter.instruction("stp x29, x30, [sp, #48]");                 // preserve the callback caller frame
            ctx.emitter.instruction("ldr x9, [x0, #32]");                       // load the source frame's caller line
            ctx.emitter.instruction("str x9, [sp]");                            // save the caller line for frame materialization
            ctx.emitter.instruction("str x1, [sp, #8]");                        // save the original debug options mask
            ctx.emitter.instruction("str x2, [sp, #24]");                       // save the materialize or print mode selector
            ctx.emitter.instruction("ldr x29, [x0, #16]");                      // switch local reads to the suspended PHP frame
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("push rbp");                                // preserve the callback caller frame pointer
            ctx.emitter.instruction("sub rsp, 48");                             // reserve callback state with SysV alignment
            ctx.emitter.instruction("mov rax, QWORD PTR [rdi + 32]");           // load the source frame's caller line
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // save the caller line for frame materialization
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
        match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.emitter.instruction("ldr x0, [x0, #24]"),      // load the hidden caller argument count
            Arch::X86_64 => ctx.emitter.instruction("mov rax, QWORD PTR [rax + 24]"), // load the hidden caller argument count
        }
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
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

/// Inserts the dynamic call-site line saved in the activation record.
fn insert_callback_line(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        CALLBACK_LINE_OFFSET + 16,
    );
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
    insert_current_boxed_hash_value(ctx, "line");
}

/// Inserts `$this` only when PHP's provide-object option is enabled for an instance frame.
fn insert_callback_object(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let Some(slot) = ctx.local_slot_by_name("this") else {
        return Ok(());
    };
    let provide = ctx.next_label("debug_backtrace_provide_object");
    let skip = ctx.next_label("debug_backtrace_skip_object");
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        CALLBACK_OPTIONS_OFFSET + 16,
    );
    emit_branch_if_bit_set(ctx, DEBUG_BACKTRACE_PROVIDE_OBJECT, &provide);
    abi::emit_jump(ctx.emitter, &skip);
    ctx.emitter.label(&provide);
    ctx.load_local_to_result(slot)?;
    let ty = ctx.local_php_type(slot)?;
    emit_box_current_value_as_mixed(ctx.emitter, &ty);
    insert_current_boxed_hash_value(ctx, "object");
    ctx.emitter.label(&skip);
    Ok(())
}

/// Inserts a live copy of the source-visible parameters unless arguments were suppressed.
fn insert_callback_arguments(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let skip = ctx.next_label("debug_backtrace_skip_args");
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        CALLBACK_OPTIONS_OFFSET + 16,
    );
    emit_branch_if_bit_set(ctx, DEBUG_BACKTRACE_IGNORE_ARGS, &skip);
    emit_argument_array_from_live_locals(ctx)?;
    emit_box_current_owned_value_as_mixed(
        ctx.emitter,
        &PhpType::Array(Box::new(PhpType::Mixed)),
    );
    insert_current_boxed_hash_value(ctx, "args");
    ctx.emitter.label(&skip);
    Ok(())
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
        if !borrowed_box {
            emit_box_current_value_as_mixed(ctx.emitter, &ty.codegen_repr());
        }
        append_box_to_saved_array(ctx, !borrowed_box);
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
        append_argument_array_range(ctx, usize::from(starts_with_count), &element_ty);
    }
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    trim_argument_array_to_actual_count(ctx);
    Ok(())
}

/// Appends an indexed-array range into the boxed-Mixed destination saved on the stack.
fn append_argument_array_range(
    ctx: &mut FunctionContext<'_>,
    start: usize,
    element_ty: &PhpType,
) {
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
    emit_append_argument_tail_value(ctx, element_ty);
    increment_argument_tail_index(ctx);
    abi::emit_jump(ctx.emitter, &loop_label);
    ctx.emitter.label(&done);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    let discard = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, discard);
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

/// Boxes and appends the current indexed hidden argument according to its slot type.
fn emit_append_argument_tail_value(ctx: &mut FunctionContext<'_>, element_ty: &PhpType) {
    let repr = element_ty.codegen_repr();
    if matches!(repr, PhpType::Mixed | PhpType::Union(_)) {
        emit_append_borrowed_argument_tail_box(ctx);
        return;
    }

    emit_load_argument_tail_value(ctx, &repr);
    emit_box_current_value_as_mixed(ctx.emitter, &repr);
    append_argument_tail_box(ctx, true);
}

/// Appends one borrowed boxed-Mixed hidden argument without rebuilding its cell.
fn emit_append_borrowed_argument_tail_box(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", 32);
            ctx.emitter.instruction("add x10, x10, #24");                       // advance from array metadata to Mixed slots
            ctx.emitter.instruction("ldr x1, [x10, x9, lsl #3]");               // load the borrowed boxed argument cell
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 48);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", 32);
            ctx.emitter.instruction("mov rsi, QWORD PTR [r11 + r10 * 8 + 24]"); // load the borrowed boxed argument cell
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 48);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), 48);
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

/// Drops defaulted parameters beyond the actual caller-supplied argument count.
fn trim_argument_array_to_actual_count(ctx: &mut FunctionContext<'_>) {
    let no_trim = ctx.next_label("debug_backtrace_args_not_trimmed");
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::secondary_scratch_reg(ctx.emitter),
        CALLBACK_ARG_COUNT_OFFSET + 16,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x10, #0");                             // test whether caller-count metadata is available
            ctx.emitter.instruction(&format!("b.lt {no_trim}"));                // keep every argument when the count is unknown
            ctx.emitter.instruction("mov x2, x10");                             // pass the caller-visible slice length
            ctx.emitter.instruction("mov x1, #0");                              // slice from the first argument
            ctx.emitter.instruction("mov x3, #1");                              // retain refcounted Mixed elements in the slice
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp r10, 0");                              // test whether caller-count metadata is available
            ctx.emitter.instruction(&format!("jl {no_trim}"));                  // keep every argument when the count is unknown
            ctx.emitter.instruction("mov rdx, r10");                            // pass the caller-visible slice length
            ctx.emitter.instruction("mov rdi, rax");                            // pass the original argument array
            ctx.emitter.instruction("xor esi, esi");                            // slice from the first argument
            ctx.emitter.instruction("mov ecx, 1");                              // retain refcounted Mixed elements in the slice
        }
    }
    let original = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_push_reg(ctx.emitter, &original);
    if ctx.emitter.target.arch == Arch::AArch64 {
        abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 0);
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_slice_refcounted");
    let sliced = abi::int_result_reg(ctx.emitter).to_string();
    emit_array_value_type_stamp(ctx.emitter, &sliced, &PhpType::Mixed);
    abi::emit_push_reg(ctx.emitter, &sliced);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &original, 16);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    abi::emit_pop_reg(ctx.emitter, &sliced);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    ctx.emitter.label(&no_trim);
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

/// Prints one suspended frame's metadata and live arguments from its reader callback.
fn emit_printed_frame(ctx: &mut FunctionContext<'_>) -> Result<()> {
    write_static_trace_bytes(ctx, "#");
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        CALLBACK_PRINT_INDEX_OFFSET,
    );
    abi::emit_write_stdout(ctx.emitter, &PhpType::Int);
    let file = ctx.module.source_path.as_deref().unwrap_or("Unknown");
    write_static_trace_bytes(ctx, &format!(" {file}("));
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        CALLBACK_LINE_OFFSET,
    );
    abi::emit_write_stdout(ctx.emitter, &PhpType::Int);
    write_static_trace_bytes(ctx, &format!("): {}(", display_function_name(ctx)));
    emit_printed_live_arguments(ctx)?;
    write_static_trace_bytes(ctx, ")\n");
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    Ok(())
}

/// Writes one compile-time byte string through the capture-aware stdout funnel.
fn write_static_trace_bytes(ctx: &mut FunctionContext<'_>, bytes: &str) {
    let (label, len) = ctx.data.add_string(bytes.as_bytes());
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    abi::emit_write_stdout(ctx.emitter, &PhpType::Str);
}

/// Prints current source-visible parameters and collected surplus arguments.
fn emit_printed_live_arguments(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let skip = ctx.next_label("debug_print_backtrace_skip_args");
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        CALLBACK_OPTIONS_OFFSET,
    );
    emit_branch_if_bit_set(ctx, DEBUG_BACKTRACE_IGNORE_ARGS, &skip);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        CALLBACK_PRINT_ARG_COUNT_OFFSET,
    );

    let done = ctx.next_label("debug_print_backtrace_args_done");
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
    for (name, _) in parameters {
        emit_printed_actual_count_guard(ctx, 0, &done);
        emit_printed_argument_separator(ctx, 0);
        let Some(slot) = ctx.local_slot_by_name(&name) else {
            continue;
        };
        let ty = ctx.load_local_to_result(slot)?;
        print_current_argument_value(ctx, &ty);
        increment_printed_argument_count(ctx, 0);
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
        emit_printed_argument_array_range(
            ctx,
            usize::from(starts_with_count),
            &element_ty,
            &done,
        );
    }
    ctx.emitter.label(&done);
    ctx.emitter.label(&skip);
    Ok(())
}

/// Stops argument printing after the number of values actually supplied by the caller.
fn emit_printed_actual_count_guard(
    ctx: &mut FunctionContext<'_>,
    stack_shift: usize,
    done: &str,
) {
    let unlimited = ctx.next_label("debug_print_backtrace_all_args");
    let actual_reg = abi::int_result_reg(ctx.emitter);
    let printed_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        actual_reg,
        CALLBACK_ARG_COUNT_OFFSET + stack_shift,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // test whether caller-count metadata is available
            ctx.emitter.instruction(&format!("b.lt {unlimited}"));              // print all arguments when the count is unknown
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                printed_reg,
                CALLBACK_PRINT_ARG_COUNT_OFFSET + stack_shift,
            );
            ctx.emitter.instruction("cmp x10, x0");                             // compare printed arguments with the actual count
            ctx.emitter.instruction(&format!("b.ge {done}"));                   // stop before defaulted parameters
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether caller-count metadata is available
            ctx.emitter.instruction(&format!("js {unlimited}"));                // print all arguments when the count is unknown
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                printed_reg,
                CALLBACK_PRINT_ARG_COUNT_OFFSET + stack_shift,
            );
            ctx.emitter.instruction("cmp r10, rax");                            // compare printed arguments with the actual count
            ctx.emitter.instruction(&format!("jge {done}"));                    // stop before defaulted parameters
        }
    }
    ctx.emitter.label(&unlimited);
}

/// Writes the comma separator before every argument except the first.
fn emit_printed_argument_separator(ctx: &mut FunctionContext<'_>, stack_shift: usize) {
    let first = ctx.next_label("debug_print_backtrace_first_arg");
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        CALLBACK_PRINT_ARG_COUNT_OFFSET + stack_shift,
    );
    emit_branch_if_zero(ctx, abi::int_result_reg(ctx.emitter), &first);
    write_static_trace_bytes(ctx, ", ");
    ctx.emitter.label(&first);
}

/// Boxes and prints the current typed value while preserving borrowed ownership.
fn print_current_argument_value(ctx: &mut FunctionContext<'_>, ty: &PhpType) {
    let repr = ty.codegen_repr();
    let borrowed_box = matches!(repr, PhpType::Mixed | PhpType::Union(_));
    if !borrowed_box {
        emit_box_current_value_as_mixed(ctx.emitter, &repr);
    }
    let box_reg = abi::int_result_reg(ctx.emitter).to_string();
    if !borrowed_box {
        abi::emit_push_reg(ctx.emitter, &box_reg);
    }
    abi::emit_call_label(ctx.emitter, "__rt_backtrace_print_arg");
    if !borrowed_box {
        abi::emit_load_temporary_stack_slot(ctx.emitter, &box_reg, 0);
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
        abi::emit_release_temporary_stack(ctx.emitter, 16);
    }
}

/// Increments the number of arguments already rendered for the active frame.
fn increment_printed_argument_count(ctx: &mut FunctionContext<'_>, stack_shift: usize) {
    let count_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        count_reg,
        CALLBACK_PRINT_ARG_COUNT_OFFSET + stack_shift,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("add x0, x0, #1"),             // count one rendered argument
        Arch::X86_64 => ctx.emitter.instruction("add rax, 1"),                  // count one rendered argument
    }
    abi::emit_store_to_sp(
        ctx.emitter,
        count_reg,
        CALLBACK_PRINT_ARG_COUNT_OFFSET + stack_shift,
    );
}

/// Prints a typed range from the hidden array that stores variadic and surplus arguments.
fn emit_printed_argument_array_range(
    ctx: &mut FunctionContext<'_>,
    start: usize,
    element_ty: &PhpType,
    all_done: &str,
) {
    let source_reg = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_push_reg(ctx.emitter, &source_reg);
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    abi::emit_load_int_immediate(ctx.emitter, &source_reg, start as i64);
    abi::emit_store_to_sp(ctx.emitter, &source_reg, 0);
    let length_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, length_reg, 32);
    abi::emit_load_from_address(ctx.emitter, length_reg, length_reg, 0);
    abi::emit_store_to_sp(ctx.emitter, length_reg, 8);

    let loop_label = ctx.next_label("debug_print_backtrace_arg_tail");
    let range_done = ctx.next_label("debug_print_backtrace_arg_tail_done");
    ctx.emitter.label(&loop_label);
    emit_argument_tail_done_guard(ctx, &range_done);
    emit_printed_actual_count_guard(ctx, 48, &range_done);
    emit_printed_argument_separator(ctx, 48);
    let repr = element_ty.codegen_repr();
    if matches!(repr, PhpType::Mixed | PhpType::Union(_)) {
        load_borrowed_argument_tail_box(ctx);
        abi::emit_call_label(ctx.emitter, "__rt_backtrace_print_arg");
    } else {
        emit_load_argument_tail_value(ctx, &repr);
        print_current_argument_value(ctx, &repr);
    }
    increment_printed_argument_count(ctx, 48);
    increment_argument_tail_index(ctx);
    abi::emit_jump(ctx.emitter, &loop_label);
    ctx.emitter.label(&range_done);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    let discard = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, discard);
    emit_printed_actual_count_guard(ctx, 0, all_done);
}

/// Loads one borrowed boxed-Mixed value from the current hidden-argument array slot.
fn load_borrowed_argument_tail_box(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", 32);
            ctx.emitter.instruction("add x10, x10, #24");                       // advance from array metadata to Mixed slots
            ctx.emitter.instruction("ldr x0, [x10, x9, lsl #3]");               // load the borrowed boxed argument for printing
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", 32);
            ctx.emitter.instruction("mov rax, QWORD PTR [r11 + r10 * 8 + 24]"); // load the borrowed boxed argument for printing
        }
    }
}

/// Returns class, function, and call-type strings for the current frame.
fn frame_function_parts(ctx: &FunctionContext<'_>) -> (Option<String>, String, &'static str) {
    match ctx.function.name.rsplit_once("::") {
        Some((class, function)) => (
            Some(class.trim_start_matches('\\').to_string()),
            function.to_string(),
            if ctx.function.flags.is_static { "::" } else { "->" },
        ),
        None => (
            None,
            ctx.function.name.trim_start_matches('\\').to_string(),
            "",
        ),
    }
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

/// Allocates an empty string-keyed hash holding boxed Mixed cells.
fn allocate_mixed_hash(ctx: &mut FunctionContext<'_>, capacity: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity.max(1) as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 7);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity.max(1) as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 7);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_new");
}

/// Inserts one static string as a boxed value into the saved frame hash.
fn insert_boxed_string(ctx: &mut FunctionContext<'_>, key: &str, value: &str) {
    let (label, len) = ctx.data.add_string(value.as_bytes());
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
    insert_current_boxed_hash_value(ctx, key);
}

/// Transfers the current boxed value into the hash pointer saved at the stack top.
fn insert_current_boxed_hash_value(ctx: &mut FunctionContext<'_>, key: &str) {
    let (key_label, key_len) = ctx.data.add_string(key.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x3, x0");                              // pass the boxed hash value
            ctx.emitter.instruction("mov x4, xzr");                             // string keys have no high payload word
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x1", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x5", 7);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rcx, rax");                            // pass the boxed hash value
            ctx.emitter.instruction("xor r8, r8");                              // string keys have no high payload word
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_symbol_address(ctx.emitter, "rsi", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            abi::emit_load_int_immediate(ctx.emitter, "r9", 7);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_set");
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
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
