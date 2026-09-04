//! Purpose:
//! Emits the `__rt_stream_get_filters` runtime helper, which builds an indexed
//! array of filter names by combining the built-in static list with the
//! user-registered filters from `_user_filter_registry`.
//!
//! Called from:
//! - The lowering of `stream_get_filters()` in the EIR backend.
//!
//! Key details:
//! - The built-in filter names are passed as a static string array by the
//!   lowering; this runtime appends any user-registered filter names found
//!   in `_user_filter_registry` (non-null slots).
//! - The input array (built-in names) is received in x0/rax; the output is
//!   the grown array in x0/rax.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// `__rt_stream_get_filters(builtin_array) -> array`.
///
/// Appends user-registered filter names to the built-in array and returns it.
///
/// Input:  AArch64 x0 = built-in filter name array pointer.
///         x86_64  rax = built-in filter name array pointer.
/// Output: x0/rax = the (possibly grown) array including user filter names.
pub fn emit_stream_get_filters(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_get_filters_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stream_get_filters ---");
    emitter.label_global("__rt_stream_get_filters");

    // Frame: [0]=array ptr [8]=registry base [16]=slot index [24]=x29 [32]=x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate the filter-enumeration frame
    emitter.instruction("stp x29, x30, [sp, #24]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #24");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                     // save the built-in array pointer

    // -- scan _user_filter_registry for non-null slots --
    abi::emit_symbol_address(emitter, "x1", "_user_filter_registry");            // registry base
    emitter.instruction("str x1, [sp, #8]");                                    // save the registry base
    emitter.instruction("mov x2, #0");                                          // slot index = 0
    emitter.instruction("str x2, [sp, #16]");                                   // save the slot index
    emitter.label("__rt_sgf_loop");
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the slot index
    emitter.instruction("cmp x2, #128");                                         // checked all 128 slots?
    emitter.instruction("b.ge __rt_sgf_done");                                  // yes → return the array
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the registry base
    emitter.instruction("add x3, x1, x2, lsl #5");                               // slot base = registry + index * 32
    emitter.instruction("ldr x4, [x3]");                                         // load the filter-name pointer
    emitter.instruction("cbz x4, __rt_sgf_next");                               // null → empty slot, skip
    emitter.instruction("ldr x5, [x3, #8]");                                     // load the filter-name length
    // -- append this user filter name to the array --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the array pointer
    emitter.instruction("mov x1, x4");                                          // filter-name pointer
    emitter.instruction("mov x2, x5");                                          // filter-name length
    emitter.instruction("bl __rt_array_push_str");                             // append the name; x0 = possibly grown array
    emitter.instruction("str x0, [sp, #0]");                                    // save the updated array pointer
    emitter.label("__rt_sgf_next");
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the slot index
    emitter.instruction("add x2, x2, #1");                                      // advance to the next slot
    emitter.instruction("str x2, [sp, #16]");                                   // save the updated slot index
    emitter.instruction("b __rt_sgf_loop");                                     // continue scanning

    emitter.label("__rt_sgf_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // load the final array pointer
    emitter.instruction("ldp x29, x30, [sp, #24]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the filter-enumeration frame
    emitter.instruction("ret");                                                 // return the array in x0
}

/// x86_64 Linux variant of `__rt_stream_get_filters`.
fn emit_stream_get_filters_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_get_filters ---");
    emitter.label_global("__rt_stream_get_filters");

    // Frame: rbp-relative, [-8]=array ptr [-16]=registry base [-24]=slot index
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve the filter-enumeration frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                         // save the built-in array pointer

    // -- scan _user_filter_registry for non-null slots --
    abi::emit_symbol_address(emitter, "r10", "_user_filter_registry");           // registry base
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                        // save the registry base
    emitter.instruction("xor ecx, ecx");                                        // slot index = 0
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                        // save the slot index
    emitter.label("__rt_sgf_loop_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                        // reload the slot index
    emitter.instruction("cmp rcx, 128");                                         // checked all 128 slots?
    emitter.instruction("jge __rt_sgf_done_x");                                  // yes → return the array
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                         // reload the registry base
    emitter.instruction("mov r11, rcx");                                         // copy the slot index for scaling
    emitter.instruction("shl r11, 5");                                          // slot offset = index * 32
    emitter.instruction("add r11, r10");                                         // slot base = registry + offset
    emitter.instruction("mov rax, QWORD PTR [r11]");                             // load the filter-name pointer
    emitter.instruction("test rax, rax");                                       // null → empty slot?
    emitter.instruction("jz __rt_sgf_next_x");                                  // skip empty slots
    emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");                         // load the filter-name length
    // -- append this user filter name to the array --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                         // reload the array pointer
    emitter.instruction("mov rsi, rax");                                         // filter-name pointer
    emitter.instruction("call __rt_array_push_str");                            // append the name; rax = possibly grown array
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                         // save the updated array pointer
    emitter.label("__rt_sgf_next_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                        // reload the slot index: rcx is caller-saved and __rt_array_push_str may clobber it
    emitter.instruction("inc rcx");                                             // advance to the next slot
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                        // save the updated slot index
    emitter.instruction("jmp __rt_sgf_loop_x");                                 // continue scanning

    emitter.label("__rt_sgf_done_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                         // load the final array pointer
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return the array in rax
}