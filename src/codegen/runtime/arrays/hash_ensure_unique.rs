use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// hash_ensure_unique: split a shared hash table before mutation.
/// Input:  x0 = candidate hash pointer
/// Output: x0 = unique hash pointer (original or cloned)
pub fn emit_hash_ensure_unique(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_ensure_unique_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_ensure_unique ---");
    emitter.label_global("__rt_hash_ensure_unique");

    // -- null hashes are already trivially unique --
    emitter.instruction("cbz x0, __rt_hash_ensure_unique_done");                // null inputs do not need copy-on-write splitting

    // -- only shared hashes need to be cloned --
    emitter.instruction("ldr w9, [x0, #-12]");                                  // load the current hash refcount from the uniform header
    emitter.instruction("cmp w9, #1");                                          // is there more than one owner of this hash?
    emitter.instruction("b.ls __rt_hash_ensure_unique_done");                   // refcount <= 1 means the hash can be mutated in place

    // -- clone the shared hash and release this mutator's old owner slot --
    emitter.instruction("sub sp, sp, #32");                                     // allocate a small stack frame for the split path
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the shared source hash pointer
    emitter.instruction("bl __rt_hash_clone_shallow");                          // clone the shared hash for this mutating owner
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the previous shared hash pointer
    emitter.instruction("ldr w10, [x9, #-12]");                                 // reload the old shared refcount
    emitter.instruction("sub w10, w10, #1");                                    // drop this mutator's old owner slot from the shared hash
    emitter.instruction("str w10, [x9, #-12]");                                 // persist the decremented refcount on the old shared hash
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate the split-path stack frame

    emitter.label("__rt_hash_ensure_unique_done");
    emitter.instruction("ret");                                                 // return with x0 = a unique hash pointer
}

fn emit_hash_ensure_unique_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_ensure_unique ---");
    emitter.label_global("__rt_hash_ensure_unique");

    emitter.instruction("mov rax, rdi");                                        // default to returning the original associative-array pointer when no copy-on-write split is needed
    emitter.instruction("test rdi, rdi");                                       // null associative-array pointers are already trivially unique
    emitter.instruction("je __rt_hash_ensure_unique_done");                     // return immediately for null inputs without touching heap metadata
    emitter.instruction("mov r10d, DWORD PTR [rdi - 12]");                      // load the current associative-array refcount from the uniform heap header
    emitter.instruction("cmp r10d, 1");                                         // does the associative array have more than one logical owner?
    emitter.instruction("jbe __rt_hash_ensure_unique_done");                    // refcount <= 1 means the associative array can be mutated in place
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving copy-on-write spill space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved shared associative-array pointer
    emitter.instruction("sub rsp, 16");                                         // reserve one aligned spill slot for the shared associative-array pointer across the clone call
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the shared associative-array pointer across the clone helper call
    emitter.instruction("call __rt_hash_clone_shallow");                        // clone the shared associative array so this mutator gets an isolated owner copy
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the previous shared associative-array pointer after the clone helper returns
    emitter.instruction("mov r11d, DWORD PTR [r10 - 12]");                      // reload the shared associative-array refcount before dropping this mutator's owner slot
    emitter.instruction("sub r11d, 1");                                         // remove this mutator from the shared associative-array refcount after cloning
    emitter.instruction("mov DWORD PTR [r10 - 12], r11d");                      // persist the decremented refcount back into the previous shared associative-array header
    emitter.instruction("add rsp, 16");                                         // release the aligned copy-on-write spill slot before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the copy-on-write split
    emitter.label("__rt_hash_ensure_unique_done");
    emitter.instruction("ret");                                                 // return to the caller with rax holding the unique associative-array pointer
}
