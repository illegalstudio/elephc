use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_pad: pad an integer array to a specified size with a value.
/// Input: x0 = array pointer, x1 = size (negative = pad left), x2 = pad value
/// Output: x0 = pointer to new padded array
/// If abs(size) <= current length, returns a copy of the original array.
pub fn emit_array_pad(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_pad_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_pad ---");
    emitter.label_global("__rt_array_pad");

    // -- set up stack frame, save arguments --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save size argument
    emitter.instruction("str x2, [sp, #16]");                                   // save pad value
    emitter.instruction("ldr x9, [x0]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #24]");                                   // save source length

    // -- determine absolute size and pad direction --
    emitter.instruction("cmp x1, #0");                                          // check if size is negative
    emitter.instruction("b.ge __rt_array_pad_positive");                        // if non-negative, pad right
    emitter.instruction("neg x3, x1");                                          // x3 = abs(size) for negative case
    emitter.instruction("mov x4, #1");                                          // x4 = 1 (flag: pad left)
    emitter.instruction("b __rt_array_pad_check");                              // continue to size check

    emitter.label("__rt_array_pad_positive");
    emitter.instruction("mov x3, x1");                                          // x3 = abs(size) = size (already positive)
    emitter.instruction("mov x4, #0");                                          // x4 = 0 (flag: pad right)

    // -- check if padding is needed --
    emitter.label("__rt_array_pad_check");
    emitter.instruction("cmp x3, x9");                                          // compare abs(size) with current length
    emitter.instruction("b.le __rt_array_pad_copy");                            // if abs(size) <= length, just copy
    emitter.instruction("str x3, [sp, #32]");                                   // save abs(size) = new array size
    emitter.instruction("str x4, [sp, #40]");                                   // save pad direction flag

    // -- create new array with capacity = abs(size) --
    emitter.instruction("mov x0, x3");                                          // x0 = capacity = abs(size)
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size = 8 (integers)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array
    emitter.instruction("str x0, [sp, #32]");                                   // reuse slot to save new array ptr temporarily

    // -- determine pad count and data offset --
    emitter.instruction("ldr x9, [sp, #24]");                                   // x9 = source length
    emitter.instruction("ldr x4, [sp, #40]");                                   // x4 = pad direction (0=right, 1=left)
    emitter.instruction("ldr x3, [sp, #8]");                                    // x3 = original size argument
    emitter.instruction("cmp x3, #0");                                          // recheck sign for abs
    emitter.instruction("b.ge __rt_array_pad_calc_right");                      // positive = pad right
    emitter.instruction("neg x3, x3");                                          // x3 = abs(size)

    // -- pad left: fill pad values first, then copy source --
    emitter.instruction("sub x5, x3, x9");                                      // x5 = pad_count = abs(size) - length
    emitter.instruction("add x10, x0, #24");                                    // x10 = dest data base
    emitter.instruction("ldr x11, [sp, #16]");                                  // x11 = pad value
    emitter.instruction("mov x6, #0");                                          // x6 = i = 0

    emitter.label("__rt_array_pad_fill_left");
    emitter.instruction("cmp x6, x5");                                          // compare i with pad_count
    emitter.instruction("b.ge __rt_array_pad_copy_left");                       // if done padding, copy source data
    emitter.instruction("str x11, [x10, x6, lsl #3]");                          // dest[i] = pad value
    emitter.instruction("add x6, x6, #1");                                      // i += 1
    emitter.instruction("b __rt_array_pad_fill_left");                          // continue loop

    // -- copy source elements after pad values --
    emitter.label("__rt_array_pad_copy_left");
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = source array pointer
    emitter.instruction("add x2, x1, #24");                                     // x2 = source data base
    emitter.instruction("mov x7, #0");                                          // x7 = j = 0

    emitter.label("__rt_array_pad_copy_left_loop");
    emitter.instruction("cmp x7, x9");                                          // compare j with source length
    emitter.instruction("b.ge __rt_array_pad_finish");                          // if done, finish up
    emitter.instruction("ldr x8, [x2, x7, lsl #3]");                            // x8 = source[j]
    emitter.instruction("add x12, x5, x7");                                     // x12 = pad_count + j (dest index)
    emitter.instruction("str x8, [x10, x12, lsl #3]");                          // dest[pad_count + j] = source[j]
    emitter.instruction("add x7, x7, #1");                                      // j += 1
    emitter.instruction("b __rt_array_pad_copy_left_loop");                     // continue loop

    // -- pad right: copy source first, then fill pad values --
    emitter.label("__rt_array_pad_calc_right");
    emitter.instruction("sub x5, x3, x9");                                      // x5 = pad_count = size - length
    emitter.instruction("add x10, x0, #24");                                    // x10 = dest data base
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = source array pointer
    emitter.instruction("add x2, x1, #24");                                     // x2 = source data base
    emitter.instruction("mov x6, #0");                                          // x6 = i = 0

    emitter.label("__rt_array_pad_copy_right");
    emitter.instruction("cmp x6, x9");                                          // compare i with source length
    emitter.instruction("b.ge __rt_array_pad_fill_right_setup");                // if done copying, start padding
    emitter.instruction("ldr x8, [x2, x6, lsl #3]");                            // x8 = source[i]
    emitter.instruction("str x8, [x10, x6, lsl #3]");                           // dest[i] = source[i]
    emitter.instruction("add x6, x6, #1");                                      // i += 1
    emitter.instruction("b __rt_array_pad_copy_right");                         // continue loop

    emitter.label("__rt_array_pad_fill_right_setup");
    emitter.instruction("ldr x11, [sp, #16]");                                  // x11 = pad value
    emitter.instruction("mov x7, #0");                                          // x7 = j = 0

    emitter.label("__rt_array_pad_fill_right");
    emitter.instruction("cmp x7, x5");                                          // compare j with pad_count
    emitter.instruction("b.ge __rt_array_pad_finish");                          // if done padding, finish up
    emitter.instruction("add x12, x9, x7");                                     // x12 = length + j (dest index after source)
    emitter.instruction("str x11, [x10, x12, lsl #3]");                         // dest[length + j] = pad value
    emitter.instruction("add x7, x7, #1");                                      // j += 1
    emitter.instruction("b __rt_array_pad_fill_right");                         // continue loop

    // -- set total length and return --
    emitter.label("__rt_array_pad_finish");
    emitter.instruction("ldr x0, [sp, #32]");                                   // x0 = new array pointer
    emitter.instruction("ldr x3, [sp, #8]");                                    // x3 = original size argument
    emitter.instruction("cmp x3, #0");                                          // check sign
    emitter.instruction("cneg x3, x3, lt");                                     // x3 = abs(size)
    emitter.instruction("str x3, [x0]");                                        // set array length = abs(size)
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = padded array

    // -- no padding needed: just create a copy --
    emitter.label("__rt_array_pad_copy");
    emitter.instruction("mov x0, x9");                                          // x0 = capacity = source length
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size = 8
    emitter.instruction("bl __rt_array_new");                                   // allocate new array
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = source array pointer
    emitter.instruction("ldr x9, [sp, #24]");                                   // x9 = source length
    emitter.instruction("add x2, x1, #24");                                     // x2 = source data base
    emitter.instruction("add x3, x0, #24");                                     // x3 = dest data base
    emitter.instruction("mov x4, #0");                                          // x4 = i = 0

    emitter.label("__rt_array_pad_copy_loop");
    emitter.instruction("cmp x4, x9");                                          // compare i with source length
    emitter.instruction("b.ge __rt_array_pad_copy_done");                       // if done, finish
    emitter.instruction("ldr x5, [x2, x4, lsl #3]");                            // x5 = source[i]
    emitter.instruction("str x5, [x3, x4, lsl #3]");                            // dest[i] = source[i]
    emitter.instruction("add x4, x4, #1");                                      // i += 1
    emitter.instruction("b __rt_array_pad_copy_loop");                          // continue loop

    emitter.label("__rt_array_pad_copy_done");
    emitter.instruction("str x9, [x0]");                                        // set array length = source length
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = copied array
}

