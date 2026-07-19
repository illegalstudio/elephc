//! Purpose:
//! Emits catchable built-in `Error` and `TypeError` objects for codegen guards.
//!
//! Called from:
//! - EIR instruction lowerers that detect PHP runtime type/null errors.
//!
//! Key details:
//! - Active handlers receive a normal throwable through `__rt_throw_current`.
//! - Unhandled errors keep a specific PHP-style fatal diagnostic instead of the
//!   runtime unwinder's generic uncaught-exception fallback.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::ValueId;

use super::super::context::FunctionContext;
use super::super::Result;

/// Throws a catchable PHP `Error` carrying a static message.
pub(super) fn emit_error(ctx: &mut FunctionContext<'_>, message: &str) {
    emit_static_exception(ctx, "Error", "_spl_error_class_id", message);
}

/// Throws a catchable PHP `TypeError` carrying a static message.
pub(super) fn emit_type_error(ctx: &mut FunctionContext<'_>, message: &str) {
    emit_static_exception(ctx, "TypeError", "_spl_type_error_class_id", message);
}

/// Throws a catchable PHP `Error` whose message is a runtime string value.
pub(super) fn emit_error_value(ctx: &mut FunctionContext<'_>, message: ValueId) -> Result<()> {
    let (message_ptr_reg, message_len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(message, message_ptr_reg, message_len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, message_ptr_reg, message_len_reg);
    emit_uncaught_dynamic_error_fatal_if_no_handler(ctx);
    emit_dynamic_error_object(ctx);
    Ok(())
}

/// Allocates one built-in throwable and transfers control to the standard unwinder.
fn emit_static_exception(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    class_id_symbol: &str,
    message: &str,
) {
    let fatal_message = format!("Fatal error: Uncaught {}: {}\n", class_name, message);
    let (fatal_label, fatal_len) = ctx.data.add_string(fatal_message.as_bytes());
    emit_uncaught_exception_fatal_if_no_handler(ctx, &fatal_label, fatal_len);

    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", 56); // compact Throwable: message/code/previous
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #6");                              // heap kind 6 = throwable object instance
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the allocation as a runtime object
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", class_id_symbol, 0);
            ctx.emitter.instruction("str x9, [x0]");                            // store the built-in throwable class id
            abi::emit_symbol_address(ctx.emitter, "x9", &message_label);
            ctx.emitter.instruction("str x9, [x0, #8]");                        // store the static exception message pointer
            abi::emit_load_int_immediate(ctx.emitter, "x9", message_len as i64);
            ctx.emitter.instruction("str x9, [x0, #16]");                       // store the exception message length
            ctx.emitter.instruction("str xzr, [x0, #24]");                      // exception code defaults to zero
            ctx.emitter.instruction("str xzr, [x0, #40]"); // previous defaults to null
            abi::emit_store_reg_to_symbol(ctx.emitter, "x0", "_exc_value", 0);
            abi::emit_jump(ctx.emitter, "__rt_throw_current");
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rax", 56); // compact Throwable: message/code/previous
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov r10, 0x4548504c00000006");             // x86_64 heap kind 6 with the runtime magic marker
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the allocation as a runtime object
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", class_id_symbol, 0);
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store the built-in throwable class id
            abi::emit_symbol_address(ctx.emitter, "r10", &message_label);
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");            // store the static exception message pointer
            abi::emit_load_int_immediate(ctx.emitter, "r10", message_len as i64);
            ctx.emitter.instruction("mov QWORD PTR [rax + 16], r10");           // store the exception message length
            ctx.emitter.instruction("mov QWORD PTR [rax + 24], 0");             // exception code defaults to zero
            ctx.emitter.instruction("mov QWORD PTR [rax + 40], 0"); // previous defaults to null
            abi::emit_store_reg_to_symbol(ctx.emitter, "rax", "_exc_value", 0);
            abi::emit_jump(ctx.emitter, "__rt_throw_current");
        }
    }
}

/// Writes the specific uncaught diagnostic and exits when no catch handler is active.
fn emit_uncaught_exception_fatal_if_no_handler(
    ctx: &mut FunctionContext<'_>,
    fatal_label: &str,
    fatal_len: usize,
) {
    let throw_label = ctx.next_label("static_exception_throw");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_exc_handler_top", 0);
            ctx.emitter.instruction(&format!("cbnz x9, {}", throw_label));      // use the standard unwinder when a catch handler is active
            abi::emit_symbol_address(ctx.emitter, "x1", fatal_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", fatal_len as i64);
            ctx.emitter.instruction("mov x0, #2");                              // write the uncaught PHP diagnostic to stderr
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_exc_handler_top", 0);
            ctx.emitter.instruction("test r10, r10");                           // check whether a catch handler is active
            ctx.emitter.instruction(&format!("jnz {}", throw_label));           // use the standard unwinder when a handler can receive the error
            abi::emit_symbol_address(ctx.emitter, "rsi", fatal_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", fatal_len as i64);
            ctx.emitter.instruction("mov edi, 2");                              // write the uncaught PHP diagnostic to stderr
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the specific fatal message
            abi::emit_exit(ctx.emitter, 1);
        }
    }
    ctx.emitter.label(&throw_label);
}

