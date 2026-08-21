//! Purpose:
//! Emits static catchable Throwable objects from generated userspace stream-wrapper adapters.
//! Keeps adapter validation failures on the ordinary PHP exception unwinder.
//!
//! Called from:
//! - `crate::codegen::user_wrapper_adapters::coercion`.
//!
//! Key details:
//! - Static messages live in the user data section and are valid for the program lifetime.
//! - Uncaught failures retain the compiler's specific PHP-style fatal diagnostic.

use crate::codegen::{abi, data_section::DataSection};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits a static catchable Throwable and transfers control to the standard unwinder.
pub(super) fn emit_static_throwable(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label_prefix: &str,
    class_name: &str,
    class_id_symbol: &str,
    message: &str,
) {
    let fatal_message = format!("Fatal error: Uncaught {class_name}: {message}\n");
    let (fatal_label, fatal_len) = data.add_string(fatal_message.as_bytes());
    let (message_label, message_len) = data.add_string(message.as_bytes());
    let throw_label = format!("{label_prefix}_throw");

    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x9", "_exc_handler_top", 0);
            emitter.instruction(&format!("cbnz x9, {}", throw_label));          // use the PHP unwinder when an active catch handler can receive this throwable
            abi::emit_symbol_address(emitter, "x1", &fatal_label);
            abi::emit_load_int_immediate(emitter, "x2", fatal_len as i64);
            emitter.instruction("mov x0, #2");                                  // write the specific uncaught wrapper-callback diagnostic to stderr
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_handler_top", 0);
            emitter.instruction("test r10, r10");                               // check whether a PHP catch handler is active
            emitter.instruction(&format!("jnz {}", throw_label));               // use the PHP unwinder when the throwable can be caught
            abi::emit_symbol_address(emitter, "rsi", &fatal_label);
            abi::emit_load_int_immediate(emitter, "rdx", fatal_len as i64);
            emitter.instruction("mov edi, 2");                                  // write the specific uncaught wrapper-callback diagnostic to stderr
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 writes the fatal diagnostic
            emitter.instruction("syscall");                                     // emit the uncaught wrapper callback error
            abi::emit_exit(emitter, 1);
        }
    }
    emitter.label(&throw_label);

    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(emitter, "x0", 56);
            abi::emit_call_label(emitter, "__rt_heap_alloc");
            emitter.instruction("mov x9, #6");                                  // heap kind 6 identifies a compact Throwable object
            emitter.instruction("str x9, [x0, #-8]");                           // stamp the allocation as a runtime object
            abi::emit_load_symbol_to_reg(emitter, "x9", class_id_symbol, 0);
            emitter.instruction("str x9, [x0]");                                // store the requested built-in Throwable class id
            abi::emit_symbol_address(emitter, "x9", &message_label);
            emitter.instruction("str x9, [x0, #8]");                            // store the static Throwable message pointer
            abi::emit_load_int_immediate(emitter, "x9", message_len as i64);
            emitter.instruction("str x9, [x0, #16]");                           // store the Throwable message byte length
            emitter.instruction("str xzr, [x0, #24]");                          // Throwable code defaults to zero
            emitter.instruction("str xzr, [x0, #40]");                          // previous Throwable defaults to null
            abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);
            abi::emit_jump(emitter, "__rt_throw_current");
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(emitter, "rax", 56);
            abi::emit_call_label(emitter, "__rt_heap_alloc");
            emitter.instruction(&format!(
                "mov r10, 0x{:x}",
                crate::codegen_support::sentinels::x86_64_heap_kind_word(6)
            )); // stamp the canonical x86_64 object heap-kind word
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the allocation as a runtime object
            abi::emit_load_symbol_to_reg(emitter, "r10", class_id_symbol, 0);
            emitter.instruction("mov QWORD PTR [rax], r10");                    // store the requested built-in Throwable class id
            abi::emit_symbol_address(emitter, "r10", &message_label);
            emitter.instruction("mov QWORD PTR [rax + 8], r10");                // store the static Throwable message pointer
            abi::emit_load_int_immediate(emitter, "r10", message_len as i64);
            emitter.instruction("mov QWORD PTR [rax + 16], r10");               // store the Throwable message byte length
            emitter.instruction("mov QWORD PTR [rax + 24], 0");                 // Throwable code defaults to zero
            emitter.instruction("mov QWORD PTR [rax + 40], 0");                 // previous Throwable defaults to null
            abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);
            abi::emit_jump(emitter, "__rt_throw_current");
        }
    }
}
