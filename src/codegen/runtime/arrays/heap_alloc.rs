use crate::codegen::emit::Emitter;

/// heap_alloc: free-list allocator with 8-byte header.
/// Each allocation has an 8-byte header [block_size] before the user pointer.
/// Free blocks are organized as a singly-linked list: [size:8][next_ptr:8].
/// Input: x0 = bytes needed
/// Output: x0 = pointer to allocated memory (after header)
pub fn emit_heap_alloc(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: heap_alloc (free-list + bump) ---");
    emitter.label("__rt_heap_alloc");

    // -- enforce minimum allocation of 8 bytes (free list needs space for next ptr) --
    emitter.instruction("cmp x0, #8");                                          // is requested size < 8?
    emitter.instruction("b.ge __rt_heap_alloc_start");                          // skip if already >= 8
    emitter.instruction("mov x0, #8");                                          // round up to minimum 8 bytes
    emitter.label("__rt_heap_alloc_start");

    // -- debug mode: validate the free list before consuming it --
    emitter.instruction("adrp x9, _heap_debug_enabled@PAGE");                   // load page of the heap-debug enabled flag
    emitter.instruction("add x9, x9, _heap_debug_enabled@PAGEOFF");             // resolve the heap-debug enabled flag address
    emitter.instruction("ldr x9, [x9]");                                        // load the heap-debug enabled flag
    emitter.instruction("cbz x9, __rt_heap_alloc_debug_checked");               // skip validation when heap-debug mode is disabled
    emitter.instruction("mov x15, x0");                                         // preserve the requested allocation size across validation
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the caller return address before making a nested call
    emitter.instruction("bl __rt_heap_debug_validate_free_list");                // verify the ordered free list before searching it
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the caller return address after validation
    emitter.instruction("mov x0, x15");                                         // restore the requested allocation size after validation
    emitter.label("__rt_heap_alloc_debug_checked");

    // -- try to find a free block first --
    // x0 = requested size, x9 = prev_next_addr, x10 = current block header
    emitter.instruction("adrp x9, _heap_free_list@PAGE");                       // load page of free list head pointer
    emitter.instruction("add x9, x9, _heap_free_list@PAGEOFF");                 // resolve address of free list head
    emitter.instruction("ldr x10, [x9]");                                       // x10 = first free block header (0 if empty)

    // -- walk the free list looking for first-fit block --
    emitter.label("__rt_heap_alloc_fl_loop");
    emitter.instruction("cbz x10, __rt_heap_alloc_bump");                       // no free block found, fall through to bump
    emitter.instruction("ldr w11, [x10]");                                      // x11 = block size (32-bit, zero-extends)
    emitter.instruction("cmp x11, x0");                                         // does this block fit the request?
    emitter.instruction("b.ge __rt_heap_alloc_fl_found");                       // yes — use this block

    // -- advance to next free block --
    emitter.instruction("add x9, x10, #8");                                     // prev_next_addr = &current->next
    emitter.instruction("ldr x10, [x10, #8]");                                  // current = current->next
    emitter.instruction("b __rt_heap_alloc_fl_loop");                           // continue searching

    // -- found a suitable free block, either split it or unlink it whole --
    emitter.label("__rt_heap_alloc_fl_found");
    emitter.instruction("sub x12, x11, x0");                                    // x12 = free block payload minus requested payload
    emitter.instruction("cmp x12, #16");                                        // is there room for a new free header plus minimum payload?
    emitter.instruction("b.lt __rt_heap_alloc_fl_take_whole");                   // no — consume the whole free block
    emitter.instruction("add x13, x10, x0");                                    // x13 = current header + requested payload
    emitter.instruction("add x13, x13, #8");                                    // x13 = split remainder header address
    emitter.instruction("sub x12, x12, #8");                                    // x12 = remainder payload size after carving out a new header
    emitter.instruction("str w12, [x13]");                                      // write split remainder size into its header
    emitter.instruction("ldr x14, [x10, #8]");                                  // x14 = current->next before splitting
    emitter.instruction("str x14, [x13, #8]");                                  // remainder->next = current->next
    emitter.instruction("str x13, [x9]");                                       // prev->next = remainder header
    emitter.instruction("str w0, [x10]");                                       // shrink allocated block header size to the requested payload
    emitter.instruction("mov w13, #1");                                         // initial refcount = 1
    emitter.instruction("str w13, [x10, #4]");                                  // reset refcount in reused header
    emitter.instruction("add x0, x10, #8");                                     // return user pointer = header + 8
    emitter.instruction("b __rt_heap_alloc_count");                             // count allocation and return

    emitter.label("__rt_heap_alloc_fl_take_whole");
    emitter.instruction("ldr x12, [x10, #8]");                                  // x12 = current->next (rest of list)
    emitter.instruction("str x12, [x9]");                                       // prev->next = current->next (unlink current)
    emitter.instruction("mov w13, #1");                                         // initial refcount = 1
    emitter.instruction("str w13, [x10, #4]");                                  // reset refcount in reused header
    emitter.instruction("add x0, x10, #8");                                     // return user pointer = header + 8

    emitter.label("__rt_heap_alloc_count");
    // -- increment gc_allocs counter --
    emitter.instruction("adrp x12, _gc_allocs@PAGE");                           // load gc_allocs page
    emitter.instruction("add x12, x12, _gc_allocs@PAGEOFF");                    // resolve address
    emitter.instruction("ldr x13, [x12]");                                      // load current count
    emitter.instruction("add x13, x13, #1");                                    // increment
    emitter.instruction("str x13, [x12]");                                      // store back
    emitter.instruction("ret");                                                 // return to caller

    // -- no free block found, bump allocate with header --
    emitter.label("__rt_heap_alloc_bump");

    // -- load current heap offset --
    emitter.instruction("adrp x9, _heap_off@PAGE");                             // load page base of _heap_off
    emitter.instruction("add x9, x9, _heap_off@PAGEOFF");                       // resolve exact address of _heap_off
    emitter.instruction("ldr x10, [x9]");                                       // x10 = current heap offset

    // -- bounds check: offset + 8 + requested <= heap_max --
    emitter.instruction("add x12, x10, x0");                                    // x12 = offset + requested size
    emitter.instruction("add x12, x12, #8");                                    // x12 = offset + requested + header (8 bytes)
    emitter.instruction("adrp x13, _heap_max@PAGE");                            // load page of heap max constant
    emitter.instruction("add x13, x13, _heap_max@PAGEOFF");                     // resolve address of heap max
    emitter.instruction("ldr x13, [x13]");                                      // x13 = heap max size in bytes
    emitter.instruction("cmp x12, x13");                                        // does the allocation fit?
    emitter.instruction("b.gt __rt_heap_exhausted");                            // no — fatal error

    // -- compute base address of heap buffer --
    emitter.instruction("adrp x11, _heap_buf@PAGE");                            // load page base of _heap_buf
    emitter.instruction("add x11, x11, _heap_buf@PAGEOFF");                     // resolve exact buffer base address

    // -- write header and bump offset --
    emitter.instruction("add x14, x11, x10");                                   // x14 = buf + offset (header location)
    emitter.instruction("str w0, [x14]");                                       // write block size to header (32-bit)
    emitter.instruction("mov w15, #1");                                         // initial refcount = 1
    emitter.instruction("str w15, [x14, #4]");                                  // write refcount to header upper half
    emitter.instruction("add x10, x10, x0");                                    // advance offset by requested size
    emitter.instruction("add x10, x10, #8");                                    // advance offset by header size
    emitter.instruction("str x10, [x9]");                                       // store updated offset to _heap_off
    emitter.instruction("add x0, x14, #8");                                     // return user pointer = header + 8
    // -- increment gc_allocs counter --
    emitter.instruction("adrp x12, _gc_allocs@PAGE");                           // load gc_allocs page
    emitter.instruction("add x12, x12, _gc_allocs@PAGEOFF");                    // resolve address
    emitter.instruction("ldr x13, [x12]");                                      // load current count
    emitter.instruction("add x13, x13, #1");                                    // increment
    emitter.instruction("str x13, [x12]");                                      // store back
    emitter.instruction("ret");                                                 // return to caller

    // -- fatal error: heap memory exhausted --
    emitter.label("__rt_heap_exhausted");
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("adrp x1, _heap_err_msg@PAGE");                         // load page of error message
    emitter.instruction("add x1, x1, _heap_err_msg@PAGEOFF");                   // resolve error message address
    emitter.instruction("mov x2, #35");                                         // message length: "Fatal error: heap memory exhausted\n"
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // write error to stderr
    emitter.instruction("mov x0, #1");                                          // exit code 1
    emitter.instruction("mov x16, #1");                                         // syscall 1 = sys_exit
    emitter.instruction("svc #0x80");                                           // terminate process
}
