use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// str_repeat: repeat a string N times into concat_buf.
/// Input: x1=ptr, x2=len, x3=times
/// Output: x1=result_ptr, x2=result_len
pub fn emit_str_repeat(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_repeat_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_repeat ---");
    emitter.label_global("__rt_str_repeat");

    // -- set up stack frame (48 bytes) --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save source pointer and length
    emitter.instruction("str x3, [sp, #16]");                                   // save repetition count

    // -- get concat_buf write position --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // compute destination pointer
    emitter.instruction("str x9, [sp, #24]");                                   // save result start pointer

    // -- outer loop: repeat N times --
    emitter.instruction("mov x10, x3");                                         // initialize repetition counter
    emitter.label("__rt_str_repeat_loop");
    emitter.instruction("cbz x10, __rt_str_repeat_done");                       // if counter is 0, done repeating
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload source pointer and length
    emitter.instruction("mov x11, x2");                                         // copy length as inner loop counter

    // -- inner loop: copy one instance of the string --
    emitter.label("__rt_str_repeat_copy");
    emitter.instruction("cbz x11, __rt_str_repeat_next");                       // if no bytes remain, move to next repetition
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte from source, advance src ptr
    emitter.instruction("strb w12, [x9], #1");                                  // store byte to dest, advance dest ptr
    emitter.instruction("sub x11, x11, #1");                                    // decrement inner byte counter
    emitter.instruction("b __rt_str_repeat_copy");                              // continue copying bytes
    emitter.label("__rt_str_repeat_next");
    emitter.instruction("sub x10, x10, #1");                                    // decrement repetition counter
    emitter.instruction("b __rt_str_repeat_loop");                              // continue to next repetition

    // -- finalize: compute result length and update concat_off --
    emitter.label("__rt_str_repeat_done");
    emitter.instruction("ldr x1, [sp, #24]");                                   // load result start pointer
    emitter.instruction("sub x2, x9, x1");                                      // result length = dest_end - dest_start
    emitter.instruction("ldr x8, [x6]");                                        // reload current concat_off
    emitter.instruction("add x8, x8, x2");                                      // advance offset by result length
    emitter.instruction("str x8, [x6]");                                        // store updated concat_off

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_str_repeat_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_repeat ---");
    emitter.label_global("__rt_str_repeat");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving the repeat-helper spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved source string, repeat count, and concat-buffer cursors
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for the source string, repeat count, result start, and destination cursor
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the source string pointer across the nested copy loops that reuse caller-saved registers
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the source string length across the nested copy loops that reuse caller-saved registers
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // preserve the requested repetition count before the outer loop decrements it

    // -- get concat_buf write position --
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer write offset before materializing the repeated output
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r11, [r10 + r9]");                                 // compute the concat-buffer destination pointer where the repeated output starts
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // preserve the repeated-string start pointer for the final string result
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // preserve the current concat-buffer destination cursor across each repeated copy

    // -- outer loop: repeat N times --
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // seed the outer repetition counter from the saved repeat-count argument
    emitter.label("__rt_str_repeat_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // stop once every requested repetition has been copied into concat storage
    emitter.instruction("jz __rt_str_repeat_done_linux_x86_64");                // jump to finalization when the repeat counter reaches zero
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the source string pointer before copying the next repeated instance
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the source string length before copying the next repeated instance
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the current concat-buffer destination cursor before copying one repeated instance

    // -- inner loop: copy one instance of the string --
    emitter.label("__rt_str_repeat_copy_linux_x86_64");
    emitter.instruction("test rsi, rsi");                                       // stop copying the current source instance once every source byte has been consumed
    emitter.instruction("jz __rt_str_repeat_next_linux_x86_64");                // continue with the next repetition after the full source string has been copied
    emitter.instruction("mov dl, BYTE PTR [r8]");                               // load one source byte before appending it to the concat-buffer destination cursor
    emitter.instruction("mov BYTE PTR [r11], dl");                              // append one source byte into concat storage before advancing both cursors
    emitter.instruction("add r8, 1");                                           // advance the source string pointer after copying one byte from the current repetition
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after storing one byte from the current repetition
    emitter.instruction("sub rsi, 1");                                          // decrement the remaining source byte count for the current repetition
    emitter.instruction("jmp __rt_str_repeat_copy_linux_x86_64");               // continue copying bytes from the current source string instance until it is exhausted

    emitter.label("__rt_str_repeat_next_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // preserve the concat-buffer destination cursor after finishing the current repetition
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining repetition count after copying one full source instance
    emitter.instruction("jmp __rt_str_repeat_loop_linux_x86_64");               // continue with the next repetition until the requested count is exhausted

    // -- finalize: compute result length and update concat_off --
    emitter.label("__rt_str_repeat_done_linux_x86_64");
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the final concat-buffer destination cursor to compute the repeated string length
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the repeated-string start pointer before computing the produced byte length
    emitter.instruction("mov rdx, r11");                                        // copy the final destination cursor before subtracting the repeated-string start pointer
    emitter.instruction("sub rdx, rax");                                        // compute the repeated string length as dest_end - dest_start so zero or negative counts yield length zero
    emitter.instruction("mov r8, QWORD PTR [rip + _concat_off]");               // reload the concat-buffer write offset before publishing the bytes produced by str_repeat()
    emitter.instruction("add r8, rdx");                                         // advance the concat-buffer write offset by the repeated string length that was just materialized
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r8");               // persist the updated concat-buffer write offset after producing the repeated string
    emitter.instruction("add rsp, 48");                                         // release the repeat-helper spill slots before returning the concat-backed string
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the concat-backed repeated string
    emitter.instruction("ret");                                                 // return the concat-backed repeated string in the standard x86_64 string result registers
}
