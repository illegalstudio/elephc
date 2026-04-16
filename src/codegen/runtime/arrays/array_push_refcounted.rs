use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_push_refcounted: push a borrowed refcounted payload into an array.
/// Input:  x0 = array pointer, x1 = borrowed heap pointer
/// Output: x0 = array pointer (may differ if array was reallocated)
pub fn emit_array_push_refcounted(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_push_refcounted_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_push_refcounted ---");
    emitter.label_global("__rt_array_push_refcounted");

    // -- preserve arguments across incref --
    emitter.instruction("sub sp, sp, #32");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save destination array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save borrowed heap pointer

    // -- split shared destination arrays before they retain/store a new child --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the destination array pointer
    emitter.instruction("bl __rt_array_ensure_unique");                         // split shared destination arrays before mutating storage
    emitter.instruction("str x0, [sp, #0]");                                    // persist the unique destination array pointer

    // -- retain borrowed payload before destination takes ownership --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the borrowed heap pointer after ensure_unique
    emitter.instruction("mov x0, x1");                                          // move borrowed heap pointer into incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain borrowed payload for the destination array

    // -- delegate the actual append to the ordinary push helper --
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore destination array pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // restore retained heap pointer
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the current packed array kind word
    emitter.instruction("ldr x10, [x1, #-8]");                                  // load the child heap kind word
    emitter.instruction("and x10, x10, #0xff");                                 // isolate the child's low-byte heap kind tag
    emitter.instruction("cmp x10, #2");                                         // is the child an indexed array?
    emitter.instruction("b.eq __rt_array_push_refcounted_kind_array");          // encode value_type 4 for nested arrays
    emitter.instruction("cmp x10, #3");                                         // is the child an associative array / hash?
    emitter.instruction("b.eq __rt_array_push_refcounted_kind_hash");           // encode value_type 5 for nested hashes
    emitter.instruction("cmp x10, #4");                                         // is the child an object instance?
    emitter.instruction("b.eq __rt_array_push_refcounted_kind_object");         // encode value_type 6 for nested objects
    emitter.instruction("cmp x10, #5");                                         // is the child a boxed mixed cell?
    emitter.instruction("b.ne __rt_array_push_refcounted_push");                // unexpected/non-refcounted children leave the existing tag unchanged
    emitter.instruction("mov x10, #7");                                         // encode value_type 7 for boxed mixed values
    emitter.instruction("b __rt_array_push_refcounted_kind_store");             // store the packed array value_type tag
    emitter.label("__rt_array_push_refcounted_kind_object");
    emitter.instruction("mov x10, #6");                                         // encode value_type 6 for nested objects
    emitter.instruction("b __rt_array_push_refcounted_kind_store");             // store the packed array value_type tag
    emitter.label("__rt_array_push_refcounted_kind_array");
    emitter.instruction("mov x10, #4");                                         // encode value_type 4 for nested indexed arrays
    emitter.instruction("b __rt_array_push_refcounted_kind_store");             // store the packed array value_type tag
    emitter.label("__rt_array_push_refcounted_kind_hash");
    emitter.instruction("mov x10, #5");                                         // encode value_type 5 for nested associative arrays
    emitter.label("__rt_array_push_refcounted_kind_store");
    emitter.instruction("mov x14, #0x80ff");                                    // preserve the indexed-array kind and the persistent COW flag
    emitter.instruction("and x9, x9, x14");                                     // keep only the persistent indexed-array metadata bits
    emitter.instruction("lsl x10, x10, #8");                                    // move the value_type tag into the packed kind-word byte lane
    emitter.instruction("orr x9, x9, x10");                                     // combine heap kind + array value_type tag
    emitter.instruction("str x9, [x0, #-8]");                                   // persist the packed kind word on the destination array
    emitter.label("__rt_array_push_refcounted_push");
    emitter.instruction("bl __rt_array_push_int");                              // append retained heap pointer into the array

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return array pointer from __rt_array_push_int
}

