use crate::codegen::emit::Emitter;

/// array_chunk_refcounted: split an array of refcounted 8-byte payloads into chunks.
/// Input:  x0=array_ptr, x1=chunk_size
/// Output: x0=outer array (array of inner array pointers)
pub fn emit_array_chunk_refcounted(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_chunk_refcounted ---");
    emitter.label_global("__rt_array_chunk_refcounted");

    // -- set up stack frame, save arguments --
    emitter.instruction("sub sp, sp, #80");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save chunk size

    // -- calculate number of chunks: ceil(length / chunk_size) --
    emitter.instruction("ldr x2, [x0]");                                        // load source array length
    emitter.instruction("sub x3, x1, #1");                                      // compute chunk_size - 1
    emitter.instruction("add x2, x2, x3");                                      // bias numerator for ceiling division
    emitter.instruction("udiv x2, x2, x1");                                     // compute number of chunks

    // -- create outer array to hold chunk pointers --
    emitter.instruction("mov x0, x2");                                          // move outer array capacity into x0
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for inner array pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate outer array
    emitter.instruction("str x0, [sp, #16]");                                   // save outer array pointer

    // -- initialize source and inner-loop indices --
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize source index i = 0

    emitter.label("__rt_array_chunk_ref_outer");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // load source array length
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload source index i
    emitter.instruction("cmp x4, x3");                                          // compare source index with source length
    emitter.instruction("b.ge __rt_array_chunk_ref_done");                      // stop when every source element has been consumed

    // -- create inner array for this chunk --
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload chunk size as inner capacity
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for refcounted payload pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate inner array
    emitter.instruction("str x0, [sp, #32]");                                   // save inner array pointer
    emitter.instruction("str xzr, [sp, #40]");                                  // initialize inner index j = 0

    emitter.label("__rt_array_chunk_ref_inner");
    emitter.instruction("ldr x5, [sp, #40]");                                   // reload inner index j
    emitter.instruction("ldr x6, [sp, #8]");                                    // reload chunk size
    emitter.instruction("cmp x5, x6");                                          // compare inner index with chunk size
    emitter.instruction("b.ge __rt_array_chunk_ref_push");                      // push current chunk once it is full
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // reload source array length
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload source index i
    emitter.instruction("cmp x4, x3");                                          // compare source index with source length
    emitter.instruction("b.ge __rt_array_chunk_ref_push");                      // push partial chunk once source is exhausted
    emitter.instruction("add x7, x0, #24");                                     // compute source data base
    emitter.instruction("ldr x1, [x7, x4, lsl #3]");                            // load source element pointer
    emitter.instruction("str x1, [sp, #48]");                                   // preserve source element pointer across incref
    emitter.instruction("mov x0, x1");                                          // move element pointer into incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain source-owned payload before copying into inner array
    emitter.instruction("ldr x1, [sp, #48]");                                   // restore retained element pointer
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload inner array pointer
    emitter.instruction("bl __rt_array_push_int");                              // append retained payload pointer to inner array
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload source index after helper calls
    emitter.instruction("add x4, x4, #1");                                      // increment source index
    emitter.instruction("str x4, [sp, #24]");                                   // persist updated source index
    emitter.instruction("ldr x5, [sp, #40]");                                   // reload inner index after helper calls
    emitter.instruction("add x5, x5, #1");                                      // increment inner index
    emitter.instruction("str x5, [sp, #40]");                                   // persist updated inner index
    emitter.instruction("b __rt_array_chunk_ref_inner");                        // continue filling the current inner chunk

    emitter.label("__rt_array_chunk_ref_push");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload outer array pointer
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload inner array pointer
    emitter.instruction("bl __rt_array_push_int");                              // transfer inner array ownership into outer array
    emitter.instruction("b __rt_array_chunk_ref_outer");                        // continue with the next chunk

    emitter.label("__rt_array_chunk_ref_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload outer array pointer as return value
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return outer array
}
