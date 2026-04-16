use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// implode: join array elements with glue string.
/// Input: x1/x2=glue, x3=array_ptr
/// Output: x1=result_ptr, x2=result_len
pub fn emit_implode(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_implode_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: implode ---");
    emitter.label_global("__rt_implode");

    // -- set up stack frame (64 bytes) --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save glue string ptr and length
    emitter.instruction("str x3, [sp, #16]");                                   // save array pointer

    // -- get concat_buf write position --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // compute destination pointer
    emitter.instruction("str x9, [sp, #24]");                                   // save result start pointer
    emitter.instruction("str x6, [sp, #32]");                                   // save offset variable address

    // -- load array length and initialize index --
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload array pointer
    emitter.instruction("ldr x10, [x3]");                                       // load array element count
    emitter.instruction("mov x11, #0");                                         // initialize element index = 0

    // -- main loop: join elements with glue --
    emitter.label("__rt_implode_loop");
    emitter.instruction("cmp x11, x10");                                        // check if all elements processed
    emitter.instruction("b.ge __rt_implode_done");                              // if done, finalize result

    // -- insert glue before element (skip for first element) --
    emitter.instruction("cbz x11, __rt_implode_elem");                          // skip glue before first element
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload glue ptr and length
    emitter.instruction("mov x12, x2");                                         // copy glue length as counter
    emitter.label("__rt_implode_glue");
    emitter.instruction("cbz x12, __rt_implode_elem");                          // if no glue bytes remain, copy element
    emitter.instruction("ldrb w13, [x1], #1");                                  // load glue byte, advance glue ptr
    emitter.instruction("strb w13, [x9], #1");                                  // store to dest, advance dest ptr
    emitter.instruction("sub x12, x12, #1");                                    // decrement glue byte counter
    emitter.instruction("b __rt_implode_glue");                                 // continue copying glue

    // -- copy current array element --
    emitter.label("__rt_implode_elem");
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload array pointer
    emitter.instruction("lsl x12, x11, #4");                                    // compute byte offset: index * 16
    emitter.instruction("add x12, x3, x12");                                    // add to array base
    emitter.instruction("add x12, x12, #24");                                   // skip 24-byte array header
    emitter.instruction("ldr x1, [x12]");                                       // load element string pointer
    emitter.instruction("ldr x2, [x12, #8]");                                   // load element string length

    // -- copy element bytes to output --
    emitter.instruction("mov x12, x2");                                         // copy element length as counter
    emitter.label("__rt_implode_copy");
    emitter.instruction("cbz x12, __rt_implode_next");                          // if no bytes remain, move to next element
    emitter.instruction("ldrb w13, [x1], #1");                                  // load element byte, advance src ptr
    emitter.instruction("strb w13, [x9], #1");                                  // store to dest, advance dest ptr
    emitter.instruction("sub x12, x12, #1");                                    // decrement byte counter
    emitter.instruction("b __rt_implode_copy");                                 // continue copying element

    // -- advance to next element --
    emitter.label("__rt_implode_next");
    emitter.instruction("add x11, x11, #1");                                    // increment element index
    emitter.instruction("b __rt_implode_loop");                                 // process next element

    // -- finalize: compute result length and update concat_off --
    emitter.label("__rt_implode_done");
    emitter.instruction("ldr x1, [sp, #24]");                                   // load result start pointer
    emitter.instruction("sub x2, x9, x1");                                      // result length = dest_end - dest_start
    emitter.instruction("ldr x6, [sp, #32]");                                   // load offset variable address
    emitter.instruction("ldr x8, [x6]");                                        // load current concat_off
    emitter.instruction("add x8, x8, x2");                                      // advance offset by result length
    emitter.instruction("str x8, [x6]");                                        // store updated concat_off

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_implode_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: implode ---");
    emitter.label_global("__rt_implode");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving implode spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for glue, array, and concat-buffer bookkeeping
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for glue, array, concat destination, array length, and loop index
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the glue string pointer across the indexed-array copy loop and concat-buffer bookkeeping
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the glue string length across the indexed-array copy loop and concat-buffer bookkeeping
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the indexed-array pointer across the element copy loop and concat-buffer bookkeeping
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer write offset before materializing the implode output start pointer
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r10, [r10 + r9]");                                 // compute the current concat-buffer destination pointer for the implode output
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // preserve the implode result start pointer so the final string result can reference the copied bytes
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the current concat-buffer destination cursor across glue and element copy loops
    emitter.instruction("mov r11, QWORD PTR [rdx]");                            // load the indexed-array logical length once before entering the implode loop
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // preserve the indexed-array logical length for the loop termination check
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // initialize the indexed-array loop cursor to the first element

    emitter.label("__rt_implode_loop");
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the current indexed-array loop cursor before deciding whether implode is complete
    emitter.instruction("cmp r11, QWORD PTR [rbp - 48]");                       // compare the current indexed-array loop cursor against the saved logical length
    emitter.instruction("jae __rt_implode_done");                               // stop once every indexed-array element has been copied into the concat buffer
    emitter.instruction("test r11, r11");                                       // check whether the current element is the first one in the indexed array
    emitter.instruction("jz __rt_implode_elem");                                // skip glue emission before copying the first indexed-array element
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the glue string pointer before copying the separator bytes
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the glue string length before copying the separator bytes
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current concat-buffer destination cursor before copying the separator bytes

    emitter.label("__rt_implode_glue");
    emitter.instruction("test r9, r9");                                         // check whether every glue byte has already been copied into the concat buffer
    emitter.instruction("jz __rt_implode_glue_done");                           // continue with the array element once the glue string has been fully copied
    emitter.instruction("mov r11b, BYTE PTR [r8]");                             // load one byte from the glue string before advancing the source pointer
    emitter.instruction("mov BYTE PTR [r10], r11b");                            // store one separator byte into the concat buffer before advancing the destination pointer
    emitter.instruction("add r8, 1");                                           // advance the glue string source pointer after copying one separator byte
    emitter.instruction("add r10, 1");                                          // advance the concat-buffer destination pointer after storing one separator byte
    emitter.instruction("sub r9, 1");                                           // decrement the remaining glue byte count after copying one separator byte
    emitter.instruction("jmp __rt_implode_glue");                               // continue copying separator bytes until the glue string is exhausted

    emitter.label("__rt_implode_glue_done");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the concat-buffer destination cursor after copying the separator bytes

    emitter.label("__rt_implode_elem");
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the current indexed-array loop cursor before locating the next string element slot
    emitter.instruction("mov rcx, r11");                                        // copy the indexed-array loop cursor before scaling it into a string-slot byte offset
    emitter.instruction("shl rcx, 4");                                          // convert the indexed-array loop cursor into the 16-byte offset of the current string slot
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // reload the indexed-array pointer before addressing the current string slot
    emitter.instruction("lea rcx, [r8 + rcx + 24]");                            // compute the address of the current indexed-array string slot after the fixed array header
    emitter.instruction("mov r8, QWORD PTR [rcx]");                             // load the current indexed-array string pointer before copying the element bytes
    emitter.instruction("mov r9, QWORD PTR [rcx + 8]");                         // load the current indexed-array string length before copying the element bytes
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current concat-buffer destination cursor before copying the element bytes

    emitter.label("__rt_implode_copy");
    emitter.instruction("test r9, r9");                                         // check whether every element byte has already been copied into the concat buffer
    emitter.instruction("jz __rt_implode_next");                                // advance to the next indexed-array element once the current string is fully copied
    emitter.instruction("mov r11b, BYTE PTR [r8]");                             // load one byte from the current indexed-array string before advancing the source pointer
    emitter.instruction("mov BYTE PTR [r10], r11b");                            // store one byte from the current indexed-array string into the concat buffer
    emitter.instruction("add r8, 1");                                           // advance the current indexed-array string source pointer after copying one byte
    emitter.instruction("add r10, 1");                                          // advance the concat-buffer destination pointer after storing one byte
    emitter.instruction("sub r9, 1");                                           // decrement the remaining current string byte count after copying one byte
    emitter.instruction("jmp __rt_implode_copy");                               // continue copying bytes from the current indexed-array string until it is exhausted

    emitter.label("__rt_implode_next");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the concat-buffer destination cursor after copying the current indexed-array element
    emitter.instruction("add QWORD PTR [rbp - 56], 1");                         // advance the indexed-array loop cursor to the next element
    emitter.instruction("jmp __rt_implode_loop");                               // continue joining indexed-array elements into the concat buffer

    emitter.label("__rt_implode_done");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the final concat-buffer destination cursor to compute the joined string length
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the implode result start pointer before computing the joined string length
    emitter.instruction("mov rdx, r10");                                        // copy the final concat-buffer destination cursor before subtracting the result start pointer
    emitter.instruction("sub rdx, rax");                                        // compute the joined string length as dest_end - dest_start
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // reload the current concat-buffer write offset after any nested helpers have advanced it
    emitter.instruction("add r9, rdx");                                         // advance the concat-buffer write offset by the joined string length that this implode call produced
    emitter.instruction("mov QWORD PTR [r8], r9");                              // persist the updated concat-buffer write offset after writing the implode output bytes
    emitter.instruction("add rsp, 64");                                         // release the implode spill slots before returning the joined string
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the joined string
    emitter.instruction("ret");                                                 // return the joined string in the standard x86_64 string result registers
}