fn emit_array_pad_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_pad ---");
    emitter.label_global("__rt_array_pad");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving scalar pad spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source indexed-array pointer, size argument, pad value, and result pointer
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for the scalar pad bookkeeping while keeping constructor calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across target-length normalization and result-array construction
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the signed pad-size argument across result-array construction
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the scalar pad value across result-array construction and copy loops
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the source indexed-array logical length before deriving the target padded length
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // preserve the source indexed-array logical length across constructor calls and copy loops
    emitter.instruction("mov r11, rsi");                                        // seed the absolute target-length scratch register from the signed pad-size argument
    emitter.instruction("cmp r11, 0");                                          // detect whether the caller requested left padding by passing a negative target size
    emitter.instruction("jge __rt_array_pad_abs_ready_x86");                    // skip the absolute-value conversion when the requested target size is already non-negative
    emitter.instruction("neg r11");                                             // convert the negative target size into its absolute padded length

    emitter.label("__rt_array_pad_abs_ready_x86");
    emitter.instruction("cmp r11, r10");                                        // compare the absolute requested target length against the current source indexed-array length
    emitter.instruction("jg __rt_array_pad_need_padding_x86");                  // continue into the padding path only when the requested target length exceeds the source length
    emitter.instruction("mov r11, r10");                                        // clamp the target padded length to the current source length when no padding is needed

    emitter.label("__rt_array_pad_need_padding_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // preserve the final target padded length across the result-array constructor call
    emitter.instruction("mov rdi, r11");                                        // pass the final target padded length as the destination indexed-array capacity to the shared constructor
    emitter.instruction("mov rsi, 8");                                          // request 8-byte scalar payload slots for the padded indexed array
    emitter.instruction("call __rt_array_new");                                 // allocate the padded indexed array through the shared x86_64 constructor
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // preserve the padded indexed-array pointer across the scalar copy and fill loops
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the source indexed-array pointer after the constructor clobbered caller-saved registers
    emitter.instruction("lea r8, [r8 + 24]");                                   // compute the first scalar payload slot address in the source indexed array
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the padded indexed-array pointer before seeding the copy and fill loops
    emitter.instruction("lea r9, [r9 + 24]");                                   // compute the first scalar payload slot address in the padded indexed array
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the source indexed-array logical length before deriving the pad count
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the final target padded length before deriving the pad count
    emitter.instruction("mov rcx, r11");                                        // seed the pad-count scratch register from the final target padded length
    emitter.instruction("sub rcx, r10");                                        // compute how many scalar pad values must be inserted to reach the target padded length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the signed pad-size argument before branching between left-padding and right-padding layouts
    emitter.instruction("cmp rdx, 0");                                          // detect whether the requested target size was negative, which means pad on the left
    emitter.instruction("jl __rt_array_pad_left_x86");                          // branch to the left-padding layout when the caller requested a negative target size

    emitter.instruction("xor rdx, rdx");                                        // initialize the source-copy cursor for the right-padding layout at the front of the source indexed array
    emitter.label("__rt_array_pad_copy_right_x86");
    emitter.instruction("cmp rdx, r10");                                        // compare the right-padding source-copy cursor against the source indexed-array logical length
    emitter.instruction("jge __rt_array_pad_fill_right_x86");                   // start appending pad values after every source scalar payload has been copied to the front
    emitter.instruction("mov rax, QWORD PTR [r8 + rdx * 8]");                   // load the current scalar payload from the source indexed array
    emitter.instruction("mov QWORD PTR [r9 + rdx * 8], rax");                   // store that scalar payload into the matching front slot of the padded indexed array
    emitter.instruction("add rdx, 1");                                          // advance the right-padding source-copy cursor after copying one scalar payload
    emitter.instruction("jmp __rt_array_pad_copy_right_x86");                   // continue copying the source prefix into the padded indexed array

    emitter.label("__rt_array_pad_fill_right_x86");
    emitter.instruction("xor rdx, rdx");                                        // initialize the right-padding fill cursor at the first trailing pad slot
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the scalar pad value before appending it into the padded indexed array
    emitter.label("__rt_array_pad_fill_right_loop_x86");
    emitter.instruction("cmp rdx, rcx");                                        // compare the trailing pad-fill cursor against the computed pad count
    emitter.instruction("jge __rt_array_pad_finish_x86");                       // finish once every trailing scalar pad slot has been written
    emitter.instruction("mov rax, r10");                                        // seed the trailing pad destination index from the source indexed-array logical length
    emitter.instruction("add rax, rdx");                                        // offset the trailing pad destination index by the current pad-fill cursor
    emitter.instruction("mov QWORD PTR [r9 + rax * 8], r11");                   // store the scalar pad value into the current trailing padded slot
    emitter.instruction("add rdx, 1");                                          // advance the trailing pad-fill cursor after writing one scalar pad value
    emitter.instruction("jmp __rt_array_pad_fill_right_loop_x86");              // continue appending trailing scalar pad values until the target length is reached

    emitter.label("__rt_array_pad_left_x86");
    emitter.instruction("xor rdx, rdx");                                        // initialize the left-padding fill cursor at the front of the padded indexed array
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the scalar pad value before filling the leading pad prefix
    emitter.label("__rt_array_pad_fill_left_loop_x86");
    emitter.instruction("cmp rdx, rcx");                                        // compare the leading pad-fill cursor against the computed pad count
    emitter.instruction("jge __rt_array_pad_copy_left_x86");                    // start copying the source suffix after every leading pad slot has been written
    emitter.instruction("mov QWORD PTR [r9 + rdx * 8], r11");                   // store the scalar pad value into the current leading padded slot
    emitter.instruction("add rdx, 1");                                          // advance the leading pad-fill cursor after writing one scalar pad value
    emitter.instruction("jmp __rt_array_pad_fill_left_loop_x86");               // continue filling the leading pad prefix until the target length is reached

    emitter.label("__rt_array_pad_copy_left_x86");
    emitter.instruction("xor rdx, rdx");                                        // initialize the left-padding source-copy cursor at the front of the source indexed array
    emitter.label("__rt_array_pad_copy_left_loop_x86");
    emitter.instruction("cmp rdx, r10");                                        // compare the left-padding source-copy cursor against the source indexed-array logical length
    emitter.instruction("jge __rt_array_pad_finish_x86");                       // finish once every source scalar payload has been copied after the leading pad prefix
    emitter.instruction("mov rax, QWORD PTR [r8 + rdx * 8]");                   // load the current scalar payload from the source indexed array
    emitter.instruction("mov r11, rcx");                                        // seed the left-padding destination index from the leading pad-count prefix
    emitter.instruction("add r11, rdx");                                        // offset the left-padding destination index by the current source-copy cursor
    emitter.instruction("mov QWORD PTR [r9 + r11 * 8], rax");                   // store the source scalar payload after the leading padded prefix in the padded indexed array
    emitter.instruction("add rdx, 1");                                          // advance the left-padding source-copy cursor after copying one scalar payload
    emitter.instruction("jmp __rt_array_pad_copy_left_loop_x86");               // continue copying the source scalar payloads after the leading pad prefix

    emitter.label("__rt_array_pad_finish_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the padded indexed-array pointer before publishing its final logical length
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the final target padded length before storing it into the array header
    emitter.instruction("mov QWORD PTR [rax], r11");                            // store the final target padded length as the logical length of the padded indexed array
    emitter.instruction("add rsp, 48");                                         // release the scalar pad spill slots before returning to the caller
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the scalar pad helper completes
    emitter.instruction("ret");                                                 // return the padded indexed-array pointer in rax
}
