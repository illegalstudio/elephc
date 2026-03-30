use crate::codegen::emit::Emitter;

/// hash_ensure_unique: split a shared hash table before mutation.
/// Input:  x0 = candidate hash pointer
/// Output: x0 = unique hash pointer (original or cloned)
pub fn emit_hash_ensure_unique(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_ensure_unique ---");
    emitter.label("__rt_hash_ensure_unique");

    // -- null hashes are already trivially unique --
    emitter.instruction("cbz x0, __rt_hash_ensure_unique_done");                  // null inputs do not need copy-on-write splitting

    // -- only shared hashes need to be cloned --
    emitter.instruction("ldr w9, [x0, #-12]");                                   // load the current hash refcount from the uniform header
    emitter.instruction("cmp w9, #1");                                           // is there more than one owner of this hash?
    emitter.instruction("b.ls __rt_hash_ensure_unique_done");                     // refcount <= 1 means the hash can be mutated in place

    // -- clone the shared hash and release this mutator's old owner slot --
    emitter.instruction("sub sp, sp, #32");                                      // allocate a small stack frame for the split path
    emitter.instruction("stp x29, x30, [sp, #16]");                              // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                     // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                     // save the shared source hash pointer
    emitter.instruction("bl __rt_hash_clone_shallow");                           // clone the shared hash for this mutating owner
    emitter.instruction("ldr x9, [sp, #0]");                                     // reload the previous shared hash pointer
    emitter.instruction("ldr w10, [x9, #-12]");                                  // reload the old shared refcount
    emitter.instruction("sub w10, w10, #1");                                     // drop this mutator's old owner slot from the shared hash
    emitter.instruction("str w10, [x9, #-12]");                                  // persist the decremented refcount on the old shared hash
    emitter.instruction("ldp x29, x30, [sp, #16]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                      // deallocate the split-path stack frame

    emitter.label("__rt_hash_ensure_unique_done");
    emitter.instruction("ret");                                                  // return with x0 = a unique hash pointer
}
