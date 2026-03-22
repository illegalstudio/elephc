use crate::codegen::emit::Emitter;

/// heap_alloc: bump allocator.
/// Input: x0 = bytes needed
/// Output: x0 = pointer to allocated memory
pub fn emit_heap_alloc(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: heap_alloc ---");
    emitter.label("__rt_heap_alloc");

    // -- load the current heap offset from the global variable --
    emitter.instruction("adrp x9, _heap_off@PAGE");                             // load page base of _heap_off into x9
    emitter.instruction("add x9, x9, _heap_off@PAGEOFF");                       // add page offset to get exact address of _heap_off
    emitter.instruction("ldr x10, [x9]");                                       // x10 = current heap offset (bytes used so far)

    // -- compute the base address of the heap buffer --
    emitter.instruction("adrp x11, _heap_buf@PAGE");                            // load page base of _heap_buf into x11
    emitter.instruction("add x11, x11, _heap_buf@PAGEOFF");                     // add page offset to get exact address of _heap_buf

    // -- bump the allocator: return current position, advance offset --
    emitter.instruction("add x12, x11, x10");                                   // x12 = heap_buf + offset = pointer to free memory
    emitter.instruction("add x10, x10, x0");                                    // advance offset by requested byte count
    emitter.instruction("str x10, [x9]");                                       // store updated offset back to _heap_off
    emitter.instruction("mov x0, x12");                                         // return the allocated pointer in x0
    emitter.instruction("ret");                                                 // return to caller
}
