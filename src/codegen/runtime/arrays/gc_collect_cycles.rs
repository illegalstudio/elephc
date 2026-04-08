use crate::codegen::emit::Emitter;

/// gc_collect_cycles: reclaim unreachable refcounted array/hash/object graphs.
/// The collector uses existing refcounts plus a transient heap-edge count stored
/// in the upper 32 bits of the 64-bit heap kind word.
pub fn emit_gc_collect_cycles(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: gc_collect_cycles ---");
    emitter.label_global("__rt_gc_collect_cycles");

    // -- avoid recursive re-entry while the collector is already running --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_gc_collecting");
    emitter.instruction("ldr x10, [x9]");                                       // load the current collector-active flag
    emitter.instruction("cbnz x10, __rt_gc_collect_cycles_done");               // nested collection attempts are ignored

    // -- set up a stack frame for the collector state --
    // Stack layout:
    //   [sp, #0]  = current heap header scan pointer
    //   [sp, #8]  = initial heap end
    //   [sp, #16] = heap base
    //   [sp, #24] = scratch / saved next header
    //   [sp, #48] = saved x19
    //   [sp, #56] = saved x20
    //   [sp, #64] = saved x29
    //   [sp, #72] = saved x30
    emitter.instruction("sub sp, sp, #80");                                     // allocate collector stack frame
    emitter.instruction("str x19, [sp, #48]");                                  // preserve the callee-saved scratch register used during child scans
    emitter.instruction("str x20, [sp, #56]");                                  // preserve the callee-saved payload-size register used during heap scans
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up the collector frame pointer

    // -- capture heap bounds once for the initial passes --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("str x9, [sp, #16]");                                   // save the heap base for later scans
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // load the current heap offset
    emitter.instruction("add x10, x9, x10");                                    // compute the current heap end
    emitter.instruction("str x10, [sp, #8]");                                   // save the initial heap end for the metadata passes
    emitter.instruction("str x9, [sp, #0]");                                    // initialize the scan pointer to the heap base

    // -- pass 1: clear all transient GC metadata while preserving kind + array value_type --
    emitter.label("__rt_gc_collect_cycles_clear_loop");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the current heap header scan pointer
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the initial heap end
    emitter.instruction("cmp x9, x10");                                         // reached the end of the bump region?
    emitter.instruction("b.ge __rt_gc_collect_cycles_count_init");              // move on once every live block has been reset
    emitter.instruction("ldr w11, [x9]");                                       // load this block payload size from the heap header
    emitter.instruction("ldr w12, [x9, #4]");                                   // load this block refcount from the heap header
    emitter.instruction("cbz w12, __rt_gc_collect_cycles_clear_next");          // free-list blocks keep kind=0 and need no reset
    emitter.instruction("ldr x13, [x9, #8]");                                   // load the full kind word with any stale GC metadata
    emitter.instruction("mov x14, #0xffff");                                    // preserve the low 16 bits (kind + array value_type)
    emitter.instruction("and x13, x13, x14");                                   // clear the transient incoming-count and reachable bits
    emitter.instruction("str x13, [x9, #8]");                                   // persist the reset kind word
    emitter.label("__rt_gc_collect_cycles_clear_next");
    emitter.instruction("add x9, x9, x11");                                     // advance by the payload size
    emitter.instruction("add x9, x9, #16");                                     // skip the uniform 16-byte heap header
    emitter.instruction("str x9, [sp, #0]");                                    // save the next heap header scan pointer
    emitter.instruction("b __rt_gc_collect_cycles_clear_loop");                 // continue clearing transient metadata

    // -- pass 2: count incoming heap edges for every live refcounted block --
    emitter.label("__rt_gc_collect_cycles_count_init");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the heap base
    emitter.instruction("str x9, [sp, #0]");                                    // restart the scan at the heap base
    emitter.label("__rt_gc_collect_cycles_count_loop");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the current heap header scan pointer
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the initial heap end
    emitter.instruction("cmp x9, x10");                                         // reached the end of the bump region?
    emitter.instruction("b.ge __rt_gc_collect_cycles_root_init");               // move on once every live block has been scanned
    emitter.instruction("ldr w20, [x9]");                                       // load this block payload size from the heap header
    emitter.instruction("add x12, x9, #16");                                    // compute the user pointer for this heap block
    emitter.instruction("ldr w13, [x9, #4]");                                   // load this block refcount from the heap header
    emitter.instruction("cbz w13, __rt_gc_collect_cycles_count_next");          // free blocks contribute no outgoing graph edges
    emitter.instruction("ldr x14, [x9, #8]");                                   // load the full kind word for this heap block
    emitter.instruction("and x15, x14, #0xff");                                 // isolate the low-byte heap kind tag
    emitter.instruction("cmp x15, #2");                                         // is this at least an indexed array?
    emitter.instruction("b.lo __rt_gc_collect_cycles_count_next");              // strings/raw blocks contribute no outgoing cycle edges
    emitter.instruction("cmp x15, #5");                                         // is this within the array/hash/object/mixed range?
    emitter.instruction("b.hi __rt_gc_collect_cycles_count_next");              // ignore unknown/raw heap kinds
    emitter.instruction("cmp x15, #2");                                         // is this an indexed array?
    emitter.instruction("b.eq __rt_gc_collect_cycles_count_array");             // scan array payload children
    emitter.instruction("cmp x15, #3");                                         // is this an associative array / hash?
    emitter.instruction("b.eq __rt_gc_collect_cycles_count_hash");              // scan hash payload children
    emitter.instruction("cmp x15, #5");                                         // is this a boxed mixed cell?
    emitter.instruction("b.eq __rt_gc_collect_cycles_count_mixed");             // scan the boxed mixed child pointer
    emitter.instruction("b __rt_gc_collect_cycles_count_object");               // remaining refcounted kind 4 is an object

    emitter.label("__rt_gc_collect_cycles_count_array");
    emitter.instruction("lsr x15, x14, #8");                                    // move the packed array value_type tag into the low bits
    emitter.instruction("and x15, x15, #0x7f");                                 // isolate the packed array value_type tag without the persistent COW flag
    emitter.instruction("cmp x15, #4");                                         // is this an array-of-arrays payload?
    emitter.instruction("b.eq __rt_gc_collect_cycles_count_array_setup");       // scan nested array child pointers
    emitter.instruction("cmp x15, #5");                                         // is this an array-of-hashes payload?
    emitter.instruction("b.eq __rt_gc_collect_cycles_count_array_setup");       // scan nested hash child pointers
    emitter.instruction("cmp x15, #6");                                         // is this an array-of-objects payload?
    emitter.instruction("b.eq __rt_gc_collect_cycles_count_array_setup");       // scan nested object child pointers
    emitter.instruction("cmp x15, #7");                                         // is this an array-of-mixed payload?
    emitter.instruction("b.ne __rt_gc_collect_cycles_count_next");              // scalar/string arrays contribute no refcounted edges
    emitter.label("__rt_gc_collect_cycles_count_array_setup");
    emitter.instruction("ldr x13, [x12]");                                      // load the array length
    emitter.instruction("str x13, [sp, #24]");                                  // save the array length for the loop bound
    emitter.instruction("mov x13, #0");                                         // initialize the array index to zero
    emitter.label("__rt_gc_collect_cycles_count_array_loop");
    emitter.instruction("ldr x14, [sp, #24]");                                  // reload the array length
    emitter.instruction("cmp x13, x14");                                        // have we visited every array element?
    emitter.instruction("b.ge __rt_gc_collect_cycles_count_next");              // finish the array scan once all elements were visited
    emitter.instruction("lsl x15, x13, #3");                                    // compute the byte offset for an 8-byte child slot
    emitter.instruction("add x15, x15, #24");                                   // skip the 24-byte array header
    emitter.instruction("ldr x0, [x12, x15]");                                  // load the nested child pointer from the array slot
    emitter.instruction("str x12, [sp, #32]");                                  // preserve the parent array pointer across the helper call
    emitter.instruction("str x13, [sp, #40]");                                  // preserve the array index across the helper call
    emitter.instruction("bl __rt_gc_note_child_ref");                           // add one incoming heap edge to the nested child
    emitter.instruction("ldr x12, [sp, #32]");                                  // restore the parent array pointer after the helper call
    emitter.instruction("ldr x13, [sp, #40]");                                  // restore the array index after the helper call
    emitter.instruction("add x13, x13, #1");                                    // advance to the next array element
    emitter.instruction("b __rt_gc_collect_cycles_count_array_loop");           // continue scanning array child pointers

    emitter.label("__rt_gc_collect_cycles_count_hash");
    emitter.instruction("ldr x13, [x12, #8]");                                  // load the hash capacity
    emitter.instruction("mov x14, #0");                                         // initialize the slot index to zero
    emitter.label("__rt_gc_collect_cycles_count_hash_loop");
    emitter.instruction("cmp x14, x13");                                        // have we visited every hash slot?
    emitter.instruction("b.ge __rt_gc_collect_cycles_count_next");              // finish the hash scan once all slots were visited
    emitter.instruction("mov x15, #64");                                        // each hash entry occupies 64 bytes with per-entry tags and insertion-order links
    emitter.instruction("mul x15, x14, x15");                                   // compute the byte offset for this hash entry
    emitter.instruction("add x15, x12, x15");                                   // advance from the hash base to the entry
    emitter.instruction("add x15, x15, #40");                                   // skip the 40-byte hash header
    emitter.instruction("ldr x0, [x15]");                                       // load the occupied flag from this slot
    emitter.instruction("cmp x0, #1");                                          // is this hash slot occupied?
    emitter.instruction("b.ne __rt_gc_collect_cycles_count_hash_next");         // skip empty or tombstone slots
    emitter.instruction("ldr x0, [x15, #40]");                                  // load this entry's runtime value_tag
    emitter.instruction("cmp x0, #4");                                          // does this entry hold a heap-backed child?
    emitter.instruction("b.lo __rt_gc_collect_cycles_count_hash_next");         // scalar/string entries contribute no graph edges
    emitter.instruction("cmp x0, #7");                                          // do the per-entry heap-backed tags stay within range?
    emitter.instruction("b.hi __rt_gc_collect_cycles_count_hash_next");         // unknown per-entry tags are ignored
    emitter.instruction("ldr x0, [x15, #24]");                                  // load the nested child pointer from the hash value
    emitter.instruction("str x12, [sp, #32]");                                  // preserve the parent hash pointer across the helper call
    emitter.instruction("str x13, [sp, #40]");                                  // preserve the hash capacity across the helper call
    emitter.instruction("str x14, [sp, #24]");                                  // preserve the slot index across the helper call
    emitter.instruction("bl __rt_gc_note_child_ref");                           // add one incoming heap edge to the nested child
    emitter.instruction("ldr x12, [sp, #32]");                                  // restore the parent hash pointer after the helper call
    emitter.instruction("ldr x13, [sp, #40]");                                  // restore the hash capacity after the helper call
    emitter.instruction("ldr x14, [sp, #24]");                                  // restore the slot index after the helper call
    emitter.label("__rt_gc_collect_cycles_count_hash_next");
    emitter.instruction("add x14, x14, #1");                                    // advance to the next hash slot
    emitter.instruction("b __rt_gc_collect_cycles_count_hash_loop");            // continue scanning hash child pointers

    emitter.label("__rt_gc_collect_cycles_count_mixed");
    emitter.instruction("ldr x15, [x12]");                                      // load the boxed mixed runtime value_tag
    emitter.instruction("cmp x15, #4");                                         // does the boxed mixed value hold a heap-backed child?
    emitter.instruction("b.lo __rt_gc_collect_cycles_count_next");              // scalar/string/null mixed payloads contribute no graph edges
    emitter.instruction("cmp x15, #7");                                         // do boxed mixed tags stay within the supported heap-backed range?
    emitter.instruction("b.hi __rt_gc_collect_cycles_count_next");              // unknown mixed tags are ignored
    emitter.instruction("ldr x0, [x12, #8]");                                   // load the boxed mixed child pointer
    emitter.instruction("bl __rt_gc_note_child_ref");                           // add one incoming heap edge to the boxed child
    emitter.instruction("b __rt_gc_collect_cycles_count_next");                 // mixed-child counting is complete

    emitter.label("__rt_gc_collect_cycles_count_object");
    emitter.instruction("ldr w13, [x9]");                                       // load the object payload size from the heap header
    emitter.instruction("sub x13, x13, #8");                                    // subtract the leading class_id field
    emitter.instruction("lsr x13, x13, #4");                                    // divide by 16 to get the number of property slots
    emitter.instruction("ldr x14, [x12]");                                      // load the runtime class_id from the object payload
    crate::codegen::abi::emit_symbol_address(emitter, "x15", "_class_gc_desc_count");
    emitter.instruction("ldr x15, [x15]");                                      // load the number of emitted class descriptors
    emitter.instruction("cmp x14, x15");                                        // is the class_id within range?
    emitter.instruction("b.hs __rt_gc_collect_cycles_count_next");              // invalid class ids contribute no traversable edges
    crate::codegen::abi::emit_symbol_address(emitter, "x15", "_class_gc_desc_ptrs");
    emitter.instruction("lsl x14, x14, #3");                                    // scale class_id by 8 bytes per descriptor pointer
    emitter.instruction("ldr x14, [x15, x14]");                                 // load the property-tag descriptor pointer
    emitter.instruction("mov x15, #0");                                         // initialize the property index to zero
    emitter.label("__rt_gc_collect_cycles_count_object_loop");
    emitter.instruction("cmp x15, x13");                                        // have we visited every property slot?
    emitter.instruction("b.ge __rt_gc_collect_cycles_count_next");              // finish the object scan once all properties were visited
    emitter.instruction("mov x0, #16");                                         // each property slot occupies 16 bytes
    emitter.instruction("mul x0, x15, x0");                                     // compute the byte offset for this property slot
    emitter.instruction("add x0, x0, #8");                                      // skip the leading class_id field
    emitter.instruction("ldrb w10, [x14, x15]");                                // load the compile-time property tag for this slot
    emitter.instruction("cmp x10, #4");                                         // is this a compile-time indexed-array property?
    emitter.instruction("b.eq __rt_gc_collect_cycles_count_object_child");      // count nested array property pointers
    emitter.instruction("cmp x10, #5");                                         // is this a compile-time associative-array property?
    emitter.instruction("b.eq __rt_gc_collect_cycles_count_object_child");      // count nested hash property pointers
    emitter.instruction("cmp x10, #6");                                         // is this a compile-time object property?
    emitter.instruction("b.eq __rt_gc_collect_cycles_count_object_child");      // count compile-time object property pointers
    emitter.instruction("cmp x10, #7");                                         // is this a compile-time mixed property?
    emitter.instruction("b.ne __rt_gc_collect_cycles_count_object_next");       // scalar and string properties contribute no refcounted edges
    emitter.instruction("add x10, x0, #8");                                     // compute the offset of the runtime metadata / length word
    emitter.instruction("ldr x10, [x12, x10]");                                 // load the runtime tag for this mixed property slot
    emitter.instruction("cmp x10, #4");                                         // does this mixed property currently hold a heap-backed child?
    emitter.instruction("b.lo __rt_gc_collect_cycles_count_object_next");       // scalar/string/null mixed payloads contribute no graph edges
    emitter.instruction("cmp x10, #7");                                         // do mixed runtime tags stay within the supported heap-backed range?
    emitter.instruction("b.hi __rt_gc_collect_cycles_count_object_next");       // unknown mixed payloads are ignored by the collector
    emitter.label("__rt_gc_collect_cycles_count_object_child");
    emitter.instruction("ldr x0, [x12, x0]");                                   // load the nested child pointer from the property slot
    emitter.instruction("str x12, [sp, #32]");                                  // preserve the parent object pointer across the helper call
    emitter.instruction("str x13, [sp, #40]");                                  // preserve the property count across the helper call
    emitter.instruction("str x14, [sp, #24]");                                  // preserve the descriptor pointer across the helper call
    emitter.instruction("mov x19, x15");                                        // preserve the property index in a callee-saved register
    emitter.instruction("bl __rt_gc_note_child_ref");                           // add one incoming heap edge to the nested child
    emitter.instruction("ldr x12, [sp, #32]");                                  // restore the parent object pointer after the helper call
    emitter.instruction("ldr x13, [sp, #40]");                                  // restore the property count after the helper call
    emitter.instruction("ldr x14, [sp, #24]");                                  // restore the descriptor pointer after the helper call
    emitter.instruction("mov x15, x19");                                        // restore the property index after the helper call
    emitter.label("__rt_gc_collect_cycles_count_object_next");
    emitter.instruction("add x15, x15, #1");                                    // advance to the next property slot
    emitter.instruction("b __rt_gc_collect_cycles_count_object_loop");          // continue scanning object child pointers

    emitter.label("__rt_gc_collect_cycles_count_next");
    emitter.instruction("ldr x9, [sp, #0]");                                    // restore the current heap header scan pointer after nested helper calls
    emitter.instruction("add x9, x9, x20");                                     // advance by this block payload size
    emitter.instruction("add x9, x9, #16");                                     // skip the uniform 16-byte heap header
    emitter.instruction("str x9, [sp, #0]");                                    // save the next heap header scan pointer
    emitter.instruction("b __rt_gc_collect_cycles_count_loop");                 // continue counting incoming heap edges

    // -- pass 3: mark every block with external refs as a root, then recurse through its children --
    emitter.label("__rt_gc_collect_cycles_root_init");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the heap base
    emitter.instruction("str x9, [sp, #0]");                                    // restart the scan at the heap base
    emitter.label("__rt_gc_collect_cycles_root_loop");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the current heap header scan pointer
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the initial heap end
    emitter.instruction("cmp x9, x10");                                         // reached the end of the bump region?
    emitter.instruction("b.ge __rt_gc_collect_cycles_free_init");               // move on once every root candidate has been checked
    emitter.instruction("ldr w11, [x9]");                                       // load this block payload size from the heap header
    emitter.instruction("ldr w12, [x9, #4]");                                   // load this block refcount from the heap header
    emitter.instruction("cbz w12, __rt_gc_collect_cycles_root_next");           // free blocks are not GC roots
    emitter.instruction("ldr x13, [x9, #8]");                                   // load the full kind word with incoming-edge counts
    emitter.instruction("and x14, x13, #0xff");                                 // isolate the low-byte heap kind tag
    emitter.instruction("cmp x14, #2");                                         // is this at least an indexed array?
    emitter.instruction("b.lo __rt_gc_collect_cycles_root_next");               // strings/raw blocks are outside the cycle collector set
    emitter.instruction("cmp x14, #5");                                         // is this within the array/hash/object/mixed range?
    emitter.instruction("b.hi __rt_gc_collect_cycles_root_next");               // ignore unknown/raw heap kinds
    emitter.instruction("cmp x14, #2");                                         // is this an indexed array candidate?
    emitter.instruction("b.ne __rt_gc_collect_cycles_root_refcounted");         // hashes/objects decide in their dedicated branches
    emitter.instruction("lsr x15, x13, #8");                                    // move the packed array value_type tag into the low bits
    emitter.instruction("and x15, x15, #0x7f");                                 // isolate the packed array value_type tag without the persistent COW flag
    emitter.instruction("cmp x15, #4");                                         // is this an array of indexed arrays?
    emitter.instruction("b.eq __rt_gc_collect_cycles_root_refcounted");         // refcounted array payloads participate in cycle collection
    emitter.instruction("cmp x15, #5");                                         // is this an array of associative arrays?
    emitter.instruction("b.eq __rt_gc_collect_cycles_root_refcounted");         // refcounted array payloads participate in cycle collection
    emitter.instruction("cmp x15, #6");                                         // is this an array of objects?
    emitter.instruction("b.eq __rt_gc_collect_cycles_root_refcounted");         // refcounted array payloads participate in cycle collection
    emitter.instruction("cmp x15, #7");                                         // is this an array of mixed boxes?
    emitter.instruction("b.ne __rt_gc_collect_cycles_root_next");               // scalar/string arrays are never cycle-collector candidates
    emitter.label("__rt_gc_collect_cycles_root_refcounted");
    emitter.instruction("uxtw x12, w12");                                       // widen the 32-bit refcount for comparison
    emitter.instruction("lsr x13, x13, #32");                                   // move the incoming heap-edge count into the low bits
    emitter.instruction("cmp x12, x13");                                        // does this block keep at least one external reference?
    emitter.instruction("b.ls __rt_gc_collect_cycles_root_next");               // refcount <= incoming means no external roots here
    emitter.instruction("add x0, x9, #16");                                     // convert the heap header address back to the user pointer
    emitter.instruction("str x9, [sp, #32]");                                   // preserve the current heap header scan pointer across recursive marking
    emitter.instruction("str x11, [sp, #24]");                                  // preserve the payload size across the recursive mark
    emitter.instruction("bl __rt_gc_mark_reachable");                           // recursively mark this externally-rooted graph reachable
    emitter.instruction("ldr x9, [sp, #32]");                                   // restore the current heap header scan pointer after the mark recursion
    emitter.instruction("ldr x11, [sp, #24]");                                  // restore the payload size after the mark recursion
    emitter.label("__rt_gc_collect_cycles_root_next");
    emitter.instruction("add x9, x9, x11");                                     // advance by this block payload size
    emitter.instruction("add x9, x9, #16");                                     // skip the uniform 16-byte heap header
    emitter.instruction("str x9, [sp, #0]");                                    // save the next heap header scan pointer
    emitter.instruction("b __rt_gc_collect_cycles_root_loop");                  // continue looking for externally-rooted nodes

    // -- pass 4: free every live refcounted block that was never marked reachable --
    emitter.label("__rt_gc_collect_cycles_free_init");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_gc_collecting");
    emitter.instruction("mov x10, #1");                                         // mark the collector as active while reclaiming blocks
    emitter.instruction("str x10, [x9]");                                       // store collector-active = 1
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the heap base
    emitter.instruction("str x9, [sp, #0]");                                    // restart the free scan at the heap base
    emitter.label("__rt_gc_collect_cycles_free_loop");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the current heap header scan pointer
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the heap base for the dynamic end calculation
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_heap_off");
    emitter.instruction("ldr x11, [x11]");                                      // load the current heap offset after any tail trimming
    emitter.instruction("add x10, x10, x11");                                   // compute the current heap end after collection frees
    emitter.instruction("cmp x9, x10");                                         // reached the end of the current bump region?
    emitter.instruction("b.ge __rt_gc_collect_cycles_finish");                  // finish once every surviving header was scanned
    emitter.instruction("ldr w11, [x9]");                                       // load this block payload size from the heap header
    emitter.instruction("ldr w12, [x9, #4]");                                   // load this block refcount from the heap header
    emitter.instruction("cbz w12, __rt_gc_collect_cycles_free_next");           // free-list blocks are already reclaimed
    emitter.instruction("ldr x13, [x9, #8]");                                   // load the full kind word with reachable marks
    emitter.instruction("and x14, x13, #0xff");                                 // isolate the low-byte heap kind tag
    emitter.instruction("cmp x14, #2");                                         // is this at least an indexed array?
    emitter.instruction("b.lo __rt_gc_collect_cycles_free_next");               // strings/raw blocks are outside the cycle collector set
    emitter.instruction("cmp x14, #5");                                         // is this within the array/hash/object/mixed range?
    emitter.instruction("b.hi __rt_gc_collect_cycles_free_next");               // ignore unknown/raw heap kinds
    emitter.instruction("cmp x14, #2");                                         // is this an indexed array candidate?
    emitter.instruction("b.ne __rt_gc_collect_cycles_free_refcounted");         // hashes/objects decide in their dedicated branches
    emitter.instruction("lsr x15, x13, #8");                                    // move the packed array value_type tag into the low bits
    emitter.instruction("and x15, x15, #0x7f");                                 // isolate the packed array value_type tag without the persistent COW flag
    emitter.instruction("cmp x15, #4");                                         // is this an array of indexed arrays?
    emitter.instruction("b.eq __rt_gc_collect_cycles_free_refcounted");         // refcounted array payloads participate in cycle collection
    emitter.instruction("cmp x15, #5");                                         // is this an array of associative arrays?
    emitter.instruction("b.eq __rt_gc_collect_cycles_free_refcounted");         // refcounted array payloads participate in cycle collection
    emitter.instruction("cmp x15, #6");                                         // is this an array of objects?
    emitter.instruction("b.eq __rt_gc_collect_cycles_free_refcounted");         // refcounted array payloads participate in cycle collection
    emitter.instruction("cmp x15, #7");                                         // is this an array of mixed boxes?
    emitter.instruction("b.ne __rt_gc_collect_cycles_free_next");               // scalar/string arrays are never cycle-collector candidates
    emitter.label("__rt_gc_collect_cycles_free_refcounted");
    emitter.instruction("mov x15, #1");                                         // prepare the reachable-bit mask
    emitter.instruction("lsl x15, x15, #16");                                   // x15 = GC reachable bit in the kind word
    emitter.instruction("tst x13, x15");                                        // did the root-mark pass keep this block reachable?
    emitter.instruction("b.ne __rt_gc_collect_cycles_free_next");               // reachable blocks stay alive
    emitter.instruction("add x10, x9, x11");                                    // compute the next header before reclaiming this block
    emitter.instruction("add x10, x10, #16");                                   // account for the 16-byte heap header
    emitter.instruction("str x10, [sp, #0]");                                   // save the next header before deep-freeing this block
    emitter.instruction("add x0, x9, #16");                                     // convert the heap header back to the user pointer
    emitter.instruction("cmp x14, #2");                                         // is this an indexed array?
    emitter.instruction("b.eq __rt_gc_collect_cycles_free_array");              // deep-free unreachable arrays
    emitter.instruction("cmp x14, #3");                                         // is this an associative array / hash?
    emitter.instruction("b.eq __rt_gc_collect_cycles_free_hash");               // deep-free unreachable hashes
    emitter.instruction("cmp x14, #5");                                         // is this a boxed mixed cell?
    emitter.instruction("b.eq __rt_gc_collect_cycles_free_mixed");              // deep-free unreachable mixed cells
    emitter.instruction("bl __rt_object_free_deep");                            // deep-free unreachable objects
    emitter.instruction("b __rt_gc_collect_cycles_free_loop");                  // continue scanning from the saved next header
    emitter.label("__rt_gc_collect_cycles_free_array");
    emitter.instruction("bl __rt_array_free_deep");                             // deep-free the unreachable array graph node
    emitter.instruction("b __rt_gc_collect_cycles_free_loop");                  // continue scanning from the saved next header
    emitter.label("__rt_gc_collect_cycles_free_hash");
    emitter.instruction("bl __rt_hash_free_deep");                              // deep-free the unreachable hash graph node
    emitter.instruction("b __rt_gc_collect_cycles_free_loop");                  // continue scanning from the saved next header
    emitter.label("__rt_gc_collect_cycles_free_mixed");
    emitter.instruction("bl __rt_mixed_free_deep");                             // deep-free the unreachable mixed graph node
    emitter.instruction("b __rt_gc_collect_cycles_free_loop");                  // continue scanning from the saved next header
    emitter.label("__rt_gc_collect_cycles_free_next");
    emitter.instruction("add x9, x9, x11");                                     // advance by this block payload size
    emitter.instruction("add x9, x9, #16");                                     // skip the uniform 16-byte heap header
    emitter.instruction("str x9, [sp, #0]");                                    // save the next heap header scan pointer
    emitter.instruction("b __rt_gc_collect_cycles_free_loop");                  // continue scanning for unreachable graph nodes

    emitter.label("__rt_gc_collect_cycles_finish");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_gc_collecting");
    emitter.instruction("str xzr, [x9]");                                       // mark the collector as inactive again
    emitter.instruction("ldr x19, [sp, #48]");                                  // restore the callee-saved scratch register after collection
    emitter.instruction("ldr x20, [sp, #56]");                                  // restore the callee-saved payload-size register after collection
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // tear down the collector stack frame

    emitter.label("__rt_gc_collect_cycles_done");
    emitter.instruction("ret");                                                 // return to the caller
}
