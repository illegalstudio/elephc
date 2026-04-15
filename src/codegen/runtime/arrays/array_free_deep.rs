use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// array_free_deep: free an array and release any owned heap-backed elements.
/// Input:  x0 = array pointer
/// Output: none
pub fn emit_array_free_deep(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_free_deep_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_free_deep ---");
    emitter.label_global("__rt_array_free_deep");

    // -- null check --
    emitter.instruction("cbz x0, __rt_array_free_deep_done");                   // skip if null

    // -- heap range check (same as heap_free_safe) --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // below heap start?
    emitter.instruction("b.lo __rt_array_free_deep_done");                      // not on heap, skip
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // current heap offset
    emitter.instruction("add x10, x9, x10");                                    // heap end = base + offset
    emitter.instruction("cmp x0, x10");                                         // beyond heap end?
    emitter.instruction("b.hs __rt_array_free_deep_done");                      // not on heap, skip

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #32");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save array pointer
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_gc_release_suppressed");
    emitter.instruction("mov x10, #1");                                         // ordinary deep-free walks suppress nested collector runs
    emitter.instruction("str x10, [x9]");                                       // store release-suppressed = 1 for child cleanup

    // -- load the packed runtime value_type tag for this array --
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the full kind word from the heap header
    emitter.instruction("lsr x10, x9, #8");                                     // move the packed array value_type tag into the low bits
    emitter.instruction("and x10, x10, #0x7f");                                 // isolate the packed array value_type tag without the persistent COW flag
    emitter.instruction("cbnz x10, __rt_array_free_deep_have_tag");             // prefer the packed tag when codegen/runtime supplied one
    emitter.instruction("ldr x9, [x0, #16]");                                   // reload elem_size for older/untyped arrays
    emitter.instruction("cmp x9, #16");                                         // does this legacy array store string payloads?
    emitter.instruction("b.ne __rt_array_free_deep_struct");                    // untagged scalar arrays need no per-element cleanup
    emitter.instruction("mov x10, #1");                                         // treat legacy 16-byte arrays as string arrays
    emitter.label("__rt_array_free_deep_have_tag");
    emitter.instruction("cmp x10, #1");                                         // is this a string array?
    emitter.instruction("b.eq __rt_array_free_deep_loop_setup");                // strings release through the uniform helper
    emitter.instruction("cmp x10, #4");                                         // is this an array of indexed arrays?
    emitter.instruction("b.eq __rt_array_free_deep_loop_setup");                // nested indexed arrays need decref_any cleanup
    emitter.instruction("cmp x10, #5");                                         // is this an array of associative arrays?
    emitter.instruction("b.eq __rt_array_free_deep_loop_setup");                // nested hashes need decref_any cleanup
    emitter.instruction("cmp x10, #6");                                         // is this an array of objects / callables?
    emitter.instruction("b.eq __rt_array_free_deep_loop_setup");                // boxed mixed values need decref_any cleanup too
    emitter.instruction("cmp x10, #7");                                         // is this an array of boxed mixed values?
    emitter.instruction("b.ne __rt_array_free_deep_struct");                    // scalar arrays need no per-element cleanup

    // -- free each releasable element --
    emitter.label("__rt_array_free_deep_loop_setup");
    emitter.instruction("ldr x11, [x0]");                                       // x11 = array length
    emitter.instruction("str x11, [sp, #8]");                                   // save length
    emitter.instruction("mov x12, #0");                                         // x12 = loop index

    emitter.label("__rt_array_free_deep_loop");
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload length
    emitter.instruction("cmp x12, x11");                                        // index >= length?
    emitter.instruction("b.ge __rt_array_free_deep_struct");                    // done freeing elements

    // -- load the heap-backed child pointer for this slot --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload array pointer
    emitter.instruction("ldr x10, [x0, #-8]");                                  // reload the full kind word from the heap header
    emitter.instruction("lsr x10, x10, #8");                                    // move the packed array value_type tag into the low bits
    emitter.instruction("and x10, x10, #0x7f");                                 // isolate the packed array value_type tag without the persistent COW flag
    emitter.instruction("cmp x10, #1");                                         // does this array store string payloads?
    emitter.instruction("b.eq __rt_array_free_deep_load_str");                  // string payloads use 16-byte slots
    emitter.instruction("lsl x13, x12, #3");                                    // compute index * 8 for pointer-sized child slots
    emitter.instruction("add x13, x13, #24");                                   // skip the 24-byte array header
    emitter.instruction("ldr x0, [x0, x13]");                                   // load the nested heap pointer from the slot
    emitter.instruction("b __rt_array_free_deep_release");                      // release pointer-sized payload through decref_any
    emitter.label("__rt_array_free_deep_load_str");
    emitter.instruction("lsl x13, x12, #4");                                    // compute index * 16 for string payload slots
    emitter.instruction("add x13, x13, #24");                                   // skip the 24-byte array header
    emitter.instruction("ldr x0, [x0, x13]");                                   // load the persisted string pointer from the slot

    emitter.label("__rt_array_free_deep_release");
    emitter.instruction("str x12, [sp, #8]");                                   // save index (reuse slot, length in x10)
    emitter.instruction("bl __rt_decref_any");                                  // release the heap-backed slot payload if needed

    // -- advance --
    emitter.instruction("ldr x12, [sp, #8]");                                   // restore index
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload array pointer
    emitter.instruction("ldr x11, [x0]");                                       // reload length
    emitter.instruction("str x11, [sp, #8]");                                   // re-save length
    emitter.instruction("add x12, x12, #1");                                    // index += 1
    emitter.instruction("b __rt_array_free_deep_loop");                         // continue

    // -- free the array struct itself --
    emitter.label("__rt_array_free_deep_struct");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_gc_release_suppressed");
    emitter.instruction("str xzr, [x9]");                                       // clear release suppression before freeing the container storage
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload array pointer
    emitter.instruction("bl __rt_heap_free");                                   // free array struct

    // -- restore frame --
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame

    emitter.label("__rt_array_free_deep_done");
    emitter.instruction("ret");                                                 // return
}

