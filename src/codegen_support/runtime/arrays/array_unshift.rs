//! Purpose:
//! Emits the `__rt_array_unshift`, `__rt_array_unshift_loop` runtime helper assembly for array unshift.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Array helpers operate on runtime array headers and element cells; mutations must respect capacity and COW contracts.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_array_unshift` for the native target.
/// Input: x0 = array pointer, x1 = integer value to prepend
/// Output: x0 = new array length
/// Behavior: shifts all existing elements right by one position, inserts value at index 0,
/// and increments the array length. Respects COW: the caller ensures the array is unique.
pub fn emit_array_unshift(emitter: &mut Emitter) {
    emit_array_rotate_last_string_to_front(emitter);
    if emitter.target.arch == Arch::X86_64 {
        emit_array_unshift_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_unshift ---");
    emitter.label_global("__rt_array_unshift");

    // -- load array metadata --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current array length
    emitter.instruction("add x10, x0, #24");                                    // x10 = base of data region

    // -- shift all elements right by one, starting from the end --
    emitter.instruction("sub x11, x9, #1");                                     // x11 = src_index = length - 1 (last element)

    emitter.label("__rt_array_unshift_loop");
    emitter.instruction("cmp x11, #0");                                         // check if src_index < 0
    emitter.instruction("b.lt __rt_array_unshift_insert");                      // if so, shifting complete
    emitter.instruction("ldr x12, [x10, x11, lsl #3]");                         // x12 = data[src_index]
    emitter.instruction("add x13, x11, #1");                                    // x13 = dst_index = src_index + 1
    emitter.instruction("str x12, [x10, x13, lsl #3]");                         // data[dst_index] = data[src_index]
    emitter.instruction("sub x11, x11, #1");                                    // src_index -= 1
    emitter.instruction("b __rt_array_unshift_loop");                           // continue loop

    // -- insert value at index 0 and update length --
    emitter.label("__rt_array_unshift_insert");
    emitter.instruction("str x1, [x10]");                                       // data[0] = value
    emitter.instruction("add x9, x9, #1");                                      // length += 1
    emitter.instruction("str x9, [x0]");                                        // write updated length to header
    emitter.instruction("mov x0, x9");                                          // return new length
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits `__rt_array_unshift` for the x86_64 Linux ABI.
/// Input: rdi = array pointer, rsi = integer value to prepend
/// Output: rax = new array length
/// Behavior: identical to the ARM64 variant — shifts existing elements right,
/// inserts at index 0, and increments length. Empty arrays skip the shift loop correctly.
fn emit_array_unshift_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_unshift ---");
    emitter.label_global("__rt_array_unshift");

    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the current indexed-array length before shifting every live scalar payload one slot to the right
    emitter.instruction("lea r11, [rdi + 24]");                                 // compute the first scalar payload slot address in the destination indexed array
    emitter.instruction("test r10, r10");                                       // detect the empty-array case before seeding the reverse shift cursor
    emitter.instruction("jz __rt_array_unshift_insert_x86");                    // skip the reverse shift loop entirely when prepending into an empty indexed array
    emitter.instruction("mov rcx, r10");                                        // seed the reverse shift cursor from the current indexed-array length
    emitter.instruction("sub rcx, 1");                                          // move the reverse shift cursor to the last live scalar payload slot

    emitter.label("__rt_array_unshift_loop_x86");
    emitter.instruction("cmp rcx, 0");                                          // check whether every live scalar payload has already been shifted one slot to the right
    emitter.instruction("jl __rt_array_unshift_insert_x86");                    // stop shifting once the reverse cursor has moved before the front of the indexed array
    emitter.instruction("mov r8, QWORD PTR [r11 + rcx * 8]");                   // load the current scalar payload before moving it one slot toward the back of the indexed array
    emitter.instruction("mov QWORD PTR [r11 + rcx * 8 + 8], r8");               // store the shifted scalar payload into the next indexed-array slot
    emitter.instruction("sub rcx, 1");                                          // move the reverse shift cursor toward the front of the indexed array
    emitter.instruction("jmp __rt_array_unshift_loop_x86");                     // continue shifting until every live scalar payload has moved one slot to the right

    emitter.label("__rt_array_unshift_insert_x86");
    emitter.instruction("mov QWORD PTR [r11], rsi");                            // store the prepended scalar payload into the now-free first indexed-array slot
    emitter.instruction("add r10, 1");                                          // increment the indexed-array logical length after prepending one scalar payload
    emitter.instruction("mov QWORD PTR [rdi], r10");                            // persist the incremented indexed-array logical length back into the array header
    emitter.instruction("mov rax, r10");                                        // return the new indexed-array length in the x86_64 integer result register
    emitter.instruction("ret");                                                 // return to the caller after prepending the scalar payload
}

/// Emits `__rt_array_rotate_last_string_to_front`: a string slot is 16 bytes, so `array_unshift()`
/// appends through `__rt_array_push_str`, which owns copy-on-write, persistence and growth, and
/// then moves that last slot to index 0.
/// Input: x0/rdi = indexed string array holding at least one element.
/// A receiver promoted to hash storage has no slots to move and is left as it is.
fn emit_array_rotate_last_string_to_front(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_rotate_last_string_to_front ---");
    emitter.label_global("__rt_array_rotate_last_string_to_front");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [x0, #-8]");                           // load the packed heap kind word
            emitter.instruction("and x9, x9, #0xff");                           // isolate the heap kind
            emitter.instruction("cmp x9, #3");                                  // kind 3 is hash storage, without slots
            emitter.instruction("b.eq __rt_array_rotate_string_done");          // nothing to move in a promoted receiver
            emitter.instruction("ldr x9, [x0]");                                // load the length, the appended slot included
            emitter.instruction("sub x9, x9, #1");                              // index of the appended slot
            emitter.instruction("add x10, x0, #24");                            // first slot of the payload
            emitter.instruction("add x11, x10, x9, lsl #4");                    // address of the appended slot
            emitter.instruction("ldp x12, x13, [x11]");                         // keep the appended string pointer and length
            emitter.label("__rt_array_rotate_string_loop");
            emitter.instruction("cmp x11, x10");                                // reached the first slot?
            emitter.instruction("b.ls __rt_array_rotate_string_store");         // every older slot has moved right
            emitter.instruction("ldp x14, x15, [x11, #-16]");                   // load the previous slot
            emitter.instruction("stp x14, x15, [x11]");                         // move it one slot right
            emitter.instruction("sub x11, x11, #16");                           // step one slot towards the front
            emitter.instruction("b __rt_array_rotate_string_loop");             // continue with the previous slot
            emitter.label("__rt_array_rotate_string_store");
            emitter.instruction("stp x12, x13, [x10]");                         // the appended string becomes element 0
            emitter.label("__rt_array_rotate_string_done");
            emitter.instruction("ret");                                         // return to the caller
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                // load the packed heap kind word
            emitter.instruction("and r10, 0xff");                               // isolate the heap kind
            emitter.instruction("cmp r10, 3");                                  // kind 3 is hash storage, without slots
            emitter.instruction("je __rt_array_rotate_string_done_x86");        // nothing to move in a promoted receiver
            emitter.instruction("mov r10, QWORD PTR [rdi]");                    // load the length, the appended slot included
            emitter.instruction("sub r10, 1");                                  // index of the appended slot
            emitter.instruction("shl r10, 4");                                  // byte offset of the appended slot
            emitter.instruction("lea r11, [rdi + 24]");                         // first slot of the payload
            emitter.instruction("add r10, r11");                                // address of the appended slot
            emitter.instruction("mov rcx, QWORD PTR [r10]");                    // keep the appended string pointer
            emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                // keep the appended string length
            emitter.label("__rt_array_rotate_string_loop_x86");
            emitter.instruction("cmp r10, r11");                                // reached the first slot?
            emitter.instruction("jbe __rt_array_rotate_string_store_x86");      // every older slot has moved right
            emitter.instruction("mov r8, QWORD PTR [r10 - 16]");                // load the previous slot pointer
            emitter.instruction("mov r9, QWORD PTR [r10 - 8]");                 // load the previous slot length
            emitter.instruction("mov QWORD PTR [r10], r8");                     // move the pointer one slot right
            emitter.instruction("mov QWORD PTR [r10 + 8], r9");                 // move the length one slot right
            emitter.instruction("sub r10, 16");                                 // step one slot towards the front
            emitter.instruction("jmp __rt_array_rotate_string_loop_x86");       // continue with the previous slot
            emitter.label("__rt_array_rotate_string_store_x86");
            emitter.instruction("mov QWORD PTR [r11], rcx");                    // the appended string pointer becomes element 0
            emitter.instruction("mov QWORD PTR [r11 + 8], rdx");                // the appended string length becomes element 0
            emitter.label("__rt_array_rotate_string_done_x86");
            emitter.instruction("ret");                                         // return to the caller
        }
    }
}
