use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_merge_into_refcounted: append all elements from source array to dest array (in-place).
/// Input: x0 = dest array pointer, x1 = source array pointer
/// Both arrays must contain 8-byte refcounted payloads (array/hash/object pointers).
pub fn emit_array_merge_into_refcounted(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_merge_into_refcounted_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_merge_into_refcounted ---");
    emitter.label_global("__rt_array_merge_into_refcounted");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save dest array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save source array pointer
    emitter.instruction("str xzr, [sp, #16]");                                  // initialize loop index to 0

    // -- check if source is empty --
    emitter.instruction("ldr x9, [x1]");                                        // load source array length
    emitter.instruction("cbz x9, __rt_amir_done");                              // return early when source is empty
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the destination packed array kind word
    emitter.instruction("ldr x11, [x1, #-8]");                                  // load the source packed array kind word
    emitter.instruction("and x10, x10, #0xff");                                 // keep only the destination low-byte heap kind
    emitter.instruction("and x11, x11, #0x7f00");                               // keep only the source packed array value_type lane without the persistent COW flag
    emitter.instruction("mov x12, #0x80ff");                                    // preserve the destination indexed-array kind and persistent COW flag
    emitter.instruction("and x10, x10, x12");                                   // drop stale destination value_type bits before propagating the source tag
    emitter.instruction("orr x10, x10, x11");                                   // combine the destination heap kind/COW bits with the source value_type tag
    emitter.instruction("str x10, [x0, #-8]");                                  // persist the propagated packed array value_type tag

    // -- ensure dest has enough capacity --
    emitter.instruction("ldr x10, [x0]");                                       // load dest array length
    emitter.instruction("ldr x11, [x0, #8]");                                   // load dest array capacity
    emitter.instruction("add x12, x10, x9");                                    // compute needed total capacity
    emitter.label("__rt_amir_grow_check");
    emitter.instruction("cmp x12, x11");                                        // compare required capacity with current capacity
    emitter.instruction("b.le __rt_amir_loop");                                 // skip resize when dest already has enough room
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload current dest array pointer before growth
    emitter.instruction("bl __rt_array_grow");                                  // grow dest array storage until it can hold the merge result
    emitter.instruction("str x0, [sp, #0]");                                    // persist the possibly-moved dest array pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload source array pointer after growth clobbers scratch regs
    emitter.instruction("ldr x9, [x1]");                                        // reload source length after growth clobbers scratch regs
    emitter.instruction("ldr x10, [x0]");                                       // reload dest length after growth clobbers scratch regs
    emitter.instruction("ldr x11, [x0, #8]");                                   // reload dest capacity after growth
    emitter.instruction("add x12, x10, x9");                                    // recompute needed capacity after growth clobbers scratch regs
    emitter.instruction("b __rt_amir_grow_check");                              // keep growing until the required capacity fits

    emitter.label("__rt_amir_loop");
    emitter.instruction("ldr x4, [sp, #16]");                                   // reload loop index
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload source array pointer
    emitter.instruction("ldr x9, [x1]");                                        // reload source length
    emitter.instruction("cmp x4, x9");                                          // compare index with source length
    emitter.instruction("b.ge __rt_amir_set_len");                              // finish once every source element has been copied
    emitter.instruction("add x2, x1, #24");                                     // compute source data base address
    emitter.instruction("ldr x5, [x2, x4, lsl #3]");                            // load source element pointer
    emitter.instruction("str x5, [sp, #24]");                                   // save copied pointer across incref call
    emitter.instruction("mov x0, x5");                                          // move element pointer into incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain borrowed heap payload before destination takes ownership
    emitter.instruction("ldr x5, [sp, #24]");                                   // restore retained pointer after incref
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload dest array pointer
    emitter.instruction("ldr x10, [x0]");                                       // reload original dest length
    emitter.instruction("add x3, x0, #24");                                     // compute dest data base address
    emitter.instruction("add x6, x10, x4");                                     // compute destination index = dest_len + loop index
    emitter.instruction("str x5, [x3, x6, lsl #3]");                            // store retained pointer into destination array
    emitter.instruction("add x4, x4, #1");                                      // increment loop index
    emitter.instruction("str x4, [sp, #16]");                                   // persist updated loop index
    emitter.instruction("b __rt_amir_loop");                                    // continue copying elements

    emitter.label("__rt_amir_set_len");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload dest array pointer
    emitter.instruction("ldr x10, [x0]");                                       // load original dest length
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload source array pointer
    emitter.instruction("ldr x9, [x1]");                                        // load source length
    emitter.instruction("add x10, x10, x9");                                    // compute new total dest length
    emitter.instruction("str x10, [x0]");                                       // store updated dest length

    emitter.label("__rt_amir_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_array_merge_into_refcounted_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_merge_into_refcounted ---");
    emitter.label_global("__rt_array_merge_into_refcounted");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving refcounted-merge spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for destination, source, loop index, and child-pointer bookkeeping
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the destination, source, loop index, and retained child pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the destination indexed-array pointer across growth and incref helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the source indexed-array pointer across growth and incref helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the source loop index spill slot to the first payload slot
    emitter.instruction("mov r8, QWORD PTR [rsi]");                             // load the source indexed-array length so empty spreads can return immediately
    emitter.instruction("test r8, r8");                                         // check whether the source indexed array contributes any elements to the refcounted merge
    emitter.instruction("jz __rt_amir_done_x86");                               // skip the merge work entirely when the source indexed array is empty
    emitter.instruction("mov r8, QWORD PTR [rdi - 8]");                         // load the destination indexed-array packed kind word before propagating the source value_type metadata
    emitter.instruction("mov r9, QWORD PTR [rsi - 8]");                         // load the source indexed-array packed kind word before propagating the source value_type metadata
    emitter.instruction("mov r10, 0xffffffff000080ff");                         // materialize the x86_64 indexed-array preservation mask that keeps the heap marker, kind byte, and persistent COW bit
    emitter.instruction("and r8, r10");                                         // preserve only the destination heap marker, indexed-array kind byte, and persistent COW bit
    emitter.instruction("and r9, 0x7f00");                                      // preserve only the source indexed-array value_type lane without copying the persistent COW bit
    emitter.instruction("or r8, r9");                                           // combine the destination heap marker and kind bits with the propagated source value_type lane
    emitter.instruction("mov QWORD PTR [rdi - 8], r8");                         // persist the propagated value_type metadata in the destination indexed-array header
    emitter.instruction("mov r11, QWORD PTR [rsi]");                            // load the source indexed-array length before checking whether growth is required
    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // load the destination indexed-array length before checking whether growth is required
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the destination indexed-array capacity before checking whether growth is required
    emitter.instruction("add r11, r9");                                         // compute the total element count that must fit after merging the refcounted source indexed array
    emitter.label("__rt_amir_grow_check_x86");
    emitter.instruction("cmp r11, r10");                                        // compare the required merged element count against the current destination capacity
    emitter.instruction("jle __rt_amir_loop_x86");                              // skip growth once the destination indexed array has enough spare capacity
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the current destination indexed-array pointer before invoking the growth helper
    emitter.instruction("call __rt_array_grow");                                // grow the destination indexed-array backing storage until it can hold the merged payload
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // persist the possibly-moved destination indexed-array pointer after the growth helper returns
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the source indexed-array pointer after the growth helper clobbered caller-saved registers
    emitter.instruction("mov r8, QWORD PTR [rsi]");                             // reload the source indexed-array length after the growth helper clobbered caller-saved registers
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // reload the destination indexed-array length after the growth helper clobbered caller-saved registers
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // reload the destination indexed-array capacity after the growth helper clobbered caller-saved registers
    emitter.instruction("lea r11, [r9 + r8]");                                  // recompute the required merged element count after the growth helper clobbered caller-saved registers
    emitter.instruction("jmp __rt_amir_grow_check_x86");                        // keep growing until the destination indexed array has enough spare capacity
    emitter.label("__rt_amir_loop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the source loop index before copying the next retained child pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the source indexed-array pointer before reading the next child pointer
    emitter.instruction("mov r8, QWORD PTR [rsi]");                             // reload the source indexed-array length for the refcounted merge loop bound
    emitter.instruction("cmp rcx, r8");                                         // compare the current source loop index against the source indexed-array length
    emitter.instruction("jge __rt_amir_set_len_x86");                           // finish once every source child pointer has been copied into the destination
    emitter.instruction("lea r9, [rsi + 24]");                                  // compute the source indexed-array payload base address before loading the next child pointer
    emitter.instruction("mov rdx, QWORD PTR [r9 + rcx * 8]");                   // load the borrowed child pointer from the current source payload slot
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // preserve the borrowed child pointer across the incref helper call
    emitter.instruction("mov rax, rdx");                                        // move the borrowed child pointer into the x86_64 incref input register
    emitter.instruction("call __rt_incref");                                    // retain the borrowed child pointer before the destination indexed array becomes an owner
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // reload the retained child pointer after the incref helper clobbered caller-saved registers
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the destination indexed-array pointer before storing the retained child pointer
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // reload the previous destination indexed-array length so the retained child lands after the live prefix
    emitter.instruction("lea r10, [rax + 24]");                                 // compute the destination indexed-array payload base address before storing the retained child pointer
    emitter.instruction("lea r11, [r9 + rcx]");                                 // compute the destination payload slot index after the live destination prefix
    emitter.instruction("mov QWORD PTR [r10 + r11 * 8], rdx");                  // store the retained child pointer into the destination indexed-array payload region
    emitter.instruction("add rcx, 1");                                          // advance the source loop index after copying one retained child pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // persist the updated source loop index across the next helper call
    emitter.instruction("jmp __rt_amir_loop_x86");                              // continue copying retained child pointers into the destination indexed array
    emitter.label("__rt_amir_set_len_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the destination indexed-array pointer before publishing the merged logical length
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the source indexed-array pointer before reading the source logical length
    emitter.instruction("mov r8, QWORD PTR [rsi]");                             // reload the source indexed-array length before publishing the merged logical length
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // reload the previous destination indexed-array length before extending it by the source length
    emitter.instruction("add r9, r8");                                          // compute the merged logical length for the destination indexed array
    emitter.instruction("mov QWORD PTR [rax], r9");                             // publish the merged logical length in the destination indexed-array header
    emitter.label("__rt_amir_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the destination indexed-array pointer even when the source indexed array was empty
    emitter.instruction("add rsp, 32");                                         // release the refcounted-merge spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the destination indexed-array pointer in rax
}
