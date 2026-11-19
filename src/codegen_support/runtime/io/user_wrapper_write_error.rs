//! Purpose:
//! Records PHP 8.6 userspace `stream_write()` failures for the PHP `fwrite()` dispatcher.
//!
//! Called from:
//! - `__rt_user_wrapper_fwrite` when `stream_write` is missing or over-reports a write.
//! - `crate::codegen_support::runtime::emitters::platform::emit_platform_runtime`.
//!
//! Key details:
//! - Publishes the wrapper class, returned count, requested count, and a missing-method
//!   flag into program globals; the PHP wrapper builds the exact php-src message.
//! - Both target architectures share one layout; only register moves differ.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_user_wrapper_write_error` for the active target.
pub(crate) fn emit_user_wrapper_write_error(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
    } else {
        emit_aarch64(emitter);
    }
}

/// Returns the program-global symbol for one write-tracking field.
fn tracking_symbol(name: &str) -> String {
    crate::names::ir_global_symbol(name)
}

/// Emits the AArch64 write-error tracker.
fn emit_aarch64(emitter: &mut Emitter) {
    let returned = tracking_symbol("__elephc_stream_write_returned");
    let requested = tracking_symbol("__elephc_stream_write_requested");
    let missing = tracking_symbol("__elephc_stream_write_missing");
    let class = tracking_symbol("__elephc_stream_write_class");

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_write_error ---");
    emitter.label_global("__rt_user_wrapper_write_error");
    // Frame: 80 bytes. [16] object, [24] returned, [32] requested, [40] missing,
    // [48] class ptr, [56] class len.
    emitter.instruction("sub sp, sp, #80");                                     // reserve the tracking helper frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the wrapper object for its class name
    emitter.instruction("str x1, [sp, #24]");                                   // save the returned byte count
    emitter.instruction("str x2, [sp, #32]");                                   // save the requested byte count
    emitter.instruction("str x3, [sp, #40]");                                   // save the missing-method flag

    // -- publish the returned count --
    abi::emit_symbol_address(emitter, "x10", &returned);
    emitter.instruction("ldr x0, [x10]");                                       // previous boxed count
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release it before overwriting
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("ldr x1, [sp, #24]");                                   // returned count payload
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_symbol_address(emitter, "x10", &returned);
    emitter.instruction("str x0, [x10]");                                       // publish the boxed returned count

    // -- publish the requested count --
    abi::emit_symbol_address(emitter, "x10", &requested);
    emitter.instruction("ldr x0, [x10]");                                       // previous boxed count
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release it before overwriting
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("ldr x1, [sp, #32]");                                   // requested count payload
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_symbol_address(emitter, "x10", &requested);
    emitter.instruction("str x0, [x10]");                                       // publish the boxed requested count

    // -- publish the missing-method flag --
    abi::emit_symbol_address(emitter, "x10", &missing);
    emitter.instruction("ldr x0, [x10]");                                       // previous boxed flag
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release it before overwriting
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("ldr x1, [sp, #40]");                                   // missing flag payload
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_symbol_address(emitter, "x10", &missing);
    emitter.instruction("str x0, [x10]");                                       // publish the boxed missing flag

    // -- class name from the dense class-id table --
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the wrapper object
    emitter.instruction("ldr x11, [x11]");                                      // object header word 0 = runtime class id
    abi::emit_symbol_address(emitter, "x10", "_class_name_count");
    emitter.instruction("ldr x10, [x10]");                                      // number of named class ids
    emitter.instruction("cmp x11, x10");                                        // is the class id within the table?
    emitter.instruction("b.hs __rt_uwwe_anon");                                 // unknown ids contribute no name
    abi::emit_symbol_address(emitter, "x12", "_class_name_entries");
    emitter.instruction("add x12, x12, x11, lsl #4");                           // each entry is a 16-byte (ptr, len) pair
    emitter.instruction("ldr x1, [x12]");                                       // class-name pointer
    emitter.instruction("ldr x2, [x12, #8]");                                   // class-name length
    emitter.instruction("b __rt_uwwe_named");                                   // emit the resolved name
    emitter.label("__rt_uwwe_anon");
    abi::emit_symbol_address(emitter, "x1", "_class_name_missing");             // empty fallback name
    emitter.instruction("mov x2, #0");                                          // zero-length class name
    emitter.label("__rt_uwwe_named");
    emitter.instruction("str x1, [sp, #48]");                                   // preserve the class-name pointer
    emitter.instruction("str x2, [sp, #56]");                                   // preserve the class-name length
    abi::emit_symbol_address(emitter, "x10", &class);
    emitter.instruction("ldr x0, [x10]");                                       // previous boxed class name
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release it before overwriting
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("ldr x1, [sp, #48]");                                   // class-name pointer payload
    emitter.instruction("ldr x2, [sp, #56]");                                   // class-name byte length
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_symbol_address(emitter, "x10", &class);
    emitter.instruction("str x0, [x10]");                                       // publish the boxed class name

    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the write helper
}

