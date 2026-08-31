//! Purpose:
//! Lowers PCNTL handler registration, lookup, explicit dispatch, and async-dispatch state.
//!
//! Called from:
//! - `super::pcntl::lower()` for the callable-aware PCNTL runtime operations.
//!
//! Key details:
//! - The process-wide table owns both an invocation descriptor and the original boxed PHP value.
//! - OS delivery only enqueues stable records; callbacks run through normal runtime safe points.

use crate::codegen::context::FunctionContext;
use crate::codegen::platform::{Arch, Platform};
use crate::codegen::{abi, emit_box_current_value_as_mixed, CodegenIrError, Result};
use crate::codegen_support::runtime::UNCAUGHT_EXIT_STATUS;
use crate::codegen_support::{
    callable_descriptor, emit_write_current_string_stderr, emit_write_literal_stderr,
};
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
    emit_validate_signal_number(ctx);
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_push_handler_kind_and_descriptor(ctx, inst, handler)?;
    emit_push_handler_value(ctx, handler)?;
    emit_signal_restart_flag(ctx, inst)?;

    let failure = ctx.next_label("pcntl_signal_failure");
    let success = ctx.next_label("pcntl_signal_success");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x2");
            ctx.emitter.instruction("ldr x1, [sp, #32]");                       // pass the staged handler disposition
            ctx.emitter.instruction("ldr x0, [sp, #48]");                       // pass the staged signal number
            ctx.emitter.instruction("mov x3, #1");                              // route queued records to the generated AOT handler table
            ctx.emitter.bl_c("elephc_pcntl_signal");
            ctx.emitter.instruction(&format!("cbz x0, {failure}"));             // terminate when the OS refuses the signal disposition
            ctx.emitter.instruction(&format!("b {success}"));                   // commit the registered handler
            ctx.emitter.label(&failure);
            emit_signal_installation_fatal(ctx, signal)?;
            ctx.emitter.label(&success);
            emit_replace_handler_table_entry_aarch64(ctx);
            ctx.emitter.instruction("mov x0, #1");                              // return true after registration
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rdx");
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 32]");           // pass the staged handler disposition
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");           // pass the staged signal number
            ctx.emitter.instruction("mov ecx, 1");                              // route queued records to the generated AOT handler table
            ctx.emitter.bl_c("elephc_pcntl_signal");
            ctx.emitter.instruction("test rax, rax");                           // inspect bridge registration success
            ctx.emitter.instruction(&format!("jz {failure}"));                  // terminate when the OS refuses the signal disposition
            ctx.emitter.instruction(&format!("jmp {success}"));                 // commit the registered handler
            ctx.emitter.label(&failure);
            emit_signal_installation_fatal(ctx, signal)?;
            ctx.emitter.label(&success);
            emit_replace_handler_table_entry_x86_64(ctx);
            ctx.emitter.instruction("mov eax, 1");                              // return true after registration
        }
    }
    abi::emit_release_temporary_stack(ctx.emitter, 64);
    store_if_result(ctx, inst)
}

/// Writes PHP's unsuppressible signal-installation fatal and exits with status 255.
fn emit_signal_installation_fatal(
    ctx: &mut FunctionContext<'_>,
    signal: crate::ir::ValueId,
) -> Result<()> {
    let (prefix, prefix_len) = ctx
        .data
        .add_string(b"Fatal error: Error installing signal handler for ");
    let (newline, newline_len) = ctx.data.add_string(b"\n");
    emit_write_literal_stderr(ctx.emitter, &prefix, prefix_len);
    ctx.load_value_to_result(signal)?;
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    emit_write_current_string_stderr(ctx.emitter);
    emit_write_literal_stderr(ctx.emitter, &newline, newline_len);
    abi::emit_exit(ctx.emitter, UNCAUGHT_EXIT_STATUS);
    Ok(())
}

