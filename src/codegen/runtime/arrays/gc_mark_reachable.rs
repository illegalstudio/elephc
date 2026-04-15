use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// gc_mark_reachable: recursively mark a live refcounted heap block and its children.
/// Input: x0 = heap-backed value pointer
/// Output: none
pub fn emit_gc_mark_reachable(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_gc_mark_reachable_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: gc_mark_reachable ---");
    emitter.label_global("__rt_gc_mark_reachable");

    // -- reject null, non-heap, freed, and non-refcounted values --
    emitter.instruction("cbz x0, __rt_gc_mark_reachable_done");                 // ignore null roots
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is the pointer below the heap buffer?
    emitter.instruction("b.lo __rt_gc_mark_reachable_done");                    // only heap pointers can be marked
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // load the current heap offset
    emitter.instruction("add x10, x9, x10");                                    // compute the current heap end
    emitter.instruction("cmp x0, x10");                                         // is the pointer at or beyond the heap end?
    emitter.instruction("b.hs __rt_gc_mark_reachable_done");                    // invalid pointers are ignored
    emitter.instruction("ldr w11, [x0, #-12]");                                 // load the current refcount from the heap header
    emitter.instruction("cbz w11, __rt_gc_mark_reachable_done");                // freed blocks are not part of the live graph
    emitter.instruction("ldr x11, [x0, #-8]");                                  // load the full kind word with transient GC metadata
    emitter.instruction("and x12, x11, #0xff");                                 // isolate the low-byte heap kind tag
    emitter.instruction("cmp x12, #2");                                         // is this at least an indexed array?
    emitter.instruction("b.lo __rt_gc_mark_reachable_done");                    // strings/raw values are not traversed by cycle collection
    emitter.instruction("cmp x12, #5");                                         // is this within the array/hash/object/mixed range?
    emitter.instruction("b.hi __rt_gc_mark_reachable_done");                    // unknown/raw heap kinds do not participate

    // -- stop recursion when this node is already marked reachable --
    emitter.instruction("mov x13, #1");                                         // prepare a single-bit reachable mask
    emitter.instruction("lsl x13, x13, #16");                                   // x13 = GC reachable bit inside the kind word
    emitter.instruction("tst x11, x13");                                        // has this node already been marked reachable?
    emitter.instruction("b.ne __rt_gc_mark_reachable_done");                    // skip duplicate work on already-marked nodes
    emitter.instruction("orr x11, x11, x13");                                   // set the reachable bit for this heap node
    emitter.instruction("str x11, [x0, #-8]");                                  // persist the reachable mark in the heap header

    // -- set up stack frame for recursive child traversal --
    // Stack layout:
    //   [sp, #0]  = current node pointer
    //   [sp, #8]  = full kind word
    //   [sp, #16] = loop/count slot
    //   [sp, #24] = loop index
    //   [sp, #32] = descriptor/value tag scratch
    //   [sp, #48] = saved x29
    //   [sp, #56] = saved x30
    emitter.instruction("sub sp, sp, #64");                                     // allocate a recursive traversal frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the current node pointer
    emitter.instruction("str x11, [sp, #8]");                                   // save the current kind word for dispatch

    // -- dispatch on the uniform heap kind --
    emitter.instruction("and x12, x11, #0xff");                                 // reload the low-byte heap kind tag
    emitter.instruction("cmp x12, #2");                                         // is this an indexed array?
    emitter.instruction("b.eq __rt_gc_mark_reachable_array");                   // traverse array children
    emitter.instruction("cmp x12, #3");                                         // is this an associative array / hash?
    emitter.instruction("b.eq __rt_gc_mark_reachable_hash");                    // traverse hash children
    emitter.instruction("cmp x12, #5");                                         // is this a boxed mixed cell?
    emitter.instruction("b.eq __rt_gc_mark_reachable_mixed");                   // traverse the boxed mixed child if it is heap-backed
    emitter.instruction("b __rt_gc_mark_reachable_object");                     // remaining refcounted kind 4 is an object

    // -- array traversal: only arrays tagged with refcounted element payloads contain graph edges --
    emitter.label("__rt_gc_mark_reachable_array");
    emitter.instruction("lsr x12, x11, #8");                                    // move the packed array value_type tag into the low bits
    emitter.instruction("and x12, x12, #0x7f");                                 // isolate the packed array value_type tag without the persistent COW flag
    emitter.instruction("cmp x12, #4");                                         // is this an array-of-arrays payload?
    emitter.instruction("b.eq __rt_gc_mark_reachable_array_setup");             // traverse nested array payloads
    emitter.instruction("cmp x12, #5");                                         // is this an array-of-hashes payload?
    emitter.instruction("b.eq __rt_gc_mark_reachable_array_setup");             // traverse nested hash payloads
    emitter.instruction("cmp x12, #6");                                         // is this an array-of-objects payload?
    emitter.instruction("b.eq __rt_gc_mark_reachable_array_setup");             // traverse nested object payloads
    emitter.instruction("cmp x12, #7");                                         // is this an array-of-mixed payload?
    emitter.instruction("b.ne __rt_gc_mark_reachable_return");                  // scalar/string arrays contribute no refcounted edges

    emitter.label("__rt_gc_mark_reachable_array_setup");
    emitter.instruction("ldr x9, [x0]");                                        // load the array length from the array header
    emitter.instruction("str x9, [sp, #16]");                                   // save the array length for the loop bound
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize the array index to zero
    emitter.label("__rt_gc_mark_reachable_array_loop");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the current array index
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the array length
    emitter.instruction("cmp x9, x10");                                         // have we visited every array element?
    emitter.instruction("b.ge __rt_gc_mark_reachable_return");                  // finish once the array elements are exhausted
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the array pointer
    emitter.instruction("lsl x11, x9, #3");                                     // compute the byte offset for an 8-byte child slot
    emitter.instruction("add x11, x11, #24");                                   // skip the 24-byte array header
    emitter.instruction("ldr x0, [x10, x11]");                                  // load the nested heap child pointer
    emitter.instruction("cbz x0, __rt_gc_mark_reachable_array_next");           // null child slots need no recursion
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the child kind word to check whether it is already marked
    emitter.instruction("mov x11, #1");                                         // prepare a single-bit reachable mask
    emitter.instruction("lsl x11, x11, #16");                                   // x11 = GC reachable bit inside the kind word
    emitter.instruction("tst x10, x11");                                        // is this child already marked reachable?
    emitter.instruction("b.ne __rt_gc_mark_reachable_array_next");              // skip the recursive call for already-marked children
    emitter.instruction("str x9, [sp, #24]");                                   // preserve the array index across recursion
    emitter.instruction("bl __rt_gc_mark_reachable");                           // recursively mark the nested child reachable
    emitter.instruction("ldr x9, [sp, #24]");                                   // restore the array index after recursion
    emitter.label("__rt_gc_mark_reachable_array_next");
    emitter.instruction("add x9, x9, #1");                                      // advance to the next array element
    emitter.instruction("str x9, [sp, #24]");                                   // save the updated array index
    emitter.instruction("b __rt_gc_mark_reachable_array_loop");                 // continue traversing array elements

    // -- hash traversal: inspect each entry's runtime value_tag for graph edges --
    emitter.label("__rt_gc_mark_reachable_hash");
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the hash capacity from the header
    emitter.instruction("str x9, [sp, #16]");                                   // save the hash capacity for the scan loop
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize the slot index to zero
    emitter.label("__rt_gc_mark_reachable_hash_loop");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the current slot index
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the hash capacity
    emitter.instruction("cmp x9, x10");                                         // have we scanned every hash slot?
    emitter.instruction("b.ge __rt_gc_mark_reachable_return");                  // finish once all slots have been scanned
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the hash pointer
    emitter.instruction("mov x11, #64");                                        // each hash entry occupies 64 bytes with per-entry tags and insertion-order links
    emitter.instruction("mul x11, x9, x11");                                    // compute the byte offset for this entry
    emitter.instruction("add x11, x10, x11");                                   // advance from the table base to the entry
    emitter.instruction("add x11, x11, #40");                                   // skip the 40-byte hash header
    emitter.instruction("ldr x12, [x11]");                                      // load the occupied flag for this slot
    emitter.instruction("cmp x12, #1");                                         // is this hash slot occupied?
    emitter.instruction("b.ne __rt_gc_mark_reachable_hash_next");               // skip empty or tombstone slots
    emitter.instruction("ldr x12, [x11, #40]");                                 // load this entry's runtime value_tag
    emitter.instruction("cmp x12, #4");                                         // does this entry hold a heap-backed child?
    emitter.instruction("b.lo __rt_gc_mark_reachable_hash_next");               // scalar/string entries contribute no graph edges
    emitter.instruction("cmp x12, #7");                                         // do the entry tags stay within the heap-backed range?
    emitter.instruction("b.hi __rt_gc_mark_reachable_hash_next");               // unknown tags are ignored by the cycle collector
    emitter.instruction("ldr x0, [x11, #24]");                                  // load the refcounted child pointer from the value payload
    emitter.instruction("str x9, [sp, #24]");                                   // preserve the slot index across recursion
    emitter.instruction("bl __rt_gc_mark_reachable");                           // recursively mark the nested child reachable
    emitter.instruction("ldr x9, [sp, #24]");                                   // restore the slot index after recursion
    emitter.label("__rt_gc_mark_reachable_hash_next");
    emitter.instruction("add x9, x9, #1");                                      // advance to the next hash slot
    emitter.instruction("str x9, [sp, #24]");                                   // save the updated slot index
    emitter.instruction("b __rt_gc_mark_reachable_hash_loop");                  // continue traversing hash entries

    // -- mixed traversal: boxed mixed values contribute at most one heap edge --
    emitter.label("__rt_gc_mark_reachable_mixed");
    emitter.instruction("ldr x12, [x0]");                                       // load the boxed mixed runtime value_tag
    emitter.instruction("cmp x12, #4");                                         // does the boxed value hold a heap-backed child?
    emitter.instruction("b.lo __rt_gc_mark_reachable_return");                  // scalar/string/null mixed payloads contribute no graph edges
    emitter.instruction("cmp x12, #7");                                         // do boxed mixed tags stay within the heap-backed range?
    emitter.instruction("b.hi __rt_gc_mark_reachable_return");                  // unknown boxed tags are ignored by the collector
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the boxed heap child pointer
    emitter.instruction("bl __rt_gc_mark_reachable");                           // recursively mark the boxed child reachable
    emitter.instruction("b __rt_gc_mark_reachable_return");                     // mixed traversal is complete

    // -- object traversal: consult the emitted per-class property descriptor table --
    emitter.label("__rt_gc_mark_reachable_object");
    emitter.instruction("ldr w9, [x0, #-16]");                                  // load the object payload size from the heap header
    emitter.instruction("sub x9, x9, #8");                                      // subtract the leading class_id field
    emitter.instruction("lsr x9, x9, #4");                                      // divide by 16 to get the property count
    emitter.instruction("str x9, [sp, #16]");                                   // save the property count for the loop bound
    emitter.instruction("ldr x10, [x0]");                                       // load the runtime class_id from the object payload
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_class_gc_desc_count");
    emitter.instruction("ldr x11, [x11]");                                      // load the number of emitted class descriptors
    emitter.instruction("cmp x10, x11");                                        // is the class_id within range?
    emitter.instruction("b.hs __rt_gc_mark_reachable_return");                  // invalid class ids contribute no traversable edges
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_class_gc_desc_ptrs");
    emitter.instruction("lsl x12, x10, #3");                                    // scale class_id by 8 bytes per descriptor pointer
    emitter.instruction("ldr x11, [x11, x12]");                                 // load the property-tag descriptor pointer
    emitter.instruction("str x11, [sp, #32]");                                  // save the property-tag descriptor pointer
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize the property index to zero
    emitter.label("__rt_gc_mark_reachable_object_loop");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the current property index
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the property count
    emitter.instruction("cmp x9, x10");                                         // have we scanned every property?
    emitter.instruction("b.ge __rt_gc_mark_reachable_return");                  // finish once every property slot has been visited
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the object pointer
    emitter.instruction("mov x11, #16");                                        // each property slot occupies 16 bytes
    emitter.instruction("mul x11, x9, x11");                                    // compute the byte offset for this property slot
    emitter.instruction("add x11, x11, #8");                                    // skip the leading class_id field
    emitter.instruction("ldr x13, [sp, #32]");                                  // reload the descriptor pointer for this property slot
    emitter.instruction("ldrb w13, [x13, x9]");                                 // load the compile-time property tag
    emitter.instruction("cmp x13, #4");                                         // is this a compile-time indexed-array property?
    emitter.instruction("b.eq __rt_gc_mark_reachable_object_child");            // recurse into nested array properties
    emitter.instruction("cmp x13, #5");                                         // is this a compile-time associative-array property?
    emitter.instruction("b.eq __rt_gc_mark_reachable_object_child");            // recurse into nested hash properties
    emitter.instruction("cmp x13, #6");                                         // is this a compile-time object property?
    emitter.instruction("b.eq __rt_gc_mark_reachable_object_child");            // recurse into compile-time object properties
    emitter.instruction("cmp x13, #7");                                         // is this a compile-time mixed property?
    emitter.instruction("b.ne __rt_gc_mark_reachable_object_next");             // scalar and string properties contribute no refcounted edges
    emitter.instruction("add x12, x11, #8");                                    // compute the offset of the runtime metadata / length word
    emitter.instruction("ldr x13, [x10, x12]");                                 // load the runtime tag for this mixed property slot
    emitter.instruction("cmp x13, #4");                                         // does the mixed property currently hold a heap-backed child?
    emitter.instruction("b.lo __rt_gc_mark_reachable_object_next");             // scalar/string/null mixed payloads contribute no graph edges
    emitter.instruction("cmp x13, #7");                                         // do the mixed runtime tags stay within the supported heap-backed range?
    emitter.instruction("b.hi __rt_gc_mark_reachable_object_next");             // unknown mixed payloads are ignored by the collector
    emitter.label("__rt_gc_mark_reachable_object_child");
    emitter.instruction("ldr x0, [x10, x11]");                                  // load the nested child pointer from the property slot
    emitter.instruction("str x9, [sp, #24]");                                   // preserve the property index across recursion
    emitter.instruction("bl __rt_gc_mark_reachable");                           // recursively mark the nested child reachable
    emitter.instruction("ldr x9, [sp, #24]");                                   // restore the property index after recursion
    emitter.label("__rt_gc_mark_reachable_object_next");
    emitter.instruction("add x9, x9, #1");                                      // advance to the next property slot
    emitter.instruction("str x9, [sp, #24]");                                   // save the updated property index
    emitter.instruction("b __rt_gc_mark_reachable_object_loop");                // continue traversing object properties

    emitter.label("__rt_gc_mark_reachable_return");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // tear down the recursive traversal frame

    emitter.label("__rt_gc_mark_reachable_done");
    emitter.instruction("ret");                                                 // return to the caller
}

