use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// strrev: reverse a string into concat_buf.
pub fn emit_strrev(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strrev_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: strrev ---");
    emitter.label_global("__rt_strrev");

    // -- get concat_buf write position --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // compute destination pointer
    emitter.instruction("mov x10, x9");                                         // save destination start for return value
    emitter.instruction("add x11, x1, x2");                                     // x11 = pointer to end of source string
    emitter.instruction("mov x12, x2");                                         // copy length as loop counter

    // -- copy bytes in reverse order (last-to-first) --
    emitter.label("__rt_strrev_loop");
    emitter.instruction("cbz x12, __rt_strrev_done");                           // if no bytes remain, done reversing
    emitter.instruction("sub x11, x11, #1");                                    // move source pointer backward (from end)
    emitter.instruction("ldrb w13, [x11]");                                     // load byte from current source position
    emitter.instruction("strb w13, [x9], #1");                                  // store to dest (forward order), advance dest
    emitter.instruction("sub x12, x12, #1");                                    // decrement remaining byte count
    emitter.instruction("b __rt_strrev_loop");                                  // continue reversing

    // -- update concat_off and return --
    emitter.label("__rt_strrev_done");
    emitter.instruction("add x8, x8, x2");                                      // advance offset by string length
    emitter.instruction("str x8, [x6]");                                        // store updated offset to _concat_off
    emitter.instruction("mov x1, x10");                                         // return pointer to reversed string
    // x2 unchanged
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_strrev_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strrev ---");
    emitter.label_global("__rt_strrev");
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer write offset before emitting the reversed string
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r11, [r10 + r9]");                                 // compute the concat-buffer destination pointer where the reversed string begins
    emitter.instruction("mov rcx, rdx");                                        // copy the source string length into the remaining-byte counter for the reverse-copy loop
    emitter.instruction("mov rsi, rdx");                                        // preserve the original source-string length so the final result length survives the byte-copy scratch register
    emitter.instruction("lea r8, [rax + rdx]");                                 // seed the reverse source cursor one byte past the end of the source string
    emitter.instruction("mov r10, r11");                                        // preserve the reversed-string start pointer for the final string return pair

    emitter.label("__rt_strrev_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // have all source bytes been copied in reverse order?
    emitter.instruction("jz __rt_strrev_done_linux_x86_64");                    // finish once the full source string has been reversed into concat storage
    emitter.instruction("sub r8, 1");                                           // move the reverse source cursor back to the next byte that should be copied
    emitter.instruction("mov dl, BYTE PTR [r8]");                               // load the next source byte in reverse order
    emitter.instruction("mov BYTE PTR [r11], dl");                              // store the reversed source byte into the concat-buffer destination
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination after storing one reversed byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining reverse-copy byte count after storing one byte
    emitter.instruction("jmp __rt_strrev_loop_linux_x86_64");                   // continue reversing bytes until the full source string has been emitted

    emitter.label("__rt_strrev_done_linux_x86_64");
    emitter.instruction("add r9, rsi");                                         // advance the concat-buffer write offset by the reversed-string length
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r9");               // publish the updated concat-buffer write offset after emitting the reversed string
    emitter.instruction("mov rax, r10");                                        // return the reversed-string start pointer in the primary x86_64 string result register
    emitter.instruction("mov rdx, rsi");                                        // restore the original source-string length into the x86_64 string result-length register
    emitter.instruction("ret");                                                 // return the reversed concat-backed string in the standard x86_64 string result registers
}