fn emit_array_push_refcounted_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_push_refcounted ---");
    emitter.label_global("__rt_array_push_refcounted");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving refcounted-append spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved destination array and child pointer
    emitter.instruction("sub rsp, 16");                                         // reserve aligned spill slots for the destination array pointer and retained child pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the destination indexed-array pointer across uniqueness and incref helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the borrowed child pointer across uniqueness and incref helper calls
    emitter.instruction("call __rt_array_ensure_unique");                       // split shared destination arrays before retaining and storing a new child pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // persist the unique destination indexed-array pointer after copy-on-write splitting
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the borrowed child pointer into the x86_64 incref input register
    emitter.instruction("call __rt_incref");                                    // retain the borrowed child pointer so the destination indexed array becomes a real owner
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the unique destination indexed-array pointer before updating its packed value_type metadata
    emitter.instruction("mov r11, QWORD PTR [r10 - 8]");                        // load the current packed indexed-array kind word from the destination heap header
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the retained child pointer before deriving its heap kind tag
    emitter.instruction("mov r9, QWORD PTR [r8 - 8]");                          // load the child heap-kind word so the destination indexed array can record the correct value_type tag
    emitter.instruction("and r9, 0xff");                                        // isolate the child low-byte heap kind tag from the packed uniform header
    emitter.instruction("cmp r9, 2");                                           // is the retained child an indexed array?
    emitter.instruction("je __rt_array_push_refcounted_kind_array");            // encode runtime value_type 4 for nested indexed arrays
    emitter.instruction("cmp r9, 3");                                           // is the retained child an associative array / hash?
    emitter.instruction("je __rt_array_push_refcounted_kind_hash");             // encode runtime value_type 5 for nested hashes
    emitter.instruction("cmp r9, 4");                                           // is the retained child an object instance?
    emitter.instruction("je __rt_array_push_refcounted_kind_object");           // encode runtime value_type 6 for nested objects
    emitter.instruction("cmp r9, 5");                                           // is the retained child a boxed mixed cell?
    emitter.instruction("jne __rt_array_push_refcounted_push");                 // unexpected children keep the existing indexed-array value_type metadata unchanged
    emitter.instruction("mov r9, 7");                                           // encode runtime value_type 7 for boxed mixed payloads
    emitter.instruction("jmp __rt_array_push_refcounted_kind_store");           // store the updated indexed-array packed value_type tag
    emitter.label("__rt_array_push_refcounted_kind_object");
    emitter.instruction("mov r9, 6");                                           // encode runtime value_type 6 for nested objects
    emitter.instruction("jmp __rt_array_push_refcounted_kind_store");           // store the updated indexed-array packed value_type tag
    emitter.label("__rt_array_push_refcounted_kind_array");
    emitter.instruction("mov r9, 4");                                           // encode runtime value_type 4 for nested indexed arrays
    emitter.instruction("jmp __rt_array_push_refcounted_kind_store");           // store the updated indexed-array packed value_type tag
    emitter.label("__rt_array_push_refcounted_kind_hash");
    emitter.instruction("mov r9, 5");                                           // encode runtime value_type 5 for nested associative arrays
    emitter.label("__rt_array_push_refcounted_kind_store");
    emitter.instruction("and r11, 0x80ff");                                     // preserve only the indexed-array kind and persistent copy-on-write bits before rewriting value_type
    emitter.instruction("shl r9, 8");                                           // move the runtime value_type tag into the packed heap-kind byte lane
    emitter.instruction("or r11, r9");                                          // combine the stable indexed-array metadata bits with the new runtime value_type tag
    emitter.instruction("mov QWORD PTR [r10 - 8], r11");                        // persist the updated packed heap-kind word back into the destination indexed-array header
    emitter.label("__rt_array_push_refcounted_push");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the unique destination indexed-array pointer for the ordinary scalar append helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the retained child pointer as the scalar payload being appended
    emitter.instruction("call __rt_array_push_int");                            // append the retained child pointer through the ordinary indexed-array append helper
    emitter.instruction("add rsp, 16");                                         // release the refcounted-append spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the updated indexed array
    emitter.instruction("ret");                                                 // return to the caller with rax holding the updated indexed-array pointer
}
