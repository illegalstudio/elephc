use crate::codegen::emit::Emitter;

/// hash_clone_shallow: duplicate a hash table for copy-on-write semantics.
/// Keys are re-persisted, string values are re-persisted, refcounted values are
/// retained for the cloned owner, and insertion order is preserved exactly.
/// Input:  x0 = source hash pointer
/// Output: x0 = cloned hash pointer
pub fn emit_hash_clone_shallow(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_clone_shallow ---");
    emitter.label("__rt_hash_clone_shallow");

    // -- set up stack frame and preserve callee-saved registers --
    // Stack layout:
    //   [sp, #0]  = insertion-order iterator cursor
    //   [sp, #8]  = cloned key pointer
    //   [sp, #16] = cloned key length
    //   [sp, #24] = source/cloned value_lo
    //   [sp, #32] = source/cloned value_hi
    //   [sp, #40] = value_tag
    //   [sp, #56] = saved x19/x20
    //   [sp, #72] = saved x21/x22
    //   [sp, #88] = saved x29/x30
    emitter.instruction("sub sp, sp, #112");                                    // allocate 112 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #88]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #88");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #56]");                             // save callee-saved x19/x20
    emitter.instruction("stp x21, x22, [sp, #72]");                             // save callee-saved x21/x22
    emitter.instruction("mov x19, x0");                                         // x19 = source hash pointer

    // -- snapshot source metadata needed for allocation and iteration --
    emitter.instruction("ldr x9, [x19, #8]");                                   // x9 = source capacity
    emitter.instruction("ldr x21, [x19, #16]");                                 // x21 = source runtime value_type tag
    emitter.instruction("ldr x22, [x19, #-8]");                                 // x22 = packed kind word from the source header

    // -- allocate a destination table with the same capacity and value_type --
    emitter.instruction("mov x0, x9");                                          // x0 = cloned table capacity
    emitter.instruction("mov x1, x21");                                         // x1 = cloned table runtime value_type
    emitter.instruction("bl __rt_hash_new");                                    // allocate a fresh destination hash table
    emitter.instruction("mov x20, x0");                                         // x20 = cloned hash pointer
    emitter.instruction("and x22, x22, #0xffff");                               // preserve only the persistent kind/COW bits
    emitter.instruction("str x22, [x20, #-8]");                                 // copy the persistent packed metadata into the clone

    // -- iterate source entries in insertion order and duplicate their owned contents --
    emitter.instruction("str xzr, [sp, #0]");                                   // iterator cursor = 0 (start from header.head)
    emitter.label("__rt_hash_clone_shallow_loop");
    emitter.instruction("mov x0, x19");                                         // x0 = source hash pointer
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = current insertion-order cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // get next source entry in insertion order
    emitter.instruction("cmn x0, #1");                                          // did the iterator signal end-of-walk?
    emitter.instruction("b.eq __rt_hash_clone_shallow_done");                   // yes — the cloned hash is complete
    emitter.instruction("str x0, [sp, #0]");                                    // save the next insertion-order cursor
    emitter.instruction("str x1, [sp, #8]");                                    // save source key pointer before helper calls
    emitter.instruction("str x2, [sp, #16]");                                   // save source key length before helper calls
    emitter.instruction("str x3, [sp, #24]");                                   // save source value_lo before helper calls
    emitter.instruction("str x4, [sp, #32]");                                   // save source value_hi before helper calls
    emitter.instruction("str x5, [sp, #40]");                                   // save source value_tag before helper calls

    // -- duplicate the owned key string for the cloned hash table --
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = source key pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = source key length
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the owned key for the cloned hash
    emitter.instruction("str x1, [sp, #8]");                                    // save cloned key pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save cloned key length

    // -- duplicate or retain the entry value according to this entry's runtime tag --
    emitter.instruction("ldr x5, [sp, #40]");                                   // x5 = source entry value_tag
    emitter.instruction("cmp x5, #1");                                          // is this entry's value a string?
    emitter.instruction("b.eq __rt_hash_clone_shallow_value_str");              // string values need fresh persisted payloads
    emitter.instruction("cmp x5, #4");                                          // is this entry's value an indexed array?
    emitter.instruction("b.eq __rt_hash_clone_shallow_value_ref");              // nested refcounted values need retains
    emitter.instruction("cmp x5, #5");                                          // is this entry's value an associative array?
    emitter.instruction("b.eq __rt_hash_clone_shallow_value_ref");              // nested refcounted values need retains
    emitter.instruction("cmp x5, #6");                                          // is this entry's value an object?
    emitter.instruction("b.eq __rt_hash_clone_shallow_value_ref");              // nested refcounted values need retains
    emitter.instruction("cmp x5, #7");                                          // is this entry's value a boxed mixed cell?
    emitter.instruction("b.eq __rt_hash_clone_shallow_value_ref");              // nested refcounted values need retains
    emitter.instruction("ldr x3, [sp, #24]");                                   // x3 = scalar/float value_lo copied as-is
    emitter.instruction("ldr x4, [sp, #32]");                                   // x4 = scalar/float value_hi copied as-is
    emitter.instruction("ldr x5, [sp, #40]");                                   // x5 = scalar/float/null value_tag copied as-is
    emitter.instruction("b __rt_hash_clone_shallow_insert");                    // scalars are ready to insert immediately

    emitter.label("__rt_hash_clone_shallow_value_str");
    emitter.instruction("ldr x1, [sp, #24]");                                   // x1 = source string value pointer
    emitter.instruction("ldr x2, [sp, #32]");                                   // x2 = source string value length
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the string value for the cloned hash
    emitter.instruction("str x1, [sp, #24]");                                   // save cloned string value pointer
    emitter.instruction("str x2, [sp, #32]");                                   // save cloned string value length
    emitter.instruction("ldr x3, [sp, #24]");                                   // x3 = cloned string value pointer
    emitter.instruction("ldr x4, [sp, #32]");                                   // x4 = cloned string value length
    emitter.instruction("ldr x5, [sp, #40]");                                   // x5 = string value_tag copied as-is
    emitter.instruction("b __rt_hash_clone_shallow_insert");                    // insert the cloned string value

    emitter.label("__rt_hash_clone_shallow_value_ref");
    emitter.instruction("ldr x3, [sp, #24]");                                   // x3 = source refcounted child pointer
    emitter.instruction("mov x0, x3");                                          // move the shared child pointer into the retain helper
    emitter.instruction("bl __rt_incref");                                      // retain the shared child pointer for the cloned hash
    emitter.instruction("ldr x3, [sp, #24]");                                   // reload the retained child pointer after the helper call
    emitter.instruction("mov x4, xzr");                                         // refcounted hash values store only value_lo
    emitter.instruction("ldr x5, [sp, #40]");                                   // x5 = refcounted value_tag copied as-is

    // -- insert the fully owned cloned entry into the destination table --
    emitter.label("__rt_hash_clone_shallow_insert");
    emitter.instruction("mov x0, x20");                                         // x0 = destination hash pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = cloned key pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = cloned key length
    emitter.instruction("bl __rt_hash_insert_owned");                           // insert the cloned owned key/value into the destination table
    emitter.instruction("mov x20, x0");                                         // keep the destination hash pointer current after insertion
    emitter.instruction("b __rt_hash_clone_shallow_loop");                      // continue cloning source entries

    // -- restore callee-saved registers and return the cloned hash --
    emitter.label("__rt_hash_clone_shallow_done");
    emitter.instruction("mov x0, x20");                                         // return the cloned hash pointer
    emitter.instruction("ldp x21, x22, [sp, #72]");                             // restore callee-saved x21/x22
    emitter.instruction("ldp x19, x20, [sp, #56]");                             // restore callee-saved x19/x20
    emitter.instruction("ldp x29, x30, [sp, #88]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // deallocate the stack frame
    emitter.instruction("ret");                                                 // return with x0 = cloned hash pointer
}
