use crate::codegen::emit::Emitter;

/// heap_debug_validate_free_list: verify ordered free-list state and small-bin caches.
pub fn emit_heap_debug_validate_free_list(emitter: &mut Emitter) {
    let msg = "Fatal error: heap debug detected free-list corruption\n";

    emitter.blank();
    emitter.comment("--- runtime: heap_debug_validate_free_list ---");
    emitter.label_global("__rt_heap_debug_validate_free_list");

    // -- load heap bounds and current free-list head --
    emitter.instruction("adrp x9, _heap_buf@PAGE");                             // load page of the heap buffer
    emitter.instruction("add x9, x9, _heap_buf@PAGEOFF");                       // resolve the heap buffer base address
    emitter.instruction("adrp x10, _heap_off@PAGE");                            // load page of the current heap offset
    emitter.instruction("add x10, x10, _heap_off@PAGEOFF");                     // resolve the heap offset address
    emitter.instruction("ldr x10, [x10]");                                      // load the current bump offset
    emitter.instruction("add x10, x9, x10");                                    // compute the current heap end address
    emitter.instruction("adrp x11, _heap_free_list@PAGE");                      // load page of the free-list head pointer
    emitter.instruction("add x11, x11, _heap_free_list@PAGEOFF");               // resolve the free-list head pointer address
    emitter.instruction("ldr x11, [x11]");                                      // x11 = current free block header

    emitter.label("__rt_heap_debug_validate_free_list_loop");
    emitter.instruction("cbz x11, __rt_heap_debug_validate_free_list_done");    // a null head means the free list is currently valid
    emitter.instruction("cmp x11, x9");                                         // does the free block begin below the heap base?
    emitter.instruction("b.lo __rt_heap_debug_validate_free_list_fail");        // blocks outside the heap buffer corrupt the free list
    emitter.instruction("cmp x11, x10");                                        // does the free block begin at or beyond the heap end?
    emitter.instruction("b.hs __rt_heap_debug_validate_free_list_fail");        // blocks outside the live heap region corrupt the free list
    emitter.instruction("ldr w12, [x11]");                                      // load the free block payload size
    emitter.instruction("cmp x12, #8");                                         // can the free block still hold the minimum payload?
    emitter.instruction("b.lo __rt_heap_debug_validate_free_list_fail");        // undersized free blocks indicate header corruption
    emitter.instruction("add x13, x11, x12");                                   // x13 = block header + payload size
    emitter.instruction("add x13, x13, #16");                                   // x13 = end of the free block including its 16-byte header
    emitter.instruction("cmp x13, x10");                                        // does the free block run past the current heap end?
    emitter.instruction("b.hi __rt_heap_debug_validate_free_list_fail");        // overrunning the heap end indicates corruption
    emitter.instruction("ldr x14, [x11, #16]");                                 // load the next free block header
    emitter.instruction("cbz x14, __rt_heap_debug_validate_free_list_done");    // the tail block is valid if all checks above passed
    emitter.instruction("cmp x14, x11");                                        // is the next block address strictly greater than the current one?
    emitter.instruction("b.ls __rt_heap_debug_validate_free_list_fail");        // cycles or descending addresses corrupt the ordered free list
    emitter.instruction("cmp x14, x10");                                        // does the next block start outside the live heap region?
    emitter.instruction("b.hs __rt_heap_debug_validate_free_list_fail");        // next pointers must stay inside the live heap region
    emitter.instruction("cmp x13, x14");                                        // does the current block overlap or touch the next one?
    emitter.instruction("b.hs __rt_heap_debug_validate_free_list_fail");        // adjacent or overlapping blocks should have been coalesced
    emitter.instruction("mov x11, x14");                                        // advance to the next free block
    emitter.instruction("b __rt_heap_debug_validate_free_list_loop");           // continue validating the ordered free list

    emitter.label("__rt_heap_debug_validate_free_list_fail");
    emitter.instruction("adrp x1, _heap_dbg_free_list_msg@PAGE");               // load page of the free-list corruption message
    emitter.instruction("add x1, x1, _heap_dbg_free_list_msg@PAGEOFF");         // resolve the free-list corruption message address
    emitter.instruction(&format!("mov x2, #{}", msg.len()));                    // pass the exact free-list corruption message length
    emitter.instruction("b __rt_heap_debug_fail");                              // report corruption and terminate immediately

    // -- small segregated bins must also point at valid cached blocks --
    emitter.label("__rt_heap_debug_validate_free_list_done");
    emitter.instruction("adrp x11, _heap_small_bins@PAGE");                     // load page of the segregated small-bin head array
    emitter.instruction("add x11, x11, _heap_small_bins@PAGEOFF");              // resolve the segregated small-bin head array address
    emitter.instruction("mov x12, #0");                                         // start with the <=8-byte bin offset

    emitter.label("__rt_heap_debug_validate_small_bins");
    emitter.instruction("cmp x12, #32");                                        // have we validated all four small-bin classes?
    emitter.instruction("b.eq __rt_heap_debug_validate_free_list_ret");         // yes — the entire cached free state is valid
    emitter.instruction("add x13, x11, x12");                                   // x13 = address of the current small-bin head slot
    emitter.instruction("ldr x14, [x13]");                                      // x14 = current cached block header for this size class
    emitter.instruction("mov x15, #0");                                         // x15 = exclusive lower bound for this bin's payload size
    emitter.instruction("mov x16, #8");                                         // x16 = inclusive upper bound for this bin's payload size
    emitter.instruction("cmp x12, #0");                                         // are we validating the <=8-byte bin?
    emitter.instruction("b.eq __rt_heap_debug_validate_small_bin_ready");       // yes — keep the default bounds
    emitter.instruction("mov x15, #8");                                         // otherwise start with the <=16-byte class bounds
    emitter.instruction("mov x16, #16");                                        // set the inclusive upper bound for the <=16-byte class
    emitter.instruction("cmp x12, #8");                                         // are we validating the <=16-byte bin?
    emitter.instruction("b.eq __rt_heap_debug_validate_small_bin_ready");       // yes — keep the <=16-byte bounds
    emitter.instruction("mov x15, #16");                                        // otherwise start with the <=32-byte class bounds
    emitter.instruction("mov x16, #32");                                        // set the inclusive upper bound for the <=32-byte class
    emitter.instruction("cmp x12, #16");                                        // are we validating the <=32-byte bin?
    emitter.instruction("b.eq __rt_heap_debug_validate_small_bin_ready");       // yes — keep the <=32-byte bounds
    emitter.instruction("mov x15, #32");                                        // the remaining case is the <=64-byte class
    emitter.instruction("mov x16, #64");                                        // set the inclusive upper bound for the <=64-byte class

    emitter.label("__rt_heap_debug_validate_small_bin_ready");
    emitter.instruction("adrp x13, _heap_off@PAGE");                            // load page of the current heap offset for chain-length budgeting
    emitter.instruction("add x13, x13, _heap_off@PAGEOFF");                     // resolve the heap offset address
    emitter.instruction("ldr x13, [x13]");                                      // x13 = total live heap bytes available to bound the cached chain walk

    emitter.label("__rt_heap_debug_validate_small_bin_loop");
    emitter.instruction("cbz x14, __rt_heap_debug_validate_small_bin_next");    // an empty chain means this size class is valid
    emitter.instruction("subs x13, x13, #24");                                  // consume the minimum block footprint from the validation budget
    emitter.instruction("b.lo __rt_heap_debug_validate_free_list_fail");        // overly long or cyclic cached chains indicate corruption
    emitter.instruction("cmp x14, x9");                                         // does the cached block begin below the heap base?
    emitter.instruction("b.lo __rt_heap_debug_validate_free_list_fail");        // cached blocks must stay inside the heap buffer
    emitter.instruction("cmp x14, x10");                                        // does the cached block begin at or beyond the heap end?
    emitter.instruction("b.hs __rt_heap_debug_validate_free_list_fail");        // cached blocks outside the live heap region indicate corruption
    emitter.instruction("ldr w17, [x14]");                                      // load the cached block payload size
    emitter.instruction("cmp x17, #8");                                         // can the cached block still hold the minimum payload?
    emitter.instruction("b.lo __rt_heap_debug_validate_free_list_fail");        // undersized cached blocks indicate header corruption
    emitter.instruction("cmp x17, x15");                                        // is the cached block too small for this size class?
    emitter.instruction("b.ls __rt_heap_debug_validate_free_list_fail");        // the block belongs in a smaller bin, so the cache is corrupt
    emitter.instruction("cmp x17, x16");                                        // is the cached block too large for this size class?
    emitter.instruction("b.hi __rt_heap_debug_validate_free_list_fail");        // the block belongs in a larger structure, so the cache is corrupt
    emitter.instruction("add x17, x14, x17");                                   // x17 = header + payload size
    emitter.instruction("add x17, x17, #16");                                   // x17 = end of the cached block including its 16-byte header
    emitter.instruction("cmp x17, x10");                                        // does the cached block run past the current heap end?
    emitter.instruction("b.hi __rt_heap_debug_validate_free_list_fail");        // cached blocks must remain fully inside the live heap window
    emitter.instruction("ldr x14, [x14, #16]");                                 // advance to the next cached block in this size class
    emitter.instruction("b __rt_heap_debug_validate_small_bin_loop");           // continue validating this cached small-bin chain

    emitter.label("__rt_heap_debug_validate_small_bin_next");
    emitter.instruction("add x12, x12, #8");                                    // advance to the next small-bin head slot
    emitter.instruction("b __rt_heap_debug_validate_small_bins");               // validate the remaining segregated small bins

    emitter.label("__rt_heap_debug_validate_free_list_ret");
    emitter.instruction("ret");                                                 // return once the free list and small bins have been fully validated
}