/// Validates a dynamic signal number with PHP's target-specific `ValueError` diagnostics.
fn emit_validate_signal_number(ctx: &mut FunctionContext<'_>) {
    let below_range = ctx.next_label("pcntl_signal_below_range");
    let above_range = ctx.next_label("pcntl_signal_above_range");
    let valid = ctx.next_label("pcntl_signal_number_valid");
    let upper_bound = match ctx.emitter.target.platform {
        Platform::MacOS => 32,
        Platform::Linux => 65,
        Platform::Windows => 1,
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // reject signal numbers below PHP's valid range
            ctx.emitter.instruction(&format!("b.lt {below_range}"));            // report PHP's lower-bound ValueError
            ctx.emitter.instruction(&format!("cmp x0, #{upper_bound}"));        // reject the target's one-past-last signal number
            ctx.emitter.instruction(&format!("b.ge {above_range}"));            // report PHP's upper-bound ValueError
            ctx.emitter.instruction(&format!("b {valid}"));                     // continue with a valid signal number
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // reject signal numbers below PHP's valid range
            ctx.emitter.instruction(&format!("jl {below_range}"));              // report PHP's lower-bound ValueError
            ctx.emitter.instruction(&format!("cmp rax, {upper_bound}"));        // reject the target's one-past-last signal number
            ctx.emitter.instruction(&format!("jge {above_range}"));             // report PHP's upper-bound ValueError
            ctx.emitter.instruction(&format!("jmp {valid}"));                   // continue with a valid signal number
        }
    }
    ctx.emitter.label(&below_range);
    super::super::exceptions::emit_value_error(
        ctx,
        "pcntl_signal(): Argument #1 ($signal) must be greater than or equal to 1",
    );
    ctx.emitter.label(&above_range);
    super::super::exceptions::emit_value_error(
        ctx,
        &format!("pcntl_signal(): Argument #1 ($signal) must be less than {upper_bound}"),
    );
    ctx.emitter.label(&valid);
}

/// Installs bridge function pointers into fixed runtime slots without coupling the runtime cache.
fn emit_initialize_signal_bridge_slots(ctx: &mut FunctionContext<'_>) {
    let (signal_symbol, next_symbol, begin_symbol, end_symbol) = match ctx.emitter.target.platform {
        Platform::MacOS => (
            "_elephc_pcntl_signal",
            "_elephc_pcntl_signal_next",
            "_elephc_pcntl_dispatch_begin",
            "_elephc_pcntl_dispatch_end",
        ),
        Platform::Linux => (
            "elephc_pcntl_signal",
            "elephc_pcntl_signal_next",
            "elephc_pcntl_dispatch_begin",
            "elephc_pcntl_dispatch_end",
        ),
        Platform::Windows => return,
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_extern_symbol_address(ctx.emitter, "x9", signal_symbol);
            abi::emit_symbol_address(ctx.emitter, "x10", "__rt_pcntl_signal_fn");
            ctx.emitter.instruction("str x9, [x10]");                           // publish the signal-registration bridge
            abi::emit_extern_symbol_address(ctx.emitter, "x9", next_symbol);
            abi::emit_symbol_address(ctx.emitter, "x10", "__rt_pcntl_signal_next_fn");
            ctx.emitter.instruction("str x9, [x10]");                           // publish the queued-signal reader bridge
            abi::emit_extern_symbol_address(ctx.emitter, "x9", begin_symbol);
            abi::emit_symbol_address(ctx.emitter, "x10", "__rt_pcntl_dispatch_begin_fn");
            ctx.emitter.instruction("str x9, [x10]");                           // publish the dispatch-begin bridge
            abi::emit_extern_symbol_address(ctx.emitter, "x9", end_symbol);
            abi::emit_symbol_address(ctx.emitter, "x10", "__rt_pcntl_dispatch_end_fn");
            ctx.emitter.instruction("str x9, [x10]");                           // publish the dispatch-end bridge
        }
        Arch::X86_64 => {
            abi::emit_extern_symbol_address(ctx.emitter, "r9", signal_symbol);
            abi::emit_symbol_address(ctx.emitter, "r10", "__rt_pcntl_signal_fn");
            ctx.emitter.instruction("mov QWORD PTR [r10], r9");                 // publish the signal-registration bridge
            abi::emit_extern_symbol_address(ctx.emitter, "r9", next_symbol);
            abi::emit_symbol_address(ctx.emitter, "r10", "__rt_pcntl_signal_next_fn");
            ctx.emitter.instruction("mov QWORD PTR [r10], r9");                 // publish the queued-signal reader bridge
            abi::emit_extern_symbol_address(ctx.emitter, "r9", begin_symbol);
            abi::emit_symbol_address(ctx.emitter, "r10", "__rt_pcntl_dispatch_begin_fn");
            ctx.emitter.instruction("mov QWORD PTR [r10], r9");                 // publish the dispatch-begin bridge
            abi::emit_extern_symbol_address(ctx.emitter, "r9", end_symbol);
            abi::emit_symbol_address(ctx.emitter, "r10", "__rt_pcntl_dispatch_end_fn");
            ctx.emitter.instruction("mov QWORD PTR [r10], r9");                 // publish the dispatch-end bridge
        }
    }
}

