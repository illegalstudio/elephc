//! Purpose:
//! Lowers PCNTL handler registration, lookup, explicit dispatch, and async-dispatch state.
//!
//! Called from:
//! - `super::pcntl::lower()` for the callable-aware PCNTL runtime operations.
//!
//! Key details:
//! - The process-wide table owns one retain for every dynamic callable descriptor.
//! - OS delivery only enqueues stable records; callbacks run through normal runtime safe points.

use crate::codegen::context::FunctionContext;
use crate::codegen::platform::{Arch, Platform};
use crate::codegen::{
    abi, emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed, CodegenIrError,
    Result,
};
use crate::codegen_support::callable_descriptor;
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::callables;
use super::super::predicates;
use super::strings::load_as_int;
use super::{ensure_arg_count_between, expect_operand, store_if_result};

const MIXED_TAG_INT: i64 = 0;
const MIXED_TAG_BOOL: i64 = 3;
const SIGALRM: i64 = 14;

/// Lowers `pcntl_signal()` and transfers one descriptor retain to the handler table on success.
pub(crate) fn lower_signal(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "pcntl_signal", 2, 3)?;
    let signal = expect_operand(inst, 0)?;
    let handler = expect_operand(inst, 1)?;
    emit_initialize_signal_bridge_slots(ctx);
    load_as_int(ctx, signal, "pcntl_signal signal")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_push_handler_kind_and_descriptor(ctx, inst, handler)?;
    emit_signal_restart_flag(ctx, inst)?;

    let failure = ctx.next_label("pcntl_signal_failure");
    let success = ctx.next_label("pcntl_signal_success");
    let done = ctx.next_label("pcntl_signal_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x2");
            ctx.emitter.instruction("ldr x1, [sp, #16]");
            ctx.emitter.instruction("ldr x0, [sp, #32]");
            ctx.emitter.bl_c("elephc_pcntl_signal");
            ctx.emitter.instruction(&format!("cbz x0, {failure}"));
            ctx.emitter.instruction(&format!("b {success}"));
            ctx.emitter.label(&failure);
            ctx.emitter.instruction("ldr x0, [sp]");
            ctx.emitter.instruction(&format!("cbz x0, {done}"));
            callable_descriptor::emit_release_current_descriptor(ctx.emitter);
            ctx.emitter.instruction("mov x0, #0");
            ctx.emitter.instruction(&format!("b {done}"));
            ctx.emitter.label(&success);
            emit_replace_handler_table_entry_aarch64(ctx);
            ctx.emitter.instruction("mov x0, #1");
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rdx");
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 16]");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");
            ctx.emitter.bl_c("elephc_pcntl_signal");
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction(&format!("jz {failure}"));
            ctx.emitter.instruction(&format!("jmp {success}"));
            ctx.emitter.label(&failure);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction(&format!("jz {done}"));
            callable_descriptor::emit_release_current_descriptor(ctx.emitter);
            ctx.emitter.instruction("xor eax, eax");
            ctx.emitter.instruction(&format!("jmp {done}"));
            ctx.emitter.label(&success);
            emit_replace_handler_table_entry_x86_64(ctx);
            ctx.emitter.instruction("mov eax, 1");
        }
    }
    ctx.emitter.label(&done);
    abi::emit_release_temporary_stack(ctx.emitter, 48);
    store_if_result(ctx, inst)
}

/// Installs bridge function pointers into fixed runtime slots without coupling the runtime cache.
fn emit_initialize_signal_bridge_slots(ctx: &mut FunctionContext<'_>) {
    let (signal_symbol, next_symbol) = match ctx.emitter.target.platform {
        Platform::MacOS => ("_elephc_pcntl_signal", "_elephc_pcntl_signal_next"),
        Platform::Linux => ("elephc_pcntl_signal", "elephc_pcntl_signal_next"),
        Platform::Windows => return,
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_extern_symbol_address(ctx.emitter, "x9", signal_symbol);
            abi::emit_symbol_address(ctx.emitter, "x10", "__rt_pcntl_signal_fn");
            ctx.emitter.instruction("str x9, [x10]");
            abi::emit_extern_symbol_address(ctx.emitter, "x9", next_symbol);
            abi::emit_symbol_address(ctx.emitter, "x10", "__rt_pcntl_signal_next_fn");
            ctx.emitter.instruction("str x9, [x10]");
        }
        Arch::X86_64 => {
            abi::emit_extern_symbol_address(ctx.emitter, "r9", signal_symbol);
            abi::emit_symbol_address(ctx.emitter, "r10", "__rt_pcntl_signal_fn");
            ctx.emitter.instruction("mov QWORD PTR [r10], r9");
            abi::emit_extern_symbol_address(ctx.emitter, "r9", next_symbol);
            abi::emit_symbol_address(ctx.emitter, "r10", "__rt_pcntl_signal_next_fn");
            ctx.emitter.instruction("mov QWORD PTR [r10], r9");
        }
    }
}