fn emit_gc_mark_reachable_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: gc_mark_reachable ---");
    emitter.label_global("__rt_gc_mark_reachable");

    // -- reject null, non-heap, freed, and non-refcounted values --
    emitter.instruction("test rax, rax");                                       // ignore null roots because they do not identify a heap-backed graph node
    emitter.instruction("jz __rt_gc_mark_reachable_done");                      // null roots need no traversal work
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_heap_buf");
    emitter.instruction("cmp rax, r8");                                         // is the candidate pointer below the managed heap buffer?
    emitter.instruction("jb __rt_gc_mark_reachable_done");                      // only heap-backed values participate in cycle traversal
    crate::codegen::abi::emit_symbol_address(emitter, "r9", "_heap_off");
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // load the current heap bump offset before computing the managed heap end
    emitter.instruction("lea r9, [r8 + r9]");                                   // compute the current heap end from the heap base plus bump offset
    emitter.instruction("cmp rax, r9");                                         // is the candidate pointer at or beyond the current heap end?
    emitter.instruction("jae __rt_gc_mark_reachable_done");                     // pointers outside the live heap window are ignored
    emitter.instruction("mov r10d, DWORD PTR [rax - 12]");                      // load the current block refcount from the uniform heap header
    emitter.instruction("test r10d, r10d");                                     // has this heap block already been freed?
    emitter.instruction("jz __rt_gc_mark_reachable_done");                      // freed blocks are not part of the live object graph
    emitter.instruction("mov r11, QWORD PTR [rax - 8]");                        // load the full kind word with any transient GC metadata
    emitter.instruction("mov rcx, r11");                                        // preserve the full kind word while isolating the uniform heap kind tag
    emitter.instruction("and rcx, 0xff");                                       // isolate the low-byte uniform heap kind tag from the packed metadata word
    emitter.instruction("cmp rcx, 2");                                          // is this at least an indexed array?
    emitter.instruction("jb __rt_gc_mark_reachable_done");                      // strings and raw buffers do not participate in cycle traversal
    emitter.instruction("cmp rcx, 5");                                          // is this within the array/hash/object/mixed range?
    emitter.instruction("ja __rt_gc_mark_reachable_done");                      // unknown/raw heap kinds are ignored by the collector

    // -- stop recursion when this node is already marked reachable --
    emitter.instruction("test r11, 0x10000");                                   // has the collector already marked this node reachable during the current pass?
    emitter.instruction("jnz __rt_gc_mark_reachable_done");                     // yes — skip duplicate recursive work on an already-marked node
    emitter.instruction("or r11, 0x10000");                                     // set the x86_64 reachable bit inside the temporary GC metadata range
    emitter.instruction("mov QWORD PTR [rax - 8], r11");                        // persist the reachable mark in the heap header before traversing children

    // -- set up a traversal frame for recursive child scans --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving traversal locals
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame pointer for the recursive traversal locals
    emitter.instruction("sub rsp, 48");                                         // reserve traversal locals for the node pointer, kind word, counts, and loop indices
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the current node pointer across child-recursion calls
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the full kind word so packed array metadata remains available after recursion

    // -- dispatch on the uniform heap kind --
    emitter.instruction("cmp rcx, 2");                                          // is this an indexed array node?
    emitter.instruction("je __rt_gc_mark_reachable_array");                     // yes — traverse array children
    emitter.instruction("cmp rcx, 3");                                          // is this an associative-array / hash node?
    emitter.instruction("je __rt_gc_mark_reachable_hash");                      // yes — traverse hash entry children
    emitter.instruction("cmp rcx, 5");                                          // is this a boxed mixed cell?
    emitter.instruction("je __rt_gc_mark_reachable_mixed");                     // yes — traverse the boxed child pointer if it is heap-backed
    emitter.instruction("jmp __rt_gc_mark_reachable_object");                   // the remaining refcounted heap kind is an object instance

    // -- array traversal: only arrays with refcounted element payloads contain graph edges --
    emitter.label("__rt_gc_mark_reachable_array");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the full array kind word before unpacking its runtime value_type tag
    emitter.instruction("shr rcx, 8");                                          // move the packed array value_type tag down into the low bits
    emitter.instruction("and rcx, 0x7f");                                       // isolate the array value_type without the persistent COW flag bit
    emitter.instruction("cmp rcx, 4");                                          // is this an array-of-arrays payload?
    emitter.instruction("jb __rt_gc_mark_reachable_return");                    // scalar and string arrays contribute no refcounted graph edges
    emitter.instruction("cmp rcx, 7");                                          // is this within the refcounted array payload range?
    emitter.instruction("ja __rt_gc_mark_reachable_return");                    // unknown array payload tags are ignored by the collector
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // reload the current array pointer before reading the array length
    emitter.instruction("mov rdx, QWORD PTR [rdx]");                            // load the array length from the array header
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the array length for the traversal loop bound
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the array element index to zero
    emitter.label("__rt_gc_mark_reachable_array_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the current array element index after any recursive child traversal
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                       // have we visited every array element in this heap node?
    emitter.instruction("jae __rt_gc_mark_reachable_return");                   // yes — finish once the array child scan is exhausted
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // reload the current array pointer before computing the child slot address
    emitter.instruction("mov r8, rcx");                                         // preserve the logical array index while scaling it into a byte offset
    emitter.instruction("shl r8, 3");                                           // scale the array index by eight bytes per child pointer slot
    emitter.instruction("add r8, 24");                                          // skip the 24-byte array header to reach the child storage region
    emitter.instruction("mov rax, QWORD PTR [rdx + r8]");                       // load the nested heap child pointer from the current array element slot
    emitter.instruction("test rax, rax");                                       // is this array element null?
    emitter.instruction("jz __rt_gc_mark_reachable_array_next");                // yes — null child slots need no recursive marking
    emitter.instruction("call __rt_gc_mark_reachable");                         // recursively mark the nested child reachable from this array node
    emitter.label("__rt_gc_mark_reachable_array_next");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the current array index after any recursive traversal
    emitter.instruction("add rcx, 1");                                          // advance to the next array element in this heap node
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // persist the updated array index for the next loop iteration
    emitter.instruction("jmp __rt_gc_mark_reachable_array_loop");               // continue traversing array child pointers

    // -- hash traversal: inspect each occupied entry's runtime value tag for graph edges --
    emitter.label("__rt_gc_mark_reachable_hash");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // reload the current hash pointer before reading its capacity
    emitter.instruction("mov rdx, QWORD PTR [rdx + 8]");                        // load the hash capacity from the hash header
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the hash capacity for the traversal loop bound
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the hash slot index to zero
    emitter.label("__rt_gc_mark_reachable_hash_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the current hash slot index after any recursive child traversal
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                       // have we scanned every hash entry slot?
    emitter.instruction("jae __rt_gc_mark_reachable_return");                   // yes — finish once the hash child scan is exhausted
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // reload the current hash pointer before computing the entry address
    emitter.instruction("mov r8, rcx");                                         // preserve the logical slot index while scaling it into an entry byte offset
    emitter.instruction("imul r8, 64");                                         // scale the slot index by 64 bytes per hash entry
    emitter.instruction("add r8, 40");                                          // skip the 40-byte hash header to reach the selected entry
    emitter.instruction("add rdx, r8");                                         // compute the address of the selected hash entry
    emitter.instruction("cmp QWORD PTR [rdx], 1");                              // is this hash entry occupied?
    emitter.instruction("jne __rt_gc_mark_reachable_hash_next");                // skip empty or tombstone slots that carry no outgoing graph edge
    emitter.instruction("mov r8, QWORD PTR [rdx + 40]");                        // load the runtime value_tag stored for this hash entry
    emitter.instruction("cmp r8, 4");                                           // does this hash entry hold a heap-backed child?
    emitter.instruction("jb __rt_gc_mark_reachable_hash_next");                 // scalar and string hash entries contribute no refcounted graph edges
    emitter.instruction("cmp r8, 7");                                           // is the value_tag within the supported heap-backed range?
    emitter.instruction("ja __rt_gc_mark_reachable_hash_next");                 // unknown runtime tags are ignored by the collector
    emitter.instruction("mov rax, QWORD PTR [rdx + 24]");                       // load the refcounted child pointer stored in the hash value payload
    emitter.instruction("call __rt_gc_mark_reachable");                         // recursively mark the nested hash child reachable
    emitter.label("__rt_gc_mark_reachable_hash_next");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the current hash slot index after any recursive traversal
    emitter.instruction("add rcx, 1");                                          // advance to the next hash slot in this heap node
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // persist the updated hash slot index for the next iteration
    emitter.instruction("jmp __rt_gc_mark_reachable_hash_loop");                // continue traversing hash entry child pointers

    // -- mixed traversal: boxed mixed cells contribute at most one heap edge --
    emitter.label("__rt_gc_mark_reachable_mixed");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // reload the current mixed-box pointer before inspecting the boxed runtime tag
    emitter.instruction("mov rcx, QWORD PTR [rdx]");                            // load the boxed mixed runtime value_tag from the mixed cell header
    emitter.instruction("cmp rcx, 4");                                          // does the boxed mixed value hold a heap-backed child?
    emitter.instruction("jb __rt_gc_mark_reachable_return");                    // scalar, string, and null boxed payloads contribute no graph edges
    emitter.instruction("cmp rcx, 7");                                          // is the boxed mixed runtime tag within the supported heap-backed range?
    emitter.instruction("ja __rt_gc_mark_reachable_return");                    // unknown boxed runtime tags are ignored by the collector
    emitter.instruction("mov rax, QWORD PTR [rdx + 8]");                        // load the boxed child pointer stored in the mixed cell payload
    emitter.instruction("call __rt_gc_mark_reachable");                         // recursively mark the boxed child reachable from this mixed node
    emitter.instruction("jmp __rt_gc_mark_reachable_return");                   // the mixed child scan is complete

    // -- object traversal: consult the emitted per-class property descriptor table --
    emitter.label("__rt_gc_mark_reachable_object");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // reload the current object pointer before computing its property count
    emitter.instruction("mov ecx, DWORD PTR [rdx - 16]");                       // load the object payload size from the uniform heap header
    emitter.instruction("sub rcx, 8");                                          // subtract the leading class_id field from the object payload size
    emitter.instruction("shr rcx, 4");                                          // divide by 16 to get the number of property slots in this object layout
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the property count for the object traversal loop bound
    emitter.instruction("mov rcx, QWORD PTR [rdx]");                            // load the runtime class_id stored at the start of the object payload
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_class_gc_desc_count");
    emitter.instruction("mov r8, QWORD PTR [r8]");                              // load the number of emitted class GC descriptors
    emitter.instruction("cmp rcx, r8");                                         // is the runtime class_id within the emitted descriptor table range?
    emitter.instruction("jae __rt_gc_mark_reachable_return");                   // invalid class ids contribute no traversable property metadata
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_class_gc_desc_ptrs");
    emitter.instruction("mov r9, QWORD PTR [r8 + rcx * 8]");                    // load the per-class property-tag descriptor pointer for this object instance
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // save the descriptor pointer across recursive property traversals
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the object property index to zero
    emitter.label("__rt_gc_mark_reachable_object_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the current property index after any recursive child traversal
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                       // have we visited every object property slot?
    emitter.instruction("jae __rt_gc_mark_reachable_return");                   // yes — finish once the object property scan is exhausted
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // reload the current object pointer before computing the selected property slot address
    emitter.instruction("mov r8, rcx");                                         // preserve the logical property index while scaling it into a byte offset
    emitter.instruction("imul r8, 16");                                         // scale the property index by 16 bytes per object property slot
    emitter.instruction("add r8, 8");                                           // skip the leading class_id field to reach the selected property slot
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the per-class descriptor pointer for the selected property slot
    emitter.instruction("movzx r9d, BYTE PTR [r9 + rcx]");                      // load the compile-time property tag for the selected object property
    emitter.instruction("cmp r9, 4");                                           // is this property statically typed as an indexed array?
    emitter.instruction("je __rt_gc_mark_reachable_object_child");              // yes — recurse into the nested array property payload
    emitter.instruction("cmp r9, 5");                                           // is this property statically typed as an associative array?
    emitter.instruction("je __rt_gc_mark_reachable_object_child");              // yes — recurse into the nested hash property payload
    emitter.instruction("cmp r9, 6");                                           // is this property statically typed as an object?
    emitter.instruction("je __rt_gc_mark_reachable_object_child");              // yes — recurse into the nested object property payload
    emitter.instruction("cmp r9, 7");                                           // is this property statically typed as a mixed slot?
    emitter.instruction("jne __rt_gc_mark_reachable_object_next");              // scalar and string properties contribute no refcounted graph edges
    emitter.instruction("mov r9, QWORD PTR [rdx + r8 + 8]");                    // load the runtime tag stored alongside the mixed property payload
    emitter.instruction("cmp r9, 4");                                           // does the mixed property currently hold a heap-backed child?
    emitter.instruction("jb __rt_gc_mark_reachable_object_next");               // scalar, string, and null mixed payloads contribute no graph edges
    emitter.instruction("cmp r9, 7");                                           // is the mixed runtime tag within the supported heap-backed range?
    emitter.instruction("ja __rt_gc_mark_reachable_object_next");               // unknown mixed payload tags are ignored by the collector
    emitter.label("__rt_gc_mark_reachable_object_child");
    emitter.instruction("mov rax, QWORD PTR [rdx + r8]");                       // load the nested child pointer stored in the selected object property slot
    emitter.instruction("call __rt_gc_mark_reachable");                         // recursively mark the nested object property child reachable
    emitter.label("__rt_gc_mark_reachable_object_next");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the current object property index after any recursive traversal
    emitter.instruction("add rcx, 1");                                          // advance to the next object property slot in this heap node
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // persist the updated object property index for the next traversal iteration
    emitter.instruction("jmp __rt_gc_mark_reachable_object_loop");              // continue traversing object property child pointers

    emitter.label("__rt_gc_mark_reachable_return");
    emitter.instruction("leave");                                               // tear down the recursive traversal frame before returning to the caller

    emitter.label("__rt_gc_mark_reachable_done");
    emitter.instruction("ret");                                                 // return after the optional recursive child traversal
}