/// Lowers `pcntl_signal_get_handler()` to an owned copy of the registered PHP value.
pub(crate) fn lower_signal_get_handler(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "pcntl_signal_get_handler", 1, 1)?;
    let signal = expect_operand(inst, 0)?;
    load_as_int(ctx, signal, "pcntl_signal_get_handler signal")?;
    let registered = ctx.next_label("pcntl_signal_get_handler_registered");
    let invalid = ctx.next_label("pcntl_signal_get_handler_invalid");
    let done = ctx.next_label("pcntl_signal_get_handler_done");
    let invalid_message = match ctx.emitter.target.platform {
        Platform::MacOS => {
            "pcntl_signal_get_handler(): Argument #1 ($signal) must be between 1 and 31"
        }
        Platform::Linux => {
            "pcntl_signal_get_handler(): Argument #1 ($signal) must be between 1 and 64"
        }
        Platform::Windows => {
            "pcntl_signal_get_handler(): Argument #1 ($signal) is invalid"
        }
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.bl_c("elephc_pcntl_signal_limit");
            abi::emit_pop_reg(ctx.emitter, "x9");
            ctx.emitter.instruction("cmp x9, #1");                              // enforce PHP's minimum signal number
            ctx.emitter.instruction(&format!("b.lt {invalid}"));                // reject values below the supported range
            ctx.emitter.instruction("cmp x9, x0");                              // compare against the target signal limit
            ctx.emitter.instruction(&format!("b.ge {invalid}"));                // reject values beyond the target range
            abi::emit_symbol_address(ctx.emitter, "x10", "__rt_pcntl_handler_value");
            ctx.emitter.instruction("ldr x0, [x10, x9, lsl #3]");               // load the preserved PHP handler value
            ctx.emitter.instruction(&format!("cbnz x0, {registered}"));         // return an independent reference when a value was registered
            ctx.emitter.instruction("mov x0, #0");                              // an untouched signal has PHP's SIG_DFL disposition
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
            ctx.emitter.instruction(&format!("b {done}"));                      // return the boxed integer disposition
            ctx.emitter.label(&registered);
            abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Mixed);
            ctx.emitter.instruction(&format!("b {done}"));                      // return the retained boxed PHP handler value
            ctx.emitter.label(&invalid);
            super::super::exceptions::emit_value_error(ctx, invalid_message);
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.bl_c("elephc_pcntl_signal_limit");
            abi::emit_pop_reg(ctx.emitter, "r9");
            ctx.emitter.instruction("cmp r9, 1");                               // enforce PHP's minimum signal number
            ctx.emitter.instruction(&format!("jl {invalid}"));                  // reject values below the supported range
            ctx.emitter.instruction("cmp r9, rax");                             // compare against the target signal limit
            ctx.emitter.instruction(&format!("jge {invalid}"));                 // reject values beyond the target range
            abi::emit_symbol_address(ctx.emitter, "r10", "__rt_pcntl_handler_value");
            ctx.emitter.instruction("mov rax, QWORD PTR [r10 + r9*8]");         // load the preserved PHP handler value
            ctx.emitter.instruction("test rax, rax");                           // distinguish registered values from untouched SIG_DFL
            ctx.emitter.instruction(&format!("jnz {registered}"));              // return an independent reference when a value was registered
            ctx.emitter.instruction("xor eax, eax");                            // an untouched signal has PHP's SIG_DFL disposition
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
            ctx.emitter.instruction(&format!("jmp {done}"));                    // return the boxed integer disposition
            ctx.emitter.label(&registered);
            abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Mixed);
            ctx.emitter.instruction(&format!("jmp {done}"));                    // return the retained boxed PHP handler value
            ctx.emitter.label(&invalid);
            super::super::exceptions::emit_value_error(ctx, invalid_message);
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
    emit_initialize_signal_bridge_slots(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_pcntl_dispatch_pending");
    store_if_result(ctx, inst)
}

