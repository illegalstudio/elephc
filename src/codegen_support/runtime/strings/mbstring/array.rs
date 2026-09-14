//! Purpose:
//! Copies packed bridge string arrays into native indexed arrays of owned Mixed cells.
//!
//! Called from:
//! - The shared mbstring status adapter for array results.
//!
//! Key details:
//! - Inputs are packed bytes, byte length, and element count in the target C argument registers.
//! - Every length is checked before reading; malformed framing releases partial ownership.
//! - The fresh array preserves ordinary COW metadata and owns each copied binary string.

use super::*;

/// Emits the packed string-array materializer for the selected target architecture.
pub(super) fn emit(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => aarch64(emitter),
        Arch::X86_64 => x86_64(emitter),
    }
}

/// Copies a validated packed buffer into an owned array, or returns zero after cleanup.
fn aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_string_array");
    emitter.instruction("sub sp, sp, #64");                                     // reserve framing state and caller linkage
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the array materializer frame
    emitter.instruction("stp x0, x1, [sp]");                                    // preserve the packed cursor and remaining byte count
    emitter.instruction("stp x2, xzr, [sp, #16]");                              // save the expected count and initialize the insertion index
    emitter.instruction("str xzr, [sp, #32]");                                  // initialize partial-array ownership before validation
    emitter.instruction("lsr x9, x1, #3");                                      // compute the maximum count allowed by the length prefixes
    emitter.instruction("cmp x2, x9");                                          // require one complete length prefix for every element
    emitter.instruction("b.hi __rt_mbstring_array_invalid");                    // reject inconsistent counts before allocation
    emitter.instruction("mov x0, x2");                                          // request capacity for exactly the reported element count
    emitter.instruction("mov x1, #8");                                          // store one owned Mixed cell pointer in each element slot
    emitter.instruction("bl __rt_array_new");                                   // allocate an exclusively owned indexed array
    emitter.instruction("ldr x9, [x0, #-8]");                                   // preserve the indexed-array heap and copy-on-write flags
    emitter.instruction("orr x9, x9, #0x700");                                  // select boxed Mixed elements for the neutral array contract
    emitter.instruction("str x9, [x0, #-8]");                                   // publish the correct element ownership metadata
    emitter.instruction("str x0, [sp, #32]");                                   // retain the array owner across element allocations
    emitter.label("__rt_mbstring_array_loop");
    emitter.instruction("ldp x9, x10, [sp, #16]");                              // reload the expected count and next insertion index
    emitter.instruction("cmp x10, x9");                                         // test whether every reported element has been materialized
    emitter.instruction("b.eq __rt_mbstring_array_done");                       // validate the final byte boundary after the last element
    emitter.instruction("ldp x9, x10, [sp]");                                   // reload the packed cursor and remaining byte count
    emitter.instruction("cmp x10, #8");                                         // require a complete little-endian length prefix
    emitter.instruction("b.lo __rt_mbstring_array_invalid");                    // reject truncated framing without reading beyond the buffer
    emitter.instruction("ldr x2, [x9]");                                        // load the next byte length on supported little-endian targets
    emitter.instruction("sub x10, x10, #8");                                    // exclude the length prefix from the remaining byte range
    emitter.instruction("cmp x2, x10");                                         // require the complete element payload to fit in the buffer
    emitter.instruction("b.hi __rt_mbstring_array_invalid");                    // reject a truncated or overflowing element
    emitter.instruction("add x1, x9, #8");                                      // borrow the string bytes after the validated length prefix
    emitter.instruction("add x9, x1, x2");                                      // advance the cursor past the complete string payload
    emitter.instruction("sub x10, x10, x2");                                    // subtract the consumed bytes from the remaining range
    emitter.instruction("stp x9, x10, [sp]");                                   // preserve framing state across runtime allocation
    emitter.instruction("mov x0, #1");                                          // select the runtime string tag for the borrowed payload
    emitter.instruction("bl __rt_mixed_from_value");                            // copy the bytes into a fresh owned string cell
    emitter.instruction("ldr x9, [sp, #32]");                                   // recover the exclusively owned array
    emitter.instruction("ldr x10, [sp, #24]");                                  // recover the next insertion index
    emitter.instruction("add x11, x9, #24");                                    // address the array payload after its fixed header
    emitter.instruction("str x0, [x11, x10, lsl #3]");                          // transfer the fresh cell directly into the array owner
    emitter.instruction("add x10, x10, #1");                                    // advance the initialized element count
    emitter.instruction("str x10, [x9]");                                       // expose only initialized cells to partial cleanup
    emitter.instruction("str x10, [sp, #24]");                                  // save the next insertion index
    emitter.instruction("b __rt_mbstring_array_loop");                          // materialize the next binary string
    emitter.label("__rt_mbstring_array_done");
    emitter.instruction("ldr x9, [sp, #8]");                                    // inspect the remaining packed byte count
    emitter.instruction("cbnz x9, __rt_mbstring_array_invalid");                // reject trailing data inconsistent with the reported count
    emitter.instruction("ldr x0, [sp, #32]");                                   // transfer the complete array to the status adapter
    emitter.instruction("b __rt_mbstring_array_return");                        // restore caller linkage after successful materialization
    emitter.label("__rt_mbstring_array_invalid");
    emitter.instruction("ldr x0, [sp, #32]");                                   // recover any partially initialized array owner
    emitter.instruction("bl __rt_decref_any");                                  // release exactly the cells already transferred into the array
    emitter.instruction("mov x0, #0");                                          // report malformed framing without leaking runtime storage
    emitter.label("__rt_mbstring_array_return");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #64");                                     // release the framing and ownership slots
    emitter.instruction("ret");                                                 // return the owned array pointer or zero for invalid framing
}