fn emit_array_free_deep_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_free_deep ---");
    emitter.label_global("__rt_array_free_deep");

    emitter.instruction("test rax, rax");                                       // skip null indexed-array pointers immediately because they do not own heap storage
    emitter.instruction("jz __rt_array_free_deep_done");                        // null indexed arrays need no deep-free work
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the stamped x86_64 heap kind word from the uniform header
    emitter.instruction("mov r11, r10");                                        // preserve the full heap kind word before isolating the ownership marker and low-byte heap kind
    emitter.instruction("shr r11, 32");                                         // isolate the high-word heap marker used by the x86_64 heap wrapper
    emitter.instruction(&format!("cmp r11d, 0x{:x}", X86_64_HEAP_MAGIC_HI32));  // ignore foreign pointers that do not carry the elephc x86_64 heap marker
    emitter.instruction("jne __rt_array_free_deep_done");                       // only elephc-owned indexed arrays participate in x86_64 deep-free bookkeeping
    emitter.instruction("and r10, 0xff");                                       // isolate the low-byte uniform heap kind tag for a final ownership sanity check
    emitter.instruction("cmp r10, 2");                                          // is this heap-backed payload really an indexed array?
    emitter.instruction("jne __rt_array_free_deep_done");                       // other heap kinds must not be released through the indexed-array deep-free helper
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving indexed-array deep-free spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved array pointer, length, and loop index
    emitter.instruction("sub rsp, 24");                                         // reserve local storage for the array pointer, logical length, and loop index while keeping SysV call alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the indexed-array pointer across nested decref_any and heap_free calls
    emitter.instruction("mov rcx, QWORD PTR [rax - 8]");                        // load the full stamped heap kind word again so the packed indexed-array value_type tag can be inspected
    emitter.instruction("shr rcx, 8");                                          // move the packed indexed-array value_type tag into the low bits
    emitter.instruction("and ecx, 0x7f");                                       // isolate the indexed-array value_type tag without the persistent COW bit
    emitter.instruction("jnz __rt_array_free_deep_have_tag");                   // prefer the packed runtime tag when codegen/runtime supplied one
    emitter.instruction("cmp QWORD PTR [rax + 16], 16");                        // legacy untyped 16-byte arrays still represent persisted string slots
    emitter.instruction("jne __rt_array_free_deep_struct");                     // untagged scalar indexed arrays need no per-element cleanup
    emitter.instruction("mov ecx, 1");                                          // treat legacy 16-byte indexed arrays as string arrays for deep-free purposes

    emitter.label("__rt_array_free_deep_have_tag");
    emitter.instruction("cmp ecx, 1");                                          // does this indexed array store persisted string payloads?
    emitter.instruction("je __rt_array_free_deep_loop_setup");                  // string slots release through the uniform decref_any helper
    emitter.instruction("cmp ecx, 4");                                          // does this indexed array store nested indexed-array pointers?
    emitter.instruction("je __rt_array_free_deep_loop_setup");                  // nested indexed arrays need decref_any cleanup
    emitter.instruction("cmp ecx, 5");                                          // does this indexed array store associative-array pointers?
    emitter.instruction("je __rt_array_free_deep_loop_setup");                  // nested hashes need decref_any cleanup
    emitter.instruction("cmp ecx, 6");                                          // does this indexed array store object / callable pointers?
    emitter.instruction("je __rt_array_free_deep_loop_setup");                  // nested objects need decref_any cleanup
    emitter.instruction("cmp ecx, 7");                                          // does this indexed array store boxed mixed values?
    emitter.instruction("jne __rt_array_free_deep_struct");                     // scalar indexed arrays need no per-element cleanup

    emitter.label("__rt_array_free_deep_loop_setup");
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the indexed-array logical length before scanning owned child payloads
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save the indexed-array logical length for the deep-free loop
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the indexed-array loop index to zero

    emitter.label("__rt_array_free_deep_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the current indexed-array loop index at the top of every iteration
    emitter.instruction("cmp r10, QWORD PTR [rbp - 16]");                       // have we already scanned every logical indexed-array slot?
    emitter.instruction("jae __rt_array_free_deep_struct");                     // finish once the loop index reaches the saved indexed-array length
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the indexed-array pointer after any nested helper call
    emitter.instruction("mov rcx, QWORD PTR [r11 - 8]");                        // reload the full stamped heap kind word so the indexed-array value_type tag stays available after nested calls
    emitter.instruction("shr rcx, 8");                                          // move the packed indexed-array value_type tag into the low bits
    emitter.instruction("and ecx, 0x7f");                                       // isolate the indexed-array value_type tag without the persistent COW bit
    emitter.instruction("cmp ecx, 1");                                          // does the current indexed array store persisted string payloads?
    emitter.instruction("je __rt_array_free_deep_load_str");                    // string payloads use 16-byte slots instead of pointer-sized slots
    emitter.instruction("mov rax, QWORD PTR [r11 + r10 * 8 + 24]");             // load the heap-backed child pointer from the current pointer-sized indexed-array slot
    emitter.instruction("jmp __rt_array_free_deep_release");                    // release pointer-sized payloads through the uniform decref_any helper

    emitter.label("__rt_array_free_deep_load_str");
    emitter.instruction("mov rcx, r10");                                        // copy the indexed-array slot index before scaling it into a 16-byte string-slot offset
    emitter.instruction("shl rcx, 4");                                          // convert the slot index into the byte offset of the current 16-byte string slot
    emitter.instruction("mov rax, QWORD PTR [r11 + rcx + 24]");                 // load the persisted string pointer from the current indexed-array string slot

    emitter.label("__rt_array_free_deep_release");
    emitter.instruction("call __rt_decref_any");                                // release the heap-backed child payload if the current indexed-array slot owns one
    emitter.instruction("add QWORD PTR [rbp - 24], 1");                         // advance the indexed-array loop index to the next logical slot
    emitter.instruction("jmp __rt_array_free_deep_loop");                       // continue scanning indexed-array slots until every owned child payload is released

    emitter.label("__rt_array_free_deep_struct");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the indexed-array pointer after finishing the optional child cleanup pass
    emitter.instruction("call __rt_heap_free");                                 // release the indexed-array storage itself through the x86_64 heap wrapper
    emitter.instruction("add rsp, 24");                                         // release the spill slots reserved for the indexed-array deep-free scan state
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to generated code

    emitter.label("__rt_array_free_deep_done");
    emitter.instruction("ret");                                                 // return to the caller after releasing the indexed array and any owned heap-backed elements
}
