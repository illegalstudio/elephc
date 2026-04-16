use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// gc_collect_cycles: reclaim unreachable refcounted array/hash/object graphs.
/// The collector uses existing refcounts plus a transient heap-edge count stored
/// in the upper 32 bits of the 64-bit heap kind word.
pub fn emit_gc_collect_cycles(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_gc_collect_cycles_linux_x86_64(emitter);
        return;
    }

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

fn emit_gc_collect_cycles_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: gc_collect_cycles ---");
    emitter.label_global("__rt_gc_collect_cycles");

    // -- avoid recursive re-entry while the collector is already running --
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_gc_collecting");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current collector-active flag before starting a new x86_64 collection pass
    emitter.instruction("test r9, r9");                                         // is the collector already running?
    emitter.instruction("jnz __rt_gc_collect_cycles_done");                     // nested collection attempts are ignored to avoid recursive frees
    emitter.instruction("mov QWORD PTR [r8], 1");                               // mark the collector active for the duration of this x86_64 cycle pass

    // -- set up a collector frame --
    // Stack layout:
    //   [rbp - 8]  = heap base
    //   [rbp - 16] = initial heap end
    //   [rbp - 24] = outer scan header
    //   [rbp - 32] = current target user pointer
    //   [rbp - 40] = scratch / saved next header
    //   [rbp - 48] = incoming heap-edge count for the current target
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving x86_64 collector locals
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame pointer for the x86_64 collector locals
    emitter.instruction("sub rsp, 48");                                         // reserve collector locals for heap bounds, scan pointers, and incoming counts

    // -- capture heap bounds once for the current collection pass --
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_heap_buf");
    emitter.instruction("mov QWORD PTR [rbp - 8], r8");                         // save the heap base so every collector pass can restart from the same managed heap window
    crate::codegen::abi::emit_symbol_address(emitter, "r9", "_heap_off");
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // load the current heap bump offset before capturing the initial heap end
    emitter.instruction("lea r9, [r8 + r9]");                                   // compute the initial heap end from the heap base plus bump offset
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // save the initial heap end for all x86_64 metadata, root, and free scans
    emitter.instruction("mov QWORD PTR [rbp - 24], r8");                        // initialize the outer scan pointer to the heap base for the clear pass

    // -- pass 1: clear the x86_64 reachable bit while preserving kind + array value_type + heap marker --
    emitter.label("__rt_gc_collect_cycles_clear_loop");
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // reload the current heap header scan pointer for the metadata-clear pass
    emitter.instruction("cmp r8, QWORD PTR [rbp - 16]");                        // have we reached the initial heap end for this collection pass?
    emitter.instruction("jae __rt_gc_collect_cycles_root_init");                // yes — move on once every live block has its transient x86_64 mark cleared
    emitter.instruction("mov r9d, DWORD PTR [r8]");                             // load this block payload size before advancing to the next heap header
    emitter.instruction("mov r10d, DWORD PTR [r8 + 4]");                        // load this block refcount so free-list blocks can be skipped during metadata clearing
    emitter.instruction("test r10d, r10d");                                     // is this heap block currently live?
    emitter.instruction("jz __rt_gc_collect_cycles_clear_next");                // free-list blocks already keep transient GC metadata cleared
    emitter.instruction("mov r11, QWORD PTR [r8 + 8]");                         // load the full kind word with any stale x86_64 reachable metadata
    emitter.instruction("mov rcx, 0xffffffff0000ffff");                         // preserve the high-word heap marker and low 16 bits while clearing the transient x86_64 mark range
    emitter.instruction("and r11, rcx");                                        // clear the x86_64 transient reachable metadata while preserving kind and value_type bits
    emitter.instruction("mov QWORD PTR [r8 + 8], r11");                         // persist the cleared x86_64 kind word back into the heap header
    emitter.label("__rt_gc_collect_cycles_clear_next");
    emitter.instruction("add r8, r9");                                          // advance by the current block payload size
    emitter.instruction("add r8, 16");                                          // skip the uniform 16-byte heap header to reach the next block header
    emitter.instruction("mov QWORD PTR [rbp - 24], r8");                        // save the next heap header scan pointer for the following clear iteration
    emitter.instruction("jmp __rt_gc_collect_cycles_clear_loop");               // continue clearing transient metadata across the managed heap window

    // -- pass 2: find externally rooted nodes by recounting incoming heap edges on demand --
    emitter.label("__rt_gc_collect_cycles_root_init");
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the heap base before starting the externally rooted node scan
    emitter.instruction("mov QWORD PTR [rbp - 24], r8");                        // restart the outer scan pointer at the heap base
    emitter.label("__rt_gc_collect_cycles_root_loop");
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // reload the current candidate heap header for the x86_64 root scan
    emitter.instruction("cmp r8, QWORD PTR [rbp - 16]");                        // have we scanned every block in the initial heap window?
    emitter.instruction("jae __rt_gc_collect_cycles_free_init");                // yes — move on to freeing the still-unreachable graph nodes
    emitter.instruction("mov r9d, DWORD PTR [r8]");                             // load this candidate block payload size before any nested rescans
    emitter.instruction("mov r10d, DWORD PTR [r8 + 4]");                        // load this candidate block refcount from the heap header
    emitter.instruction("test r10d, r10d");                                     // is this candidate block live?
    emitter.instruction("jz __rt_gc_collect_cycles_root_next");                 // free-list blocks cannot be GC roots
    emitter.instruction("mov r11, QWORD PTR [r8 + 8]");                         // load the candidate kind word before deciding whether it participates in cycle collection
    emitter.instruction("mov rcx, r11");                                        // preserve the full kind word while isolating the low-byte heap kind tag
    emitter.instruction("and rcx, 0xff");                                       // isolate the low-byte heap kind tag for candidate dispatch
    emitter.instruction("cmp rcx, 2");                                          // is this candidate at least an indexed array?
    emitter.instruction("jb __rt_gc_collect_cycles_root_next");                 // strings and raw buffers never participate in cycle collection
    emitter.instruction("cmp rcx, 5");                                          // is this candidate within the array/hash/object/mixed range?
    emitter.instruction("ja __rt_gc_collect_cycles_root_next");                 // unknown/raw heap kinds are ignored by the collector
    emitter.instruction("cmp rcx, 2");                                          // is this candidate an indexed array?
    emitter.instruction("jne __rt_gc_collect_cycles_root_candidate_ready");     // hashes, objects, and mixed boxes remain collector candidates
    emitter.instruction("mov rdx, r11");                                        // preserve the full array kind word while unpacking the runtime array value_type tag
    emitter.instruction("shr rdx, 8");                                          // move the packed array value_type tag into the low bits
    emitter.instruction("and rdx, 0x7f");                                       // isolate the array value_type without the persistent COW flag bit
    emitter.instruction("cmp rdx, 4");                                          // does this array contain refcounted element payloads?
    emitter.instruction("jb __rt_gc_collect_cycles_root_next");                 // scalar and string arrays cannot participate in heap cycles
    emitter.instruction("cmp rdx, 7");                                          // is the array value_type within the supported refcounted range?
    emitter.instruction("ja __rt_gc_collect_cycles_root_next");                 // unknown array payload tags are ignored by the collector
    emitter.label("__rt_gc_collect_cycles_root_candidate_ready");
    emitter.instruction("lea rdx, [r8 + 16]");                                  // compute the current candidate user pointer for the incoming-edge recount
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save the candidate user pointer across the nested full-heap rescan
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // reset the incoming heap-edge count before rescanning the entire heap
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // reload the heap base before starting the nested incoming-edge rescan
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // initialize the nested scan pointer to the heap base

    // -- nested rescan: count heap edges that point at the current candidate --
    emitter.label("__rt_gc_collect_cycles_count_loop");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the current source heap header for the incoming-edge rescan
    emitter.instruction("cmp rdx, QWORD PTR [rbp - 16]");                       // have we finished rescanning the initial heap window?
    emitter.instruction("jae __rt_gc_collect_cycles_root_compare");             // yes — compare the candidate refcount against the recounted incoming edges
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the current candidate user pointer for outgoing-edge comparisons
    emitter.instruction("mov ecx, DWORD PTR [rdx]");                            // load this source block payload size before deciding whether it emits outgoing edges
    emitter.instruction("mov rdi, rcx");                                        // preserve the source block payload size across the outgoing-edge dispatch loops
    emitter.instruction("mov r8d, DWORD PTR [rdx + 4]");                        // load the source block refcount so free-list blocks can be skipped
    emitter.instruction("test r8d, r8d");                                       // is this source block live?
    emitter.instruction("jz __rt_gc_collect_cycles_count_next");                // free blocks contribute no outgoing graph edges
    emitter.instruction("mov r9, QWORD PTR [rdx + 8]");                         // load the source kind word before dispatching on its outgoing edge layout
    emitter.instruction("mov r10, r9");                                         // preserve the full source kind word while isolating the low-byte heap kind tag
    emitter.instruction("and r10, 0xff");                                       // isolate the source heap kind tag for outgoing-edge dispatch
    emitter.instruction("cmp r10, 2");                                          // is this source at least an indexed array?
    emitter.instruction("jb __rt_gc_collect_cycles_count_next");                // strings and raw buffers contribute no outgoing cycle edges
    emitter.instruction("cmp r10, 5");                                          // is this source within the array/hash/object/mixed range?
    emitter.instruction("ja __rt_gc_collect_cycles_count_next");                // unknown/raw heap kinds are ignored by the collector
    emitter.instruction("cmp r10, 2");                                          // is the source block an indexed array?
    emitter.instruction("je __rt_gc_collect_cycles_count_array");               // yes — scan array child slots
    emitter.instruction("cmp r10, 3");                                          // is the source block an associative array / hash?
    emitter.instruction("je __rt_gc_collect_cycles_count_hash");                // yes — scan hash entry child payloads
    emitter.instruction("cmp r10, 5");                                          // is the source block a boxed mixed cell?
    emitter.instruction("je __rt_gc_collect_cycles_count_mixed");               // yes — compare the boxed child pointer against the candidate
    emitter.instruction("jmp __rt_gc_collect_cycles_count_object");             // the remaining refcounted heap kind is an object instance

    emitter.label("__rt_gc_collect_cycles_count_array");
    emitter.instruction("mov r10, r9");                                         // preserve the full array kind word while unpacking the runtime array value_type tag
    emitter.instruction("shr r10, 8");                                          // move the packed array value_type tag into the low bits
    emitter.instruction("and r10, 0x7f");                                       // isolate the array value_type without the persistent COW flag bit
    emitter.instruction("cmp r10, 4");                                          // does this array contain refcounted element payloads?
    emitter.instruction("jb __rt_gc_collect_cycles_count_next");                // scalar and string arrays emit no refcounted outgoing edges
    emitter.instruction("cmp r10, 7");                                          // is the array value_type within the supported refcounted range?
    emitter.instruction("ja __rt_gc_collect_cycles_count_next");                // unknown array payload tags are ignored by the collector
    emitter.instruction("lea r9, [rdx + 16]");                                  // compute the source array user pointer from its heap header
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the source array length before iterating its child slots
    emitter.instruction("xor r11, r11");                                        // initialize the source array index to zero for the incoming-edge comparison loop
    emitter.label("__rt_gc_collect_cycles_count_array_loop");
    emitter.instruction("cmp r11, r10");                                        // have we inspected every child slot in this source array?
    emitter.instruction("jae __rt_gc_collect_cycles_count_next");               // yes — move on to the next source block in the rescan
    emitter.instruction("mov r8, r11");                                         // preserve the logical array index while scaling it into a byte offset
    emitter.instruction("shl r8, 3");                                           // scale the array index by eight bytes per child pointer slot
    emitter.instruction("add r8, 24");                                          // skip the 24-byte array header to reach the selected child slot
    emitter.instruction("cmp QWORD PTR [r9 + r8], rsi");                        // does this source array child point at the current candidate node?
    emitter.instruction("jne __rt_gc_collect_cycles_count_array_next");         // no — this child does not contribute an incoming edge to the candidate
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // count one incoming heap edge from this array slot into the current candidate
    emitter.label("__rt_gc_collect_cycles_count_array_next");
    emitter.instruction("add r11, 1");                                          // advance to the next child slot in the source array
    emitter.instruction("jmp __rt_gc_collect_cycles_count_array_loop");         // continue comparing source array children against the current candidate

    emitter.label("__rt_gc_collect_cycles_count_hash");
    emitter.instruction("lea r9, [rdx + 16]");                                  // compute the source hash user pointer from its heap header
    emitter.instruction("mov r10, QWORD PTR [r9 + 8]");                         // load the source hash capacity before iterating its entry slots
    emitter.instruction("xor r11, r11");                                        // initialize the source hash slot index to zero for the incoming-edge rescan
    emitter.label("__rt_gc_collect_cycles_count_hash_loop");
    emitter.instruction("cmp r11, r10");                                        // have we inspected every hash entry slot?
    emitter.instruction("jae __rt_gc_collect_cycles_count_next");               // yes — move on to the next source block in the rescan
    emitter.instruction("mov r8, r11");                                         // preserve the logical hash slot index while scaling it into an entry byte offset
    emitter.instruction("imul r8, 64");                                         // scale the slot index by 64 bytes per hash entry
    emitter.instruction("add r8, 40");                                          // skip the 40-byte hash header to reach the selected entry
    emitter.instruction("lea r8, [r9 + r8]");                                   // compute the address of the selected hash entry inside the source hash table
    emitter.instruction("cmp QWORD PTR [r8], 1");                               // is this hash entry occupied?
    emitter.instruction("jne __rt_gc_collect_cycles_count_hash_next");          // skip empty and tombstone entries that carry no outgoing edge
    emitter.instruction("mov rax, QWORD PTR [r8 + 40]");                        // load the runtime value_tag stored for this hash entry
    emitter.instruction("cmp rax, 4");                                          // does this hash entry hold a heap-backed child?
    emitter.instruction("jb __rt_gc_collect_cycles_count_hash_next");           // scalar and string entries contribute no incoming edge to the candidate
    emitter.instruction("cmp rax, 7");                                          // is the runtime value_tag within the supported heap-backed range?
    emitter.instruction("ja __rt_gc_collect_cycles_count_hash_next");           // unknown runtime tags are ignored by the collector
    emitter.instruction("cmp QWORD PTR [r8 + 24], rsi");                        // does this hash value payload point at the current candidate node?
    emitter.instruction("jne __rt_gc_collect_cycles_count_hash_next");          // no — this entry does not contribute an incoming edge to the candidate
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // count one incoming heap edge from this hash entry into the current candidate
    emitter.label("__rt_gc_collect_cycles_count_hash_next");
    emitter.instruction("add r11, 1");                                          // advance to the next hash entry slot in the source table
    emitter.instruction("jmp __rt_gc_collect_cycles_count_hash_loop");          // continue comparing source hash children against the current candidate

    emitter.label("__rt_gc_collect_cycles_count_mixed");
    emitter.instruction("lea r9, [rdx + 16]");                                  // compute the source mixed-box user pointer from its heap header
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the boxed mixed runtime value_tag before comparing the boxed child pointer
    emitter.instruction("cmp r10, 4");                                          // does this boxed mixed value hold a heap-backed child?
    emitter.instruction("jb __rt_gc_collect_cycles_count_next");                // scalar, string, and null boxed values contribute no incoming edge
    emitter.instruction("cmp r10, 7");                                          // is the boxed runtime tag within the supported heap-backed range?
    emitter.instruction("ja __rt_gc_collect_cycles_count_next");                // unknown boxed runtime tags are ignored by the collector
    emitter.instruction("cmp QWORD PTR [r9 + 8], rsi");                         // does the boxed mixed child pointer equal the current candidate node?
    emitter.instruction("jne __rt_gc_collect_cycles_count_next");               // no — this mixed box does not contribute an incoming edge to the candidate
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // count one incoming heap edge from this mixed box into the current candidate
    emitter.instruction("jmp __rt_gc_collect_cycles_count_next");               // boxed mixed-child comparison is complete for this source block

    emitter.label("__rt_gc_collect_cycles_count_object");
    emitter.instruction("lea r9, [rdx + 16]");                                  // compute the source object user pointer from its heap header
    emitter.instruction("mov eax, DWORD PTR [rdx]");                            // load the source object payload size before deriving the property count
    emitter.instruction("sub rax, 8");                                          // subtract the leading class_id field from the object payload size
    emitter.instruction("shr rax, 4");                                          // divide by 16 to get the source object property count
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the runtime class_id stored at the start of the source object payload
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_class_gc_desc_count");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the number of emitted class GC descriptors for bounds checking
    emitter.instruction("cmp r10, r11");                                        // is the runtime class_id within the emitted descriptor table range?
    emitter.instruction("jae __rt_gc_collect_cycles_count_next");               // invalid class ids contribute no traversable property metadata
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_class_gc_desc_ptrs");
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // load the per-class property-tag descriptor pointer for the source object
    emitter.instruction("xor r10, r10");                                        // initialize the source object property index to zero for the incoming-edge scan
    emitter.label("__rt_gc_collect_cycles_count_object_loop");
    emitter.instruction("cmp r10, rax");                                        // have we inspected every property slot in this source object?
    emitter.instruction("jae __rt_gc_collect_cycles_count_next");               // yes — move on to the next source block in the rescan
    emitter.instruction("movzx ecx, BYTE PTR [r11 + r10]");                     // load the compile-time property tag for the selected source object property
    emitter.instruction("mov r8, r10");                                         // preserve the logical property index while scaling it into a byte offset
    emitter.instruction("imul r8, 16");                                         // scale the property index by 16 bytes per object property slot
    emitter.instruction("add r8, 8");                                           // skip the leading class_id field to reach the selected property slot
    emitter.instruction("cmp rcx, 4");                                          // is this property statically typed as an indexed array?
    emitter.instruction("je __rt_gc_collect_cycles_count_object_child");        // yes — compare the direct property child pointer against the current candidate
    emitter.instruction("cmp rcx, 5");                                          // is this property statically typed as an associative array?
    emitter.instruction("je __rt_gc_collect_cycles_count_object_child");        // yes — compare the direct property child pointer against the current candidate
    emitter.instruction("cmp rcx, 6");                                          // is this property statically typed as an object?
    emitter.instruction("je __rt_gc_collect_cycles_count_object_child");        // yes — compare the direct property child pointer against the current candidate
    emitter.instruction("cmp rcx, 7");                                          // is this property statically typed as a mixed slot?
    emitter.instruction("jne __rt_gc_collect_cycles_count_object_next");        // scalar and string properties contribute no incoming heap edges
    emitter.instruction("mov rcx, QWORD PTR [r9 + r8 + 8]");                    // load the runtime tag stored alongside the mixed property payload
    emitter.instruction("cmp rcx, 4");                                          // does the mixed property currently hold a heap-backed child?
    emitter.instruction("jb __rt_gc_collect_cycles_count_object_next");         // scalar, string, and null mixed payloads contribute no incoming edge
    emitter.instruction("cmp rcx, 7");                                          // is the mixed runtime tag within the supported heap-backed range?
    emitter.instruction("ja __rt_gc_collect_cycles_count_object_next");         // unknown mixed runtime tags are ignored by the collector
    emitter.label("__rt_gc_collect_cycles_count_object_child");
    emitter.instruction("cmp QWORD PTR [r9 + r8], rsi");                        // does the selected object property point at the current candidate node?
    emitter.instruction("jne __rt_gc_collect_cycles_count_object_next");        // no — this property does not contribute an incoming heap edge
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // count one incoming heap edge from this object property into the candidate
    emitter.label("__rt_gc_collect_cycles_count_object_next");
    emitter.instruction("add r10, 1");                                          // advance to the next object property slot in the source object
    emitter.instruction("jmp __rt_gc_collect_cycles_count_object_loop");        // continue comparing source object property children against the candidate

    emitter.label("__rt_gc_collect_cycles_count_next");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the current source heap header after any nested child scans
    emitter.instruction("add rdx, rdi");                                        // advance by the preserved source block payload size to reach the next header candidate
    emitter.instruction("add rdx, 16");                                         // skip the uniform 16-byte heap header to reach the next source block header
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // persist the next source heap header for the incoming-edge rescan loop
    emitter.instruction("jmp __rt_gc_collect_cycles_count_loop");               // continue rescanning heap edges into the current candidate node

    emitter.label("__rt_gc_collect_cycles_root_compare");
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // reload the current candidate heap header after the nested incoming-edge rescan clobbered caller-saved registers
    emitter.instruction("mov r10d, DWORD PTR [r8 + 4]");                        // reload the candidate refcount after the nested full-heap incoming-edge recount
    emitter.instruction("cmp r10, QWORD PTR [rbp - 48]");                       // does this candidate still have an external reference beyond heap-internal edges?
    emitter.instruction("jbe __rt_gc_collect_cycles_root_next");                // no — refcount less than or equal to incoming edges means the node is only heap-rooted
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the candidate user pointer before marking it reachable from an external root
    emitter.instruction("call __rt_gc_mark_reachable");                         // recursively mark the externally rooted graph component reachable

    emitter.label("__rt_gc_collect_cycles_root_next");
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // reload the current candidate heap header after any nested mark traversal
    emitter.instruction("mov r9d, DWORD PTR [r8]");                             // reload the candidate payload size so the outer scan can advance correctly
    emitter.instruction("add r8, r9");                                          // advance by the candidate payload size to the next heap header
    emitter.instruction("add r8, 16");                                          // skip the uniform 16-byte heap header to reach the next outer candidate
    emitter.instruction("mov QWORD PTR [rbp - 24], r8");                        // persist the next candidate heap header for the outer root scan
    emitter.instruction("jmp __rt_gc_collect_cycles_root_loop");                // continue looking for externally rooted graph nodes

    // -- pass 3: free every still-unreachable live refcounted node --
    emitter.label("__rt_gc_collect_cycles_free_init");
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the heap base before starting the unreachable-node free scan
    emitter.instruction("mov QWORD PTR [rbp - 24], r8");                        // restart the outer scan pointer at the heap base for the free pass
    emitter.label("__rt_gc_collect_cycles_free_loop");
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // reload the current heap header for the unreachable-node free scan
    emitter.instruction("cmp r8, QWORD PTR [rbp - 16]");                        // have we scanned every block in the initial heap window?
    emitter.instruction("jae __rt_gc_collect_cycles_finish");                   // yes — finish once every initial live block has been checked
    emitter.instruction("mov r9d, DWORD PTR [r8]");                             // load this block payload size before saving the next header across any deep frees
    emitter.instruction("lea r10, [r8 + r9 + 16]");                             // compute the next heap header before freeing the current node can mutate allocator state
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save the next heap header so the scan can continue after a deep free
    emitter.instruction("mov r10d, DWORD PTR [r8 + 4]");                        // load this block refcount to skip already-free nodes during the free pass
    emitter.instruction("test r10d, r10d");                                     // is this block still live?
    emitter.instruction("jz __rt_gc_collect_cycles_free_next");                 // free-list blocks are already reclaimed and need no collector action
    emitter.instruction("mov r11, QWORD PTR [r8 + 8]");                         // load the current kind word before deciding whether this block is a collector candidate
    emitter.instruction("mov rcx, r11");                                        // preserve the full kind word while isolating the low-byte heap kind tag
    emitter.instruction("and rcx, 0xff");                                       // isolate the low-byte heap kind tag for free-pass dispatch
    emitter.instruction("cmp rcx, 2");                                          // is this block at least an indexed array?
    emitter.instruction("jb __rt_gc_collect_cycles_free_next");                 // strings and raw buffers are outside the cycle collector set
    emitter.instruction("cmp rcx, 5");                                          // is this block within the array/hash/object/mixed range?
    emitter.instruction("ja __rt_gc_collect_cycles_free_next");                 // unknown/raw heap kinds are ignored by the collector
    emitter.instruction("cmp rcx, 2");                                          // is this block an indexed array?
    emitter.instruction("jne __rt_gc_collect_cycles_free_candidate_ready");     // hashes, objects, and mixed boxes remain collector candidates
    emitter.instruction("mov rdx, r11");                                        // preserve the full array kind word while unpacking the runtime array value_type tag
    emitter.instruction("shr rdx, 8");                                          // move the packed array value_type tag into the low bits
    emitter.instruction("and rdx, 0x7f");                                       // isolate the array value_type without the persistent COW flag bit
    emitter.instruction("cmp rdx, 4");                                          // does this array contain refcounted element payloads?
    emitter.instruction("jb __rt_gc_collect_cycles_free_next");                 // scalar and string arrays cannot participate in heap cycles
    emitter.instruction("cmp rdx, 7");                                          // is the array value_type within the supported refcounted range?
    emitter.instruction("ja __rt_gc_collect_cycles_free_next");                 // unknown array payload tags are ignored by the collector
    emitter.label("__rt_gc_collect_cycles_free_candidate_ready");
    emitter.instruction("test r11, 0x10000");                                   // was this live block marked reachable from an external root during the root pass?
    emitter.instruction("jnz __rt_gc_collect_cycles_free_next");                // yes — reachable graph nodes remain live
    emitter.instruction("mov DWORD PTR [r8 + 4], 0");                           // pre-clear the doomed node refcount so back-edges released during deep-free cannot recursively reclaim it again
    emitter.instruction("lea rax, [r8 + 16]");                                  // compute the current user pointer before dispatching to the deep-free helper
    emitter.instruction("cmp rcx, 2");                                          // is this unreachable node an indexed array?
    emitter.instruction("je __rt_gc_collect_cycles_free_array");                // yes — deep-free the unreachable array and its child payloads
    emitter.instruction("cmp rcx, 3");                                          // is this unreachable node an associative array / hash?
    emitter.instruction("je __rt_gc_collect_cycles_free_hash");                 // yes — deep-free the unreachable hash and its owned entries
    emitter.instruction("cmp rcx, 5");                                          // is this unreachable node a boxed mixed cell?
    emitter.instruction("je __rt_gc_collect_cycles_free_mixed");                // yes — deep-free the unreachable mixed box and its boxed child
    emitter.instruction("call __rt_object_free_deep");                          // deep-free the remaining unreachable object node and its properties
    emitter.instruction("jmp __rt_gc_collect_cycles_free_next");                // continue scanning from the saved next header after freeing the object node
    emitter.label("__rt_gc_collect_cycles_free_array");
    emitter.instruction("call __rt_array_free_deep");                           // deep-free the unreachable array node and its nested payloads
    emitter.instruction("jmp __rt_gc_collect_cycles_free_next");                // continue scanning from the saved next header after freeing the array node
    emitter.label("__rt_gc_collect_cycles_free_hash");
    emitter.instruction("call __rt_hash_free_deep");                            // deep-free the unreachable hash node and its owned entries
    emitter.instruction("jmp __rt_gc_collect_cycles_free_next");                // continue scanning from the saved next header after freeing the hash node
    emitter.label("__rt_gc_collect_cycles_free_mixed");
    emitter.instruction("call __rt_mixed_free_deep");                           // deep-free the unreachable mixed box and its boxed child

    emitter.label("__rt_gc_collect_cycles_free_next");
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // reload the next saved heap header after any deep free mutated allocator state
    emitter.instruction("mov QWORD PTR [rbp - 24], r8");                        // persist the next heap header so the free pass can continue scanning
    emitter.instruction("jmp __rt_gc_collect_cycles_free_loop");                // continue scanning the initial heap window for unreachable graph nodes

    emitter.label("__rt_gc_collect_cycles_finish");
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_gc_collecting");
    emitter.instruction("mov QWORD PTR [r8], 0");                               // clear the collector-active flag now that the x86_64 cycle pass is complete
    emitter.instruction("leave");                                               // tear down the x86_64 collector frame before returning to generated code

    emitter.label("__rt_gc_collect_cycles_done");
    emitter.instruction("ret");                                                 // return immediately when collection is skipped or after a full x86_64 cycle pass
}