/// Copies a validated packed buffer into an owned array, or returns zero after cleanup.
fn x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_string_array");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish an aligned array materializer frame
    emitter.instruction("sub rsp, 48");                                         // reserve framing state and partial-array ownership
    emitter.instruction("mov QWORD PTR [rsp], rdi");                            // preserve the packed byte cursor
    emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                        // preserve the remaining byte count
    emitter.instruction("mov QWORD PTR [rsp + 16], rdx");                       // preserve the expected element count
    emitter.instruction("mov QWORD PTR [rsp + 24], 0");                         // initialize the next insertion index
    emitter.instruction("mov QWORD PTR [rsp + 32], 0");                         // initialize partial-array ownership before validation
    emitter.instruction("mov r10, rsi");                                        // copy the available packed byte count
    emitter.instruction("shr r10, 3");                                          // compute the maximum count allowed by the length prefixes
    emitter.instruction("cmp rdx, r10");                                        // require one complete length prefix for every element
    emitter.instruction("ja __rt_mbstring_array_invalid");                      // reject inconsistent counts before allocation
    emitter.instruction("mov rdi, rdx");                                        // request capacity for exactly the reported element count
    emitter.instruction("mov esi, 8");                                          // store one owned Mixed cell pointer in each element slot
    emitter.instruction("call __rt_array_new");                                 // allocate an exclusively owned indexed array
    emitter.instruction("or QWORD PTR [rax - 8], 0x700");                       // select boxed Mixed elements while preserving heap magic and COW
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // retain the array owner across element allocations
    emitter.label("__rt_mbstring_array_loop");
    emitter.instruction("mov r10, QWORD PTR [rsp + 24]");                       // reload the next insertion index
    emitter.instruction("cmp r10, QWORD PTR [rsp + 16]");                       // test whether every reported element has been materialized
    emitter.instruction("je __rt_mbstring_array_done");                         // validate the final byte boundary after the last element
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // reload the packed cursor
    emitter.instruction("mov r11, QWORD PTR [rsp + 8]");                        // reload the remaining byte count
    emitter.instruction("cmp r11, 8");                                          // require a complete little-endian length prefix
    emitter.instruction("jb __rt_mbstring_array_invalid");                      // reject truncated framing without reading beyond the buffer
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // load the next string byte length
    emitter.instruction("sub r11, 8");                                          // exclude the length prefix from the remaining byte range
    emitter.instruction("cmp rsi, r11");                                        // require the complete element payload to fit in the buffer
    emitter.instruction("ja __rt_mbstring_array_invalid");                      // reject a truncated or overflowing element
    emitter.instruction("lea rdi, [r10 + 8]");                                  // borrow the string bytes after the validated length prefix
    emitter.instruction("lea r10, [rdi + rsi]");                                // advance the cursor past the complete string payload
    emitter.instruction("sub r11, rsi");                                        // subtract the consumed bytes from the remaining range
    emitter.instruction("mov QWORD PTR [rsp], r10");                            // preserve the next cursor across runtime allocation
    emitter.instruction("mov QWORD PTR [rsp + 8], r11");                        // preserve the remaining range across runtime allocation
    emitter.instruction("mov eax, 1");                                          // select the runtime string tag for the borrowed payload
    emitter.instruction("call __rt_mixed_from_value");                          // copy the bytes into a fresh owned string cell
    emitter.instruction("mov r10, QWORD PTR [rsp + 32]");                       // recover the exclusively owned array
    emitter.instruction("mov r11, QWORD PTR [rsp + 24]");                       // recover the next insertion index
    emitter.instruction("mov QWORD PTR [r10 + r11 * 8 + 24], rax");             // transfer the fresh cell directly into the array owner
    emitter.instruction("inc r11");                                             // advance the initialized element count
    emitter.instruction("mov QWORD PTR [r10], r11");                            // expose only initialized cells to partial cleanup
    emitter.instruction("mov QWORD PTR [rsp + 24], r11");                       // save the next insertion index
    emitter.instruction("jmp __rt_mbstring_array_loop");                        // materialize the next binary string
    emitter.label("__rt_mbstring_array_done");
    emitter.instruction("cmp QWORD PTR [rsp + 8], 0");                          // inspect the remaining packed byte count
    emitter.instruction("jne __rt_mbstring_array_invalid");                     // reject trailing data inconsistent with the reported count
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // transfer the complete array to the status adapter
    emitter.instruction("jmp __rt_mbstring_array_return");                      // restore caller linkage after successful materialization
    emitter.label("__rt_mbstring_array_invalid");
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // recover any partially initialized array owner
    emitter.instruction("call __rt_decref_any");                                // release exactly the cells already transferred into the array
    emitter.instruction("xor eax, eax");                                        // report malformed framing without leaking runtime storage
    emitter.label("__rt_mbstring_array_return");
    emitter.instruction("mov rsp, rbp");                                        // release the framing and ownership slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the owned array pointer or zero for invalid framing
}