/// Lowers querying or changing the process-wide asynchronous-dispatch flag.
pub(crate) fn lower_async_signals(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "pcntl_async_signals", 0, 1)?;
    emit_initialize_signal_bridge_slots(ctx);
    let Some(enable) = inst.operands.first().copied() else {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_symbol_address(ctx.emitter, "x9", "__rt_pcntl_async_enabled");
                ctx.emitter.instruction("ldr x0, [x9]");                        // return the current asynchronous-dispatch flag
            }
            Arch::X86_64 => {
                abi::emit_symbol_address(ctx.emitter, "r9", "__rt_pcntl_async_enabled");
                ctx.emitter.instruction("mov rax, QWORD PTR [r9]");             // return the current asynchronous-dispatch flag
            }
        }
        return store_if_result(ctx, inst);
    };

    let query = ctx.next_label("pcntl_async_signals_query");
    let done = ctx.next_label("pcntl_async_signals_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "__rt_pcntl_async_enabled");
            ctx.emitter.instruction("ldr x10, [x9]");                           // preserve the previous asynchronous-dispatch flag
            abi::emit_push_reg(ctx.emitter, "x10");
            predicates::emit_is_null_result(ctx, enable)?;
            ctx.emitter.instruction(&format!("cbnz x0, {query}"));              // treat null as a query without mutation
            load_as_int(ctx, enable, "pcntl_async_signals enable")?;
            ctx.emitter.instruction("cmp x0, #0");                              // normalize the requested flag to PHP boolean truth
            ctx.emitter.instruction("cset x0, ne");                             // materialize the normalized flag
            abi::emit_symbol_address(ctx.emitter, "x9", "__rt_pcntl_async_enabled");
            ctx.emitter.instruction("str x0, [x9]");                            // publish the new asynchronous-dispatch flag
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.instruction(&format!("b {done}"));                      // return the flag that was previously active
            ctx.emitter.label(&query);
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "__rt_pcntl_async_enabled");
            ctx.emitter.instruction("mov r10, QWORD PTR [r9]");                 // preserve the previous asynchronous-dispatch flag
            abi::emit_push_reg(ctx.emitter, "r10");
            predicates::emit_is_null_result(ctx, enable)?;
            ctx.emitter.instruction("test rax, rax");                           // detect the null query form
            ctx.emitter.instruction(&format!("jnz {query}"));                   // bypass mutation for a query
            load_as_int(ctx, enable, "pcntl_async_signals enable")?;
            ctx.emitter.instruction("test rax, rax");                           // normalize the requested flag to PHP boolean truth
            ctx.emitter.instruction("setne al");                                // materialize the normalized low byte
            ctx.emitter.instruction("movzx eax, al");                           // widen the normalized flag for storage
            abi::emit_symbol_address(ctx.emitter, "r9", "__rt_pcntl_async_enabled");
            ctx.emitter.instruction("mov QWORD PTR [r9], rax");                 // publish the new asynchronous-dispatch flag
            abi::emit_pop_reg(ctx.emitter, "rax");
            ctx.emitter.instruction(&format!("jmp {done}"));                    // return the flag that was previously active
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
        PhpType::Int => {
            load_as_int(ctx, handler, "pcntl_signal handler disposition")?;
            emit_normalize_integer_disposition(ctx);
            emit_push_integer_handler_pair(ctx);
        }
        PhpType::Bool | PhpType::False => {
            super::super::exceptions::emit_type_error(
                ctx,
                "pcntl_signal(): Argument #2 ($handler) must be of type callable|int, bool given",
            );
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

/// Pushes an owned boxed copy of the original PHP handler value for later introspection.
fn emit_push_handler_value(
    ctx: &mut FunctionContext<'_>,
    handler: crate::ir::ValueId,
) -> Result<()> {
    let handler_ty = ctx.value_php_type(handler)?.codegen_repr();
    ctx.load_value_to_result(handler)?;
    match handler_ty {
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Mixed);
        }
        other => emit_box_current_value_as_mixed(ctx.emitter, &other),
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    Ok(())
}

