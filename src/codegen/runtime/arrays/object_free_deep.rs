use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// object_free_deep: free an object instance and release all heap-backed properties.
/// Input:  x0 = object pointer
/// Output: none
pub fn emit_object_free_deep(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_object_free_deep_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: object_free_deep ---");
    emitter.label_global("__rt_object_free_deep");

    // -- null and heap-range checks --
    emitter.instruction("cbz x0, __rt_object_free_deep_done");                  // skip null objects
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is the object below the heap buffer?
    emitter.instruction("b.lo __rt_object_free_deep_done");                     // skip non-heap pointers
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // load the current heap offset
    emitter.instruction("add x10, x9, x10");                                    // compute the current heap end
    emitter.instruction("cmp x0, x10");                                         // is the object at or beyond the heap end?
    emitter.instruction("b.hs __rt_object_free_deep_done");                     // skip invalid pointers

    // -- set up stack frame --
    // Stack layout:
    //   [sp, #0]  = object pointer
    //   [sp, #8]  = descriptor pointer
    //   [sp, #16] = property count
    //   [sp, #24] = loop index
    //   [sp, #32] = saved x29
    //   [sp, #40] = saved x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame for object cleanup
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the object pointer
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_gc_release_suppressed");
    emitter.instruction("mov x10, #1");                                         // ordinary deep-free walks suppress nested collector runs
    emitter.instruction("str x10, [x9]");                                       // store release-suppressed = 1 for child cleanup

    // -- derive property count from the object payload size --
    emitter.instruction("ldr w9, [x0, #-16]");                                  // load the object payload size from the heap header
    emitter.instruction("sub x9, x9, #8");                                      // subtract the leading class_id field
    emitter.instruction("lsr x9, x9, #4");                                      // divide by 16 to get the number of property slots
    emitter.instruction("str x9, [sp, #16]");                                   // save the property count for the cleanup loop

    // -- resolve the per-class property tag descriptor --
    emitter.instruction("ldr x10, [x0]");                                       // load the runtime class_id from the object payload
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_class_gc_desc_count");
    emitter.instruction("ldr x11, [x11]");                                      // load the number of emitted class descriptors
    emitter.instruction("cmp x10, x11");                                        // is class_id within the descriptor table?
    emitter.instruction("b.hs __rt_object_free_deep_struct");                   // invalid class ids fall back to a shallow free
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_class_gc_desc_ptrs");
    emitter.instruction("lsl x12, x10, #3");                                    // scale class_id by 8 bytes per descriptor pointer
    emitter.instruction("ldr x11, [x11, x12]");                                 // load the tag descriptor pointer for this class
    emitter.instruction("str x11, [sp, #8]");                                   // save descriptor pointer for the cleanup loop
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize property index = 0

    // -- walk each property and release heap-backed values based on the descriptor tags --
    emitter.label("__rt_object_free_deep_loop");
    emitter.instruction("ldr x12, [sp, #24]");                                  // reload the current property index
    emitter.instruction("ldr x13, [sp, #16]");                                  // reload the total property count
    emitter.instruction("cmp x12, x13");                                        // have we visited every property slot?
    emitter.instruction("b.ge __rt_object_free_deep_struct");                   // finish once every property has been scanned

    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the object pointer
    emitter.instruction("mov x10, #16");                                        // each property slot occupies 16 bytes
    emitter.instruction("mul x10, x12, x10");                                   // compute the property slot byte offset
    emitter.instruction("add x10, x10, #8");                                    // skip the leading class_id field
    emitter.instruction("ldr x14, [x9, x10]");                                  // load the property payload pointer / low word
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the descriptor pointer for this property slot
    emitter.instruction("ldrb w15, [x11, x12]");                                // load the compile-time property tag
    emitter.instruction("cmp x15, #1");                                         // is this a compile-time string property?
    emitter.instruction("b.eq __rt_object_free_deep_release_runtime");          // strings always release through the uniform helper
    emitter.instruction("cmp x15, #4");                                         // is this a compile-time indexed-array property?
    emitter.instruction("b.eq __rt_object_free_deep_release_runtime");          // arrays always release through the uniform helper
    emitter.instruction("cmp x15, #5");                                         // is this a compile-time associative-array property?
    emitter.instruction("b.eq __rt_object_free_deep_release_runtime");          // hashes always release through the uniform helper
    emitter.instruction("cmp x15, #6");                                         // is this a compile-time object property?
    emitter.instruction("b.eq __rt_object_free_deep_release_runtime");          // objects always release through the uniform helper
    emitter.instruction("cmp x15, #7");                                         // is this a compile-time mixed property?
    emitter.instruction("b.eq __rt_object_free_deep_release_runtime");          // mixed payloads may or may not be heap-backed, but decref_any handles both safely
    emitter.instruction("b __rt_object_free_deep_next");                        // scalars and nulls need no cleanup

    emitter.label("__rt_object_free_deep_release_runtime");
    emitter.instruction("mov x0, x14");                                         // move the property payload pointer into the uniform release helper arg reg
    emitter.instruction("str x12, [sp, #24]");                                  // preserve the property index across the helper call
    emitter.instruction("bl __rt_decref_any");                                  // release the heap-backed property payload if needed
    emitter.instruction("ldr x12, [sp, #24]");                                  // restore the property index after the helper call

    emitter.label("__rt_object_free_deep_next");
    emitter.instruction("add x12, x12, #1");                                    // advance to the next property slot
    emitter.instruction("str x12, [sp, #24]");                                  // save the updated property index
    emitter.instruction("b __rt_object_free_deep_loop");                        // continue scanning property slots

    // -- free the object storage itself --
    emitter.label("__rt_object_free_deep_struct");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_gc_release_suppressed");
    emitter.instruction("str xzr, [x9]");                                       // clear release suppression before freeing the object storage
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer before freeing it
    emitter.instruction("bl __rt_heap_free");                                   // return the object storage to the heap allocator
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // tear down the object cleanup stack frame

    emitter.label("__rt_object_free_deep_done");
    emitter.instruction("ret");                                                 // return to the caller
}