/// Writes an uncaught dynamic `Error` diagnostic, or continues when a handler exists.
fn emit_uncaught_dynamic_error_fatal_if_no_handler(ctx: &mut FunctionContext<'_>) {
    let throw_label = ctx.next_label("dynamic_error_throw");
    let (prefix_label, prefix_len) = ctx.data.add_string(b"Fatal error: Uncaught Error: ");
    let (suffix_label, suffix_len) = ctx.data.add_string(b"\n");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_exc_handler_top", 0);
            ctx.emitter.instruction(&format!("cbnz x9, {}", throw_label));      // use the standard unwinder when a catch handler is active
            ctx.emitter.instruction("mov x0, #2");                              // write the uncaught dynamic-error prefix to stderr
            abi::emit_symbol_address(ctx.emitter, "x1", &prefix_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", prefix_len as i64);
            ctx.emitter.syscall(4);
            ctx.emitter.instruction("mov x0, #2");                              // write the runtime error message to stderr
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", 8);
            ctx.emitter.syscall(4);
            ctx.emitter.instruction("mov x0, #2");                              // terminate the uncaught diagnostic with a newline
            abi::emit_symbol_address(ctx.emitter, "x1", &suffix_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", suffix_len as i64);
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_exc_handler_top", 0);
            ctx.emitter.instruction("test r10, r10");                           // check whether a catch handler is active
            ctx.emitter.instruction(&format!("jnz {}", throw_label));           // use the standard unwinder when a handler can receive the error
            abi::emit_symbol_address(ctx.emitter, "rsi", &prefix_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", prefix_len as i64);
            ctx.emitter.instruction("mov edi, 2");                              // write the uncaught dynamic-error prefix to stderr
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the dynamic-error prefix
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", 8);
            ctx.emitter.instruction("mov edi, 2");                              // write the runtime error message to stderr
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the runtime error message
            abi::emit_symbol_address(ctx.emitter, "rsi", &suffix_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", suffix_len as i64);
            ctx.emitter.instruction("mov edi, 2");                              // terminate the uncaught diagnostic with a newline
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the dynamic-error suffix
            abi::emit_exit(ctx.emitter, 1);
        }
    }
    ctx.emitter.label(&throw_label);
}

/// Allocates a built-in `Error` that owns the runtime message stored on the stack.
fn emit_dynamic_error_object(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", 56); // compact Throwable: message/code/previous
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #6");                              // heap kind 6 = throwable object instance
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the allocation as a runtime object
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_spl_error_class_id", 0);
            ctx.emitter.instruction("str x9, [x0]");                            // store the built-in Error class id
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 0);
            ctx.emitter.instruction("str x9, [x0, #8]");                        // store the runtime exception message pointer
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 8);
            ctx.emitter.instruction("str x9, [x0, #16]");                       // store the runtime exception message length
            ctx.emitter.instruction("str xzr, [x0, #24]");                      // exception code defaults to zero
            ctx.emitter.instruction("str xzr, [x0, #40]"); // previous defaults to null
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            abi::emit_store_reg_to_symbol(ctx.emitter, "x0", "_exc_value", 0);
            abi::emit_jump(ctx.emitter, "__rt_throw_current");
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rax", 56); // compact Throwable: message/code/previous
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov r10, 0x4548504c00000006");             // x86_64 heap kind 6 with the runtime magic marker
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the allocation as a runtime object
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_spl_error_class_id", 0);
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store the built-in Error class id
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 0);
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");            // store the runtime exception message pointer
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 8);
            ctx.emitter.instruction("mov QWORD PTR [rax + 16], r10");           // store the runtime exception message length
            ctx.emitter.instruction("mov QWORD PTR [rax + 24], 0");             // exception code defaults to zero
            ctx.emitter.instruction("mov QWORD PTR [rax + 40], 0"); // previous defaults to null
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            abi::emit_store_reg_to_symbol(ctx.emitter, "rax", "_exc_value", 0);
            abi::emit_jump(ctx.emitter, "__rt_throw_current");
        }
    }
}
