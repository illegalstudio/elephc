use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// stripslashes: remove escape backslashes.
/// Input: x1/x2=string. Output: x1/x2=result.
pub fn emit_stripslashes(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stripslashes_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stripslashes ---");
    emitter.label_global("__rt_stripslashes");

    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    emitter.label("__rt_stripslashes_loop");
    emitter.instruction("cbz x11, __rt_stripslashes_done");                     // done if no bytes left
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte, advance source
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    emitter.instruction("cmp w12, #92");                                        // is it a backslash?
    emitter.instruction("b.ne __rt_stripslashes_store");                        // no → store as-is
    // -- backslash: skip it and store the next char --
    emitter.instruction("cbz x11, __rt_stripslashes_store");                    // trailing backslash → store it
    emitter.instruction("ldrb w12, [x1], #1");                                  // load escaped char, advance
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    emitter.label("__rt_stripslashes_store");
    emitter.instruction("strb w12, [x9], #1");                                  // store byte to output
    emitter.instruction("b __rt_stripslashes_loop");                            // next byte

    emitter.label("__rt_stripslashes_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}

fn emit_stripslashes_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stripslashes ---");
    emitter.label_global("__rt_stripslashes");

    emitter.instruction("mov r8, QWORD PTR [rip + _concat_off]");               // load the current concat-buffer absolute offset before appending the unescaped string
    emitter.instruction("lea r9, [rip + _concat_buf]");                         // materialize the concat-buffer base pointer for the unescaped string write
    emitter.instruction("add r9, r8");                                          // compute the current concat-buffer write pointer from the base plus offset
    emitter.instruction("mov r10, r9");                                         // preserve the unescaped-string start pointer for the final result slice
    emitter.instruction("mov rcx, rdx");                                        // track how many source bytes remain to be scanned for escape prefixes

    emitter.label("__rt_stripslashes_loop");
    emitter.instruction("test rcx, rcx");                                       // have we consumed every byte of the escaped source string?
    emitter.instruction("je __rt_stripslashes_done");                           // finish once no source bytes remain
    emitter.instruction("movzx r11d, BYTE PTR [rax]");                          // load the next source byte and widen it for unsigned backslash comparisons
    emitter.instruction("add rax, 1");                                          // advance the source pointer after consuming the current byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining source-byte count after the load
    emitter.instruction("cmp r11b, 92");                                        // does the current source byte start an escape sequence?
    emitter.instruction("jne __rt_stripslashes_store");                         // ordinary bytes copy through unchanged when no backslash prefix is present
    emitter.instruction("test rcx, rcx");                                       // is the backslash the final byte of the source string?
    emitter.instruction("je __rt_stripslashes_store");                          // trailing backslashes stay literal because there is no escaped byte to consume
    emitter.instruction("movzx r11d, BYTE PTR [rax]");                          // load the escaped byte that follows the backslash prefix
    emitter.instruction("add rax, 1");                                          // advance past the escaped byte after discarding the prefix backslash
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining count for the escaped byte we just consumed

    emitter.label("__rt_stripslashes_store");
    emitter.instruction("mov BYTE PTR [r9], r11b");                             // copy the current logical output byte into the concat buffer
    emitter.instruction("add r9, 1");                                           // advance the concat-buffer write pointer past the copied output byte
    emitter.instruction("jmp __rt_stripslashes_loop");                          // continue processing the remaining source bytes

    emitter.label("__rt_stripslashes_done");
    emitter.instruction("mov rax, r10");                                        // return the unescaped-string start pointer in the x86_64 string result pointer register
    emitter.instruction("mov rdx, r9");                                         // snapshot the final concat-buffer write pointer before computing the unescaped result length
    emitter.instruction("sub rdx, r10");                                        // compute the unescaped result length from the write pointer minus the start pointer
    emitter.instruction("mov r8, QWORD PTR [rip + _concat_off]");               // reload the previous concat-buffer absolute offset before publishing the appended slice
    emitter.instruction("add r8, rdx");                                         // advance the concat-buffer absolute offset by the unescaped result length
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r8");               // publish the updated concat-buffer absolute offset for later writers
    emitter.instruction("ret");                                                 // return to the caller with the unescaped string slice in rax/rdx
}