/// Classifies a boxed handler as an integer disposition or a runtime callable descriptor.
fn emit_push_mixed_handler_pair(
    ctx: &mut FunctionContext<'_>,
    handler: crate::ir::ValueId,
) -> Result<()> {
    let scalar = ctx.next_label("pcntl_signal_mixed_scalar");
    let bool_error = ctx.next_label("pcntl_signal_mixed_bool_error");
    let done = ctx.next_label("pcntl_signal_mixed_handler_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(handler, "x0")?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction(&format!("cmp x0, #{MIXED_TAG_INT}"));      // detect an integer signal disposition
            ctx.emitter.instruction(&format!("b.eq {scalar}"));                 // normalize an integer through the scalar path
            ctx.emitter.instruction(&format!("cmp x0, #{MIXED_TAG_BOOL}"));     // detect PHP's rejected boolean case
            ctx.emitter.instruction(&format!("b.eq {bool_error}"));             // throw the callable-or-int type error
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(handler, "rax")?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction(&format!("cmp rax, {MIXED_TAG_INT}"));      // detect an integer signal disposition
            ctx.emitter.instruction(&format!("je {scalar}"));                   // normalize an integer through the scalar path
            ctx.emitter.instruction(&format!("cmp rax, {MIXED_TAG_BOOL}"));     // detect PHP's rejected boolean case
            ctx.emitter.instruction(&format!("je {bool_error}"));               // throw the callable-or-int type error
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
        Arch::AArch64 => ctx.emitter.instruction("mov x0, x1"),                 // recover the unboxed integer disposition
        Arch::X86_64 => ctx.emitter.instruction("mov rax, rdi"),                // recover the unboxed integer disposition
    }
    emit_normalize_integer_disposition(ctx);
    emit_push_integer_handler_pair(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&bool_error);
    super::super::exceptions::emit_type_error(
        ctx,
        "pcntl_signal(): Argument #2 ($handler) must be of type callable|int, bool given",
    );
    ctx.emitter.label(&done);
    Ok(())
}

/// Accepts only PHP's integer dispositions and throws `ValueError` for every other integer.
fn emit_normalize_integer_disposition(ctx: &mut FunctionContext<'_>) {
    let valid = ctx.next_label("pcntl_signal_integer_handler_valid");
    let invalid = ctx.next_label("pcntl_signal_integer_handler_invalid");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // accept only SIG_DFL or SIG_IGN
            ctx.emitter.instruction(&format!("b.ls {valid}"));                  // continue for either supported disposition
            ctx.emitter.instruction(&format!("b {invalid}"));                   // reject every other integer handler
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // accept only SIG_DFL or SIG_IGN
            ctx.emitter.instruction(&format!("jbe {valid}"));                   // continue for either supported disposition
            ctx.emitter.instruction(&format!("jmp {invalid}"));                 // reject every other integer handler
        }
    }
    ctx.emitter.label(&invalid);
    super::super::exceptions::emit_value_error(
        ctx,
        "pcntl_signal(): Argument #2 ($handler) must be either SIG_DFL or SIG_IGN when an integer value is given",
    );
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
                ctx.emitter.instruction("cmp x0, #0");                          // normalize the explicit restart flag
                ctx.emitter.instruction("cset x0, ne");                         // materialize the normalized flag
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("test rax, rax");                       // normalize the explicit restart flag
                ctx.emitter.instruction("setne al");                            // materialize the normalized low byte
                ctx.emitter.instruction("movzx eax, al");                       // widen the flag for the bridge ABI
            }
        }
    } else {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("ldr x0, [sp, #48]");                   // recover the staged signal number
                ctx.emitter.instruction(&format!("cmp x0, #{SIGALRM}"));        // apply SIGALRM's non-restarting default
                ctx.emitter.instruction("cset x0, ne");                         // restart syscalls for every other signal by default
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("cmp QWORD PTR [rsp + 48], 14");        // apply SIGALRM's non-restarting default
                ctx.emitter.instruction("setne al");                            // restart syscalls for every other signal by default
                ctx.emitter.instruction("movzx eax, al");                       // widen the flag for the bridge ABI
            }
        }
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    Ok(())
}

