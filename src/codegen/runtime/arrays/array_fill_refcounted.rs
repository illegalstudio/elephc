use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_fill_refcounted: create an array filled with copies of a borrowed refcounted payload.
/// Input: x0 = start_index (ignored), x1 = count, x2 = borrowed heap pointer
/// Output: x0 = pointer to new array with count retained copies of the payload
pub fn emit_array_fill_refcounted(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_fill_refcounted_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_fill_refcounted ---");
    emitter.label_global("__rt_array_fill_refcounted");

    // -- set up stack frame, save count and payload --
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save element count
    emitter.instruction("str x2, [sp, #8]");                                    // save borrowed payload pointer

    // -- create destination array --
    emitter.instruction("mov x0, x1");                                          // use count as destination capacity
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for heap pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate destination array
    emitter.instruction("str x0, [sp, #16]");                                   // save destination array pointer

    // -- append retained copies of the payload --
    emitter.instruction("mov x6, #0");                                          // initialize loop index
    emitter.label("__rt_array_fill_ref_loop");
    emitter.instruction("ldr x4, [sp, #0]");                                    // reload count
    emitter.instruction("cmp x6, x4");                                          // compare loop index with count
    emitter.instruction("b.ge __rt_array_fill_ref_done");                       // finish after pushing count elements
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload borrowed payload pointer
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload destination array pointer
    emitter.instruction("bl __rt_array_push_refcounted");                       // append retained payload into destination array
    emitter.instruction("str x0, [sp, #16]");                                   // persist destination pointer after possible growth
    emitter.instruction("add x6, x6, #1");                                      // increment loop index
    emitter.instruction("b __rt_array_fill_ref_loop");                          // continue filling

    emitter.label("__rt_array_fill_ref_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload destination array pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return filled array
}

fn emit_array_fill_refcounted_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_fill_refcounted ---");
    emitter.label_global("__rt_array_fill_refcounted");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving array-fill spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for count, payload, destination array, and loop index bookkeeping
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for count, payload, destination array, and loop index bookkeeping
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // preserve the requested element count across destination-array allocation and append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the borrowed payload pointer across destination-array allocation and append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the fill loop index spill slot to the first destination payload slot
    emitter.instruction("mov rdi, rsi");                                        // pass the requested element count as the destination indexed-array capacity to the x86_64 constructor
    emitter.instruction("mov rsi, 8");                                          // use 8-byte payload slots because the destination indexed array stores retained child pointers
    emitter.instruction("call __rt_array_new");                                 // allocate the destination indexed array through the shared x86_64 constructor
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the destination indexed-array pointer across repeated refcounted append helper calls
    emitter.label("__rt_array_fill_ref_loop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the fill loop index before appending the next retained child pointer
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 8]");                        // compare the current fill loop index against the requested element count
    emitter.instruction("jge __rt_array_fill_ref_done_x86");                    // stop once the destination indexed array contains the requested number of retained child pointers
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the current destination indexed-array pointer before invoking the refcounted append helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the borrowed payload pointer before invoking the refcounted append helper
    emitter.instruction("call __rt_array_push_refcounted");                     // append one retained copy of the borrowed payload into the destination indexed array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // persist the possibly-grown destination indexed-array pointer after the refcounted append helper returns
    emitter.instruction("add rcx, 1");                                          // advance the fill loop index after appending one retained child pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // persist the updated fill loop index across the next append helper call
    emitter.instruction("jmp __rt_array_fill_ref_loop_x86");                    // continue appending retained payload copies until the destination reaches the requested length
    emitter.label("__rt_array_fill_ref_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the filled destination indexed-array pointer in the standard x86_64 integer result register
    emitter.instruction("add rsp, 32");                                         // release the array-fill spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the filled destination indexed-array pointer in rax
}
