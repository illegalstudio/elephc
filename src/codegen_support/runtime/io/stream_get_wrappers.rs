//! Purpose:
//! Emits the `__rt_stream_get_wrappers` runtime helper, which appends the
//! user-registered scheme names from `_user_wrappers` to the built-in wrapper
//! list produced by the `stream_get_wrappers()` lowering.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The lowering of `stream_get_wrappers()` in the EIR backend.
//!
//! Key details:
//! - PHP reports every registered wrapper from `stream_get_wrappers()`, so a
//!   scheme registered through `stream_wrapper_register()` must appear in the
//!   list. Emitting only the static built-in names made user schemes invisible.
//! - The registration table holds `_user_wrappers_cap` 32-byte slots whose first
//!   two words are the owned `(protocol_ptr, protocol_len)` pair; a null pointer
//!   marks a free slot. The base is null and the capacity zero until the first
//!   registration, so an unregistered program scans nothing.
//! - The slot index is reloaded from the frame after each append: the array
//!   helper is a call and may clobber the caller-saved scratch registers.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// `__rt_stream_get_wrappers(builtin_array) -> array`.
///
/// Appends user-registered wrapper scheme names to the built-in array.
///
/// Input:  AArch64 x0 = built-in wrapper name array pointer.
///         x86_64  rax = built-in wrapper name array pointer.
/// Output: x0/rax = the (possibly grown) array including user scheme names.
pub fn emit_stream_get_wrappers(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_get_wrappers_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stream_get_wrappers ---");
    emitter.label_global("__rt_stream_get_wrappers");

    // Frame: [0]=array ptr [8]=table base [16]=slot index [24]=x29 [32]=x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate the wrapper-enumeration frame
    emitter.instruction("stp x29, x30, [sp, #24]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #24");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the built-in array pointer

    // -- scan _user_wrappers for occupied slots --
    super::emit_load_table_base(emitter, "x1");                  // registration table base
    emitter.instruction("str x1, [sp, #8]");                                    // save the table base
    emitter.instruction("mov x2, #0");                                          // slot index = 0
    emitter.instruction("str x2, [sp, #16]");                                   // save the slot index
    emitter.label("__rt_sgw_loop");
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the slot index
    // x3 is dead at the top of each iteration, so the capacity is reloaded into it.
    super::emit_load_table_cap(emitter, "x3");
    emitter.instruction("cmp x2, x3");                                          // scanned every allocated registration slot?
    emitter.instruction("b.ge __rt_sgw_done");                                  // yes → return the array
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the table base
    emitter.instruction("add x3, x1, x2, lsl #5");                              // slot base = table + index * 32
    emitter.instruction("ldr x4, [x3]");                                        // load the owned protocol pointer
    emitter.instruction("cbz x4, __rt_sgw_next");                               // null → free slot, skip
    emitter.instruction("ldr x5, [x3, #8]");                                    // load the protocol length
    // -- append this user scheme name to the array --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the array pointer
    emitter.instruction("mov x1, x4");                                          // scheme-name pointer
    emitter.instruction("mov x2, x5");                                          // scheme-name length
    emitter.instruction("bl __rt_array_push_str");                              // append the name; x0 = possibly grown array
    emitter.instruction("str x0, [sp, #0]");                                    // save the updated array pointer
    emitter.label("__rt_sgw_next");
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the slot index after the append call
    emitter.instruction("add x2, x2, #1");                                      // advance to the next slot
    emitter.instruction("str x2, [sp, #16]");                                   // save the updated slot index
    emitter.instruction("b __rt_sgw_loop");                                     // continue scanning

    emitter.label("__rt_sgw_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // load the final array pointer
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the wrapper-enumeration frame
    emitter.instruction("ret");                                                 // return the array in x0
}

/// x86_64 Linux variant of `__rt_stream_get_wrappers`.
fn emit_stream_get_wrappers_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_get_wrappers ---");
    emitter.label_global("__rt_stream_get_wrappers");

    // Frame: rbp-relative, [-8]=array ptr [-16]=table base [-24]=slot index
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve the wrapper-enumeration frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the built-in array pointer

    // -- scan _user_wrappers for occupied slots --
    super::emit_load_table_base(emitter, "r10");                 // registration table base
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save the table base
    emitter.instruction("xor ecx, ecx");                                        // slot index = 0
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the slot index
    emitter.label("__rt_sgw_loop_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the slot index
    // r11 is dead at the top of each iteration, so the capacity is reloaded into it.
    super::emit_load_table_cap(emitter, "r11");
    emitter.instruction("cmp rcx, r11");                                        // scanned every allocated registration slot?
    emitter.instruction("jge __rt_sgw_done_x");                                 // yes → return the array
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the table base
    emitter.instruction("mov r11, rcx");                                        // copy the slot index for scaling
    emitter.instruction("shl r11, 5");                                          // slot offset = index * 32
    emitter.instruction("add r11, r10");                                        // slot base = table + offset
    emitter.instruction("mov rax, QWORD PTR [r11]");                            // load the owned protocol pointer
    emitter.instruction("test rax, rax");                                       // null → free slot?
    emitter.instruction("jz __rt_sgw_next_x");                                  // skip free slots
    emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");                        // load the protocol length
    // -- append this user scheme name to the array --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the array pointer
    emitter.instruction("mov rsi, rax");                                        // scheme-name pointer
    emitter.instruction("call __rt_array_push_str");                            // append the name; rax = possibly grown array
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the updated array pointer
    emitter.label("__rt_sgw_next_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the slot index after the append call
    emitter.instruction("inc rcx");                                             // advance to the next slot
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the updated slot index
    emitter.instruction("jmp __rt_sgw_loop_x");                                 // continue scanning

    emitter.label("__rt_sgw_done_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // load the final array pointer
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return the array in rax
}
