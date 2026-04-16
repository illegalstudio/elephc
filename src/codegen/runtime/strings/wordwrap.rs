use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// wordwrap: wrap text at word boundaries.
/// Input: x1/x2=string, x3=width, x4/x5=break_str. Output: x1/x2=result.
pub fn emit_wordwrap(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_wordwrap_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: wordwrap ---");
    emitter.label_global("__rt_wordwrap");
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set frame pointer
    emitter.instruction("stp x4, x5, [sp]");                                    // save break string ptr/len
    emitter.instruction("str x3, [sp, #16]");                                   // save width

    // -- set up concat_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("str x9, [sp, #24]");                                   // save result start
    emitter.instruction("mov x10, #0");                                         // current line length

    emitter.label("__rt_wordwrap_loop");
    emitter.instruction("cbz x2, __rt_wordwrap_done");                          // no input left → done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte, advance source
    emitter.instruction("sub x2, x2, #1");                                      // decrement remaining

    // -- check for existing newlines (reset counter) --
    emitter.instruction("cmp w12, #10");                                        // is it '\n'?
    emitter.instruction("b.ne __rt_wordwrap_check");                            // no → check width
    emitter.instruction("strb w12, [x9], #1");                                  // store newline
    emitter.instruction("mov x10, #0");                                         // reset line length
    emitter.instruction("b __rt_wordwrap_loop");                                // next byte

    emitter.label("__rt_wordwrap_check");
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload width
    emitter.instruction("cmp x10, x3");                                         // line length >= width?
    emitter.instruction("b.lt __rt_wordwrap_store");                            // no → just store char

    // -- insert break string at width boundary --
    emitter.instruction("ldp x4, x5, [sp]");                                    // reload break string
    emitter.instruction("mov x14, #0");                                         // break copy index
    emitter.label("__rt_wordwrap_brk");
    emitter.instruction("cmp x14, x5");                                         // all break chars written?
    emitter.instruction("b.ge __rt_wordwrap_brk_done");                         // yes → continue with char
    emitter.instruction("ldrb w13, [x4, x14]");                                 // load break char
    emitter.instruction("strb w13, [x9], #1");                                  // write to output
    emitter.instruction("add x14, x14, #1");                                    // next break char
    emitter.instruction("b __rt_wordwrap_brk");                                 // continue
    emitter.label("__rt_wordwrap_brk_done");
    emitter.instruction("mov x10, #0");                                         // reset line length

    emitter.label("__rt_wordwrap_store");
    emitter.instruction("strb w12, [x9], #1");                                  // store current byte
    emitter.instruction("add x10, x10, #1");                                    // increment line length
    emitter.instruction("b __rt_wordwrap_loop");                                // next byte

    emitter.label("__rt_wordwrap_done");
    emitter.instruction("ldr x1, [sp, #24]");                                   // result pointer
    emitter.instruction("sub x2, x9, x1");                                      // result length
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.instruction("add x8, x8, x2");                                      // advance
    emitter.instruction("str x8, [x6]");                                        // store
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame
    emitter.instruction("add sp, sp, #48");                                     // deallocate
    emitter.instruction("ret");                                                 // return
}