fn emit_object_free_deep_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object_free_deep ---");
    emitter.label_global("__rt_object_free_deep");

    emitter.instruction("test rax, rax");                                       // skip null object pointers immediately because they do not own heap storage
    emitter.instruction("jz __rt_object_free_deep_done");                       // null objects need no deep-free work
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the stamped x86_64 heap kind word from the uniform header
    emitter.instruction("mov r11, r10");                                        // preserve the full heap kind word before isolating the ownership marker and heap kind
    emitter.instruction("shr r11, 32");                                         // isolate the high-word heap marker used by the x86_64 heap wrapper
    emitter.instruction(&format!("cmp r11d, 0x{:x}", X86_64_HEAP_MAGIC_HI32));  // ignore foreign pointers that do not carry the elephc x86_64 heap marker
    emitter.instruction("jne __rt_object_free_deep_done");                      // only elephc-owned objects participate in x86_64 deep-free bookkeeping
    emitter.instruction("and r10, 0xff");                                       // isolate the low-byte uniform heap kind tag for a final ownership sanity check
    emitter.instruction("cmp r10, 4");                                          // is this heap-backed payload really an object instance?
    emitter.instruction("jne __rt_object_free_deep_done");                      // other heap kinds must not be released through the object deep-free helper
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving object deep-free spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved object pointer, descriptor pointer, count, and loop index
    emitter.instruction("sub rsp, 32");                                         // reserve local storage for the object pointer, descriptor pointer, property count, and loop index
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the object pointer across nested helper calls while releasing properties
    emitter.instruction("mov r10d, DWORD PTR [rax - 16]");                      // load the object payload size from the uniform heap header
    emitter.instruction("sub r10, 8");                                          // subtract the leading class_id field from the payload size to isolate property storage
    emitter.instruction("shr r10, 4");                                          // divide by 16 because every property slot occupies two qwords
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the total property count for the deep-free loop
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the runtime class id from the object payload
    emitter.instruction("cmp r10, QWORD PTR [rip + _class_gc_desc_count]");     // is the runtime class id within the emitted descriptor table?
    emitter.instruction("jae __rt_object_free_deep_struct");                    // invalid class ids fall back to a shallow object free on x86_64
    emitter.instruction("lea r11, [rip + _class_gc_desc_ptrs]");                // materialize the base address of the class property-tag descriptor table
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // load the property-tag descriptor pointer for this object class
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the descriptor pointer for the object-property cleanup loop
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the object-property loop index to zero

    emitter.label("__rt_object_free_deep_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current object-property index at the top of every loop iteration
    emitter.instruction("cmp r10, QWORD PTR [rbp - 24]");                       // have we already scanned every property slot owned by this object?
    emitter.instruction("jae __rt_object_free_deep_struct");                    // finish once the property index reaches the saved property count
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the object pointer after any nested helper call
    emitter.instruction("mov rcx, r10");                                        // copy the current property index before scaling it into a byte offset
    emitter.instruction("shl rcx, 4");                                          // convert the property index into a 16-byte property-slot offset
    emitter.instruction("add rcx, 8");                                          // skip the leading class_id field to land on the low word of the property slot
    emitter.instruction("mov rax, QWORD PTR [r11 + rcx]");                      // load the low word of the current property slot as the potential heap-backed child pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the per-class property-tag descriptor pointer after any nested helper call
    emitter.instruction("movzx r8, BYTE PTR [r11 + r10]");                      // load the compile-time property tag for the current property slot
    emitter.instruction("cmp r8, 1");                                           // does the property hold a persisted string pointer?
    emitter.instruction("je __rt_object_free_deep_release_runtime");            // strings release through the uniform x86_64 decref_any helper
    emitter.instruction("cmp r8, 4");                                           // does the property hold a nested indexed-array pointer?
    emitter.instruction("je __rt_object_free_deep_release_runtime");            // indexed arrays release through the uniform x86_64 decref_any helper
    emitter.instruction("cmp r8, 5");                                           // does the property hold a nested associative-array pointer?
    emitter.instruction("je __rt_object_free_deep_release_runtime");            // associative arrays release through the uniform x86_64 decref_any helper
    emitter.instruction("cmp r8, 6");                                           // does the property hold a nested object pointer?
    emitter.instruction("je __rt_object_free_deep_release_runtime");            // objects release through the uniform x86_64 decref_any helper
    emitter.instruction("cmp r8, 7");                                           // does the property hold a boxed mixed pointer?
    emitter.instruction("je __rt_object_free_deep_release_runtime");            // mixed cells release through the uniform x86_64 decref_any helper
    emitter.instruction("jmp __rt_object_free_deep_next");                      // scalar, float, and null property slots need no heap cleanup

    emitter.label("__rt_object_free_deep_release_runtime");
    emitter.instruction("call __rt_decref_any");                                // release the heap-backed property payload if the current property slot owns one

    emitter.label("__rt_object_free_deep_next");
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // advance the property index to the next slot in the object layout
    emitter.instruction("jmp __rt_object_free_deep_loop");                      // continue scanning property slots until the whole object payload is released

    emitter.label("__rt_object_free_deep_struct");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the object pointer after finishing the optional property cleanup pass
    emitter.instruction("call __rt_heap_free");                                 // release the object storage itself through the x86_64 heap wrapper
    emitter.instruction("add rsp, 32");                                         // release the spill slots reserved for the object deep-free scan state
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to generated code

    emitter.label("__rt_object_free_deep_done");
    emitter.instruction("ret");                                                 // return to the caller after releasing the object and any owned heap-backed properties
}
