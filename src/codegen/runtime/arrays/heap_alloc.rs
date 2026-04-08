use crate::codegen::emit::Emitter;

/// heap_alloc: free-list allocator with 16-byte header.
/// Each allocation has a 16-byte header [size:4][refcount:4][kind:8] before the user pointer.
/// Small freed blocks are cached in size-segregated bins before falling back to the
/// general address-ordered free list: [size:4][refcount:4][kind:8][next_ptr:8].
/// Input: x0 = bytes needed
/// Output: x0 = pointer to allocated memory (after header)
pub fn emit_heap_alloc(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: heap_alloc (free-list + bump) ---");
    emitter.label_global("__rt_heap_alloc");

    // -- enforce minimum allocation of 8 bytes (free payload needs space for next ptr) --
    emitter.instruction("cmp x0, #8");                                          // is requested size < 8?
    emitter.instruction("b.ge __rt_heap_alloc_start");                          // skip if already >= 8
    emitter.instruction("mov x0, #8");                                          // round up to minimum 8 bytes
    emitter.label("__rt_heap_alloc_start");

    // -- debug mode: validate the free list before consuming it --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_debug_enabled");
    emitter.instruction("ldr x9, [x9]");                                        // load the heap-debug enabled flag
    emitter.instruction("cbz x9, __rt_heap_alloc_debug_checked");               // skip validation when heap-debug mode is disabled
    emitter.instruction("mov x15, x0");                                         // preserve the requested allocation size across validation
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the caller return address before making a nested call
    emitter.instruction("bl __rt_heap_debug_validate_free_list");               // verify the ordered free list before searching it
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the caller return address after validation
    emitter.instruction("mov x0, x15");                                         // restore the requested allocation size after validation
    emitter.label("__rt_heap_alloc_debug_checked");

    // -- try small segregated bins before walking the general free list --
    emitter.instruction("cmp x0, #64");                                         // do we fit in the small-block cache classes?
    emitter.instruction("b.hi __rt_heap_alloc_fl_start");                       // larger requests still use the general free list
    emitter.instruction("mov x13, #0");                                         // default to the <=8-byte bin
    emitter.instruction("cmp x0, #8");                                          // does the request fit in the smallest payload class?
    emitter.instruction("b.ls __rt_heap_alloc_small_bins");                     // yes — start searching at the <=8-byte bin
    emitter.instruction("mov x13, #8");                                         // otherwise start at the <=16-byte bin
    emitter.instruction("cmp x0, #16");                                         // does the request fit in the <=16-byte class?
    emitter.instruction("b.ls __rt_heap_alloc_small_bins");                     // yes — search from the <=16-byte bin upward
    emitter.instruction("mov x13, #16");                                        // otherwise start at the <=32-byte bin
    emitter.instruction("cmp x0, #32");                                         // does the request fit in the <=32-byte class?
    emitter.instruction("b.ls __rt_heap_alloc_small_bins");                     // yes — search from the <=32-byte bin upward
    emitter.instruction("mov x13, #24");                                        // requests up to 64 bytes start at the largest small-bin class
    emitter.label("__rt_heap_alloc_small_bins");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_small_bins");
    emitter.instruction("add x9, x9, x13");                                     // x9 = address of the first candidate bin head
    emitter.label("__rt_heap_alloc_small_bin_loop");
    emitter.instruction("ldr x10, [x9]");                                       // x10 = current small-bin head block (0 if this bin is empty)
    emitter.instruction("cbnz x10, __rt_heap_alloc_small_bin_found");           // use the first available cached block in this size class or larger
    emitter.instruction("cmp x13, #24");                                        // have we already checked the <=64-byte bin?
    emitter.instruction("b.eq __rt_heap_alloc_fl_start");                       // yes — fall back to the general free list
    emitter.instruction("add x13, x13, #8");                                    // advance to the next larger small-bin class
    emitter.instruction("add x9, x9, #8");                                      // move to the next bin-head slot
    emitter.instruction("b __rt_heap_alloc_small_bin_loop");                    // keep searching the remaining small bins

    emitter.label("__rt_heap_alloc_small_bin_found");
    emitter.instruction("ldr x11, [x10, #16]");                                 // x11 = cached_small_block->next within this size class
    emitter.instruction("str x11, [x9]");                                       // pop the cached block from the segregated small bin
    emitter.instruction("mov w13, #1");                                         // initial refcount = 1 for the reused block
    emitter.instruction("str w13, [x10, #4]");                                  // restore the live refcount in the reused header
    emitter.instruction("str xzr, [x10, #8]");                                  // reset heap kind to raw until a typed constructor overwrites it
    emitter.instruction("add x0, x10, #16");                                    // return user pointer = header + 16
    emitter.instruction("b __rt_heap_alloc_count");                             // reuse the shared allocation-accounting path

    // -- walk the general free list looking for first-fit block --
    // x0 = requested size, x9 = prev_next_addr, x10 = current block header
    emitter.label("__rt_heap_alloc_fl_start");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_free_list");
    emitter.instruction("ldr x10, [x9]");                                       // x10 = first free block header (0 if empty)

    // -- walk the free list looking for first-fit block --
    emitter.label("__rt_heap_alloc_fl_loop");
    emitter.instruction("cbz x10, __rt_heap_alloc_bump");                       // no free block found, fall through to bump
    emitter.instruction("ldr w11, [x10]");                                      // x11 = block size (32-bit, zero-extends)
    emitter.instruction("cmp x11, x0");                                         // does this block fit the request?
    emitter.instruction("b.ge __rt_heap_alloc_fl_found");                       // yes — use this block

    // -- advance to next free block --
    emitter.instruction("add x9, x10, #16");                                    // prev_next_addr = &current->next after the 16-byte free-block header
    emitter.instruction("ldr x10, [x10, #16]");                                 // current = current->next
    emitter.instruction("b __rt_heap_alloc_fl_loop");                           // continue searching

    // -- found a suitable free block, either split it or unlink it whole --
    emitter.label("__rt_heap_alloc_fl_found");
    emitter.instruction("sub x12, x11, x0");                                    // x12 = free block payload minus requested payload
    emitter.instruction("cmp x12, #24");                                        // is there room for a new 16-byte header plus minimum payload?
    emitter.instruction("b.lt __rt_heap_alloc_fl_take_whole");                  // no — consume the whole free block
    emitter.instruction("add x13, x10, x0");                                    // x13 = current header + requested payload
    emitter.instruction("add x13, x13, #16");                                   // x13 = split remainder header address
    emitter.instruction("sub x12, x12, #16");                                   // x12 = remainder payload size after carving out a new header
    emitter.instruction("str w12, [x13]");                                      // write split remainder size into its header
    emitter.instruction("str wzr, [x13, #4]");                                  // free remainder keeps refcount cleared while on the free list
    emitter.instruction("str xzr, [x13, #8]");                                  // free remainder has no heap kind while on the free list
    emitter.instruction("ldr x14, [x10, #16]");                                 // x14 = current->next before splitting
    emitter.instruction("str x14, [x13, #16]");                                 // remainder->next = current->next
    emitter.instruction("str x13, [x9]");                                       // prev->next = remainder header
    emitter.instruction("str w0, [x10]");                                       // shrink allocated block header size to the requested payload
    emitter.instruction("mov w13, #1");                                         // initial refcount = 1
    emitter.instruction("str w13, [x10, #4]");                                  // reset refcount in reused header
    emitter.instruction("str xzr, [x10, #8]");                                  // reset heap kind to raw until a typed constructor overwrites it
    emitter.instruction("add x0, x10, #16");                                    // return user pointer = header + 16
    emitter.instruction("b __rt_heap_alloc_count");                             // count allocation and return

    emitter.label("__rt_heap_alloc_fl_take_whole");
    emitter.instruction("ldr x12, [x10, #16]");                                 // x12 = current->next (rest of list)
    emitter.instruction("str x12, [x9]");                                       // prev->next = current->next (unlink current)
    emitter.instruction("mov w13, #1");                                         // initial refcount = 1
    emitter.instruction("str w13, [x10, #4]");                                  // reset refcount in reused header
    emitter.instruction("str xzr, [x10, #8]");                                  // reset heap kind to raw until a typed constructor overwrites it
    emitter.instruction("add x0, x10, #16");                                    // return user pointer = header + 16

    emitter.label("__rt_heap_alloc_count");
    // -- increment gc_allocs counter --
    crate::codegen::abi::emit_symbol_address(emitter, "x12", "_gc_allocs");
    emitter.instruction("ldr x13, [x12]");                                      // load current count
    emitter.instruction("add x13, x13, #1");                                    // increment
    emitter.instruction("str x13, [x12]");                                      // store back
    // -- update current/peak live heap footprint --
    emitter.instruction("ldr w14, [x10]");                                      // load the allocated payload size from the finalized header
    emitter.instruction("add x14, x14, #16");                                   // include the 16-byte header in the live-footprint accounting
    crate::codegen::abi::emit_symbol_address(emitter, "x12", "_gc_live");
    emitter.instruction("ldr x13, [x12]");                                      // load current live bytes
    emitter.instruction("add x13, x13, x14");                                   // add this block's total footprint to live bytes
    emitter.instruction("str x13, [x12]");                                      // store updated live bytes
    crate::codegen::abi::emit_symbol_address(emitter, "x12", "_gc_peak");
    emitter.instruction("ldr x15, [x12]");                                      // load the previous live-byte high watermark
    emitter.instruction("cmp x13, x15");                                        // did this allocation raise the live-byte peak?
    emitter.instruction("csel x15, x13, x15, hi");                              // keep the larger of current live bytes and the previous peak
    emitter.instruction("str x15, [x12]");                                      // store the updated peak-live-bytes counter
    emitter.instruction("ret");                                                 // return to caller

    // -- no free block found, bump allocate with header --
    emitter.label("__rt_heap_alloc_bump");

    // -- load current heap offset --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_off");
    emitter.instruction("ldr x10, [x9]");                                       // x10 = current heap offset

    // -- bounds check: offset + 16 + requested <= heap_max --
    emitter.instruction("add x12, x10, x0");                                    // x12 = offset + requested size
    emitter.instruction("add x12, x12, #16");                                   // x12 = offset + requested + header (16 bytes)
    crate::codegen::abi::emit_symbol_address(emitter, "x13", "_heap_max");
    emitter.instruction("ldr x13, [x13]");                                      // x13 = heap max size in bytes
    emitter.instruction("cmp x12, x13");                                        // does the allocation fit?
    emitter.instruction("b.gt __rt_heap_exhausted");                            // no — fatal error

    // -- compute base address of heap buffer --
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_heap_buf");

    // -- write header and bump offset --
    emitter.instruction("add x14, x11, x10");                                   // x14 = buf + offset (header location)
    emitter.instruction("str w0, [x14]");                                       // write block size to header (32-bit)
    emitter.instruction("mov w15, #1");                                         // initial refcount = 1
    emitter.instruction("str w15, [x14, #4]");                                  // write refcount to header upper half
    emitter.instruction("str xzr, [x14, #8]");                                  // initialize heap kind to raw until a typed constructor overwrites it
    emitter.instruction("add x10, x10, x0");                                    // advance offset by requested size
    emitter.instruction("add x10, x10, #16");                                   // advance offset by header size
    emitter.instruction("str x10, [x9]");                                       // store updated offset to _heap_off
    emitter.instruction("add x0, x14, #16");                                    // return user pointer = header + 16
    emitter.instruction("mov x10, x14");                                        // reuse the common allocation-accounting path with the new block header pointer
    emitter.instruction("b __rt_heap_alloc_count");                             // count alloc/live/peak stats and return

    // -- fatal error: heap memory exhausted --
    emitter.label("__rt_heap_exhausted");
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_heap_err_msg");
    emitter.instruction("mov x2, #35");                                         // message length: "Fatal error: heap memory exhausted\n"
    emitter.syscall(4);
    emitter.instruction("mov x0, #1");                                          // exit code 1
    emitter.syscall(1);
}
