use crate::codegen::emit::Emitter;

/// implode_int: join integer array elements with glue string, converting each to string.
/// Input: x1/x2=glue, x3=array_ptr
/// Output: x1=result_ptr, x2=result_len
pub fn emit_implode_int(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: implode_int ---");
    emitter.label_global("__rt_implode_int");

    // -- set up stack frame (80 bytes) --
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish new frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save glue string ptr and length
    emitter.instruction("str x3, [sp, #16]");                                   // save array pointer

    // -- get concat_buf write position --
    emitter.adrp("x6", "_concat_off");                           // load page address of concat buffer offset
    emitter.add_lo12("x6", "x6", "_concat_off");                     // resolve exact address of offset variable
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    emitter.adrp("x7", "_concat_buf");                           // load page address of concat buffer
    emitter.add_lo12("x7", "x7", "_concat_buf");                     // resolve exact buffer base address
    emitter.instruction("add x9, x7, x8");                                      // compute destination pointer
    emitter.instruction("str x9, [sp, #24]");                                   // save result start pointer
    emitter.instruction("str x6, [sp, #32]");                                   // save offset variable address
    emitter.instruction("str x9, [sp, #40]");                                   // save current dest pointer

    // -- load array length and initialize index --
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload array pointer
    emitter.instruction("ldr x10, [x3]");                                       // load array element count
    emitter.instruction("str x10, [sp, #48]");                                  // save element count
    emitter.instruction("str xzr, [sp, #56]");                                  // initialize element index = 0

    // -- main loop: join elements with glue --
    emitter.label("__rt_implode_int_loop");
    emitter.instruction("ldr x11, [sp, #56]");                                  // load current element index
    emitter.instruction("ldr x10, [sp, #48]");                                  // load element count
    emitter.instruction("cmp x11, x10");                                        // check if all elements processed
    emitter.instruction("b.ge __rt_implode_int_done");                          // if done, finalize result

    // -- insert glue before element (skip for first element) --
    emitter.instruction("cbz x11, __rt_implode_int_elem");                      // skip glue before first element
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload glue ptr and length
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload current dest pointer
    emitter.instruction("mov x12, x2");                                         // copy glue length as counter
    emitter.label("__rt_implode_int_glue");
    emitter.instruction("cbz x12, __rt_implode_int_elem");                      // if no glue bytes remain, copy element
    emitter.instruction("ldrb w13, [x1], #1");                                  // load glue byte, advance glue ptr
    emitter.instruction("strb w13, [x9], #1");                                  // store to dest, advance dest ptr
    emitter.instruction("sub x12, x12, #1");                                    // decrement glue byte counter
    emitter.instruction("b __rt_implode_int_glue");                             // continue copying glue

    // -- convert current integer element to string via itoa --
    emitter.label("__rt_implode_int_elem");
    emitter.instruction("str x9, [sp, #40]");                                   // save updated dest pointer
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload array pointer
    emitter.instruction("ldr x11, [sp, #56]");                                  // reload current element index
    emitter.instruction("add x3, x3, #24");                                     // skip 24-byte array header to reach data
    emitter.instruction("ldr x0, [x3, x11, lsl #3]");                           // load integer element at index (8 bytes each)
    emitter.instruction("bl __rt_itoa");                                        // convert integer to string → x1=ptr, x2=len

    // -- copy itoa result bytes to output --
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload dest pointer
    emitter.instruction("mov x12, x2");                                         // copy string length as counter
    emitter.label("__rt_implode_int_copy");
    emitter.instruction("cbz x12, __rt_implode_int_next");                      // if no bytes remain, move to next element
    emitter.instruction("ldrb w13, [x1], #1");                                  // load string byte, advance src ptr
    emitter.instruction("strb w13, [x9], #1");                                  // store to dest, advance dest ptr
    emitter.instruction("sub x12, x12, #1");                                    // decrement byte counter
    emitter.instruction("b __rt_implode_int_copy");                             // continue copying string

    // -- advance to next element --
    emitter.label("__rt_implode_int_next");
    emitter.instruction("str x9, [sp, #40]");                                   // save updated dest pointer
    emitter.instruction("ldr x11, [sp, #56]");                                  // reload element index
    emitter.instruction("add x11, x11, #1");                                    // increment element index
    emitter.instruction("str x11, [sp, #56]");                                  // save updated index
    emitter.instruction("b __rt_implode_int_loop");                             // process next element

    // -- finalize: compute result length and update concat_off --
    emitter.label("__rt_implode_int_done");
    emitter.instruction("ldr x9, [sp, #40]");                                   // load final dest pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // load result start pointer
    emitter.instruction("sub x2, x9, x1");                                      // result length = dest_end - dest_start
    emitter.instruction("ldr x6, [sp, #32]");                                   // load offset variable address
    emitter.instruction("ldr x8, [x6]");                                        // load current concat_off
    emitter.instruction("add x8, x8, x2");                                      // advance offset by result length
    emitter.instruction("str x8, [x6]");                                        // store updated concat_off

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
