use crate::codegen::emit::Emitter;

/// heap_free: return a heap block to the free list.
/// The block header (8 bytes before user pointer) contains the block size.
///
/// Optimization: if the block is at the END of the heap (most recently
/// bump-allocated), just decrement the bump pointer instead of adding to
/// the free list. This makes .= loops O(1) with zero fragmentation.
///
/// Otherwise, inserts into the free list in address order, coalesces adjacent
/// blocks, and trims any newly-exposed free tail back into the bump pointer.
/// Free block layout: [size:8][next_ptr:8][...unused...]
/// Input: x0 = user pointer (as returned by heap_alloc)
pub fn emit_heap_free(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: heap_free ---");
    emitter.label("__rt_heap_free");

    // -- validate pointer is not null --
    emitter.instruction("cbz x0, __rt_heap_free_done");                         // skip if null pointer

    // -- debug mode: validate the free list before mutating it --
    emitter.instruction("adrp x16, _heap_debug_enabled@PAGE");                  // load page of the heap-debug enabled flag
    emitter.instruction("add x16, x16, _heap_debug_enabled@PAGEOFF");           // resolve the heap-debug enabled flag address
    emitter.instruction("ldr x16, [x16]");                                      // load the heap-debug enabled flag
    emitter.instruction("cbz x16, __rt_heap_free_debug_checked");               // skip validation when heap-debug mode is disabled
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the caller return address before nested validation
    emitter.instruction("bl __rt_heap_debug_validate_free_list");                // verify the ordered free list before inserting into it
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the caller return address after validation
    emitter.label("__rt_heap_free_debug_checked");

    // -- compute header address and block end --
    emitter.instruction("sub x9, x0, #8");                                      // x9 = header address (block_size lives here)
    emitter.instruction("ldr w11, [x9]");                                       // x11 = block size (32-bit, zero-extends)
    emitter.instruction("str wzr, [x9, #4]");                                   // mark the block header as not live while it is being freed
    emitter.instruction("adrp x15, _heap_buf@PAGE");                            // load page of heap buffer
    emitter.instruction("add x15, x15, _heap_buf@PAGEOFF");                     // resolve heap buffer base
    emitter.instruction("adrp x13, _heap_off@PAGE");                            // load page of heap offset
    emitter.instruction("add x13, x13, _heap_off@PAGEOFF");                     // resolve heap offset address
    emitter.instruction("ldr x14, [x13]");                                      // x14 = current heap offset
    emitter.instruction("add x14, x15, x14");                                   // x14 = heap_buf + heap_off = heap end

    // -- check if this is the last bump-allocated block --
    emitter.instruction("add x12, x0, x11");                                    // x12 = user_ptr + size = block end
    emitter.instruction("cmp x12, x14");                                        // is block end == heap end?
    emitter.instruction("b.ne __rt_heap_free_insert");                          // no — insert into the free list instead

    // -- bump reset: block is at end of heap, just shrink the bump pointer --
    emitter.instruction("sub x14, x9, x15");                                    // x14 = header - heap_buf = new offset
    emitter.instruction("str x14, [x13]");                                      // heap_off = header offset (shrink heap)
    emitter.instruction("b __rt_heap_free_trim_tail");                          // trim any newly-exposed free tail blocks too

    // -- otherwise: insert block into the free list in address order --
    emitter.label("__rt_heap_free_insert");
    emitter.instruction("adrp x10, _heap_free_list@PAGE");                      // load page of free list head
    emitter.instruction("add x10, x10, _heap_free_list@PAGEOFF");               // resolve address of free list head
    emitter.instruction("ldr x12, [x10]");                                      // x12 = current free block while scanning for insertion point

    emitter.label("__rt_heap_free_insert_loop");
    emitter.instruction("cbz x12, __rt_heap_free_insert_here");                  // reached list end — insert here
    emitter.instruction("cmp x12, x9");                                          // does current block live at or after the freed block?
    emitter.instruction("b.eq __rt_heap_free_duplicate_candidate");              // equal addresses mean this block is already present in the free list
    emitter.instruction("b.hs __rt_heap_free_insert_here");                      // yes — this is the insertion point
    emitter.instruction("add x10, x12, #8");                                     // x10 = address of current->next for the next iteration
    emitter.instruction("ldr x12, [x12, #8]");                                   // x12 = current->next
    emitter.instruction("b __rt_heap_free_insert_loop");                         // continue scanning the ordered free list

    emitter.label("__rt_heap_free_duplicate_candidate");
    emitter.instruction("adrp x16, _heap_debug_enabled@PAGE");                  // load page of the heap-debug enabled flag
    emitter.instruction("add x16, x16, _heap_debug_enabled@PAGEOFF");           // resolve the heap-debug enabled flag address
    emitter.instruction("ldr x16, [x16]");                                      // load the heap-debug enabled flag
    emitter.instruction("cbz x16, __rt_heap_free_insert_here");                 // keep legacy behavior when heap-debug mode is disabled
    emitter.instruction("adrp x1, _heap_dbg_double_free_msg@PAGE");             // load page of the double-free debug message
    emitter.instruction("add x1, x1, _heap_dbg_double_free_msg@PAGEOFF");       // resolve the double-free debug message address
    emitter.instruction("mov x2, #45");                                         // message length: Fatal error: heap debug detected double free\n
    emitter.instruction("b __rt_heap_debug_fail");                              // report the double-free and terminate immediately

    emitter.label("__rt_heap_free_insert_here");
    emitter.instruction("str x12, [x9, #8]");                                    // new_block->next = current insertion successor
    emitter.instruction("str x9, [x10]");                                        // splice new block into the free list

    // -- merge with the next free block when it is immediately adjacent --
    emitter.instruction("cbz x12, __rt_heap_free_merge_prev");                   // skip next-merge when this block was inserted at the tail
    emitter.instruction("add x14, x9, x11");                                     // x14 = header + payload size
    emitter.instruction("add x14, x14, #8");                                     // x14 = end of freed block including header
    emitter.instruction("cmp x14, x12");                                         // does the freed block end exactly where the next one begins?
    emitter.instruction("b.ne __rt_heap_free_merge_prev");                       // no — keep both blocks separate
    emitter.instruction("ldr w14, [x12]");                                       // x14 = successor block size
    emitter.instruction("add x11, x11, x14");                                    // accumulate successor payload size into current block
    emitter.instruction("add x11, x11, #8");                                     // include the removed successor header in the merged payload size
    emitter.instruction("str w11, [x9]");                                        // write merged size back to the current block header
    emitter.instruction("ldr x12, [x12, #8]");                                   // x12 = successor->next
    emitter.instruction("str x12, [x9, #8]");                                    // current->next = successor->next

    // -- merge with the previous free block when it is immediately adjacent --
    emitter.label("__rt_heap_free_merge_prev");
    emitter.instruction("adrp x14, _heap_free_list@PAGE");                       // load page of free list head
    emitter.instruction("add x14, x14, _heap_free_list@PAGEOFF");                // resolve address of the free list head pointer
    emitter.instruction("cmp x10, x14");                                         // was the block inserted at the head of the list?
    emitter.instruction("b.eq __rt_heap_free_trim_tail");                        // yes — there is no previous block to merge with
    emitter.instruction("sub x14, x10, #8");                                     // x14 = previous free block header (prev_next_addr - 8)
    emitter.instruction("ldr w12, [x14]");                                       // x12 = previous block size
    emitter.instruction("add x16, x14, x12");                                    // x16 = previous header + previous payload size
    emitter.instruction("add x16, x16, #8");                                     // x16 = end of previous free block including header
    emitter.instruction("cmp x16, x9");                                          // does the previous free block end where the inserted one begins?
    emitter.instruction("b.ne __rt_heap_free_trim_tail");                        // no — nothing more to merge locally
    emitter.instruction("add x12, x12, x11");                                    // accumulate current payload size into the previous block
    emitter.instruction("add x12, x12, #8");                                     // include the inserted block header in the merged payload size
    emitter.instruction("str w12, [x14]");                                       // write merged size back to the previous block header
    emitter.instruction("ldr x16, [x9, #8]");                                    // x16 = inserted_block->next
    emitter.instruction("str x16, [x14, #8]");                                   // previous->next = inserted_block->next

    // -- repeatedly trim any free block that now touches the bump tail --
    emitter.label("__rt_heap_free_trim_tail");
    emitter.instruction("adrp x13, _heap_off@PAGE");                             // load page of heap offset
    emitter.instruction("add x13, x13, _heap_off@PAGEOFF");                      // resolve heap offset address
    emitter.instruction("ldr x14, [x13]");                                       // x14 = current heap offset
    emitter.instruction("adrp x15, _heap_buf@PAGE");                             // load page of heap buffer
    emitter.instruction("add x15, x15, _heap_buf@PAGEOFF");                      // resolve heap buffer base
    emitter.instruction("add x14, x15, x14");                                    // x14 = current heap end
    emitter.instruction("adrp x10, _heap_free_list@PAGE");                       // load page of free list head
    emitter.instruction("add x10, x10, _heap_free_list@PAGEOFF");                // resolve address of free list head pointer
    emitter.instruction("ldr x11, [x10]");                                       // x11 = first free block header

    emitter.label("__rt_heap_free_trim_tail_scan");
    emitter.instruction("cbz x11, __rt_heap_free_post_validate");                // no free block reaches the tail anymore
    emitter.instruction("ldr w12, [x11]");                                       // x12 = candidate free block size
    emitter.instruction("add x16, x11, x12");                                    // x16 = header + payload size
    emitter.instruction("add x16, x16, #8");                                     // x16 = end of candidate free block
    emitter.instruction("cmp x16, x14");                                         // does this free block reach the current heap end?
    emitter.instruction("b.eq __rt_heap_free_trim_tail_found");                  // yes — reclaim it back into the bump pointer
    emitter.instruction("add x10, x11, #8");                                     // x10 = address of candidate->next for the next iteration
    emitter.instruction("ldr x11, [x11, #8]");                                   // x11 = candidate->next
    emitter.instruction("b __rt_heap_free_trim_tail_scan");                      // continue scanning the free list

    emitter.label("__rt_heap_free_trim_tail_found");
    emitter.instruction("ldr x12, [x11, #8]");                                   // x12 = candidate->next
    emitter.instruction("str x12, [x10]");                                       // unlink the tail-touching free block from the free list
    emitter.instruction("sub x12, x11, x15");                                    // x12 = candidate header offset from the heap base
    emitter.instruction("str x12, [x13]");                                       // shrink the bump pointer back to the start of the reclaimed block
    emitter.instruction("b __rt_heap_free_trim_tail");                           // keep trimming while more adjacent free blocks reach the tail

    // -- debug mode: validate the free list after mutation --
    emitter.label("__rt_heap_free_post_validate");
    emitter.instruction("adrp x16, _heap_debug_enabled@PAGE");                  // load page of the heap-debug enabled flag
    emitter.instruction("add x16, x16, _heap_debug_enabled@PAGEOFF");           // resolve the heap-debug enabled flag address
    emitter.instruction("ldr x16, [x16]");                                      // load the heap-debug enabled flag
    emitter.instruction("cbz x16, __rt_heap_free_count");                       // skip validation when heap-debug mode is disabled
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the caller return address before nested validation
    emitter.instruction("bl __rt_heap_debug_validate_free_list");                // verify the free list after insertion, coalescing, and tail trimming
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the caller return address after validation

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
