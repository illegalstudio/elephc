use crate::codegen::emit::Emitter;

/// heap_debug_validate_free_list: verify sorted, non-overlapping free-list state.
pub fn emit_heap_debug_validate_free_list(emitter: &mut Emitter) {
    let msg = "Fatal error: heap debug detected free-list corruption\n";

    emitter.blank();
    emitter.comment("--- runtime: heap_debug_validate_free_list ---");
    emitter.label("__rt_heap_debug_validate_free_list");

    // -- load heap bounds and current free-list head --
    emitter.instruction("adrp x9, _heap_buf@PAGE");                              // load page of the heap buffer
    emitter.instruction("add x9, x9, _heap_buf@PAGEOFF");                        // resolve the heap buffer base address
    emitter.instruction("adrp x10, _heap_off@PAGE");                             // load page of the current heap offset
    emitter.instruction("add x10, x10, _heap_off@PAGEOFF");                      // resolve the heap offset address
    emitter.instruction("ldr x10, [x10]");                                       // load the current bump offset
    emitter.instruction("add x10, x9, x10");                                     // compute the current heap end address
    emitter.instruction("adrp x11, _heap_free_list@PAGE");                       // load page of the free-list head pointer
    emitter.instruction("add x11, x11, _heap_free_list@PAGEOFF");                // resolve the free-list head pointer address
    emitter.instruction("ldr x11, [x11]");                                       // x11 = current free block header

    emitter.label("__rt_heap_debug_validate_free_list_loop");
    emitter.instruction("cbz x11, __rt_heap_debug_validate_free_list_done");      // a null head means the free list is currently valid
    emitter.instruction("cmp x11, x9");                                           // does the free block begin below the heap base?
    emitter.instruction("b.lo __rt_heap_debug_validate_free_list_fail");          // blocks outside the heap buffer corrupt the free list
    emitter.instruction("cmp x11, x10");                                          // does the free block begin at or beyond the heap end?
    emitter.instruction("b.hs __rt_heap_debug_validate_free_list_fail");          // blocks outside the live heap region corrupt the free list
    emitter.instruction("ldr w12, [x11]");                                        // load the free block payload size
    emitter.instruction("cmp x12, #8");                                           // can the free block still hold the minimum payload?
    emitter.instruction("b.lo __rt_heap_debug_validate_free_list_fail");          // undersized free blocks indicate header corruption
    emitter.instruction("add x13, x11, x12");                                     // x13 = block header + payload size
    emitter.instruction("add x13, x13, #16");                                     // x13 = end of the free block including its 16-byte header
    emitter.instruction("cmp x13, x10");                                          // does the free block run past the current heap end?
    emitter.instruction("b.hi __rt_heap_debug_validate_free_list_fail");          // overrunning the heap end indicates corruption
    emitter.instruction("ldr x14, [x11, #16]");                                   // load the next free block header
    emitter.instruction("cbz x14, __rt_heap_debug_validate_free_list_done");      // the tail block is valid if all checks above passed
    emitter.instruction("cmp x14, x11");                                          // is the next block address strictly greater than the current one?
    emitter.instruction("b.ls __rt_heap_debug_validate_free_list_fail");          // cycles or descending addresses corrupt the ordered free list
    emitter.instruction("cmp x14, x10");                                          // does the next block start outside the live heap region?
    emitter.instruction("b.hs __rt_heap_debug_validate_free_list_fail");          // next pointers must stay inside the live heap region
    emitter.instruction("cmp x13, x14");                                          // does the current block overlap or touch the next one?
    emitter.instruction("b.hs __rt_heap_debug_validate_free_list_fail");          // adjacent or overlapping blocks should have been coalesced
    emitter.instruction("mov x11, x14");                                          // advance to the next free block
    emitter.instruction("b __rt_heap_debug_validate_free_list_loop");             // continue validating the ordered free list

    emitter.label("__rt_heap_debug_validate_free_list_fail");
    emitter.instruction("adrp x1, _heap_dbg_free_list_msg@PAGE");                 // load page of the free-list corruption message
    emitter.instruction("add x1, x1, _heap_dbg_free_list_msg@PAGEOFF");           // resolve the free-list corruption message address
    emitter.instruction(&format!("mov x2, #{}", msg.len()));                      // pass the exact free-list corruption message length
    emitter.instruction("b __rt_heap_debug_fail");                                // report corruption and terminate immediately

    emitter.label("__rt_heap_debug_validate_free_list_done");
    emitter.instruction("ret");                                                   // return once the free list has been fully validated
}
