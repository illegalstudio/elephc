use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_merge_into: append all elements from source array to dest array (in-place).
/// Input: x0 = dest array pointer, x1 = source array pointer
/// Both arrays must have 8-byte elements.
pub fn emit_array_merge_into(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_merge_into_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_merge_into ---");
    emitter.label_global("__rt_array_merge_into");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save dest array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save source array pointer

    // -- check if source is empty --
    emitter.instruction("ldr x9, [x1]");                                        // x9 = source array length
    emitter.instruction("cbz x9, __rt_ami_done");                               // if source is empty, nothing to do

    // -- ensure dest has enough capacity --
    emitter.instruction("ldr x10, [x0]");                                       // x10 = dest array length
    emitter.instruction("ldr x11, [x0, #8]");                                   // x11 = dest array capacity
    emitter.instruction("add x12, x10, x9");                                    // x12 = needed capacity (dest_len + src_len)
    emitter.label("__rt_ami_grow_check");
    emitter.instruction("cmp x12, x11");                                        // check if we need to grow
    emitter.instruction("b.le __rt_ami_copy");                                  // skip resize if capacity is enough
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload current dest array pointer before growth
    emitter.instruction("bl __rt_array_grow");                                  // grow dest array storage until it can hold the merge result
    emitter.instruction("str x0, [sp, #0]");                                    // persist the possibly-moved dest array pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload source array pointer after growth clobbers scratch regs
    emitter.instruction("ldr x9, [x1]");                                        // reload source length after growth clobbers scratch regs
    emitter.instruction("ldr x10, [x0]");                                       // reload dest length after growth clobbers scratch regs
    emitter.instruction("ldr x11, [x0, #8]");                                   // reload dest capacity after growth
    emitter.instruction("add x12, x10, x9");                                    // recompute needed capacity after growth clobbers scratch regs
    emitter.instruction("b __rt_ami_grow_check");                               // keep growing until the required capacity fits

    emitter.label("__rt_ami_copy");
    // -- copy elements from source to dest --
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = dest array pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = source array pointer
    emitter.instruction("ldr x9, [x1]");                                        // x9 = source length
    emitter.instruction("ldr x10, [x0]");                                       // x10 = dest current length
    emitter.instruction("add x2, x1, #24");                                     // x2 = source data base
    emitter.instruction("add x3, x0, #24");                                     // x3 = dest data base
    emitter.instruction("mov x4, #0");                                          // x4 = loop index i = 0

    emitter.label("__rt_ami_loop");
    emitter.instruction("cmp x4, x9");                                          // compare i with source length
    emitter.instruction("b.ge __rt_ami_set_len");                               // if done, set new length
    emitter.instruction("ldr x5, [x2, x4, lsl #3]");                            // x5 = source[i]
    emitter.instruction("add x6, x10, x4");                                     // x6 = dest_len + i (target index)
    emitter.instruction("str x5, [x3, x6, lsl #3]");                            // dest[dest_len + i] = source[i]
    emitter.instruction("add x4, x4, #1");                                      // i += 1
    emitter.instruction("b __rt_ami_loop");                                     // continue loop

    emitter.label("__rt_ami_set_len");
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = dest array pointer
    emitter.instruction("ldr x10, [x0]");                                       // x10 = dest old length
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = source pointer
    emitter.instruction("ldr x9, [x1]");                                        // x9 = source length
    emitter.instruction("add x10, x10, x9");                                    // x10 = new total length
    emitter.instruction("str x10, [x0]");                                       // update dest length

    emitter.label("__rt_ami_done");
    // -- tear down stack frame --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_array_merge_into_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_merge_into ---");
    emitter.label_global("__rt_array_merge_into");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving merge spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the destination and source indexed-array pointers
    emitter.instruction("sub rsp, 16");                                         // reserve aligned spill slots for the destination and source indexed-array pointers
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the destination indexed-array pointer across growth helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the source indexed-array pointer across growth helper calls
    emitter.instruction("mov r8, QWORD PTR [rsi]");                             // load the source indexed-array length so empty spreads can return immediately
    emitter.instruction("test r8, r8");                                         // check whether the source indexed array contributes any elements to the merge
    emitter.instruction("jz __rt_ami_done_x86");                                // skip the merge work entirely when the source indexed array is empty
    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // load the destination indexed-array length before checking whether growth is required
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the destination indexed-array capacity before checking whether growth is required
    emitter.instruction("lea r11, [r9 + r8]");                                  // compute the total element count that must fit after merging the source indexed array
    emitter.label("__rt_ami_grow_check_x86");
    emitter.instruction("cmp r11, r10");                                        // compare the required merged element count against the current destination capacity
    emitter.instruction("jle __rt_ami_copy_x86");                               // skip growth once the destination indexed array has enough spare capacity
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the current destination indexed-array pointer before invoking the growth helper
    emitter.instruction("call __rt_array_grow");                                // grow the destination indexed-array backing storage until it can hold the merged payload
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // persist the possibly-moved destination indexed-array pointer after the growth helper returns
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the source indexed-array pointer after the growth helper clobbered caller-saved registers
    emitter.instruction("mov r8, QWORD PTR [rsi]");                             // reload the source indexed-array length after the growth helper clobbered caller-saved registers
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // reload the destination indexed-array length after the growth helper clobbered caller-saved registers
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // reload the destination indexed-array capacity after the growth helper clobbered caller-saved registers
    emitter.instruction("lea r11, [r9 + r8]");                                  // recompute the required merged element count after the growth helper clobbered caller-saved registers
    emitter.instruction("jmp __rt_ami_grow_check_x86");                         // keep growing until the destination indexed array has enough spare capacity
    emitter.label("__rt_ami_copy_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the destination indexed-array pointer before copying payload slots from the source
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the source indexed-array pointer before copying payload slots from the source
    emitter.instruction("mov r8, QWORD PTR [rsi]");                             // reload the source indexed-array length for the payload copy loop bound
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // reload the destination indexed-array length so merged elements land after the live prefix
    emitter.instruction("lea r10, [rsi + 24]");                                 // compute the source indexed-array payload base address
    emitter.instruction("lea r11, [rax + 24]");                                 // compute the destination indexed-array payload base address
    emitter.instruction("xor rcx, rcx");                                        // initialize the source loop index to the first payload slot
    emitter.label("__rt_ami_loop_x86");
    emitter.instruction("cmp rcx, r8");                                         // compare the current source loop index against the source indexed-array length
    emitter.instruction("jge __rt_ami_set_len_x86");                            // finish once every source payload slot has been copied into the destination
    emitter.instruction("mov rdx, QWORD PTR [r10 + rcx * 8]");                  // load the current source payload slot from the source indexed-array data region
    emitter.instruction("lea rsi, [r9 + rcx]");                                 // compute the destination payload slot index after the live destination prefix
    emitter.instruction("mov QWORD PTR [r11 + rsi * 8], rdx");                  // store the copied payload slot into the destination indexed-array data region
    emitter.instruction("add rcx, 1");                                          // advance the source loop index after copying one payload slot
    emitter.instruction("jmp __rt_ami_loop_x86");                               // continue copying source payload slots into the destination indexed array
    emitter.label("__rt_ami_set_len_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the destination indexed-array pointer before publishing the merged logical length
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the source indexed-array pointer before reading the source logical length
    emitter.instruction("mov r8, QWORD PTR [rsi]");                             // reload the source indexed-array length before publishing the merged logical length
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // reload the previous destination indexed-array length before extending it by the source length
    emitter.instruction("add r9, r8");                                          // compute the merged logical length for the destination indexed array
    emitter.instruction("mov QWORD PTR [rax], r9");                             // publish the merged logical length in the destination indexed-array header
    emitter.label("__rt_ami_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the destination indexed-array pointer even when the source indexed array was empty
    emitter.instruction("add rsp, 16");                                         // release the destination and source indexed-array spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the destination indexed-array pointer in rax
}
