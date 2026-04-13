use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// mixed_free_deep: free a mixed cell and release its owned child payload.
/// Input: x0 = mixed cell pointer
/// Output: none
pub fn emit_mixed_free_deep(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_free_deep_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_free_deep ---");
    emitter.label_global("__rt_mixed_free_deep");

    emitter.instruction("cbz x0, __rt_mixed_free_deep_done");                   // skip null mixed cells immediately
    emitter.instruction("sub sp, sp, #32");                                     // allocate a small frame to preserve the mixed pointer
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the mixed pointer across child release
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed runtime value_tag
    emitter.instruction("cmp x9, #1");                                          // is the boxed payload a string?
    emitter.instruction("b.eq __rt_mixed_free_deep_string");                    // strings release through heap_free_safe
    emitter.instruction("cmp x9, #4");                                          // does the boxed payload hold a heap-backed child?
    emitter.instruction("b.lo __rt_mixed_free_deep_box");                       // scalars/bools/floats/null need no nested release
    emitter.instruction("cmp x9, #7");                                          // do boxed heap-backed tags stay within the supported range?
    emitter.instruction("b.hi __rt_mixed_free_deep_box");                       // unknown tags are ignored by mixed deep-free
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the boxed heap child pointer
    emitter.instruction("bl __rt_decref_any");                                  // release the boxed child through the uniform dispatcher
    emitter.instruction("b __rt_mixed_free_deep_box");                          // free the mixed cell storage after releasing the child

    emitter.label("__rt_mixed_free_deep_string");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the boxed string pointer
    emitter.instruction("bl __rt_heap_free_safe");                              // release the boxed string payload

    emitter.label("__rt_mixed_free_deep_box");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the mixed pointer after child release
    emitter.instruction("bl __rt_heap_free");                                   // free the mixed cell storage itself
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate the mixed-free frame

    emitter.label("__rt_mixed_free_deep_done");
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_mixed_free_deep_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_free_deep ---");
    emitter.label_global("__rt_mixed_free_deep");

    emitter.instruction("test rax, rax");                                       // skip null mixed cells immediately because they do not own heap storage
    emitter.instruction("jz __rt_mixed_free_deep_done");                        // null mixed values need no release work
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before spilling the mixed pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved mixed pointer
    emitter.instruction("sub rsp, 16");                                         // reserve local storage for the mixed pointer across nested helper calls
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the mixed pointer across any nested child release helper call
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the boxed runtime value tag to decide whether the child owns heap storage
    emitter.instruction("cmp r10, 1");                                          // detect string payloads that need their owned string storage released explicitly
    emitter.instruction("je __rt_mixed_free_deep_string");                      // string payloads release through heap_free_safe before the mixed box storage itself is freed
    emitter.instruction("cmp r10, 4");                                          // does the mixed cell point at a heap-backed child such as array/hash/object/mixed?
    emitter.instruction("jl __rt_mixed_free_deep_box");                         // scalar, bool, float, and null payloads can skip directly to freeing the mixed box storage itself
    emitter.instruction("cmp r10, 7");                                          // do the heap-backed child tags stay within the supported runtime range?
    emitter.instruction("jg __rt_mixed_free_deep_box");                         // unknown tags are ignored by the current x86_64 mixed deep-free helper
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the boxed string pointer from the mixed payload before releasing it
    emitter.instruction("call __rt_decref_any");                                // release the boxed heap-backed child through the uniform x86_64 dispatcher before freeing the mixed box
    emitter.instruction("jmp __rt_mixed_free_deep_box");                        // free the mixed box storage itself after the boxed heap-backed child has been released

    emitter.label("__rt_mixed_free_deep_string");
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the boxed string pointer from the mixed payload before releasing it
    emitter.instruction("call __rt_heap_free_safe");                            // release the boxed string payload when the mixed cell owns a persisted string

    emitter.label("__rt_mixed_free_deep_box");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the mixed pointer after the optional child release helper call
    emitter.instruction("call __rt_heap_free");                                 // release the mixed box storage itself through the shared x86_64 heap wrapper
    emitter.instruction("add rsp, 16");                                         // release the spill slot reserved for the mixed pointer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.label("__rt_mixed_free_deep_done");
    emitter.instruction("ret");                                                 // return to the caller after releasing the mixed box and its optional string child
}