/// Lowers `pcntl_signal_get_handler()` to an owned boxed callable or integer disposition.
pub(crate) fn lower_signal_get_handler(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "pcntl_signal_get_handler", 1, 1)?;
    let signal = expect_operand(inst, 0)?;
    load_as_int(ctx, signal, "pcntl_signal_get_handler signal")?;
    let callable = ctx.next_label("pcntl_signal_get_handler_callable");
    let invalid = ctx.next_label("pcntl_signal_get_handler_invalid");
    let done = ctx.next_label("pcntl_signal_get_handler_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.bl_c("elephc_pcntl_signal_limit");
            abi::emit_pop_reg(ctx.emitter, "x9");
            ctx.emitter.instruction("cmp x9, #1");
            ctx.emitter.instruction(&format!("b.lt {invalid}"));
            ctx.emitter.instruction("cmp x9, x0");
            ctx.emitter.instruction(&format!("b.ge {invalid}"));
            abi::emit_symbol_address(ctx.emitter, "x10", "__rt_pcntl_handler_kind");
            ctx.emitter.instruction("ldr x0, [x10, x9, lsl #3]");
            ctx.emitter.instruction("cmp x0, #2");
            ctx.emitter.instruction(&format!("b.eq {callable}"));
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
            ctx.emitter.instruction(&format!("b {done}"));
            ctx.emitter.label(&callable);
            abi::emit_symbol_address(ctx.emitter, "x10", "__rt_pcntl_handler_descriptor");
            ctx.emitter.instruction("ldr x0, [x10, x9, lsl #3]");
            callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
            emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Callable);
            ctx.emitter.instruction(&format!("b {done}"));
            ctx.emitter.label(&invalid);
            ctx.emitter.instruction("mov x0, #0");
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.bl_c("elephc_pcntl_signal_limit");
            abi::emit_pop_reg(ctx.emitter, "r9");
            ctx.emitter.instruction("cmp r9, 1");
            ctx.emitter.instruction(&format!("jl {invalid}"));
            ctx.emitter.instruction("cmp r9, rax");
            ctx.emitter.instruction(&format!("jge {invalid}"));
            abi::emit_symbol_address(ctx.emitter, "r10", "__rt_pcntl_handler_kind");
            ctx.emitter.instruction("mov rax, QWORD PTR [r10 + r9*8]");
            ctx.emitter.instruction("cmp rax, 2");
            ctx.emitter.instruction(&format!("je {callable}"));
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
            ctx.emitter.instruction(&format!("jmp {done}"));
            ctx.emitter.label(&callable);
            abi::emit_symbol_address(ctx.emitter, "r10", "__rt_pcntl_handler_descriptor");
            ctx.emitter.instruction("mov rax, QWORD PTR [r10 + r9*8]");
            callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
            emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Callable);
            ctx.emitter.instruction(&format!("jmp {done}"));
            ctx.emitter.label(&invalid);
            ctx.emitter.instruction("xor eax, eax");
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
        }
    }
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Lowers explicit pending-signal dispatch to the target-neutral runtime drain.
pub(crate) fn lower_signal_dispatch(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "pcntl_signal_dispatch", 0, 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_pcntl_dispatch_pending");
    store_if_result(ctx, inst)
}

