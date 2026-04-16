use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_fill: create an array filled with a specified integer value.
/// Input: x0 = start_index (ignored for indexed arrays), x1 = count, x2 = value
/// Output: x0 = pointer to new array with count elements all set to value
pub fn emit_array_fill(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_fill_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_fill ---");
    emitter.label_global("__rt_array_fill");

    // -- set up stack frame, save arguments --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save count
    emitter.instruction("str x2, [sp, #8]");                                    // save fill value

    // -- create new array with capacity = count --
    emitter.instruction("mov x0, x1");                                          // x0 = capacity = count
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size = 8 (integers)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array
    emitter.instruction("str x0, [sp, #16]");                                   // save new array pointer

    // -- fill array with the value --
    emitter.instruction("add x3, x0, #24");                                     // x3 = data base of new array
    emitter.instruction("ldr x4, [sp, #0]");                                    // x4 = count
    emitter.instruction("ldr x5, [sp, #8]");                                    // x5 = fill value
    emitter.instruction("mov x6, #0");                                          // x6 = i = 0

    emitter.label("__rt_array_fill_loop");
    emitter.instruction("cmp x6, x4");                                          // compare i with count
    emitter.instruction("b.ge __rt_array_fill_done");                           // if i >= count, filling complete
    emitter.instruction("str x5, [x3, x6, lsl #3]");                            // data[i] = fill value
    emitter.instruction("add x6, x6, #1");                                      // i += 1
    emitter.instruction("b __rt_array_fill_loop");                              // continue loop

    // -- set length and return --
    emitter.label("__rt_array_fill_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = new array pointer
    emitter.instruction("ldr x9, [sp, #0]");                                    // x9 = count
    emitter.instruction("str x9, [x0]");                                        // set array length = count

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = filled array
}

fn emit_array_fill_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_fill ---");
    emitter.label_global("__rt_array_fill");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving scalar fill spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for count, value, destination array, and loop index bookkeeping
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for count, value, destination array, and loop index bookkeeping
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // preserve the requested element count across destination-array allocation and fill bookkeeping
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the scalar fill payload across destination-array allocation and fill bookkeeping
    emitter.instruction("mov rdi, rsi");                                        // pass the requested element count as the destination indexed-array capacity to the x86_64 constructor
    emitter.instruction("mov rsi, 8");                                          // use 8-byte payload slots because this helper fills scalar indexed arrays
    emitter.instruction("call __rt_array_new");                                 // allocate the destination indexed array through the shared x86_64 constructor
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the destination indexed-array pointer while the fill loop writes payload slots
    emitter.instruction("lea r8, [rax + 24]");                                  // compute the destination indexed-array payload base address once before the fill loop starts
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the requested element count before entering the fill loop
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the scalar fill payload before entering the fill loop
    emitter.instruction("xor rcx, rcx");                                        // initialize the fill loop index to the first destination payload slot
    emitter.label("__rt_array_fill_loop_x86");
    emitter.instruction("cmp rcx, r9");                                         // compare the current fill loop index against the requested element count
    emitter.instruction("jge __rt_array_fill_done_x86");                        // stop once every requested destination payload slot has been initialized
    emitter.instruction("mov QWORD PTR [r8 + rcx * 8], r10");                   // write the scalar fill payload into the current destination indexed-array slot
    emitter.instruction("add rcx, 1");                                          // advance the fill loop index after initializing one destination payload slot
    emitter.instruction("jmp __rt_array_fill_loop_x86");                        // continue filling scalar payload slots until the requested count is satisfied
    emitter.label("__rt_array_fill_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the destination indexed-array pointer before publishing the filled logical length
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the requested element count before publishing the filled logical length
    emitter.instruction("mov QWORD PTR [rax], r9");                             // publish the filled logical length in the destination indexed-array header
    emitter.instruction("add rsp, 32");                                         // release the scalar fill spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the filled destination indexed-array pointer in rax
}