/// Replaces one AArch64 handler-table entry and releases its prior descriptor ownership.
fn emit_replace_handler_table_entry_aarch64(ctx: &mut FunctionContext<'_>) {
    ctx.emitter.instruction("ldr x9, [sp, #48]");                               // recover the staged signal number
    abi::emit_symbol_address(ctx.emitter, "x10", "__rt_pcntl_handler_kind");
    ctx.emitter.instruction("ldr x11, [x10, x9, lsl #3]");                      // inspect the previous handler kind
    ctx.emitter.instruction("cmp x11, #2");                                     // detect an owned callable descriptor
    let skip_descriptor_release = ctx.next_label("pcntl_signal_no_old_descriptor");
    ctx.emitter.instruction(&format!("b.ne {skip_descriptor_release}"));        // preserve non-callable dispositions
    abi::emit_symbol_address(ctx.emitter, "x11", "__rt_pcntl_handler_descriptor");
    ctx.emitter.instruction("ldr x0, [x11, x9, lsl #3]");                       // load the descriptor being replaced
    callable_descriptor::emit_release_current_descriptor(ctx.emitter);
    ctx.emitter.label(&skip_descriptor_release);
    ctx.emitter.instruction("ldr x9, [sp, #48]");                               // recover the staged signal number after descriptor release
    abi::emit_symbol_address(ctx.emitter, "x10", "__rt_pcntl_handler_value");
    ctx.emitter.instruction("ldr x0, [x10, x9, lsl #3]");                       // load the prior PHP handler value
    let skip_value_release = ctx.next_label("pcntl_signal_no_old_value");
    ctx.emitter.instruction(&format!("cbz x0, {skip_value_release}"));          // untouched signals have no boxed value owner
    abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    ctx.emitter.label(&skip_value_release);
    ctx.emitter.instruction("ldr x9, [sp, #48]");                               // recover the staged signal number after value release
    ctx.emitter.instruction("ldr x11, [sp, #32]");                              // recover the new handler kind
    abi::emit_symbol_address(ctx.emitter, "x10", "__rt_pcntl_handler_kind");
    ctx.emitter.instruction("str x11, [x10, x9, lsl #3]");                      // publish the new handler kind
    ctx.emitter.instruction("ldr x11, [sp, #16]");                              // recover the new callable descriptor
    abi::emit_symbol_address(ctx.emitter, "x10", "__rt_pcntl_handler_descriptor");
    ctx.emitter.instruction("str x11, [x10, x9, lsl #3]");                      // transfer descriptor ownership to the table
    ctx.emitter.instruction("ldr x11, [sp]");                                   // recover the preserved PHP handler value
    abi::emit_symbol_address(ctx.emitter, "x10", "__rt_pcntl_handler_value");
    ctx.emitter.instruction("str x11, [x10, x9, lsl #3]");                      // transfer PHP-value ownership to the table
}

/// Replaces one x86_64 handler-table entry and releases its prior descriptor ownership.
fn emit_replace_handler_table_entry_x86_64(ctx: &mut FunctionContext<'_>) {
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 48]");                    // recover the staged signal number
    abi::emit_symbol_address(ctx.emitter, "r10", "__rt_pcntl_handler_kind");
    ctx.emitter.instruction("cmp QWORD PTR [r10 + r9*8], 2");                   // detect an owned callable descriptor
    let skip_descriptor_release = ctx.next_label("pcntl_signal_no_old_descriptor");
    ctx.emitter.instruction(&format!("jne {skip_descriptor_release}"));         // preserve non-callable dispositions
    abi::emit_symbol_address(ctx.emitter, "r10", "__rt_pcntl_handler_descriptor");
    ctx.emitter.instruction("mov rax, QWORD PTR [r10 + r9*8]");                 // load the descriptor being replaced
    callable_descriptor::emit_release_current_descriptor(ctx.emitter);
    ctx.emitter.label(&skip_descriptor_release);
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 48]");                    // recover the staged signal number after descriptor release
    abi::emit_symbol_address(ctx.emitter, "r10", "__rt_pcntl_handler_value");
    ctx.emitter.instruction("mov rax, QWORD PTR [r10 + r9*8]");                 // load the prior PHP handler value
    ctx.emitter.instruction("test rax, rax");                                   // untouched signals have no boxed value owner
    let skip_value_release = ctx.next_label("pcntl_signal_no_old_value");
    ctx.emitter.instruction(&format!("jz {skip_value_release}"));               // skip release for an empty value slot
    abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    ctx.emitter.label(&skip_value_release);
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 48]");                    // recover the staged signal number after value release
    ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 32]");                   // recover the new handler kind
    abi::emit_symbol_address(ctx.emitter, "r10", "__rt_pcntl_handler_kind");
    ctx.emitter.instruction("mov QWORD PTR [r10 + r9*8], r11");                 // publish the new handler kind
    ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 16]");                   // recover the new callable descriptor
    abi::emit_symbol_address(ctx.emitter, "r10", "__rt_pcntl_handler_descriptor");
    ctx.emitter.instruction("mov QWORD PTR [r10 + r9*8], r11");                 // transfer descriptor ownership to the table
    ctx.emitter.instruction("mov r11, QWORD PTR [rsp]");                        // recover the preserved PHP handler value
    abi::emit_symbol_address(ctx.emitter, "r10", "__rt_pcntl_handler_value");
    ctx.emitter.instruction("mov QWORD PTR [r10 + r9*8], r11");                 // transfer PHP-value ownership to the table
}
