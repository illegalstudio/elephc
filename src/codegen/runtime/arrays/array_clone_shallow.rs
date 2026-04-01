use crate::codegen::emit::Emitter;

/// array_clone_shallow: duplicate an indexed array for copy-on-write semantics.
/// Scalar payloads are byte-copied, string payloads are re-persisted, and
/// refcounted child pointers are retained for the cloned owner.
/// Input:  x0 = source array pointer
/// Output: x0 = cloned array pointer
pub fn emit_array_clone_shallow(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_clone_shallow ---");
    emitter.label_global("__rt_array_clone_shallow");

    // -- set up stack frame and preserve callee-saved registers --
    emitter.instruction("sub sp, sp, #96");                                     // allocate 96 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #64]");                             // save callee-saved x19/x20
    emitter.instruction("stp x21, x22, [sp, #48]");                             // save callee-saved x21/x22
    emitter.instruction("stp x23, x24, [sp, #32]");                             // save callee-saved x23/x24
    emitter.instruction("str x0, [sp, #0]");                                    // save the source array pointer

    // -- snapshot source metadata needed for allocation and post-copy fixups --
    emitter.instruction("ldr x19, [x0]");                                       // x19 = source length
    emitter.instruction("ldr x20, [x0, #8]");                                   // x20 = source capacity
    emitter.instruction("ldr x21, [x0, #16]");                                  // x21 = source elem_size
    emitter.instruction("ldr x22, [x0, #-8]");                                  // x22 = packed kind word from the source header

    // -- allocate a destination array with the same capacity/layout --
    emitter.instruction("mov x0, x20");                                         // x0 = cloned array capacity
    emitter.instruction("mov x1, x21");                                         // x1 = cloned array elem_size
    emitter.instruction("bl __rt_array_new");                                   // allocate a fresh destination array
    emitter.instruction("str x0, [sp, #8]");                                    // save the cloned array pointer
    emitter.instruction("mov x20, x0");                                         // keep the cloned array pointer in a callee-saved register
    emitter.instruction("and x22, x22, #0xffff");                               // preserve only persistent kind/value-type/COW bits
    emitter.instruction("str x22, [x20, #-8]");                                 // copy the persistent packed metadata into the clone
    emitter.instruction("str x19, [x20]");                                      // restore the logical length on the cloned array

    // -- byte-copy the payload region so scalars/floats arrive intact --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = source array pointer
    emitter.instruction("add x1, x1, #24");                                     // x1 = source payload base
    emitter.instruction("add x2, x20, #24");                                    // x2 = clone payload base
    emitter.instruction("mul x3, x19, x21");                                    // x3 = payload bytes to copy
    emitter.label("__rt_array_clone_shallow_copy");
    emitter.instruction("cbz x3, __rt_array_clone_shallow_fixup");              // skip the copy loop when the array is empty
    emitter.instruction("ldrb w4, [x1], #1");                                   // load one byte from the source payload
    emitter.instruction("strb w4, [x2], #1");                                   // store one byte into the cloned payload
    emitter.instruction("sub x3, x3, #1");                                      // decrement the remaining byte count
    emitter.instruction("b __rt_array_clone_shallow_copy");                     // continue copying until the payload is exhausted

    // -- repair cloned ownership according to the runtime value_type tag --
    emitter.label("__rt_array_clone_shallow_fixup");
    emitter.instruction("lsr x9, x22, #8");                                     // move the packed array value_type tag into the low bits
    emitter.instruction("and x9, x9, #0x7f");                                   // isolate the value_type without the persistent COW flag
    emitter.instruction("cmp x9, #1");                                          // is this a string array?
    emitter.instruction("b.eq __rt_array_clone_shallow_strings");               // string slots need fresh persisted payloads
    emitter.instruction("cmp x9, #4");                                          // is this an array of indexed arrays?
    emitter.instruction("b.eq __rt_array_clone_shallow_refs");                  // nested refcounted payloads need retains
    emitter.instruction("cmp x9, #5");                                          // is this an array of associative arrays?
    emitter.instruction("b.eq __rt_array_clone_shallow_refs");                  // nested refcounted payloads need retains
    emitter.instruction("cmp x9, #6");                                          // is this an array of objects?
    emitter.instruction("b.eq __rt_array_clone_shallow_refs");                  // nested refcounted payloads need retains
    emitter.instruction("cmp x9, #7");                                          // is this an array of boxed mixed values?
    emitter.instruction("b.eq __rt_array_clone_shallow_refs");                  // boxed mixed payloads also need retains
    emitter.instruction("b __rt_array_clone_shallow_done");                     // scalar payloads are already correct after the byte copy

    // -- string arrays must own their own persisted payloads after the split --
    emitter.label("__rt_array_clone_shallow_strings");
    emitter.instruction("mov x23, #0");                                         // x23 = slot index for string re-persistence
    emitter.label("__rt_array_clone_shallow_strings_loop");
    emitter.instruction("cmp x23, x19");                                        // have we handled every live string slot?
    emitter.instruction("b.ge __rt_array_clone_shallow_done");                  // yes — string fixups are complete
    emitter.instruction("lsl x10, x23, #4");                                    // x10 = slot byte offset for 16-byte string entries
    emitter.instruction("add x10, x20, x10");                                   // advance from clone base to the current slot
    emitter.instruction("add x10, x10, #24");                                   // skip the array header to string storage
    emitter.instruction("ldr x1, [x10]");                                       // x1 = cloned string pointer from the copied slot
    emitter.instruction("ldr x2, [x10, #8]");                                   // x2 = cloned string length from the copied slot
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the immutable string payload for the cloned owner
    emitter.instruction("lsl x10, x23, #4");                                    // recompute the slot byte offset after the helper call
    emitter.instruction("add x10, x20, x10");                                   // advance from clone base to the current slot again
    emitter.instruction("add x10, x10, #24");                                   // skip the array header to string storage again
    emitter.instruction("str x1, [x10]");                                       // install the newly persisted string pointer into the cloned slot
    emitter.instruction("str x2, [x10, #8]");                                   // install the newly persisted string length into the cloned slot
    emitter.instruction("add x23, x23, #1");                                    // advance to the next live string slot
    emitter.instruction("b __rt_array_clone_shallow_strings_loop");             // continue duplicating cloned string payloads

    // -- refcounted arrays share child pointers, so the clone must retain them --
    emitter.label("__rt_array_clone_shallow_refs");
    emitter.instruction("mov x23, #0");                                         // x23 = slot index for child retains
    emitter.instruction("add x24, x20, #24");                                   // x24 = cloned payload base for 8-byte child pointers
    emitter.label("__rt_array_clone_shallow_refs_loop");
    emitter.instruction("cmp x23, x19");                                        // have we visited every live child pointer slot?
    emitter.instruction("b.ge __rt_array_clone_shallow_done");                  // yes — refcounted fixups are complete
    emitter.instruction("ldr x0, [x24, x23, lsl #3]");                          // load the cloned child pointer from the copied payload
    emitter.instruction("bl __rt_incref");                                      // retain the shared child pointer for the cloned array owner
    emitter.instruction("add x23, x23, #1");                                    // advance to the next live child slot
    emitter.instruction("b __rt_array_clone_shallow_refs_loop");                // continue retaining shared child pointers

    // -- restore callee-saved registers and return the cloned array --
    emitter.label("__rt_array_clone_shallow_done");
    emitter.instruction("mov x0, x20");                                         // return the cloned array pointer
    emitter.instruction("ldp x23, x24, [sp, #32]");                             // restore callee-saved x23/x24
    emitter.instruction("ldp x21, x22, [sp, #48]");                             // restore callee-saved x21/x22
    emitter.instruction("ldp x19, x20, [sp, #64]");                             // restore callee-saved x19/x20
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return with x0 = cloned array pointer
}
