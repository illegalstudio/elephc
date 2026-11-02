//! Purpose:
//! Emits the `__rt_array_unbox_slots` runtime helper: converts an indexed array whose slots
//! are boxed Mixed cells into a fresh indexed array of raw 8-byte payload slots of one kind.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//! - The by-reference Mixed writeback (`codegen::lower_inst::reference_arguments`), when a
//!   callee replaced a typed `array<T>` argument with an array built from Mixed values.
//!
//! Key details:
//! - Every element must unbox to the expected runtime tag; on the first mismatch the helper
//!   returns 0 and the caller raises the diagnostic (the partial copy is abandoned to the exit).
//! - Refcounted payloads (indexed arrays, hashes, objects) are retained by the new array; the
//!   source array keeps its own boxed references, so the caller still owns and releases it.
//! - Scalars and heap pointers fill 8-byte slots; strings (tag 1) fill 16-byte `(ptr, len)`
//!   slots holding an owned copy made by `__rt_str_persist`, the layout `__rt_array_push_str`
//!   produces.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// array_unbox_slots: unbox every Mixed slot of an indexed array into raw payload slots.
/// Input:  x0 = source indexed array (value_type 7), x1 = expected runtime element tag
/// Output: x0 = new owned indexed array with raw slots of that tag, or 0 on a tag mismatch
pub fn emit_array_unbox_slots(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_unbox_slots_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_unbox_slots ---");
    emitter.label_global("__rt_array_unbox_slots");

    // -- frame: [0] source, [8] expected tag, [16] length, [24] destination, [32] index, [40] payload --
    emitter.instruction("sub sp, sp, #64");                                     // allocate the helper's spill frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp]");                                        // remember the boxed-slot source array
    emitter.instruction("str x1, [sp, #8]");                                    // remember the expected element tag
    emitter.instruction("ldr x0, [x0]");                                        // load the source element count
    emitter.instruction("str x0, [sp, #16]");                                   // remember the element count for the loop bound
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the expected element tag
    emitter.instruction("cmp x9, #1");                                          // do the elements become strings?
    emitter.instruction("mov x1, #8");                                          // scalar and heap payload slots are one machine word wide
    emitter.instruction("mov x10, #16");                                        // string slots hold a (pointer, length) pair
    emitter.instruction("csel x1, x10, x1, eq");                                // pick the slot width for the expected tag
    emitter.instruction("bl __rt_array_new");                                   // allocate the destination with capacity = count
    emitter.instruction("str x0, [sp, #24]");                                   // remember the destination array

    // -- stamp the destination's value_type with the expected element tag --
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the expected element tag
    emitter.instruction("lsl x9, x9, #8");                                      // move the tag into the kind word's value_type lane
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the fresh array's packed kind word
    emitter.instruction("mov x11, #0x7f00");                                    // mask of the value_type lane
    emitter.instruction("bic x10, x10, x11");                                   // clear the default value_type, keeping heap marker and flags
    emitter.instruction("orr x10, x10, x9");                                    // stamp the expected element tag
    emitter.instruction("str x10, [x0, #-8]");                                  // publish the destination's packed kind word
    emitter.instruction("str xzr, [sp, #32]");                                  // start at element 0

    emitter.label("__rt_array_unbox_slots_loop");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the element index
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the element count
    emitter.instruction("cmp x9, x10");                                         // has every element been converted?
    emitter.instruction("b.ge __rt_array_unbox_slots_done");                    // yes: return the destination
    emitter.instruction("ldr x10, [sp]");                                       // reload the source array
    emitter.instruction("add x10, x10, #24");                                   // skip the 24-byte array header to the slots
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                           // load this element's boxed Mixed cell
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0 = element tag, x1 = payload word
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the expected element tag
    emitter.instruction("cmp x0, x9");                                          // does the element have the expected kind?
    emitter.instruction("b.ne __rt_array_unbox_slots_mismatch");                // no: report the mismatch to the caller
    emitter.instruction("cmp x0, #1");                                          // is the element a string?
    emitter.instruction("b.eq __rt_array_unbox_slots_string");                  // strings take the owned-copy path
    emitter.instruction("str x1, [sp, #40]");                                   // remember the payload across the retain
    emitter.instruction("cmp x0, #4");                                          // tags below 4 are unmanaged scalars
    emitter.instruction("b.lt __rt_array_unbox_slots_store");                   // scalars need no retain
    emitter.instruction("cmp x0, #6");                                          // tags 4..6 are arrays, hashes, and objects
    emitter.instruction("b.gt __rt_array_unbox_slots_store");                   // anything above is not retained here
    emitter.instruction("mov x0, x1");                                          // pass the heap payload to the retain helper
    emitter.instruction("bl __rt_incref");                                      // the destination owns its own reference

    emitter.label("__rt_array_unbox_slots_store");
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the payload word
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the element index
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the destination array
    emitter.instruction("add x11, x10, #24");                                   // skip the destination header to its slots
    emitter.instruction("str x1, [x11, x9, lsl #3]");                           // store the raw payload into its slot
    emitter.instruction("add x9, x9, #1");                                      // advance to the next element
    emitter.instruction("str x9, [sp, #32]");                                   // persist the advanced index
    emitter.instruction("str x9, [x10]");                                       // grow the destination length to cover the slot
    emitter.instruction("b __rt_array_unbox_slots_loop");                       // convert the next element

    emitter.label("__rt_array_unbox_slots_string");
    emitter.instruction("bl __rt_str_persist");                                 // copy the string payload into owned storage, x1 = ptr, x2 = len
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the element index
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the destination array
    emitter.instruction("add x11, x10, #24");                                   // skip the destination header to its slots
    emitter.instruction("add x11, x11, x9, lsl #4");                            // address this element's 16-byte slot
    emitter.instruction("stp x1, x2, [x11]");                                   // store the owned string pointer and length
    emitter.instruction("add x9, x9, #1");                                      // advance to the next element
    emitter.instruction("str x9, [sp, #32]");                                   // persist the advanced index
    emitter.instruction("str x9, [x10]");                                       // grow the destination length to cover the slot
    emitter.instruction("b __rt_array_unbox_slots_loop");                       // convert the next element

    emitter.label("__rt_array_unbox_slots_mismatch");
    emitter.instruction("str xzr, [sp, #24]");                                  // report failure with a null destination

    emitter.label("__rt_array_unbox_slots_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // return the destination (or 0)
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the spill frame
    emitter.instruction("ret");                                                 // return to the writeback
}

/// x86_64 twin of [`emit_array_unbox_slots`].
/// Input:  rax = source indexed array, rdi = expected runtime element tag
/// Output: rax = new owned indexed array, or 0 on a tag mismatch
fn emit_array_unbox_slots_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_unbox_slots ---");
    emitter.label_global("__rt_array_unbox_slots");

    // -- frame: [rbp-8] source, [-16] tag, [-24] length, [-32] destination, [-40] index, [-48] payload --
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // set up the helper frame
    emitter.instruction("sub rsp, 48");                                         // reserve the aligned spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // remember the boxed-slot source array
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // remember the expected element tag
    emitter.instruction("mov rdi, QWORD PTR [rax]");                            // pass the source element count as capacity
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // remember the element count for the loop bound
    emitter.instruction("mov rsi, 8");                                          // scalar and heap payload slots are one machine word wide
    emitter.instruction("mov r10, 16");                                         // string slots hold a (pointer, length) pair
    emitter.instruction("cmp QWORD PTR [rbp - 16], 1");                         // do the elements become strings?
    emitter.instruction("cmove rsi, r10");                                      // pick the slot width for the expected tag
    emitter.instruction("call __rt_array_new");                                 // allocate the destination with capacity = count
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // remember the destination array

    // -- stamp the destination's value_type with the expected element tag --
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the expected element tag
    emitter.instruction("shl r10, 8");                                          // move the tag into the kind word's value_type lane
    emitter.instruction("mov r11, QWORD PTR [rax - 8]");                        // load the fresh array's packed kind word
    emitter.instruction("and r11, -32513");                                     // clear the value_type lane (~0x7f00), keeping heap marker and flags
    emitter.instruction("or r11, r10");                                         // stamp the expected element tag
    emitter.instruction("mov QWORD PTR [rax - 8], r11");                        // publish the destination's packed kind word
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // start at element 0

    emitter.label("__rt_array_unbox_slots_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the element index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 24]");                       // has every element been converted?
    emitter.instruction("jge __rt_array_unbox_slots_done");                     // yes: return the destination
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the source array
    emitter.instruction("mov rax, QWORD PTR [r11 + 24 + r10 * 8]");             // load this element's boxed Mixed cell
    emitter.instruction("call __rt_mixed_unbox");                               // rax = element tag, rdi = payload word
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // does the element have the expected kind?
    emitter.instruction("jne __rt_array_unbox_slots_mismatch");                 // no: report the mismatch to the caller
    emitter.instruction("cmp rax, 1");                                          // is the element a string?
    emitter.instruction("je __rt_array_unbox_slots_string");                    // strings take the owned-copy path
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // remember the payload across the retain
    emitter.instruction("cmp rax, 4");                                          // tags below 4 are unmanaged scalars
    emitter.instruction("jl __rt_array_unbox_slots_store");                     // scalars need no retain
    emitter.instruction("cmp rax, 6");                                          // tags 4..6 are arrays, hashes, and objects
    emitter.instruction("jg __rt_array_unbox_slots_store");                     // anything above is not retained here
    emitter.instruction("mov rax, rdi");                                        // pass the heap payload to the retain helper
    emitter.instruction("call __rt_incref");                                    // the destination owns its own reference

    emitter.label("__rt_array_unbox_slots_store");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the payload word
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the element index
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the destination array
    emitter.instruction("mov QWORD PTR [r11 + 24 + r10 * 8], rdi");             // store the raw payload into its slot
    emitter.instruction("add r10, 1");                                          // advance to the next element
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // persist the advanced index
    emitter.instruction("mov QWORD PTR [r11], r10");                            // grow the destination length to cover the slot
    emitter.instruction("jmp __rt_array_unbox_slots_loop");                     // convert the next element

    emitter.label("__rt_array_unbox_slots_string");
    emitter.instruction("mov rax, rdi");                                        // pass the string pointer to the persist helper
    emitter.instruction("call __rt_str_persist");                               // copy the string payload into owned storage, rax = ptr, rdx = len
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the element index
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the destination array
    emitter.instruction("mov rcx, r10");                                        // copy the index to scale it by the slot width
    emitter.instruction("shl rcx, 4");                                          // byte offset of this element's 16-byte slot
    emitter.instruction("mov QWORD PTR [r11 + 24 + rcx], rax");                 // store the owned string pointer
    emitter.instruction("mov QWORD PTR [r11 + 32 + rcx], rdx");                 // store the owned string length
    emitter.instruction("add r10, 1");                                          // advance to the next element
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // persist the advanced index
    emitter.instruction("mov QWORD PTR [r11], r10");                            // grow the destination length to cover the slot
    emitter.instruction("jmp __rt_array_unbox_slots_loop");                     // convert the next element

    emitter.label("__rt_array_unbox_slots_mismatch");
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // report failure with a null destination

    emitter.label("__rt_array_unbox_slots_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the destination (or 0)
    emitter.instruction("leave");                                               // release the helper frame
    emitter.instruction("ret");                                                 // return to the writeback
}
