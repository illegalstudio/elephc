use crate::codegen::emit::Emitter;

/// gc_mark_reachable: recursively mark a live refcounted heap block and its children.
/// Input: x0 = heap-backed value pointer
/// Output: none
pub fn emit_gc_mark_reachable(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: gc_mark_reachable ---");
    emitter.label("__rt_gc_mark_reachable");

    // -- reject null, non-heap, freed, and non-refcounted values --
    emitter.instruction("cbz x0, __rt_gc_mark_reachable_done");                 // ignore null roots
    emitter.instruction("adrp x9, _heap_buf@PAGE");                             // load page of the heap buffer
    emitter.instruction("add x9, x9, _heap_buf@PAGEOFF");                       // resolve the heap buffer base
    emitter.instruction("cmp x0, x9");                                          // is the pointer below the heap buffer?
    emitter.instruction("b.lo __rt_gc_mark_reachable_done");                    // only heap pointers can be marked
    emitter.instruction("adrp x10, _heap_off@PAGE");                            // load page of the heap offset
    emitter.instruction("add x10, x10, _heap_off@PAGEOFF");                     // resolve the heap offset address
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
    emitter.instruction("ldr x12, [x0]");                                        // load the boxed mixed runtime value_tag
    emitter.instruction("cmp x12, #4");                                          // does the boxed value hold a heap-backed child?
    emitter.instruction("b.lo __rt_gc_mark_reachable_return");                   // scalar/string/null mixed payloads contribute no graph edges
    emitter.instruction("cmp x12, #7");                                          // do boxed mixed tags stay within the heap-backed range?
    emitter.instruction("b.hi __rt_gc_mark_reachable_return");                   // unknown boxed tags are ignored by the collector
    emitter.instruction("ldr x0, [x0, #8]");                                     // load the boxed heap child pointer
    emitter.instruction("bl __rt_gc_mark_reachable");                            // recursively mark the boxed child reachable
    emitter.instruction("b __rt_gc_mark_reachable_return");                      // mixed traversal is complete

    // -- object traversal: consult the emitted per-class property descriptor table --
    emitter.label("__rt_gc_mark_reachable_object");
    emitter.instruction("ldr w9, [x0, #-16]");                                  // load the object payload size from the heap header
    emitter.instruction("sub x9, x9, #8");                                      // subtract the leading class_id field
    emitter.instruction("lsr x9, x9, #4");                                      // divide by 16 to get the property count
    emitter.instruction("str x9, [sp, #16]");                                   // save the property count for the loop bound
    emitter.instruction("ldr x10, [x0]");                                       // load the runtime class_id from the object payload
    emitter.instruction("adrp x11, _class_gc_desc_count@PAGE");                 // load page of the descriptor count table
    emitter.instruction("add x11, x11, _class_gc_desc_count@PAGEOFF");          // resolve the descriptor count address
    emitter.instruction("ldr x11, [x11]");                                      // load the number of emitted class descriptors
    emitter.instruction("cmp x10, x11");                                        // is the class_id within range?
    emitter.instruction("b.hs __rt_gc_mark_reachable_return");                  // invalid class ids contribute no traversable edges
    emitter.instruction("adrp x11, _class_gc_desc_ptrs@PAGE");                  // load page of the descriptor pointer table
    emitter.instruction("add x11, x11, _class_gc_desc_ptrs@PAGEOFF");           // resolve the descriptor pointer table
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
    emitter.instruction("add x12, x11, #8");                                    // compute the offset of the runtime metadata / length word
    emitter.instruction("ldr x13, [x10, x12]");                                 // load the runtime metadata / length word for this property slot
    emitter.instruction("cmp x13, #4");                                         // was this property last written with an indexed array?
    emitter.instruction("b.eq __rt_gc_mark_reachable_object_child");            // recurse into nested array properties
    emitter.instruction("cmp x13, #5");                                         // was this property last written with an associative array?
    emitter.instruction("b.eq __rt_gc_mark_reachable_object_child");            // recurse into nested hash properties
    emitter.instruction("cmp x13, #6");                                         // was this property last written with an object?
    emitter.instruction("b.eq __rt_gc_mark_reachable_object_child");            // recurse into nested object properties
    emitter.instruction("cmp x13, #7");                                         // was this property last written with a boxed mixed value?
    emitter.instruction("b.eq __rt_gc_mark_reachable_object_child");            // recurse into nested mixed properties
    emitter.instruction("ldr x13, [sp, #32]");                                  // reload the fallback descriptor pointer
    emitter.instruction("ldrb w13, [x13, x9]");                                 // load the compile-time fallback tag for this property slot
    emitter.instruction("cmp x13, #4");                                         // is this a compile-time indexed-array property?
    emitter.instruction("b.eq __rt_gc_mark_reachable_object_child");            // recurse into nested array properties
    emitter.instruction("cmp x13, #5");                                         // is this a compile-time associative-array property?
    emitter.instruction("b.eq __rt_gc_mark_reachable_object_child");            // recurse into nested hash properties
    emitter.instruction("cmp x13, #6");                                         // is this a compile-time object property?
    emitter.instruction("b.eq __rt_gc_mark_reachable_object_child");            // recurse into compile-time object properties
    emitter.instruction("cmp x13, #7");                                         // is this a compile-time mixed property?
    emitter.instruction("b.ne __rt_gc_mark_reachable_object_next");             // scalar and string properties contribute no refcounted edges
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
