use crate::codegen::emit::Emitter;

/// hash_clone_shallow: duplicate a hash table for copy-on-write semantics.
/// Keys are re-persisted, string values are re-persisted, and refcounted values
/// are retained for the cloned owner.
/// Input:  x0 = source hash pointer
/// Output: x0 = cloned hash pointer
pub fn emit_hash_clone_shallow(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_clone_shallow ---");
    emitter.label("__rt_hash_clone_shallow");

    // -- set up stack frame and preserve callee-saved registers --
    emitter.instruction("sub sp, sp, #112");                                    // allocate 112 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #80]");                             // save callee-saved x19/x20
    emitter.instruction("stp x21, x22, [sp, #64]");                             // save callee-saved x21/x22
    emitter.instruction("stp x23, x24, [sp, #48]");                             // save callee-saved x23/x24
    emitter.instruction("stp x25, x26, [sp, #32]");                             // save callee-saved x25/x26
    emitter.instruction("mov x19, x0");                                         // x19 = source hash pointer

    // -- snapshot source metadata needed for allocation and iteration --
    emitter.instruction("ldr x21, [x19, #8]");                                  // x21 = source capacity
    emitter.instruction("ldr x22, [x19, #16]");                                 // x22 = source runtime value_type tag
    emitter.instruction("ldr x25, [x19, #-8]");                                 // x25 = packed kind word from the source header

    // -- allocate a destination table with the same capacity and value_type --
    emitter.instruction("mov x0, x21");                                         // x0 = cloned table capacity
    emitter.instruction("mov x1, x22");                                         // x1 = cloned table runtime value_type
    emitter.instruction("bl __rt_hash_new");                                    // allocate a fresh destination hash table
    emitter.instruction("mov x20, x0");                                         // x20 = cloned hash pointer
    emitter.instruction("and x25, x25, #0xffff");                               // preserve only the persistent kind/COW bits
    emitter.instruction("str x25, [x20, #-8]");                                 // copy the persistent packed metadata into the clone

    // -- iterate occupied source entries and duplicate their owned contents --
    emitter.instruction("mov x23, #0");                                         // x23 = entry index
    emitter.label("__rt_hash_clone_shallow_loop");
    emitter.instruction("cmp x23, x21");                                        // have we scanned every source slot?
    emitter.instruction("b.ge __rt_hash_clone_shallow_done");                   // yes — the cloned hash is complete
    emitter.instruction("mov x10, #40");                                        // x10 = hash entry size in bytes
    emitter.instruction("mul x11, x23, x10");                                   // x11 = entry byte offset for the current index
    emitter.instruction("add x11, x19, x11");                                   // advance from source hash base to the current slot
    emitter.instruction("add x11, x11, #24");                                   // skip the 24-byte hash header to entry storage
    emitter.instruction("ldr x12, [x11]");                                      // x12 = occupied flag for the current source entry
    emitter.instruction("cmp x12, #1");                                         // is the current source entry occupied?
    emitter.instruction("b.ne __rt_hash_clone_shallow_next");                   // skip empty/tombstone entries

    // -- duplicate the owned key string for the cloned hash table --
    emitter.instruction("ldr x1, [x11, #8]");                                   // x1 = source key pointer
    emitter.instruction("ldr x2, [x11, #16]");                                  // x2 = source key length
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the owned key for the cloned hash
    emitter.instruction("mov x24, x1");                                         // x24 = cloned key pointer
    emitter.instruction("mov x25, x2");                                         // x25 = cloned key length

    // -- duplicate or retain the entry value according to the hash value_type --
    emitter.instruction("cmp x22, #1");                                         // is this a hash of strings?
    emitter.instruction("b.eq __rt_hash_clone_shallow_value_str");              // string values need fresh persisted payloads
    emitter.instruction("cmp x22, #4");                                         // is this a hash of indexed arrays?
    emitter.instruction("b.eq __rt_hash_clone_shallow_value_ref");              // nested refcounted values need retains
    emitter.instruction("cmp x22, #5");                                         // is this a hash of associative arrays?
    emitter.instruction("b.eq __rt_hash_clone_shallow_value_ref");              // nested refcounted values need retains
    emitter.instruction("cmp x22, #6");                                         // is this a hash of objects?
    emitter.instruction("b.eq __rt_hash_clone_shallow_value_ref");              // nested refcounted values need retains
    emitter.instruction("ldr x3, [x11, #24]");                                  // x3 = scalar/float value_lo copied as-is
    emitter.instruction("ldr x4, [x11, #32]");                                  // x4 = scalar/float value_hi copied as-is
    emitter.instruction("b __rt_hash_clone_shallow_insert");                    // scalars are ready to insert immediately

    emitter.label("__rt_hash_clone_shallow_value_str");
    emitter.instruction("ldr x1, [x11, #24]");                                  // x1 = source string value pointer
    emitter.instruction("ldr x2, [x11, #32]");                                  // x2 = source string value length
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the string value for the cloned hash
    emitter.instruction("mov x3, x1");                                          // x3 = cloned string value pointer
    emitter.instruction("mov x4, x2");                                          // x4 = cloned string value length
    emitter.instruction("b __rt_hash_clone_shallow_insert");                    // insert the cloned string value

    emitter.label("__rt_hash_clone_shallow_value_ref");
    emitter.instruction("ldr x3, [x11, #24]");                                  // x3 = source refcounted child pointer
    emitter.instruction("mov x0, x3");                                          // move the shared child pointer into the retain helper
    emitter.instruction("bl __rt_incref");                                      // retain the shared child pointer for the cloned hash
    emitter.instruction("mov x4, xzr");                                         // refcounted hash values store only value_lo

    // -- insert the fully owned cloned entry into the destination table --
    emitter.label("__rt_hash_clone_shallow_insert");
    emitter.instruction("mov x0, x20");                                         // x0 = destination hash pointer
    emitter.instruction("mov x1, x24");                                         // x1 = cloned key pointer
    emitter.instruction("mov x2, x25");                                         // x2 = cloned key length
    emitter.instruction("bl __rt_hash_insert_owned");                           // insert the cloned owned key/value into the destination table
    emitter.instruction("mov x20, x0");                                         // keep the destination hash pointer current after insertion

    emitter.label("__rt_hash_clone_shallow_next");
    emitter.instruction("add x23, x23, #1");                                    // advance to the next source slot
    emitter.instruction("b __rt_hash_clone_shallow_loop");                      // continue cloning source entries

    // -- restore callee-saved registers and return the cloned hash --
    emitter.label("__rt_hash_clone_shallow_done");
    emitter.instruction("mov x0, x20");                                         // return the cloned hash pointer
    emitter.instruction("ldp x25, x26, [sp, #32]");                             // restore callee-saved x25/x26
    emitter.instruction("ldp x23, x24, [sp, #48]");                             // restore callee-saved x23/x24
    emitter.instruction("ldp x21, x22, [sp, #64]");                             // restore callee-saved x21/x22
    emitter.instruction("ldp x19, x20, [sp, #80]");                             // restore callee-saved x19/x20
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // deallocate the stack frame
    emitter.instruction("ret");                                                 // return with x0 = cloned hash pointer
}
