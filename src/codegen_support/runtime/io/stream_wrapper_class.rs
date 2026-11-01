//! Purpose:
//! Emits `elephc_runtime_stream_wrapper_class`, the lookup used by `ZipArchive::extractTo()`
//! to name the registered wrapper class in a PHP-shaped stream-open failure warning.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::platform` via the IO runtime emitter set.
//! - The zip prelude's `extern "elephc_zip"` declaration.
//!
//! Key details:
//! - Scans the same `_user_wrappers` table `stream_wrapper_register()` fills (64 slots).
//! - Copies the class name into a bounded process buffer and NUL-terminates it for the
//!   compiler's extern string-return ABI.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the wrapper-class lookup for the active target.
pub fn emit_stream_wrapper_class(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
    } else {
        emit_aarch64(emitter);
    }
}

/// AArch64 helper: x0 = NUL-terminated scheme pointer; x0 = NUL-terminated class name.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_wrapper_class ---");
    let symbol = emitter.target.extern_symbol("elephc_runtime_stream_wrapper_class");
    emitter.label_global(&symbol);
    emitter.instruction("sub sp, sp, #64");                                     // reserve callee-saved spill space
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("stp x19, x20, [sp, #0]");                              // save callee-saved x19/x20
    emitter.instruction("stp x21, x22, [sp, #16]");                             // save callee-saved x21/x22
    emitter.instruction("mov x19, x0");                                         // x19 = scheme bytes
    emitter.instruction("mov x20, #0");                                         // scheme length counter
    emitter.label("elephc_runtime_swc_strlen");
    emitter.instruction("ldrb w9, [x19, x20]");                                 // load the next scheme byte
    emitter.instruction("cbz w9, elephc_runtime_swc_scan");                     // stop at the terminator
    emitter.instruction("add x20, x20, #1");                                    // count one byte
    emitter.instruction("b elephc_runtime_swc_strlen");                         // continue scanning
    emitter.label("elephc_runtime_swc_scan");
    emitter.instruction("mov x21, #0");                                         // wrapper slot index
    emitter.label("elephc_runtime_swc_slot");
    emitter.instruction("cmp x21, #64");                                        // checked every registration slot?
    emitter.instruction("b.ge elephc_runtime_swc_miss");                        // no scheme matched
    abi::emit_symbol_address(emitter, "x9", "_user_wrappers");                  // wrapper table base
    emitter.instruction("add x10, x9, x21, lsl #5");                            // slot base
    emitter.instruction("ldr x11, [x10]");                                      // protocol pointer
    emitter.instruction("cbz x11, elephc_runtime_swc_next");                    // empty slot
    emitter.instruction("ldr x12, [x10, #8]");                                  // protocol length
    emitter.instruction("cmp x12, x20");                                        // same scheme length?
    emitter.instruction("b.ne elephc_runtime_swc_next");                        // different scheme
    emitter.instruction("mov x13, #0");                                         // byte compare index
    emitter.label("elephc_runtime_swc_bytes");
    emitter.instruction("cmp x13, x20");                                        // compared every byte?
    emitter.instruction("b.ge elephc_runtime_swc_found");                       // full match
    emitter.instruction("ldrb w14, [x11, x13]");                                // stored scheme byte
    emitter.instruction("ldrb w15, [x19, x13]");                                // requested scheme byte
    emitter.instruction("cmp w14, w15");                                        // do they match?
    emitter.instruction("b.ne elephc_runtime_swc_next");                        // scheme differs
    emitter.instruction("add x13, x13, #1");                                    // advance the compare
    emitter.instruction("b elephc_runtime_swc_bytes");                          // compare the next byte
    emitter.label("elephc_runtime_swc_next");
    emitter.instruction("add x21, x21, #1");                                    // advance the slot index
    emitter.instruction("b elephc_runtime_swc_slot");                           // try the next slot
    emitter.label("elephc_runtime_swc_found");
    emitter.instruction("ldr x11, [x10, #16]");                                 // class-name pointer
    emitter.instruction("ldr x12, [x10, #24]");                                 // class-name length
    abi::emit_symbol_address(emitter, "x9", "_runtime_wrapper_class_buf");     // destination buffer
    emitter.instruction("mov x13, #0");                                         // copied byte count
    emitter.label("elephc_runtime_swc_copy");
    emitter.instruction("cmp x13, x12");                                        // copied the whole class name?
    emitter.instruction("b.ge elephc_runtime_swc_copy_done");                   // copy finished
    emitter.instruction("cmp x13, #1023");                                      // stay inside the buffer?
    emitter.instruction("b.ge elephc_runtime_swc_copy_done");                   // buffer is full
    emitter.instruction("ldrb w14, [x11, x13]");                                // class-name byte
    emitter.instruction("strb w14, [x9, x13]");                                 // copy into the buffer
    emitter.instruction("add x13, x13, #1");                                    // advance the copy
    emitter.instruction("b elephc_runtime_swc_copy");                           // copy the next byte
    emitter.label("elephc_runtime_swc_copy_done");
    emitter.instruction("strb wzr, [x9, x13]");                                 // NUL-terminate the result
    emitter.instruction("mov x0, x9");                                          // return the class name
    emitter.instruction("b elephc_runtime_swc_done");                           // restore and return
    emitter.label("elephc_runtime_swc_miss");
    abi::emit_symbol_address(emitter, "x0", "_runtime_wrapper_class_empty");   // empty class name
    emitter.label("elephc_runtime_swc_done");
    emitter.instruction("ldp x19, x20, [sp, #0]");                              // restore callee-saved x19/x20
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore callee-saved x21/x22
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the class name pointer
}

