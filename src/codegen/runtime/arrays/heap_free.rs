use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// heap_free: return a heap block to the free list.
/// The block header (16 bytes before user pointer) contains the block size,
/// refcount, and uniform heap kind metadata.
///
/// Optimization: if the block is at the END of the heap (most recently
/// bump-allocated), just decrement the bump pointer instead of adding to
/// the free list. This makes .= loops O(1) with zero fragmentation.
///
/// Otherwise, small blocks are cached in segregated bins while larger blocks
/// stay in the ordered free list that still coalesces and trims the heap tail.
/// Free block layout: [size:4][refcnt:4][kind:8][next_ptr:8][...unused...]
/// Input: x0 = user pointer (as returned by heap_alloc)
pub fn emit_heap_free(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_heap_free_linux_x86_64(emitter);
        return;
    }

    let double_free_msg = "Fatal error: heap debug detected double free\n";

    emitter.blank();
    emitter.comment("--- runtime: heap_free ---");
    emitter.label_global("__rt_heap_free");

    // -- validate pointer is not null --
    emitter.instruction("cbz x0, __rt_heap_free_done");                         // skip if null pointer

    // -- debug mode: validate the free list before mutating it --
    crate::codegen::abi::emit_symbol_address(emitter, "x16", "_heap_debug_enabled");
    emitter.instruction("ldr x16, [x16]");                                      // load the heap-debug enabled flag
    emitter.instruction("cbz x16, __rt_heap_free_debug_checked");               // skip validation when heap-debug mode is disabled
    emitter.instruction("stp x0, x30, [sp, #-16]!");                            // preserve the freed pointer and caller return address before nested validation
    emitter.instruction("bl __rt_heap_debug_validate_free_list");               // verify the ordered free list before inserting into it
    emitter.instruction("ldp x0, x30, [sp], #16");                              // restore the freed pointer and caller return address after validation
    emitter.label("__rt_heap_free_debug_checked");

    // -- compute header address and block end --
    emitter.instruction("sub x9, x0, #16");                                     // x9 = header address (block_size lives here)
    emitter.instruction("ldr w11, [x9]");                                       // x11 = block size (32-bit, zero-extends)
    crate::codegen::abi::emit_symbol_address(emitter, "x16", "_heap_debug_enabled");
    emitter.instruction("ldr x16, [x16]");                                      // load the heap-debug enabled flag
    emitter.instruction("cbz x16, __rt_heap_free_poison_done");                 // skip freed-block poisoning when heap-debug mode is disabled
    emitter.instruction("mov x12, x0");                                         // start poisoning at the beginning of the user payload
    emitter.instruction("add x13, x0, x11");                                    // compute the end of the user payload
    emitter.instruction("mov w14, #0xA5");                                      // use a recognizable freed-memory poison byte pattern
    emitter.label("__rt_heap_free_poison_loop");
    emitter.instruction("cmp x12, x13");                                        // have we poisoned every byte in the freed payload?
    emitter.instruction("b.hs __rt_heap_free_poison_done");                     // stop once the entire payload has been overwritten
    emitter.instruction("strb w14, [x12], #1");                                 // write the poison byte and advance to the next payload byte
    emitter.instruction("b __rt_heap_free_poison_loop");                        // continue poisoning the remaining payload bytes
    emitter.label("__rt_heap_free_poison_done");
    // -- update current live heap footprint before the block joins the free list --
    emitter.instruction("add x12, x11, #16");                                   // include the 16-byte header in the freed block footprint
    crate::codegen::abi::emit_symbol_address(emitter, "x13", "_gc_live");
    emitter.instruction("ldr x14, [x13]");                                      // load current live bytes
    emitter.instruction("sub x14, x14, x12");                                   // subtract the freed block footprint from the live-byte total
    emitter.instruction("str x14, [x13]");                                      // store updated live bytes
    emitter.instruction("str wzr, [x9, #4]");                                   // mark the block header as not live while it is being freed
    emitter.instruction("str xzr, [x9, #8]");                                   // clear the heap kind while the block sits on the free list
    crate::codegen::abi::emit_symbol_address(emitter, "x15", "_heap_buf");
    crate::codegen::abi::emit_symbol_address(emitter, "x13", "_heap_off");
    emitter.instruction("ldr x14, [x13]");                                      // x14 = current heap offset
    emitter.instruction("add x14, x15, x14");                                   // x14 = heap_buf + heap_off = heap end

    // -- check if this is the last bump-allocated block --
    emitter.instruction("add x12, x0, x11");                                    // x12 = user_ptr + size = block end
    emitter.instruction("cmp x12, x14");                                        // is block end == heap end?
    emitter.instruction("b.ne __rt_heap_free_cache_small");                     // no — cache small blocks or insert into the free list

    // -- bump reset: block is at end of heap, just shrink the bump pointer --
    emitter.instruction("sub x14, x9, x15");                                    // x14 = header - heap_buf = new offset
    emitter.instruction("str x14, [x13]");                                      // heap_off = header offset (shrink heap)
    emitter.instruction("b __rt_heap_free_trim_tail");                          // trim any newly-exposed free tail blocks too

    // -- small non-tail blocks go through segregated bins first --
    emitter.label("__rt_heap_free_cache_small");
    emitter.instruction("cmp x11, #64");                                        // does this payload fit in the segregated small-bin cache?
    emitter.instruction("b.hi __rt_heap_free_insert");                          // no — keep using the general coalescing free list
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_small_bins");
    emitter.instruction("mov x12, #0");                                         // default to the <=8-byte bin offset
    emitter.instruction("cmp x11, #8");                                         // does the freed payload fit in the smallest class?
    emitter.instruction("b.ls __rt_heap_free_cache_small_ready");               // yes — keep the <=8-byte bin offset
    emitter.instruction("mov x12, #8");                                         // otherwise target the <=16-byte bin
    emitter.instruction("cmp x11, #16");                                        // does the freed payload fit in the <=16-byte class?
    emitter.instruction("b.ls __rt_heap_free_cache_small_ready");               // yes — keep the <=16-byte bin offset
    emitter.instruction("mov x12, #16");                                        // otherwise target the <=32-byte bin
    emitter.instruction("cmp x11, #32");                                        // does the freed payload fit in the <=32-byte class?
    emitter.instruction("b.ls __rt_heap_free_cache_small_ready");               // yes — keep the <=32-byte bin offset
    emitter.instruction("mov x12, #24");                                        // the remaining cached case is the <=64-byte bin
    emitter.label("__rt_heap_free_cache_small_ready");
    emitter.instruction("add x10, x10, x12");                                   // x10 = address of the chosen small-bin head slot
    crate::codegen::abi::emit_symbol_address(emitter, "x16", "_heap_debug_enabled");
    emitter.instruction("ldr x16, [x16]");                                      // load the heap-debug enabled flag
    emitter.instruction("cbz x16, __rt_heap_free_cache_small_insert");          // skip duplicate detection when heap-debug mode is disabled
    emitter.instruction("ldr x12, [x10]");                                      // x12 = current cached block while checking for duplicates
    emitter.label("__rt_heap_free_cache_small_scan");
    emitter.instruction("cbz x12, __rt_heap_free_cache_small_insert");          // a null next pointer means the block is not already cached
    emitter.instruction("cmp x12, x9");                                         // is this exact header already present in the small bin?
    emitter.instruction("b.eq __rt_heap_free_cache_small_duplicate");           // yes — report a double free under heap-debug mode
    emitter.instruction("ldr x12, [x12, #16]");                                 // advance to the next cached block in this size class
    emitter.instruction("b __rt_heap_free_cache_small_scan");                   // keep scanning the small-bin chain for duplicates

    emitter.label("__rt_heap_free_cache_small_duplicate");
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_heap_dbg_double_free_msg");
    emitter.instruction(&format!("mov x2, #{}", double_free_msg.len()));        // pass the exact double-free debug message length
    emitter.instruction("b __rt_heap_debug_fail");                              // report the duplicate cached block and terminate immediately

    emitter.label("__rt_heap_free_cache_small_insert");
    emitter.instruction("ldr x12, [x10]");                                      // x12 = current small-bin head before insertion
    emitter.instruction("str x12, [x9, #16]");                                  // cached_block->next = previous small-bin head
    emitter.instruction("str x9, [x10]");                                       // publish the freed block as the new head of the selected bin
    emitter.instruction("b __rt_heap_free_post_validate");                      // finish through the common debug validation and free counting path

    // -- larger blocks still use the ordered free list for coalescing --
    emitter.label("__rt_heap_free_insert");
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_free_list");
    emitter.instruction("ldr x12, [x10]");                                      // x12 = current free block while scanning for insertion point

    emitter.label("__rt_heap_free_insert_loop");
    emitter.instruction("cbz x12, __rt_heap_free_insert_here");                 // reached list end — insert here
    emitter.instruction("cmp x12, x9");                                         // does current block live at or after the freed block?
    emitter.instruction("b.eq __rt_heap_free_duplicate_candidate");             // equal addresses mean this block is already present in the free list
    emitter.instruction("b.hs __rt_heap_free_insert_here");                     // yes — this is the insertion point
    emitter.instruction("add x10, x12, #16");                                   // x10 = address of current->next for the next iteration
    emitter.instruction("ldr x12, [x12, #16]");                                 // x12 = current->next
    emitter.instruction("b __rt_heap_free_insert_loop");                        // continue scanning the ordered free list

    emitter.label("__rt_heap_free_duplicate_candidate");
    crate::codegen::abi::emit_symbol_address(emitter, "x16", "_heap_debug_enabled");
    emitter.instruction("ldr x16, [x16]");                                      // load the heap-debug enabled flag
    emitter.instruction("cbz x16, __rt_heap_free_insert_here");                 // keep legacy behavior when heap-debug mode is disabled
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_heap_dbg_double_free_msg");
    emitter.instruction(&format!("mov x2, #{}", double_free_msg.len()));        // pass the exact double-free debug message length
    emitter.instruction("b __rt_heap_debug_fail");                              // report the double-free and terminate immediately

    emitter.label("__rt_heap_free_insert_here");
    emitter.instruction("str x12, [x9, #16]");                                  // new_block->next = current insertion successor
    emitter.instruction("str x9, [x10]");                                       // splice new block into the free list

    // -- merge with the next free block when it is immediately adjacent --
    emitter.instruction("cbz x12, __rt_heap_free_merge_prev");                  // skip next-merge when this block was inserted at the tail
    emitter.instruction("add x14, x9, x11");                                    // x14 = header + payload size
    emitter.instruction("add x14, x14, #16");                                   // x14 = end of freed block including header
    emitter.instruction("cmp x14, x12");                                        // does the freed block end exactly where the next one begins?
    emitter.instruction("b.ne __rt_heap_free_merge_prev");                      // no — keep both blocks separate
    emitter.instruction("ldr w14, [x12]");                                      // x14 = successor block size
    emitter.instruction("add x11, x11, x14");                                   // accumulate successor payload size into current block
    emitter.instruction("add x11, x11, #16");                                   // include the removed successor header in the merged payload size
    emitter.instruction("str w11, [x9]");                                       // write merged size back to the current block header
    emitter.instruction("ldr x12, [x12, #16]");                                 // x12 = successor->next
    emitter.instruction("str x12, [x9, #16]");                                  // current->next = successor->next

    // -- merge with the previous free block when it is immediately adjacent --
    emitter.label("__rt_heap_free_merge_prev");
    crate::codegen::abi::emit_symbol_address(emitter, "x14", "_heap_free_list");
    emitter.instruction("cmp x10, x14");                                        // was the block inserted at the head of the list?
    emitter.instruction("b.eq __rt_heap_free_trim_tail");                       // yes — there is no previous block to merge with
    emitter.instruction("sub x14, x10, #16");                                   // x14 = previous free block header (prev_next_addr - 16)
    emitter.instruction("ldr w12, [x14]");                                      // x12 = previous block size
    emitter.instruction("add x16, x14, x12");                                   // x16 = previous header + previous payload size
    emitter.instruction("add x16, x16, #16");                                   // x16 = end of previous free block including header
    emitter.instruction("cmp x16, x9");                                         // does the previous free block end where the inserted one begins?
    emitter.instruction("b.ne __rt_heap_free_trim_tail");                       // no — nothing more to merge locally
    emitter.instruction("add x12, x12, x11");                                   // accumulate current payload size into the previous block
    emitter.instruction("add x12, x12, #16");                                   // include the inserted block header in the merged payload size
    emitter.instruction("str w12, [x14]");                                      // write merged size back to the previous block header
    emitter.instruction("ldr x16, [x9, #16]");                                  // x16 = inserted_block->next
    emitter.instruction("str x16, [x14, #16]");                                 // previous->next = inserted_block->next

    // -- repeatedly trim any free block that now touches the bump tail --
    emitter.label("__rt_heap_free_trim_tail");
    crate::codegen::abi::emit_symbol_address(emitter, "x13", "_heap_off");
    emitter.instruction("ldr x14, [x13]");                                      // x14 = current heap offset
    crate::codegen::abi::emit_symbol_address(emitter, "x15", "_heap_buf");
    emitter.instruction("add x14, x15, x14");                                   // x14 = current heap end
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_free_list");
    emitter.instruction("ldr x11, [x10]");                                      // x11 = first free block header

    emitter.label("__rt_heap_free_trim_tail_scan");
    emitter.instruction("cbz x11, __rt_heap_free_post_validate");               // no free block reaches the tail anymore
    emitter.instruction("ldr w12, [x11]");                                      // x12 = candidate free block size
    emitter.instruction("add x16, x11, x12");                                   // x16 = header + payload size
    emitter.instruction("add x16, x16, #16");                                   // x16 = end of candidate free block
    emitter.instruction("cmp x16, x14");                                        // does this free block reach the current heap end?
    emitter.instruction("b.eq __rt_heap_free_trim_tail_found");                 // yes — reclaim it back into the bump pointer
    emitter.instruction("add x10, x11, #16");                                   // x10 = address of candidate->next for the next iteration
    emitter.instruction("ldr x11, [x11, #16]");                                 // x11 = candidate->next
    emitter.instruction("b __rt_heap_free_trim_tail_scan");                     // continue scanning the free list

    emitter.label("__rt_heap_free_trim_tail_found");
    emitter.instruction("ldr x12, [x11, #16]");                                 // x12 = candidate->next
    emitter.instruction("str x12, [x10]");                                      // unlink the tail-touching free block from the free list
    emitter.instruction("sub x12, x11, x15");                                   // x12 = candidate header offset from the heap base
    emitter.instruction("str x12, [x13]");                                      // shrink the bump pointer back to the start of the reclaimed block
    emitter.instruction("b __rt_heap_free_trim_tail");                          // keep trimming while more adjacent free blocks reach the tail

    // -- debug mode: validate the free list after mutation --
    emitter.label("__rt_heap_free_post_validate");
    crate::codegen::abi::emit_symbol_address(emitter, "x16", "_heap_debug_enabled");
    emitter.instruction("ldr x16, [x16]");                                      // load the heap-debug enabled flag
    emitter.instruction("cbz x16, __rt_heap_free_count");                       // skip validation when heap-debug mode is disabled
    emitter.instruction("stp x0, x30, [sp, #-16]!");                            // preserve the freed pointer and caller return address before nested validation
    emitter.instruction("bl __rt_heap_debug_validate_free_list");               // verify the free list after insertion, coalescing, and tail trimming
    emitter.instruction("ldp x0, x30, [sp], #16");                              // restore the freed pointer and caller return address after validation

    // -- increment gc_frees counter --
    emitter.label("__rt_heap_free_count");
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_gc_frees");
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
    emitter.label_global("__rt_heap_free_safe");

    // -- null check --
    emitter.instruction("cbz x0, __rt_heap_free_safe_skip");                    // skip if null pointer

    // -- check lower bound: x0 >= _heap_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is pointer below heap start?
    emitter.instruction("b.lo __rt_heap_free_safe_skip");                       // yes — not a heap pointer, skip

    // -- check upper bound: x0 < _heap_buf + _heap_off --
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // x10 = current heap offset
    emitter.instruction("add x10, x9, x10");                                    // x10 = heap_buf + heap_off = heap end
    emitter.instruction("cmp x0, x10");                                         // is pointer at or beyond heap end?
    emitter.instruction("b.hs __rt_heap_free_safe_skip");                       // yes — not a valid heap pointer, skip

    // -- pointer is in heap range, delegate to heap_free --
    emitter.instruction("b __rt_heap_free");                                    // tail-call to heap_free

    emitter.label("__rt_heap_free_safe_skip");
    emitter.instruction("ret");                                                 // return without freeing
}