/// Lowers querying or changing the process-wide asynchronous-dispatch flag.
pub(crate) fn lower_async_signals(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "pcntl_async_signals", 0, 1)?;
    let Some(enable) = inst.operands.first().copied() else {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_symbol_address(ctx.emitter, "x9", "__rt_pcntl_async_enabled");
                ctx.emitter.instruction("ldr x0, [x9]");
            }
            Arch::X86_64 => {
                abi::emit_symbol_address(ctx.emitter, "r9", "__rt_pcntl_async_enabled");
                ctx.emitter.instruction("mov rax, QWORD PTR [r9]");
            }
        }
        return store_if_result(ctx, inst);
    };

    let query = ctx.next_label("pcntl_async_signals_query");
    let done = ctx.next_label("pcntl_async_signals_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "__rt_pcntl_async_enabled");
            ctx.emitter.instruction("ldr x10, [x9]");
            abi::emit_push_reg(ctx.emitter, "x10");
            predicates::emit_is_null_result(ctx, enable)?;
            ctx.emitter.instruction(&format!("cbnz x0, {query}"));
            load_as_int(ctx, enable, "pcntl_async_signals enable")?;
            ctx.emitter.instruction("cmp x0, #0");
            ctx.emitter.instruction("cset x0, ne");
            abi::emit_symbol_address(ctx.emitter, "x9", "__rt_pcntl_async_enabled");
            ctx.emitter.instruction("str x0, [x9]");
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.instruction(&format!("b {done}"));
            ctx.emitter.label(&query);
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "__rt_pcntl_async_enabled");
            ctx.emitter.instruction("mov r10, QWORD PTR [r9]");
            abi::emit_push_reg(ctx.emitter, "r10");
            predicates::emit_is_null_result(ctx, enable)?;
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction(&format!("jnz {query}"));
            load_as_int(ctx, enable, "pcntl_async_signals enable")?;
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction("setne al");
            ctx.emitter.instruction("movzx eax, al");
            abi::emit_symbol_address(ctx.emitter, "r9", "__rt_pcntl_async_enabled");
            ctx.emitter.instruction("mov QWORD PTR [r9], rax");
            abi::emit_pop_reg(ctx.emitter, "rax");
            ctx.emitter.instruction(&format!("jmp {done}"));
            ctx.emitter.label(&query);
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Materializes a handler and pushes its internal kind followed by its descriptor pointer.
fn emit_push_handler_kind_and_descriptor(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    handler: crate::ir::ValueId,
) -> Result<()> {
    match ctx.value_php_type(handler)?.codegen_repr() {
        PhpType::Int | PhpType::Bool | PhpType::False => {
            load_as_int(ctx, handler, "pcntl_signal handler disposition")?;
            emit_normalize_integer_disposition(ctx);
            emit_push_integer_handler_pair(ctx);
        }
        PhpType::Callable => {
            ctx.load_value_to_result(handler)?;
            callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
            emit_push_callable_handler_pair(ctx);
        }
        PhpType::Str => {
            callables::emit_runtime_string_descriptor_value(
                ctx,
                handler,
                abi::int_result_reg(ctx.emitter),
                "pcntl_signal",
                super::super::instruction_strict_php_profile(inst),
            )?;
            emit_push_callable_handler_pair(ctx);
        }
        PhpType::Array(_) => {
            callables::emit_runtime_callable_array_descriptor_value(ctx, handler, "pcntl_signal")?;
            emit_push_callable_handler_pair(ctx);
        }
        PhpType::Object(class_name) => {
            callables::emit_invokable_object_descriptor_value(
                ctx,
                handler,
                &class_name,
                "pcntl_signal",
            )?;
            emit_push_callable_handler_pair(ctx);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_push_mixed_handler_pair(ctx, handler)?;
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "pcntl_signal handler for PHP type {other:?}"
            )))
        }
    }
    Ok(())
}

/// Classifies a boxed handler as an integer disposition or a runtime callable descriptor.
fn emit_push_mixed_handler_pair(
    ctx: &mut FunctionContext<'_>,
    handler: crate::ir::ValueId,
) -> Result<()> {
    let scalar = ctx.next_label("pcntl_signal_mixed_scalar");
    let done = ctx.next_label("pcntl_signal_mixed_handler_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(handler, "x0")?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction(&format!("cmp x0, #{MIXED_TAG_INT}"));
            ctx.emitter.instruction(&format!("b.eq {scalar}"));
            ctx.emitter.instruction(&format!("cmp x0, #{MIXED_TAG_BOOL}"));
            ctx.emitter.instruction(&format!("b.eq {scalar}"));
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(handler, "rax")?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction(&format!("cmp rax, {MIXED_TAG_INT}"));
            ctx.emitter.instruction(&format!("je {scalar}"));
            ctx.emitter.instruction(&format!("cmp rax, {MIXED_TAG_BOOL}"));
            ctx.emitter.instruction(&format!("je {scalar}"));
        }
    }
    callables::emit_runtime_mixed_callable_descriptor_value(
        ctx,
        handler,
        "pcntl_signal",
        true,
    )?;
    emit_push_callable_handler_pair(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&scalar);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("mov x0, x1"),
        Arch::X86_64 => ctx.emitter.instruction("mov rax, rdi"),
    }
    emit_normalize_integer_disposition(ctx);
    emit_push_integer_handler_pair(ctx);
    ctx.emitter.label(&done);
    Ok(())
}

/// Maps a runtime integer to disposition zero/one, or the bridge-invalid sentinel three.
fn emit_normalize_integer_disposition(ctx: &mut FunctionContext<'_>) {
    let valid = ctx.next_label("pcntl_signal_integer_handler_valid");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");
            ctx.emitter.instruction(&format!("b.ls {valid}"));
            ctx.emitter.instruction("mov x0, #3");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");
            ctx.emitter.instruction(&format!("jbe {valid}"));
            ctx.emitter.instruction("mov rax, 3");
        }
    }
    ctx.emitter.label(&valid);
}

