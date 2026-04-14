use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// range: create an integer array from start to end (inclusive).
/// Input: x0 = start, x1 = end
/// Output: x0 = pointer to new array containing values from start to end
/// Supports both ascending (start <= end) and descending (start > end) ranges.
pub fn emit_range(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_range_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: range ---");
    emitter.label_global("__rt_range");

    // -- set up stack frame, save arguments --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save start value
    emitter.instruction("str x1, [sp, #8]");                                    // save end value

    // -- determine direction and calculate count --
    emitter.instruction("cmp x0, x1");                                          // compare start with end
    emitter.instruction("b.gt __rt_range_descending");                          // if start > end, use descending path

    // -- ascending: count = end - start + 1 --
    emitter.instruction("sub x2, x1, x0");                                      // x2 = end - start
    emitter.instruction("add x2, x2, #1");                                      // x2 = count = end - start + 1
    emitter.instruction("mov x7, #1");                                          // x7 = step = +1 (ascending)
    emitter.instruction("b __rt_range_alloc");                                  // jump to allocation

    // -- descending: count = start - end + 1 --
    emitter.label("__rt_range_descending");
    emitter.instruction("sub x2, x0, x1");                                      // x2 = start - end
    emitter.instruction("add x2, x2, #1");                                      // x2 = count = start - end + 1
    emitter.instruction("mov x7, #-1");                                         // x7 = step = -1 (descending)

    // -- allocate array --
    emitter.label("__rt_range_alloc");
    emitter.instruction("str x2, [sp, #16]");                                   // save count
    emitter.instruction("str x7, [sp, #8]");                                    // save step (reuse end slot, no longer needed)
    emitter.instruction("mov x0, x2");                                          // x0 = capacity = count
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size = 8 (integers)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array
    emitter.instruction("str x0, [sp, #24]");                                   // save new array pointer

    // -- fill array with values from start, stepping by +1 or -1 --
    emitter.instruction("add x3, x0, #24");                                     // x3 = data base of new array
    emitter.instruction("ldr x4, [sp, #0]");                                    // x4 = current value = start
    emitter.instruction("ldr x5, [sp, #16]");                                   // x5 = count
    emitter.instruction("ldr x7, [sp, #8]");                                    // x7 = step (+1 or -1)
    emitter.instruction("mov x6, #0");                                          // x6 = i = 0

    emitter.label("__rt_range_loop");
    emitter.instruction("cmp x6, x5");                                          // compare i with count
    emitter.instruction("b.ge __rt_range_done");                                // if i >= count, filling complete
    emitter.instruction("str x4, [x3, x6, lsl #3]");                            // data[i] = current value
    emitter.instruction("add x4, x4, x7");                                      // current value += step (+1 or -1)
    emitter.instruction("add x6, x6, #1");                                      // i += 1
    emitter.instruction("b __rt_range_loop");                                   // continue loop

    // -- set length and return --
    emitter.label("__rt_range_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // x0 = new array pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // x9 = count
    emitter.instruction("str x9, [x0]");                                        // set array length = count

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = array [start..end]
}

fn emit_range_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: range ---");
    emitter.label_global("__rt_range");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving range-construction spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for start, end, count, step, and destination array bookkeeping
    emitter.instruction("sub rsp, 40");                                         // reserve aligned spill slots for range-construction bookkeeping while keeping nested calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the inclusive range start value across count calculation and destination-array allocation
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the inclusive range end value across count calculation and destination-array allocation
    emitter.instruction("cmp rdi, rsi");                                        // compare the inclusive range start and end values to choose the traversal direction
    emitter.instruction("jg __rt_range_descending_x86");                        // switch to the descending range path when the start value is greater than the end value
    emitter.instruction("mov rax, rsi");                                        // copy the inclusive range end value before subtracting the start value to derive the element count
    emitter.instruction("sub rax, rdi");                                        // compute end - start for the ascending integer range
    emitter.instruction("add rax, 1");                                          // convert the inclusive ascending difference into the final element count
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the computed ascending element count across destination-array allocation
    emitter.instruction("mov QWORD PTR [rbp - 32], 1");                         // preserve the ascending traversal step so the fill loop can advance by +1
    emitter.instruction("jmp __rt_range_alloc_x86");                            // jump to the shared destination-array allocation path after preparing the ascending count and step
    emitter.label("__rt_range_descending_x86");
    emitter.instruction("mov rax, rdi");                                        // copy the inclusive range start value before subtracting the end value to derive the element count
    emitter.instruction("sub rax, rsi");                                        // compute start - end for the descending integer range
    emitter.instruction("add rax, 1");                                          // convert the inclusive descending difference into the final element count
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the computed descending element count across destination-array allocation
    emitter.instruction("mov QWORD PTR [rbp - 32], -1");                        // preserve the descending traversal step so the fill loop can advance by -1
    emitter.label("__rt_range_alloc_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the final integer range length as the destination indexed-array capacity to the constructor
    emitter.instruction("mov rsi, 8");                                          // use 8-byte payload slots because the range helper produces an indexed array of integers
    emitter.instruction("call __rt_array_new");                                 // allocate the destination integer range array through the shared x86_64 indexed-array constructor
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the destination integer range array pointer while the fill loop writes payload slots
    emitter.instruction("lea r8, [rax + 24]");                                  // compute the destination integer range payload base address once before entering the fill loop
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the current integer value from the inclusive range start before entering the fill loop
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the final integer range element count before entering the fill loop
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the traversal step before entering the fill loop
    emitter.instruction("xor rcx, rcx");                                        // initialize the range fill loop index to the first destination payload slot
    emitter.label("__rt_range_loop_x86");
    emitter.instruction("cmp rcx, r10");                                        // compare the current range fill loop index against the final element count
    emitter.instruction("jge __rt_range_done_x86");                             // stop once every destination integer payload slot has been initialized
    emitter.instruction("mov QWORD PTR [r8 + rcx * 8], r9");                    // store the current integer value into the selected destination range payload slot
    emitter.instruction("add r9, r11");                                         // advance the current integer value by the preserved traversal step for the next payload slot
    emitter.instruction("add rcx, 1");                                          // advance the range fill loop index after initializing one destination payload slot
    emitter.instruction("jmp __rt_range_loop_x86");                             // continue filling integer range payload slots until the inclusive interval is exhausted
    emitter.label("__rt_range_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the destination integer range array pointer before publishing the final logical length
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the computed integer range element count before publishing the final logical length
    emitter.instruction("mov QWORD PTR [rax], r10");                            // publish the final logical length in the destination integer range array header
    emitter.instruction("add rsp, 40");                                         // release the range-construction spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the constructed integer range array pointer in rax
}