fn emit_heap_free_linux_x86_64(emitter: &mut Emitter) {
    let double_free_msg = "Fatal error: heap debug detected double free\n";

    emitter.blank();
    emitter.comment("--- runtime: heap_free ---");
    emitter.label_global("__rt_heap_free");

    emitter.instruction("test rax, rax");                                       // ignore null pointers so the x86_64 heap runtime matches the shared heap_free contract
    emitter.instruction("jz __rt_heap_free_done");                              // null payloads do not own heap storage and therefore need no release work
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_heap_debug_enabled");
    emitter.instruction("mov r8, QWORD PTR [r8]");                              // load the heap-debug enabled flag before mutating free-list state
    emitter.instruction("test r8, r8");                                         // is heap-debug validation enabled for this free path?
    emitter.instruction("jz __rt_heap_free_debug_checked");                     // skip the validator and double-free guard when heap-debug mode is disabled
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    emitter.instruction("cmp rax, r10");                                        // does the candidate freed pointer begin below the heap base?
    emitter.instruction("jb __rt_heap_free_debug_checked");                     // pointers outside the heap cannot participate in heap-debug double-free checks
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_heap_off");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the current bump offset before deriving the live heap end
    emitter.instruction("lea r11, [r10 + r11]");                                // compute the current live heap end from the base plus bump offset
    emitter.instruction("cmp rax, r11");                                        // does the candidate freed pointer lie at or beyond the live heap end?
    emitter.instruction("jae __rt_heap_free_debug_checked");                    // pointers outside the live heap window cannot participate in heap-debug double-free checks
    emitter.instruction("sub rsp, 16");                                         // reserve one aligned stack slot to preserve the user pointer across the nested call
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // save the user pointer across the free-list validator call
    emitter.instruction("call __rt_heap_debug_validate_free_list");             // verify the ordered free list and cached small bins before mutating them
    emitter.instruction("mov rax, QWORD PTR [rsp]");                            // restore the user pointer after the free-list validator call returns
    emitter.instruction("add rsp, 16");                                         // release the temporary validator spill slot
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the current heap kind word before deciding whether a zero refcount is stale or legitimately being freed
    emitter.instruction("mov r11, r10");                                        // preserve the full heap kind word while isolating the ownership marker for the stale-free check
    emitter.instruction("shr r10, 32");                                         // isolate the high-word heap marker from the packed kind metadata
    emitter.instruction(&format!("cmp r10d, 0x{:x}", X86_64_HEAP_MAGIC_HI32));  // does this heap-range pointer still carry a live x86_64 heap marker?
    emitter.instruction("je __rt_heap_free_debug_checked");                     // yes — a live marker means this is the first legitimate free path, even if refcount is already zero
    emitter.instruction("mov ecx, DWORD PTR [rax - 12]");                       // load the current live refcount before any x86_64 free-side mutations
    emitter.instruction("test ecx, ecx");                                       // does the header still look like a live heap block?
    emitter.instruction("jnz __rt_heap_free_debug_checked");                    // yes — continue through the ordinary x86_64 free path
    crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_heap_dbg_double_free_msg");
    emitter.instruction(&format!("mov edx, {}", double_free_msg.len()));        // pass the exact double-free debug message length to the failure helper
    emitter.instruction("jmp __rt_heap_debug_fail");                            // report the duplicate free immediately under heap-debug mode
    emitter.label("__rt_heap_free_debug_checked");
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the stamped x86_64 heap kind word from the uniform header
    emitter.instruction("shr r10, 32");                                         // isolate the high-word heap marker used to distinguish owned heap payloads from foreign pointers
    emitter.instruction(&format!("cmp r10d, 0x{:x}", X86_64_HEAP_MAGIC_HI32));  // verify that this payload belongs to the x86_64 heap runtime before mutating allocator state
    emitter.instruction("jne __rt_heap_free_done");                             // silently ignore foreign/static pointers so callers can safely pass literals or concat-buffer storage
    emitter.instruction("lea r9, [rax - 16]");                                  // recover the internal block header address from the user payload pointer
    emitter.instruction("mov r11d, DWORD PTR [r9]");                            // load the block payload size from the uniform heap header before releasing it
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_heap_debug_enabled");
    emitter.instruction("mov r8, QWORD PTR [r8]");                              // reload the heap-debug enabled flag before optional payload poisoning
    emitter.instruction("test r8, r8");                                         // should the x86_64 heap runtime poison freed payload bytes?
    emitter.instruction("jz __rt_heap_free_poison_done");                       // skip payload poisoning entirely when heap-debug mode is disabled
    emitter.instruction("mov rcx, rax");                                        // start poisoning at the first user payload byte
    emitter.instruction("lea rdx, [rax + r11]");                                // compute the end of the user payload for the poison loop
    emitter.instruction("mov esi, 0xa5");                                       // use the recognizable freed-memory poison byte pattern
    emitter.label("__rt_heap_free_poison_loop");
    emitter.instruction("cmp rcx, rdx");                                        // have all freed payload bytes been overwritten already?
    emitter.instruction("jae __rt_heap_free_poison_done");                      // yes — stop the payload poisoning loop
    emitter.instruction("mov BYTE PTR [rcx], sil");                             // overwrite the current freed payload byte with the poison marker
    emitter.instruction("add rcx, 1");                                          // advance to the next payload byte in the poison loop
    emitter.instruction("jmp __rt_heap_free_poison_loop");                      // continue poisoning the remaining freed payload bytes
    emitter.label("__rt_heap_free_poison_done");
    emitter.instruction("mov r10, r11");                                        // widen the payload size into a 64-bit scratch register for live-byte accounting
    emitter.instruction("add r10, 16");                                         // include the uniform 16-byte header in the freed block footprint
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_gc_live");
    emitter.instruction("mov rcx, QWORD PTR [r8]");                             // load the current live-byte count before subtracting the freed block footprint
    emitter.instruction("sub rcx, r10");                                        // subtract this block's payload-plus-header footprint from the live-byte count
    emitter.instruction("mov QWORD PTR [r8], rcx");                             // store the updated live-byte count after freeing the block
    emitter.instruction("mov DWORD PTR [r9 + 4], 0");                           // clear the live refcount while this block sits on the free list or in a small bin
    emitter.instruction("mov QWORD PTR [r9 + 8], 0");                           // clear the heap kind so free blocks do not look like live typed payloads
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_heap_off");
    emitter.instruction("mov rcx, QWORD PTR [r8]");                             // load the current bump offset before checking whether this is the tail block
    emitter.instruction("lea rcx, [r10 + rcx]");                                // compute the current heap end address from the heap base plus bump offset
    emitter.instruction("lea rdx, [rax + r11]");                                // compute the freed block end address from the user pointer plus payload size
    emitter.instruction("cmp rdx, rcx");                                        // does the freed block reach the current heap end?
    emitter.instruction("jne __rt_heap_free_cache_small");                      // no — cache small blocks or insert larger blocks into the general free list

    // -- bump reset: block is at end of heap, just shrink the bump pointer --
    emitter.instruction("mov rdx, r9");                                         // preserve the freed block header address while converting it back into a bump offset
    emitter.instruction("sub rdx, r10");                                        // compute the new bump offset from the heap base to the reclaimed block header
    emitter.instruction("mov QWORD PTR [r8], rdx");                             // shrink the bump pointer back to the start of the freed tail block
    emitter.instruction("jmp __rt_heap_free_trim_tail");                        // trim any newly exposed free tail blocks too before returning

    // -- small non-tail blocks go through segregated bins first --
    emitter.label("__rt_heap_free_cache_small");
    emitter.instruction("cmp r11, 64");                                         // does the freed payload fit in the segregated small-bin cache?
    emitter.instruction("ja __rt_heap_free_insert");                            // larger payloads still use the ordered coalescing free list
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_heap_small_bins");
    emitter.instruction("xor rcx, rcx");                                        // default to the <=8-byte bin offset
    emitter.instruction("cmp r11, 8");                                          // does the freed payload fit in the smallest cached class?
    emitter.instruction("jbe __rt_heap_free_cache_small_ready");                // yes — keep the <=8-byte bin offset
    emitter.instruction("mov rcx, 8");                                          // otherwise target the <=16-byte bin offset
    emitter.instruction("cmp r11, 16");                                         // does the freed payload fit in the <=16-byte class?
    emitter.instruction("jbe __rt_heap_free_cache_small_ready");                // yes — keep the <=16-byte bin offset
    emitter.instruction("mov rcx, 16");                                         // otherwise target the <=32-byte bin offset
    emitter.instruction("cmp r11, 32");                                         // does the freed payload fit in the <=32-byte class?
    emitter.instruction("jbe __rt_heap_free_cache_small_ready");                // yes — keep the <=32-byte bin offset
    emitter.instruction("mov rcx, 24");                                         // remaining cached payloads belong to the <=64-byte bin offset
    emitter.label("__rt_heap_free_cache_small_ready");
    emitter.instruction("add r10, rcx");                                        // r10 = address of the selected small-bin head slot
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_heap_debug_enabled");
    emitter.instruction("mov r8, QWORD PTR [r8]");                              // reload the heap-debug enabled flag before checking cached-bin duplicates
    emitter.instruction("test r8, r8");                                         // is duplicate detection enabled for the small-bin cache?
    emitter.instruction("jz __rt_heap_free_cache_small_insert");                // skip duplicate detection when heap-debug mode is disabled
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // start scanning the cached small-bin chain for duplicate headers
    emitter.label("__rt_heap_free_cache_small_scan");
    emitter.instruction("test rdx, rdx");                                       // did the cached small-bin scan reach the tail?
    emitter.instruction("jz __rt_heap_free_cache_small_insert");                // yes — this block is not already cached in the selected size class
    emitter.instruction("cmp rdx, r9");                                         // is this exact header already present in the selected cached size class?
    emitter.instruction("je __rt_heap_free_cache_small_duplicate");             // yes — report the double free while heap-debug mode is enabled
    emitter.instruction("mov rdx, QWORD PTR [rdx + 16]");                       // advance to the next cached small-bin header while scanning for duplicates
    emitter.instruction("jmp __rt_heap_free_cache_small_scan");                 // keep scanning the cached small-bin chain for duplicates
    emitter.label("__rt_heap_free_cache_small_duplicate");
    crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_heap_dbg_double_free_msg");
    emitter.instruction(&format!("mov edx, {}", double_free_msg.len()));        // pass the exact double-free debug message length to the failure helper
    emitter.instruction("jmp __rt_heap_debug_fail");                            // report the duplicate cached block and terminate immediately
    emitter.label("__rt_heap_free_cache_small_insert");
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // load the previous cached head for this small-bin size class
    emitter.instruction("mov QWORD PTR [r9 + 16], rdx");                        // splice the freed block onto the front of the selected small-bin chain
    emitter.instruction("mov QWORD PTR [r10], r9");                             // publish the freed block as the new small-bin head
    emitter.instruction("jmp __rt_heap_free_post_validate");                    // finish through the shared post-mutation validation and free-counting path

    // -- larger blocks still use the ordered free list for coalescing --
    emitter.label("__rt_heap_free_insert");
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_heap_free_list");
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // load the current free-list head while scanning for the insertion point
    emitter.label("__rt_heap_free_insert_loop");
    emitter.instruction("test rdx, rdx");                                       // did the free-list scan reach the tail?
    emitter.instruction("jz __rt_heap_free_insert_here");                       // yes — insert the freed block at the end of the ordered free list
    emitter.instruction("cmp rdx, r9");                                         // does the current free block begin at or after the freed block?
    emitter.instruction("je __rt_heap_free_duplicate_candidate");               // equal addresses mean this exact block is already present in the free list
    emitter.instruction("ja __rt_heap_free_insert_here");                       // yes — this is the ordered insertion point
    emitter.instruction("lea r10, [rdx + 16]");                                 // advance prev_next_addr to the current block's next field
    emitter.instruction("mov rdx, QWORD PTR [rdx + 16]");                       // move on to the next block in the ordered free list
    emitter.instruction("jmp __rt_heap_free_insert_loop");                      // continue searching for the ordered insertion point

    emitter.label("__rt_heap_free_duplicate_candidate");
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_heap_debug_enabled");
    emitter.instruction("mov r8, QWORD PTR [r8]");                              // reload the heap-debug enabled flag before deciding whether duplicate headers are fatal
    emitter.instruction("test r8, r8");                                         // is heap-debug mode enabled for duplicate free-list detection?
    emitter.instruction("jz __rt_heap_free_insert_here");                       // no — keep legacy non-debug behavior for duplicate free-list headers
    crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_heap_dbg_double_free_msg");
    emitter.instruction(&format!("mov edx, {}", double_free_msg.len()));        // pass the exact double-free debug message length to the failure helper
    emitter.instruction("jmp __rt_heap_debug_fail");                            // report the duplicate ordered free-list header and terminate immediately

    emitter.label("__rt_heap_free_insert_here");
    emitter.instruction("mov QWORD PTR [r9 + 16], rdx");                        // splice the freed block in front of the current successor
    emitter.instruction("mov QWORD PTR [r10], r9");                             // publish the freed block at the chosen ordered insertion point

    // -- merge with the next free block when it is immediately adjacent --
    emitter.instruction("test rdx, rdx");                                       // was the freed block inserted at the tail of the ordered free list?
    emitter.instruction("jz __rt_heap_free_merge_prev");                        // yes — there is no successor to merge with
    emitter.instruction("lea rcx, [r9 + r11 + 16]");                            // compute the end address of the newly inserted free block
    emitter.instruction("cmp rcx, rdx");                                        // does the inserted block end exactly where the successor begins?
    emitter.instruction("jne __rt_heap_free_merge_prev");                       // no — keep the successor as a separate free block
    emitter.instruction("mov ecx, DWORD PTR [rdx]");                            // load the successor payload size before collapsing it into the current block
    emitter.instruction("add r11, rcx");                                        // accumulate the successor payload size into the current free block size
    emitter.instruction("add r11, 16");                                         // include the removed successor header in the merged payload size
    emitter.instruction("mov DWORD PTR [r9], r11d");                            // write the merged payload size back into the current block header
    emitter.instruction("mov rdx, QWORD PTR [rdx + 16]");                       // preserve the successor's successor before unlinking the merged block
    emitter.instruction("mov QWORD PTR [r9 + 16], rdx");                        // update the merged block next pointer after removing the adjacent successor

    // -- merge with the previous free block when it is immediately adjacent --
    emitter.label("__rt_heap_free_merge_prev");
    crate::codegen::abi::emit_symbol_address(emitter, "rcx", "_heap_free_list");
    emitter.instruction("cmp r10, rcx");                                        // was the block inserted at the head of the ordered free list?
    emitter.instruction("je __rt_heap_free_trim_tail");                         // yes — there is no previous free block to merge with
    emitter.instruction("lea rcx, [r10 - 16]");                                 // recover the previous free block header from prev_next_addr
    emitter.instruction("mov edx, DWORD PTR [rcx]");                            // load the previous free block payload size before checking adjacency
    emitter.instruction("lea r8, [rcx + rdx + 16]");                            // compute the end address of the previous free block
    emitter.instruction("cmp r8, r9");                                          // does the previous free block end where the inserted block begins?
    emitter.instruction("jne __rt_heap_free_trim_tail");                        // no — there is no previous neighbor to coalesce with
    emitter.instruction("add rdx, r11");                                        // accumulate the inserted block payload size into the previous block
    emitter.instruction("add rdx, 16");                                         // include the inserted block header in the merged previous block size
    emitter.instruction("mov DWORD PTR [rcx], edx");                            // write the merged payload size back into the previous block header
    emitter.instruction("mov r8, QWORD PTR [r9 + 16]");                         // preserve the inserted block successor before unlinking the merged header
    emitter.instruction("mov QWORD PTR [rcx + 16], r8");                        // splice the merged previous block directly to the inserted block successor

    // -- repeatedly trim any ordered free block that now touches the bump tail --
    emitter.label("__rt_heap_free_trim_tail");
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_heap_off");
    emitter.instruction("mov rcx, QWORD PTR [r8]");                             // reload the current bump offset before scanning for tail-touching free blocks
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    emitter.instruction("lea rcx, [r10 + rcx]");                                // compute the current heap end from the heap base plus bump offset
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_heap_free_list");
    emitter.instruction("mov rdx, QWORD PTR [r11]");                            // start scanning at the ordered free-list head
    emitter.label("__rt_heap_free_trim_tail_scan");
    emitter.instruction("test rdx, rdx");                                       // did the scan run out of ordered free blocks?
    emitter.instruction("jz __rt_heap_free_count");                             // yes — no more free blocks reach the current bump tail
    emitter.instruction("mov esi, DWORD PTR [rdx]");                            // load this candidate free block payload size before checking whether it reaches the tail
    emitter.instruction("lea rdi, [rdx + rsi + 16]");                           // compute the end address of the candidate free block
    emitter.instruction("cmp rdi, rcx");                                        // does this ordered free block end at the current heap tail?
    emitter.instruction("je __rt_heap_free_trim_tail_found");                   // yes — reclaim it back into the bump pointer
    emitter.instruction("lea r11, [rdx + 16]");                                 // advance prev_next_addr to the candidate block next field
    emitter.instruction("mov rdx, QWORD PTR [rdx + 16]");                       // move on to the next ordered free block
    emitter.instruction("jmp __rt_heap_free_trim_tail_scan");                   // keep scanning for a free block that now touches the bump tail

    emitter.label("__rt_heap_free_trim_tail_found");
    emitter.instruction("mov rsi, QWORD PTR [rdx + 16]");                       // preserve the reclaimed block successor before unlinking it from the free list
    emitter.instruction("mov QWORD PTR [r11], rsi");                            // unlink the reclaimed tail block from the ordered free list
    emitter.instruction("mov rsi, rdx");                                        // preserve the reclaimed block header while converting it into a new bump offset
    emitter.instruction("sub rsi, r10");                                        // compute the new bump offset from the heap base to the reclaimed block header
    emitter.instruction("mov QWORD PTR [r8], rsi");                             // shrink the bump pointer back to the reclaimed free block start
    emitter.instruction("jmp __rt_heap_free_trim_tail");                        // continue trimming while more adjacent free blocks now reach the new heap tail

    emitter.label("__rt_heap_free_post_validate");
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_heap_debug_enabled");
    emitter.instruction("mov r8, QWORD PTR [r8]");                              // reload the heap-debug enabled flag after mutating free-list or cached-bin state
    emitter.instruction("test r8, r8");                                         // should the x86_64 runtime validate the updated free state now?
    emitter.instruction("jz __rt_heap_free_count");                             // skip the post-mutation validator when heap-debug mode is disabled
    emitter.instruction("call __rt_heap_debug_validate_free_list");             // verify the ordered free list and cached bins after insertion, coalescing, and trimming

    // -- increment gc_frees counter --
    emitter.label("__rt_heap_free_count");
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_gc_frees");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current free counter before recording this released heap block
    emitter.instruction("add r9, 1");                                           // count the released heap block in the runtime free counter
    emitter.instruction("mov QWORD PTR [r8], r9");                              // store the updated free counter back into runtime state

    emitter.label("__rt_heap_free_done");
    emitter.instruction("ret");                                                 // return to the caller after the optional release path

    emitter.blank();
    emitter.comment("--- runtime: heap_free_safe ---");
    emitter.label_global("__rt_heap_free_safe");
    emitter.instruction("jmp __rt_heap_free");                                  // reuse the same guarded x86_64 release path for the safe helper variant
}
