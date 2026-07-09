//! Purpose:
//! Emits the `__rt_array_unshift` runtime helper assembly for prepending one 8-byte payload
//! to an indexed array.  The helper now checks capacity before shifting and grows the backing
//! store when full, mirroring the hardened array-push helpers.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Input: x0/rdi = array pointer, x1/rsi = 8-byte payload to prepend.
//! - Output: x0/rax = updated array pointer (possibly reallocated); the new PHP count is read
//!   by the caller from `[array+0]`.
//! - Caller must ensure the array is unique (COW split) before calling.
//! - Capacity is checked against the header and `__rt_array_grow` is invoked before shifting
//!   when length equals capacity.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_array_unshift` for the native target.
/// Input: x0 = array pointer, x1 = 8-byte payload to prepend
/// Output: x0 = updated array pointer (possibly reallocated); new count at [x0+0]
/// Behavior: grows if full, shifts existing elements right, inserts at index 0,
/// increments length. COW is handled by the caller.
pub fn emit_array_unshift(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_unshift_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_unshift ---");
    emitter.label_global("__rt_array_unshift");

    // -- set up stack frame to preserve value and array pointer across optional growth --
    emitter.instruction("sub sp, sp, #32");                                     // allocate a 32-byte stack frame for the payload value and array pointer
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save the caller frame pointer and link register around helper calls
    emitter.instruction("add x29, sp, #16");                                    // establish a fresh frame pointer for this helper
    emitter.instruction("str x1, [sp, #0]");                                    // preserve the 8-byte payload value across a potential __rt_array_grow call
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the array pointer across a potential __rt_array_grow reallocation

    // -- load metadata and grow the array when length equals capacity --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current indexed-array logical length
    emitter.instruction("ldr x10, [x0, #8]");                                   // x10 = current indexed-array capacity in slots
    emitter.instruction("cmp x9, x10");                                         // compare logical length with allocated capacity
    emitter.instruction("b.lt __rt_array_unshift_grow_done");                   // skip growth when at least one free slot remains
    emitter.instruction("bl __rt_array_grow");                                  // double capacity; x0 may become a new array pointer
    emitter.instruction("str x0, [sp, #8]");                                    // update the saved array pointer after potential reallocation

    // -- shift existing payloads one slot to the right, then insert at the front --
    emitter.label("__rt_array_unshift_grow_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the unique array pointer before shifting
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the 8-byte payload value after any growth call
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current length after possible growth
    emitter.instruction("add x10, x0, #24");                                    // x10 = base address of the inline data region

    emitter.instruction("sub x11, x9, #1");                                     // x11 = src_index = last live element
    emitter.label("__rt_array_unshift_loop");
    emitter.instruction("cmp x11, #0");                                         // check whether the reverse cursor has passed the front
    emitter.instruction("b.lt __rt_array_unshift_insert");                      // shifting complete once the cursor is negative
    emitter.instruction("ldr x12, [x10, x11, lsl #3]");                         // x12 = data[src_index] (8-byte payload)
    emitter.instruction("add x13, x11, #1");                                    // x13 = dst_index = src_index + 1
    emitter.instruction("str x12, [x10, x13, lsl #3]");                         // data[dst_index] = data[src_index]
    emitter.instruction("sub x11, x11, #1");                                    // move the reverse cursor toward the front
    emitter.instruction("b __rt_array_unshift_loop");                           // continue shifting until the front is reached

    emitter.label("__rt_array_unshift_insert");
    emitter.instruction("str x1, [x10]");                                       // store the prepended payload into the now-free first slot
    emitter.instruction("add x9, x9, #1");                                      // increment the indexed-array logical length
    emitter.instruction("str x9, [x0]");                                        // publish the updated logical length in the array header

    // -- restore frame and return the (possibly reallocated) array pointer --
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame pointer and link register
    emitter.instruction("add sp, sp, #32");                                     // deallocate the helper stack frame
    emitter.instruction("ret");                                                 // return the updated array pointer in x0
}

/// Emits `__rt_array_unshift` for the x86_64 Linux ABI.
/// Input: rdi = array pointer, rsi = 8-byte payload to prepend
/// Output: rax = updated array pointer (possibly reallocated); new count at [rax+0]
/// Behavior: grows if full, shifts existing elements right, inserts at index 0,
/// increments length. COW is handled by the caller.
fn emit_array_unshift_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_unshift ---");
    emitter.label_global("__rt_array_unshift");

    emitter.instruction("push rbp");                                            // establish a stack frame to preserve the payload value and array pointer
    emitter.instruction("mov rbp, rsp");                                        // set a stable frame base for the unshift spill slots
    emitter.instruction("sub rsp, 16");                                         // reserve aligned spill slots for the array pointer and payload value
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // preserve the 8-byte payload value across a potential __rt_array_grow call
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // preserve the array pointer across a potential __rt_array_grow reallocation

    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the current indexed-array logical length
    emitter.instruction("mov rcx, QWORD PTR [rdi + 8]");                        // load the current indexed-array capacity in slots
    emitter.instruction("cmp r10, rcx");                                        // compare logical length with allocated capacity
    emitter.instruction("jb __rt_array_unshift_shift_x86");                     // skip growth when at least one free slot remains
    emitter.instruction("call __rt_array_grow");                                // allocate a larger backing store; rax may become a new array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // update the saved array pointer after potential reallocation

    emitter.label("__rt_array_unshift_shift_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the array pointer before shifting
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the 8-byte payload value after any growth call
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // reload length after possible growth
    emitter.instruction("lea r11, [rdi + 24]");                                 // compute the first scalar payload slot address

    emitter.instruction("test r10, r10");                                       // detect the empty-array case before seeding the reverse cursor
    emitter.instruction("jz __rt_array_unshift_insert_x86");                    // empty arrays skip the shift loop entirely
    emitter.instruction("mov rcx, r10");                                        // seed the reverse shift cursor from the current length
    emitter.instruction("sub rcx, 1");                                          // move the cursor to the last live scalar payload slot

    emitter.label("__rt_array_unshift_loop_x86");
    emitter.instruction("cmp rcx, 0");                                          // check whether the reverse cursor has passed the front
    emitter.instruction("jl __rt_array_unshift_insert_x86");                    // shifting complete once the cursor is negative
    emitter.instruction("mov r8, QWORD PTR [r11 + rcx * 8]");                   // load the current 8-byte payload
    emitter.instruction("mov QWORD PTR [r11 + rcx * 8 + 8], r8");               // store it one slot toward the back of the indexed array
    emitter.instruction("sub rcx, 1");                                          // move the reverse cursor toward the front
    emitter.instruction("jmp __rt_array_unshift_loop_x86");                     // continue shifting until the front is reached

    emitter.label("__rt_array_unshift_insert_x86");
    emitter.instruction("mov QWORD PTR [r11], rsi");                            // store the prepended payload into the now-free first slot
    emitter.instruction("add r10, 1");                                          // increment the indexed-array logical length
    emitter.instruction("mov QWORD PTR [rdi], r10");                            // publish the updated length in the array header
    emitter.instruction("mov rax, rdi");                                        // return the updated array pointer in the integer result register
    emitter.instruction("add rsp, 16");                                         // release the unshift spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}