/// x86_64 helper: rdi = NUL-terminated scheme pointer; rax = NUL-terminated class name.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_wrapper_class ---");
    let symbol = emitter.target.extern_symbol("elephc_runtime_stream_wrapper_class");
    emitter.label_global(&symbol);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("push r12");                                            // save callee-saved r12
    emitter.instruction("push r13");                                            // save callee-saved r13
    emitter.instruction("push r14");                                            // save callee-saved r14
    emitter.instruction("push r15");                                            // save callee-saved r15
    emitter.instruction("push rbx");                                            // save callee-saved rbx
    emitter.instruction("sub rsp, 8");                                          // realign the stack
    emitter.instruction("mov r12, rdi");                                        // r12 = scheme bytes
    emitter.instruction("xor r13d, r13d");                                      // scheme length counter
    emitter.label("elephc_runtime_swc_strlen_x86");
    emitter.instruction("cmp BYTE PTR [r12 + r13], 0");                         // end of the scheme?
    emitter.instruction("je elephc_runtime_swc_scan_x86");                      // start scanning registrations
    emitter.instruction("inc r13");                                             // count one byte
    emitter.instruction("jmp elephc_runtime_swc_strlen_x86");                   // continue scanning
    emitter.label("elephc_runtime_swc_scan_x86");
    emitter.instruction("xor r14d, r14d");                                      // wrapper slot index
    emitter.label("elephc_runtime_swc_slot_x86");
    emitter.instruction("cmp r14, 64");                                         // checked every registration slot?
    emitter.instruction("jge elephc_runtime_swc_miss_x86");                     // no scheme matched
    abi::emit_symbol_address(emitter, "r10", "_user_wrappers");                 // wrapper table base
    emitter.instruction("mov r11, r14");                                        // copy the slot index
    emitter.instruction("shl r11, 5");                                          // slot * 32
    emitter.instruction("add r10, r11");                                        // slot base
    emitter.instruction("mov r15, QWORD PTR [r10]");                            // protocol pointer
    emitter.instruction("test r15, r15");                                       // empty slot?
    emitter.instruction("jz elephc_runtime_swc_next_x86");                      // skip empty slots
    emitter.instruction("mov rbx, QWORD PTR [r10 + 8]");                        // protocol length
    emitter.instruction("cmp rbx, r13");                                        // same scheme length?
    emitter.instruction("jne elephc_runtime_swc_next_x86");                     // different scheme
    emitter.instruction("xor ecx, ecx");                                        // byte compare index
    emitter.label("elephc_runtime_swc_bytes_x86");
    emitter.instruction("cmp rcx, r13");                                        // compared every byte?
    emitter.instruction("jge elephc_runtime_swc_found_x86");                    // full match
    emitter.instruction("mov al, BYTE PTR [r15 + rcx]");                        // stored scheme byte
    emitter.instruction("cmp al, BYTE PTR [r12 + rcx]");                        // requested scheme byte
    emitter.instruction("jne elephc_runtime_swc_next_x86");                     // scheme differs
    emitter.instruction("inc rcx");                                             // advance the compare
    emitter.instruction("jmp elephc_runtime_swc_bytes_x86");                    // compare the next byte
    emitter.label("elephc_runtime_swc_next_x86");
    emitter.instruction("inc r14");                                             // advance the slot index
    emitter.instruction("jmp elephc_runtime_swc_slot_x86");                     // try the next slot
    emitter.label("elephc_runtime_swc_found_x86");
    emitter.instruction("mov r15, QWORD PTR [r10 + 16]");                       // class-name pointer
    emitter.instruction("mov rbx, QWORD PTR [r10 + 24]");                       // class-name length
    abi::emit_symbol_address(emitter, "r9", "_runtime_wrapper_class_buf");     // destination buffer
    emitter.instruction("xor ecx, ecx");                                        // copied byte count
    emitter.label("elephc_runtime_swc_copy_x86");
    emitter.instruction("cmp rcx, rbx");                                        // copied the whole class name?
    emitter.instruction("jge elephc_runtime_swc_copy_done_x86");                // copy finished
    emitter.instruction("cmp rcx, 1023");                                       // stay inside the buffer?
    emitter.instruction("jge elephc_runtime_swc_copy_done_x86");                // buffer is full
    emitter.instruction("mov al, BYTE PTR [r15 + rcx]");                        // class-name byte
    emitter.instruction("mov BYTE PTR [r9 + rcx], al");                         // copy into the buffer
    emitter.instruction("inc rcx");                                             // advance the copy
    emitter.instruction("jmp elephc_runtime_swc_copy_x86");                     // copy the next byte
    emitter.label("elephc_runtime_swc_copy_done_x86");
    emitter.instruction("mov BYTE PTR [r9 + rcx], 0");                          // NUL-terminate the result
    emitter.instruction("mov rax, r9");                                         // return the class name
    emitter.instruction("jmp elephc_runtime_swc_done_x86");                     // restore and return
    emitter.label("elephc_runtime_swc_miss_x86");
    abi::emit_symbol_address(emitter, "rax", "_runtime_wrapper_class_empty");  // empty class name
    emitter.label("elephc_runtime_swc_done_x86");
    emitter.instruction("add rsp, 8");                                          // undo the alignment padding
    emitter.instruction("pop rbx");                                             // restore callee-saved rbx
    emitter.instruction("pop r15");                                             // restore callee-saved r15
    emitter.instruction("pop r14");                                             // restore callee-saved r14
    emitter.instruction("pop r13");                                             // restore callee-saved r13
    emitter.instruction("pop r12");                                             // restore callee-saved r12
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the class name pointer
}