/// Pushes an integer disposition plus a null descriptor pointer.
fn emit_push_integer_handler_pair(ctx: &mut FunctionContext<'_>) {
    let result = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_push_reg(ctx.emitter, &result);
    abi::emit_load_int_immediate(ctx.emitter, &result, 0);
    abi::emit_push_reg(ctx.emitter, &result);
}

/// Pushes callable kind two plus the owned descriptor currently in the result register.
fn emit_push_callable_handler_pair(ctx: &mut FunctionContext<'_>) {
    let result = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_push_reg(ctx.emitter, &result);
    abi::emit_load_int_immediate(ctx.emitter, &result, 2);
    abi::emit_push_reg(ctx.emitter, &result);
    abi::emit_pop_reg(ctx.emitter, &result);
    let scratch = abi::secondary_scratch_reg(ctx.emitter).to_string();
    abi::emit_pop_reg(ctx.emitter, &scratch);
    abi::emit_push_reg(ctx.emitter, &result);
    abi::emit_push_reg(ctx.emitter, &scratch);
}

/// Pushes the explicit or SIGALRM-aware default restart-syscalls flag.
fn emit_signal_restart_flag(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if let Some(restart) = inst.operands.get(2).copied() {
        load_as_int(ctx, restart, "pcntl_signal restart_syscalls")?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("cmp x0, #0");
                ctx.emitter.instruction("cset x0, ne");
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("test rax, rax");
                ctx.emitter.instruction("setne al");
                ctx.emitter.instruction("movzx eax, al");
            }
        }
    } else {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("ldr x0, [sp, #32]");
                ctx.emitter.instruction(&format!("cmp x0, #{SIGALRM}"));
                ctx.emitter.instruction("cset x0, ne");
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("cmp QWORD PTR [rsp + 32], 14");
                ctx.emitter.instruction("setne al");
                ctx.emitter.instruction("movzx eax, al");
            }
        }
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    Ok(())
}

/// Replaces one AArch64 handler-table entry and releases its prior descriptor ownership.
fn emit_replace_handler_table_entry_aarch64(ctx: &mut FunctionContext<'_>) {
    ctx.emitter.instruction("ldr x9, [sp, #32]");
    abi::emit_symbol_address(ctx.emitter, "x10", "__rt_pcntl_handler_kind");
    ctx.emitter.instruction("ldr x11, [x10, x9, lsl #3]");
    ctx.emitter.instruction("cmp x11, #2");
    let skip_release = ctx.next_label("pcntl_signal_no_old_descriptor");
    ctx.emitter.instruction(&format!("b.ne {skip_release}"));
    abi::emit_symbol_address(ctx.emitter, "x11", "__rt_pcntl_handler_descriptor");
    ctx.emitter.instruction("ldr x0, [x11, x9, lsl #3]");
    callable_descriptor::emit_release_current_descriptor(ctx.emitter);
    ctx.emitter.label(&skip_release);
    ctx.emitter.instruction("ldr x9, [sp, #32]");
    ctx.emitter.instruction("ldr x11, [sp, #16]");
    abi::emit_symbol_address(ctx.emitter, "x10", "__rt_pcntl_handler_kind");
    ctx.emitter.instruction("str x11, [x10, x9, lsl #3]");
    ctx.emitter.instruction("ldr x11, [sp]");
    abi::emit_symbol_address(ctx.emitter, "x10", "__rt_pcntl_handler_descriptor");
    ctx.emitter.instruction("str x11, [x10, x9, lsl #3]");
}

/// Replaces one x86_64 handler-table entry and releases its prior descriptor ownership.
fn emit_replace_handler_table_entry_x86_64(ctx: &mut FunctionContext<'_>) {
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 32]");
    abi::emit_symbol_address(ctx.emitter, "r10", "__rt_pcntl_handler_kind");
    ctx.emitter.instruction("cmp QWORD PTR [r10 + r9*8], 2");
    let skip_release = ctx.next_label("pcntl_signal_no_old_descriptor");
    ctx.emitter.instruction(&format!("jne {skip_release}"));
    abi::emit_symbol_address(ctx.emitter, "r10", "__rt_pcntl_handler_descriptor");
    ctx.emitter.instruction("mov rax, QWORD PTR [r10 + r9*8]");
    callable_descriptor::emit_release_current_descriptor(ctx.emitter);
    ctx.emitter.label(&skip_release);
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 32]");
    ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 16]");
    abi::emit_symbol_address(ctx.emitter, "r10", "__rt_pcntl_handler_kind");
    ctx.emitter.instruction("mov QWORD PTR [r10 + r9*8], r11");
    ctx.emitter.instruction("mov r11, QWORD PTR [rsp]");
    abi::emit_symbol_address(ctx.emitter, "r10", "__rt_pcntl_handler_descriptor");
    ctx.emitter.instruction("mov QWORD PTR [r10 + r9*8], r11");
}