/// Emits the x86_64 write-error tracker.
fn emit_x86_64(emitter: &mut Emitter) {
    let returned = tracking_symbol("__elephc_stream_write_returned");
    let requested = tracking_symbol("__elephc_stream_write_requested");
    let missing = tracking_symbol("__elephc_stream_write_missing");
    let class = tracking_symbol("__elephc_stream_write_class");

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_write_error ---");
    emitter.label_global("__rt_user_wrapper_write_error");
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 64");                                         // reserve the tracking scratch slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the wrapper object for its class name
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the returned byte count
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the requested byte count
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the missing-method flag

    // -- publish the returned count --
    abi::emit_symbol_address(emitter, "r10", &returned);
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // previous boxed count
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release it before overwriting
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = int
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // returned count payload
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a second word
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_symbol_address(emitter, "r10", &returned);
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the boxed returned count

    // -- publish the requested count --
    abi::emit_symbol_address(emitter, "r10", &requested);
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // previous boxed count
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release it before overwriting
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = int
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // requested count payload
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a second word
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_symbol_address(emitter, "r10", &requested);
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the boxed requested count

    // -- publish the missing-method flag --
    abi::emit_symbol_address(emitter, "r10", &missing);
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // previous boxed flag
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release it before overwriting
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = int
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // missing flag payload
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a second word
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_symbol_address(emitter, "r10", &missing);
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the boxed missing flag

    // -- class name from the dense class-id table --
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the wrapper object
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // object header word 0 = runtime class id
    abi::emit_symbol_address(emitter, "r10", "_class_name_count");
    emitter.instruction("cmp r11, QWORD PTR [r10]");                            // is the class id within the table?
    emitter.instruction("jae __rt_uwwe_anon_x86");                              // unknown ids contribute no name
    abi::emit_symbol_address(emitter, "r10", "_class_name_entries");
    emitter.instruction("shl r11, 4");                                          // each entry is a 16-byte (ptr, len) pair
    emitter.instruction("add r10, r11");                                        // advance to this class's entry
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // class-name length
    emitter.instruction("jmp __rt_uwwe_named_x86");                             // emit the resolved name
    emitter.label("__rt_uwwe_anon_x86");
    abi::emit_symbol_address(emitter, "rdi", "_class_name_missing");            // empty fallback name
    emitter.instruction("xor esi, esi");                                        // zero-length class name
    emitter.label("__rt_uwwe_named_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // preserve the class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rsi");                       // preserve the class-name length
    abi::emit_symbol_address(emitter, "r10", &class);
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // previous boxed class name
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release it before overwriting
    emitter.instruction("mov rax, 1");                                          // runtime tag 1 = string
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // class-name pointer payload
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // class-name byte length
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_symbol_address(emitter, "r10", &class);
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the boxed class name

    emitter.instruction("mov rsp, rbp");                                        // discard the helper slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the write helper
}
