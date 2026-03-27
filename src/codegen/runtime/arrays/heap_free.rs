use crate::codegen::emit::Emitter;

/// heap_free: return a heap block to the free list.
/// The block header (8 bytes before user pointer) contains the block size.
///
/// Optimization: if the block is at the END of the heap (most recently
/// bump-allocated), just decrement the bump pointer instead of adding to
/// the free list. This makes .= loops O(1) with zero fragmentation.
///
/// Otherwise, inserts at the head of the free list (LIFO).
/// Free block layout: [size:8][next_ptr:8][...unused...]
/// Input: x0 = user pointer (as returned by heap_alloc)
pub fn emit_heap_free(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: heap_free ---");
    emitter.label("__rt_heap_free");

    // -- validate pointer is not null --
    emitter.instruction("cbz x0, __rt_heap_free_done");                         // skip if null pointer

    // -- compute header address and block end --
    emitter.instruction("sub x9, x0, #8");                                      // x9 = header address (block_size lives here)
    emitter.instruction("ldr w11, [x9]");                                       // x11 = block size (32-bit, zero-extends)

    // -- check if this is the last bump-allocated block --
    emitter.instruction("add x12, x0, x11");                                    // x12 = user_ptr + size = block end
    emitter.instruction("adrp x13, _heap_off@PAGE");                            // load page of heap offset
    emitter.instruction("add x13, x13, _heap_off@PAGEOFF");                     // resolve heap offset address
    emitter.instruction("ldr x14, [x13]");                                      // x14 = current heap offset
    emitter.instruction("adrp x15, _heap_buf@PAGE");                            // load page of heap buffer
    emitter.instruction("add x15, x15, _heap_buf@PAGEOFF");                     // resolve heap buffer base
    emitter.instruction("add x14, x15, x14");                                   // x14 = heap_buf + heap_off = heap end
    emitter.instruction("cmp x12, x14");                                        // is block end == heap end?
    emitter.instruction("b.ne __rt_heap_free_list");                            // no — add to free list instead

    // -- bump reset: block is at end of heap, just shrink the bump pointer --
    emitter.instruction("sub x14, x9, x15");                                    // x14 = header - heap_buf = new offset
    emitter.instruction("str x14, [x13]");                                      // heap_off = header offset (shrink heap)
    emitter.instruction("b __rt_heap_free_count");                              // go increment gc_frees

    // -- otherwise: insert block at head of free list --
    emitter.label("__rt_heap_free_list");
    emitter.instruction("adrp x10, _heap_free_list@PAGE");                      // load page of free list head
    emitter.instruction("add x10, x10, _heap_free_list@PAGEOFF");               // resolve address of free list head
    emitter.instruction("ldr x14, [x10]");                                      // x14 = old head of free list
    emitter.instruction("str x14, [x9, #8]");                                   // block->next = old head (stored after size)
    emitter.instruction("str x9, [x10]");                                       // free_list_head = this block

    // -- increment gc_frees counter --
    emitter.label("__rt_heap_free_count");
    emitter.instruction("adrp x10, _gc_frees@PAGE");                            // load gc_frees page
    emitter.instruction("add x10, x10, _gc_frees@PAGEOFF");                     // resolve address
    emitter.instruction("ldr x11, [x10]");                                      // load current count
    emitter.instruction("add x11, x11, #1");                                    // increment
    emitter.instruction("str x11, [x10]");                                      // store back

    emitter.label("__rt_heap_free_done");
    emitter.instruction("ret");                                                 // return to caller

    // -- heap_free_safe: only frees if pointer is within heap range --
    // Validates that x0 points into _heap_buf before freeing.
    // Safe to call with garbage/null/.data pointers — silently skips.
    emitter.blank();
    emitter.comment("--- runtime: heap_free_safe ---");
    emitter.label("__rt_heap_free_safe");

    // -- null check --
    emitter.instruction("cbz x0, __rt_heap_free_safe_skip");                    // skip if null pointer

    // -- check lower bound: x0 >= _heap_buf --
    emitter.instruction("adrp x9, _heap_buf@PAGE");                             // load page of heap buffer
    emitter.instruction("add x9, x9, _heap_buf@PAGEOFF");                       // resolve heap buffer base address
    emitter.instruction("cmp x0, x9");                                          // is pointer below heap start?
    emitter.instruction("b.lo __rt_heap_free_safe_skip");                       // yes — not a heap pointer, skip

    // -- check upper bound: x0 < _heap_buf + _heap_off --
    emitter.instruction("adrp x10, _heap_off@PAGE");                            // load page of heap offset
    emitter.instruction("add x10, x10, _heap_off@PAGEOFF");                     // resolve heap offset address
    emitter.instruction("ldr x10, [x10]");                                      // x10 = current heap offset
    emitter.instruction("add x10, x9, x10");                                    // x10 = heap_buf + heap_off = heap end
    emitter.instruction("cmp x0, x10");                                         // is pointer at or beyond heap end?
    emitter.instruction("b.hs __rt_heap_free_safe_skip");                       // yes — not a valid heap pointer, skip

    // -- pointer is in heap range, delegate to heap_free --
    emitter.instruction("b __rt_heap_free");                                    // tail-call to heap_free

    emitter.label("__rt_heap_free_safe_skip");
    emitter.instruction("ret");                                                 // return without freeing
}
