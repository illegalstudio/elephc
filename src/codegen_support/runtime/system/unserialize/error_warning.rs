//! Purpose:
//! Emits `__rt_unserialize_report_error`, the PHP warning for malformed serialized data.
//!
//! Called from:
//! - `super::emit_unserialize()` before the target-specific entry and validator.
//!
//! Key details:
//! - Reports PHP's exact `Warning: unserialize(): Error at offset N of M bytes` shape.
//! - The call-site line comes from `_unser_call_line`, stored by the builtin lowering.
//! - Fragments go through the shared diagnostic dispatcher, so user error handlers and
//!   `error_reporting` see one complete warning and the default output matches PHP CLI.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the malformed-unserialize warning builder for the active target.
pub(super) fn emit(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 warning builder.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize_error_warning ---");
    emitter.label_global("__rt_unserialize_report_error");
    emitter.instruction("sub sp, sp, #48");                                     // reserve offset, total, and call-site line storage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a stable warning frame
    emitter.instruction("stp x0, x1, [sp, #0]");                                // save the failing offset and total source length
    emitter.instruction("str x2, [sp, #16]");                                   // save the unserialize() call-site line
    abi::emit_symbol_address(emitter, "x1", "_unser_error_prefix");
    emitter.instruction("mov x2, #40");                                         // pass the static warning prefix length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    emitter.instruction("ldr x0, [sp, #0]");                                    // format the failing byte offset
    abi::emit_call_label(emitter, "__rt_itoa");
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    abi::emit_symbol_address(emitter, "x1", "_unser_error_of");
    emitter.instruction("mov x2, #4");                                          // pass the offset separator length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    emitter.instruction("ldr x0, [sp, #8]");                                    // format the total source length
    abi::emit_call_label(emitter, "__rt_itoa");
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    abi::emit_symbol_address(emitter, "x1", "_unser_error_bytes_in");
    emitter.instruction("mov x2, #10");                                         // pass the byte-count separator length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    abi::emit_symbol_address(emitter, "x1", "_script_source_file");
    abi::emit_load_symbol_to_reg(emitter, "x2", "_script_source_file_len", 0);
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    abi::emit_symbol_address(emitter, "x1", "_unser_error_on_line");
    emitter.instruction("mov x2, #9");                                          // pass the location separator length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    emitter.instruction("ldr x0, [sp, #16]");                                   // format the unserialize() call-site line
    abi::emit_call_label(emitter, "__rt_itoa");
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    abi::emit_symbol_address(emitter, "x1", "_unser_error_newline");
    emitter.instruction("mov x2, #1");                                          // pass the terminating newline length
    abi::emit_call_label(emitter, "__rt_diag_warning");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #48");                                     // release the warning frame
    emitter.instruction("ret");                                                 // return after the warning has been dispatched
}

/// Emits the x86_64 warning builder.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize_error_warning ---");
    emitter.label_global("__rt_unserialize_report_error");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable warning frame
    emitter.instruction("sub rsp, 48");                                         // reserve offset, total, and call-site line storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the failing offset
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the total source length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the unserialize() call-site line
    abi::emit_symbol_address(emitter, "rdi", "_unser_error_prefix");
    emitter.instruction("mov esi, 40");                                         // pass the static warning prefix length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // format the failing byte offset
    abi::emit_call_label(emitter, "__rt_itoa");
    emitter.instruction("mov rdi, rax");                                        // pass the formatted offset pointer
    emitter.instruction("mov rsi, rdx");                                        // pass the formatted offset length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    abi::emit_symbol_address(emitter, "rdi", "_unser_error_of");
    emitter.instruction("mov esi, 4");                                          // pass the offset separator length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // format the total source length
    abi::emit_call_label(emitter, "__rt_itoa");
    emitter.instruction("mov rdi, rax");                                        // pass the formatted total pointer
    emitter.instruction("mov rsi, rdx");                                        // pass the formatted total length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    abi::emit_symbol_address(emitter, "rdi", "_unser_error_bytes_in");
    emitter.instruction("mov esi, 10");                                         // pass the byte-count separator length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    abi::emit_symbol_address(emitter, "rdi", "_script_source_file");
    abi::emit_load_symbol_to_reg(emitter, "rsi", "_script_source_file_len", 0);
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    abi::emit_symbol_address(emitter, "rdi", "_unser_error_on_line");
    emitter.instruction("mov esi, 9");                                          // pass the location separator length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // format the unserialize() call-site line
    abi::emit_call_label(emitter, "__rt_itoa");
    emitter.instruction("mov rdi, rax");                                        // pass the formatted line pointer
    emitter.instruction("mov rsi, rdx");                                        // pass the formatted line length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    abi::emit_symbol_address(emitter, "rdi", "_unser_error_newline");
    emitter.instruction("mov esi, 1");                                          // pass the terminating newline length
    abi::emit_call_label(emitter, "__rt_diag_warning");
    emitter.instruction("mov rsp, rbp");                                        // release local warning storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return after the warning has been dispatched
}