fn emit_wordwrap_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: wordwrap ---");
    emitter.label_global("__rt_wordwrap");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving wordwrap() spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved break string, width, and concat-buffer bookkeeping
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for the break string, width, result start, and current line length
    emitter.instruction("mov QWORD PTR [rbp - 8], rcx");                        // preserve the break-string pointer across the wrapping loop
    emitter.instruction("mov QWORD PTR [rbp - 16], r8");                        // preserve the break-string length across the wrapping loop
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // preserve the requested wrap width across the wrapping loop
    crate::codegen::abi::emit_symbol_address(emitter, "r9", "_concat_off");
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current concat-buffer write offset before emitting wrapped output
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_concat_buf");
    emitter.instruction("lea r11, [r11 + r10]");                                // compute the concat-buffer destination pointer where the wrapped string begins
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // preserve the wrapped-string start pointer for the final string return pair
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // preserve the concat-offset symbol address so the helper can publish the new write position
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // start the current line-length counter at zero before processing the source bytes

    emitter.label("__rt_wordwrap_loop_linux_x86_64");
    emitter.instruction("test rdx, rdx");                                       // have all input bytes been consumed by the wrapping loop?
    emitter.instruction("jz __rt_wordwrap_done_linux_x86_64");                  // finalize the wrapped string once there are no source bytes left
    emitter.instruction("mov al, BYTE PTR [rax]");                              // load the next source byte before applying newline and width-boundary rules
    emitter.instruction("add rax, 1");                                          // advance the source-string cursor after consuming one byte
    emitter.instruction("sub rdx, 1");                                          // decrement the remaining source-byte count after consuming one byte
    emitter.instruction("cmp al, 10");                                          // is the current source byte already a newline that resets the current line-length counter?
    emitter.instruction("jne __rt_wordwrap_check_linux_x86_64");                // only run the width-boundary logic when the consumed byte is not an existing newline
    emitter.instruction("mov BYTE PTR [r11], al");                              // copy the existing newline into the wrapped output unchanged
    emitter.instruction("add r11, 1");                                          // advance the wrapped-output destination after copying the existing newline
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // reset the current line-length counter after an existing newline in the input
    emitter.instruction("jmp __rt_wordwrap_loop_linux_x86_64");                 // continue processing the remaining source bytes after handling the existing newline

    emitter.label("__rt_wordwrap_check_linux_x86_64");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the current line-length counter before testing the configured wrap width
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the configured wrap width before testing whether a line break should be inserted
    emitter.instruction("cmp rcx, r9");                                         // has the current line already reached or exceeded the configured wrap width?
    emitter.instruction("jl __rt_wordwrap_store_linux_x86_64");                 // skip inserting the break string when the current line is still below the configured width
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the break-string length before copying the wrap break into the output
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the break-string pointer before copying the wrap break into the output
    emitter.instruction("xor r9d, r9d");                                        // start copying the break string from byte index zero

    emitter.label("__rt_wordwrap_break_linux_x86_64");
    emitter.instruction("cmp r9, rcx");                                         // have all break-string bytes been emitted at the wrap boundary?
    emitter.instruction("jge __rt_wordwrap_break_done_linux_x86_64");           // resume emitting the current source byte once the full break string has been copied
    emitter.instruction("mov bl, BYTE PTR [r8 + r9]");                          // load the next break-string byte that should be emitted at the wrap boundary
    emitter.instruction("mov BYTE PTR [r11], bl");                              // store the next break-string byte into the wrapped output
    emitter.instruction("add r11, 1");                                          // advance the wrapped-output destination after storing one break-string byte
    emitter.instruction("add r9, 1");                                           // advance to the next break-string byte after a successful copy
    emitter.instruction("jmp __rt_wordwrap_break_linux_x86_64");                // continue copying the break string until the full wrap separator is emitted

    emitter.label("__rt_wordwrap_break_done_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // reset the current line-length counter after inserting the configured wrap separator

    emitter.label("__rt_wordwrap_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], al");                              // store the current source byte into the wrapped output after any required break insertion
    emitter.instruction("add r11, 1");                                          // advance the wrapped-output destination after storing one source byte
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the current line-length counter before incrementing it for the stored source byte
    emitter.instruction("add rcx, 1");                                          // increment the current line-length counter after storing one non-newline source byte
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // preserve the updated line-length counter for the next wrapping-loop iteration
    emitter.instruction("jmp __rt_wordwrap_loop_linux_x86_64");                 // continue processing the remaining source bytes until wrapping is complete

    emitter.label("__rt_wordwrap_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the wrapped-string start pointer in the primary x86_64 string result register
    emitter.instruction("mov rdx, r11");                                        // copy the wrapped-output end pointer so the final wrapped-string length can be derived
    emitter.instruction("sub rdx, rax");                                        // derive the wrapped-string length from the concat-buffer start/end pointers
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the concat-offset symbol address before publishing the new write position
    emitter.instruction("mov r8, QWORD PTR [rcx]");                             // reload the old concat-buffer write offset before advancing it by the wrapped-string length
    emitter.instruction("add r8, rdx");                                         // advance the concat-buffer write offset by the emitted wrapped-string length
    emitter.instruction("mov QWORD PTR [rcx], r8");                             // publish the updated concat-buffer write offset after emitting the wrapped string
    emitter.instruction("add rsp, 64");                                         // release the wordwrap() spill slots before returning the wrapped string
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the caller
    emitter.instruction("ret");                                                 // return the wrapped string in the standard x86_64 string result registers
}
