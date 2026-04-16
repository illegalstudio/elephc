use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// hash_clone_shallow: duplicate a hash table for copy-on-write semantics.
/// Keys are re-persisted, string values are re-persisted, refcounted values are
/// retained for the cloned owner, and insertion order is preserved exactly.
/// Input:  x0 = source hash pointer
/// Output: x0 = cloned hash pointer
pub fn emit_hash_clone_shallow(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_clone_shallow_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_clone_shallow ---");
    emitter.label_global("__rt_hash_clone_shallow");

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

    // -- share the key string with the cloned hash via refcount bump --
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = source key pointer for incref
    emitter.instruction("bl __rt_incref");                                      // retain shared key for the cloned hash

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

fn emit_hash_clone_shallow_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_clone_shallow ---");
    emitter.label_global("__rt_hash_clone_shallow");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving clone-state spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved source hash, clone hash, and iterator state
    emitter.instruction("sub rsp, 128");                                        // reserve aligned spill space plus callee-saved register slots for the shallow-clone walk
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source associative-array pointer across allocation and iteration helper calls
    emitter.instruction("mov QWORD PTR [rbp - 72], r12");                       // preserve r12 because the clone walk uses it as the long-lived source hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 80], r13");                       // preserve r13 because the clone walk uses it as the long-lived destination hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 88], r14");                       // preserve r14 because the clone walk uses it to keep the packed heap metadata stable across calls
    emitter.instruction("mov QWORD PTR [rbp - 96], r15");                       // preserve r15 because the clone walk reuses it as a helper scratch across nested calls
    emitter.instruction("mov r12, QWORD PTR [rbp - 8]");                        // keep the source associative-array pointer in a callee-saved register across the whole clone walk
    emitter.instruction("mov r14, QWORD PTR [r12 - 8]");                        // snapshot the packed heap-kind metadata so the clone preserves the stable kind and copy-on-write bits
    emitter.instruction("mov rdi, QWORD PTR [r12 + 8]");                        // pass the source hash capacity to the allocator helper in the first SysV integer argument register
    emitter.instruction("mov rsi, QWORD PTR [r12 + 16]");                       // pass the source hash runtime value_type tag to the allocator helper in the second SysV integer argument register
    emitter.instruction("call __rt_hash_new");                                  // allocate a fresh destination associative-array with the same capacity and table-wide value tag
    emitter.instruction("mov r13, rax");                                        // keep the destination associative-array pointer in a callee-saved register across the clone walk
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the destination associative-array pointer in the local spill area for the return path
    emitter.instruction("mov r15, QWORD PTR [r13 - 8]");                        // snapshot the freshly allocated clone header so the x86_64 heap marker survives the metadata rewrite
    emitter.instruction("and r15, -65536");                                     // keep the high x86_64 heap-marker bits while clearing the low container-kind payload lane
    emitter.instruction("and r14, 0xffff");                                     // preserve only the stable associative-array kind and copy-on-write metadata bits from the source header
    emitter.instruction("or r15, r14");                                         // combine the fresh clone header marker bits with the stable source container-kind payload bits
    emitter.instruction("mov QWORD PTR [r13 - 8], r15");                        // stamp the cloned associative-array with the preserved container metadata without losing the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the insertion-order iterator cursor so the clone walk starts from the source hash head

    emitter.label("__rt_hash_clone_shallow_loop");
    emitter.instruction("mov rdi, r12");                                        // pass the source associative-array pointer to the insertion-order iterator helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the saved insertion-order cursor so iteration resumes from the previous returned slot
    emitter.instruction("call __rt_hash_iter_next");                            // fetch the next source entry in insertion order together with its key/value payload tuple
    emitter.instruction("cmp rax, -1");                                         // did the iterator report that no more entries remain in the source hash?
    emitter.instruction("je __rt_hash_clone_shallow_done");                     // finish once every source entry has been duplicated into the destination hash
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the next insertion-order cursor for the subsequent iteration step
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // preserve the current source key pointer across nested value-ownership helper calls
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // preserve the current source key length across nested value-ownership helper calls
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // preserve the current source value_lo across nested value-ownership helper calls
    emitter.instruction("mov QWORD PTR [rbp - 56], r8");                        // preserve the current source value_hi across nested value-ownership helper calls
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // preserve the current source runtime value_tag across nested value-ownership helper calls
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // load the shared source key pointer so the cloned associative array can retain it for owned insertion
    emitter.instruction("call __rt_incref");                                    // retain the shared source key payload for the cloned associative-array owner instead of allocating a new key copy
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // load the current source runtime value_tag before deciding how the cloned owner should retain it
    emitter.instruction("cmp r10, 1");                                          // is the current source entry value a string payload that needs a fresh owned copy?
    emitter.instruction("je __rt_hash_clone_shallow_value_str");                // duplicate string payloads so the cloned associative array owns independent string storage
    emitter.instruction("cmp r10, 4");                                          // is the current source entry value an indexed-array child pointer that needs a retain?
    emitter.instruction("je __rt_hash_clone_shallow_value_ref");                // retain nested refcounted child pointers for the cloned associative-array owner
    emitter.instruction("cmp r10, 5");                                          // is the current source entry value an associative-array child pointer that needs a retain?
    emitter.instruction("je __rt_hash_clone_shallow_value_ref");                // retain nested refcounted child pointers for the cloned associative-array owner
    emitter.instruction("cmp r10, 6");                                          // is the current source entry value an object child pointer that needs a retain?
    emitter.instruction("je __rt_hash_clone_shallow_value_ref");                // retain nested refcounted child pointers for the cloned associative-array owner
    emitter.instruction("cmp r10, 7");                                          // is the current source entry value a boxed mixed child pointer that needs a retain?
    emitter.instruction("je __rt_hash_clone_shallow_value_ref");                // retain nested refcounted child pointers for the cloned associative-array owner
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the scalar or float low payload word that can be forwarded into the destination hash unchanged
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // reload the scalar or float high payload word that can be forwarded into the destination hash unchanged
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload the scalar or float runtime value_tag that can be forwarded into the destination hash unchanged
    emitter.instruction("jmp __rt_hash_clone_shallow_insert");                  // insert the already-owned scalar payload into the destination associative-array

    emitter.label("__rt_hash_clone_shallow_value_str");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // move the source string value pointer into the x86_64 string helper input register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // move the source string value length into the paired x86_64 string helper register
    emitter.instruction("call __rt_str_persist");                               // duplicate the source string payload so the cloned associative array owns an independent string copy
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the cloned string pointer so the destination insert sees owned storage
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // save the cloned string length so the destination insert sees the preserved payload size
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the cloned string pointer into the hash-set value_lo register
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // reload the cloned string length into the hash-set value_hi register
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload the string runtime value_tag into the hash-set value_tag register
    emitter.instruction("jmp __rt_hash_clone_shallow_insert");                  // insert the freshly duplicated string payload into the destination associative-array

    emitter.label("__rt_hash_clone_shallow_value_ref");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // load the shared refcounted child pointer that the cloned associative array must retain
    emitter.instruction("call __rt_incref");                                    // retain the shared child pointer for the cloned associative-array owner
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the retained child pointer into the hash-set value_lo register
    emitter.instruction("xor r8d, r8d");                                        // clear value_hi because refcounted associative-array payloads only occupy the low word
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload the refcounted runtime value_tag into the hash-set value_tag register

    emitter.label("__rt_hash_clone_shallow_insert");
    emitter.instruction("mov rdi, r13");                                        // pass the destination associative-array pointer to the hash insert helper in the first SysV argument register
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // pass the retained key pointer that already belongs to the cloned associative-array owner
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // pass the retained key length that already belongs to the cloned associative-array owner
    emitter.instruction("call __rt_hash_insert_owned");                         // insert the cloned owned key/value pair without allocating a second persisted key copy
    emitter.instruction("mov r13, rax");                                        // keep the destination associative-array pointer current after the owned insert helper returns
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the latest destination associative-array pointer for the final return path
    emitter.instruction("jmp __rt_hash_clone_shallow_loop");                    // continue duplicating source entries until the insertion-order walk is exhausted

    emitter.label("__rt_hash_clone_shallow_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the cloned associative-array pointer in the x86_64 integer result register
    emitter.instruction("mov r15, QWORD PTR [rbp - 96]");                       // restore r15 after using it as a nested-call scratch in the clone walk
    emitter.instruction("mov r14, QWORD PTR [rbp - 88]");                       // restore r14 after using it to preserve packed associative-array metadata across helper calls
    emitter.instruction("mov r13, QWORD PTR [rbp - 80]");                       // restore r13 after using it as the long-lived destination associative-array pointer
    emitter.instruction("mov r12, QWORD PTR [rbp - 72]");                       // restore r12 after using it as the long-lived source associative-array pointer
    emitter.instruction("add rsp, 128");                                        // release the clone-state spill area before returning to the caller
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the cloned associative-array pointer
    emitter.instruction("ret");                                                 // return to the caller with rax holding the cloned associative-array pointer
}
