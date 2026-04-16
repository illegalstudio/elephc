use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// str_pad: pad a string to a target length.
/// Input: x1/x2=input, x3/x4=pad_str, x5=target_len, x7=pad_type (0=left, 1=right, 2=both).
/// Output: x1/x2=result.
pub fn emit_str_pad(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_pad_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_pad ---");
    emitter.label_global("__rt_str_pad");
    emitter.instruction("sub sp, sp, #64");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save input string
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save pad string
    emitter.instruction("str x5, [sp, #32]");                                   // save target length
    emitter.instruction("str x7, [sp, #40]");                                   // save pad type

    // -- if input already >= target, return as-is --
    emitter.instruction("cmp x2, x5");                                          // compare input len with target
    emitter.instruction("b.ge __rt_str_pad_noop");                              // already long enough → return copy

    // -- set up concat_buf destination --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x12, x11, x10");                                   // destination pointer
    emitter.instruction("mov x13, x12");                                        // save result start

    emitter.instruction("sub x14, x5, x2");                                     // pad_needed = target - input_len
    emitter.instruction("ldr x7, [sp, #40]");                                   // reload pad_type

    // -- compute left_pad and right_pad amounts --
    emitter.instruction("cmp x7, #0");                                          // STR_PAD_LEFT?
    emitter.instruction("b.eq __rt_str_pad_left_all");                          // all padding on left
    emitter.instruction("cmp x7, #2");                                          // STR_PAD_BOTH?
    emitter.instruction("b.eq __rt_str_pad_both");                              // split padding
    // -- STR_PAD_RIGHT (default): all padding on right --
    emitter.instruction("mov x15, #0");                                         // left_pad = 0
    emitter.instruction("mov x16, x14");                                        // right_pad = all
    emitter.instruction("b __rt_str_pad_emit");                                 // start emitting

    emitter.label("__rt_str_pad_left_all");
    emitter.instruction("mov x15, x14");                                        // left_pad = all
    emitter.instruction("mov x16, #0");                                         // right_pad = 0
    emitter.instruction("b __rt_str_pad_emit");                                 // start emitting

    emitter.label("__rt_str_pad_both");
    emitter.instruction("lsr x15, x14, #1");                                    // left_pad = pad_needed / 2
    emitter.instruction("sub x16, x14, x15");                                   // right_pad = pad_needed - left_pad
    // fall through to emit

    // -- emit: left_pad chars, then input, then right_pad chars --
    emitter.label("__rt_str_pad_emit");
    // left padding
    emitter.instruction("mov x17, x15");                                        // left pad counter
    emitter.instruction("mov x18, #0");                                         // pad string index
    emitter.label("__rt_str_pad_lp");
    emitter.instruction("cbz x17, __rt_str_pad_input");                         // left padding done → copy input
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload pad string
    emitter.instruction("ldrb w0, [x3, x18]");                                  // load pad char at index
    emitter.instruction("strb w0, [x12], #1");                                  // write to output
    emitter.instruction("sub x17, x17, #1");                                    // decrement left pad remaining
    emitter.instruction("add x18, x18, #1");                                    // advance pad index
    emitter.instruction("cmp x18, x4");                                         // wrap around if past pad string
    emitter.instruction("csel x18, xzr, x18, ge");                              // reset to 0 if >= pad_len
    emitter.instruction("b __rt_str_pad_lp");                                   // continue

    // copy input
    emitter.label("__rt_str_pad_input");
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload input string
    emitter.instruction("mov x17, x2");                                         // input copy counter
    emitter.label("__rt_str_pad_inp_loop");
    emitter.instruction("cbz x17, __rt_str_pad_rp");                            // input done → right padding
    emitter.instruction("ldrb w0, [x1], #1");                                   // load input byte
    emitter.instruction("strb w0, [x12], #1");                                  // write to output
    emitter.instruction("sub x17, x17, #1");                                    // decrement
    emitter.instruction("b __rt_str_pad_inp_loop");                             // continue

    // right padding
    emitter.label("__rt_str_pad_rp");
    emitter.instruction("mov x17, x16");                                        // right pad counter
    emitter.instruction("mov x18, #0");                                         // pad string index
    emitter.label("__rt_str_pad_rp_loop");
    emitter.instruction("cbz x17, __rt_str_pad_done");                          // right padding done
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload pad string
    emitter.instruction("ldrb w0, [x3, x18]");                                  // load pad char
    emitter.instruction("strb w0, [x12], #1");                                  // write to output
    emitter.instruction("sub x17, x17, #1");                                    // decrement
    emitter.instruction("add x18, x18, #1");                                    // advance pad index
    emitter.instruction("cmp x18, x4");                                         // wrap around
    emitter.instruction("csel x18, xzr, x18, ge");                              // reset to 0
    emitter.instruction("b __rt_str_pad_rp_loop");                              // continue

    emitter.label("__rt_str_pad_done");
    emitter.instruction("mov x1, x13");                                         // result pointer
    emitter.instruction("sub x2, x12, x13");                                    // result length
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("add x10, x10, x2");                                    // advance by result length
    emitter.instruction("str x10, [x9]");                                       // store updated offset
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame
    emitter.instruction("add sp, sp, #64");                                     // deallocate
    emitter.instruction("ret");                                                 // return

    emitter.label("__rt_str_pad_noop");
    emitter.instruction("bl __rt_strcopy");                                     // copy input as-is
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame
    emitter.instruction("add sp, sp, #64");                                     // deallocate
    emitter.instruction("ret");                                                 // return
}

fn emit_str_pad_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_pad ---");
    emitter.label_global("__rt_str_pad");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving str_pad() spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved strings, pad metadata, and concat-buffer bookkeeping
    emitter.instruction("sub rsp, 96");                                         // reserve aligned spill slots for the input string, pad string, target length, pad type, result start, and pad counters
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the input string pointer across the padding loops and the strcopy() noop path
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the input string length across the padding loops and the strcopy() noop path
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // preserve the pad string pointer across the left/right padding loops
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // preserve the pad string length across the left/right padding loops
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // preserve the requested target length across the padding loops
    emitter.instruction("mov QWORD PTR [rbp - 48], r8");                        // preserve the requested pad type across the padding loops
    emitter.instruction("cmp rdx, rcx");                                        // does the input string already meet or exceed the requested target width?
    emitter.instruction("jge __rt_str_pad_noop_linux_x86_64");                  // return a copied input string immediately when no padding is required
    crate::codegen::abi::emit_symbol_address(emitter, "r9", "_concat_off");
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current concat-buffer write offset before emitting the padded result
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_concat_buf");
    emitter.instruction("lea r11, [r11 + r10]");                                // compute the concat-buffer destination pointer where the padded result begins
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // preserve the padded-result start pointer for the final string return pair
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // preserve the concat-offset symbol address so the helper can publish the final write offset
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the requested target length before computing the total number of pad bytes
    emitter.instruction("sub r10, QWORD PTR [rbp - 16]");                       // compute how many pad bytes are needed to reach the requested target width
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the requested pad type before splitting the total pad budget
    emitter.instruction("cmp rax, 0");                                          // is the requested padding mode STR_PAD_LEFT?
    emitter.instruction("je __rt_str_pad_left_all_linux_x86_64");               // assign the entire pad budget to the left side for STR_PAD_LEFT
    emitter.instruction("cmp rax, 2");                                          // is the requested padding mode STR_PAD_BOTH?
    emitter.instruction("je __rt_str_pad_both_linux_x86_64");                   // split the pad budget across both sides for STR_PAD_BOTH
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // assign zero left-padding bytes for the default STR_PAD_RIGHT path
    emitter.instruction("mov QWORD PTR [rbp - 80], r10");                       // assign the full pad budget to the right side for the default STR_PAD_RIGHT path
    emitter.instruction("jmp __rt_str_pad_emit_linux_x86_64");                  // start emitting padding bytes once the left/right budgets are initialized

    emitter.label("__rt_str_pad_left_all_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // assign the full pad budget to the left side for STR_PAD_LEFT
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // assign zero right-padding bytes for STR_PAD_LEFT
    emitter.instruction("jmp __rt_str_pad_emit_linux_x86_64");                  // start emitting padding bytes once the left/right budgets are initialized

    emitter.label("__rt_str_pad_both_linux_x86_64");
    emitter.instruction("mov rax, r10");                                        // copy the total pad budget before splitting it across both sides
    emitter.instruction("shr rax, 1");                                          // compute floor(total_pad / 2) for the left-padding budget
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // preserve the left-padding budget for the left-padding loop
    emitter.instruction("mov rcx, r10");                                        // copy the total pad budget before deriving the right-padding budget
    emitter.instruction("sub rcx, rax");                                        // compute the remaining pad bytes that should go on the right side
    emitter.instruction("mov QWORD PTR [rbp - 80], rcx");                       // preserve the right-padding budget for the right-padding loop

    emitter.label("__rt_str_pad_emit_linux_x86_64");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 72]");                       // reload the remaining left-padding byte budget before entering the left-padding loop
    emitter.instruction("xor r8d, r8d");                                        // start the pad-string byte index at zero for the left-padding loop

    emitter.label("__rt_str_pad_left_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // has the left-padding budget been fully emitted?
    emitter.instruction("jz __rt_str_pad_input_linux_x86_64");                  // start copying the input string once the left-padding budget reaches zero
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the pad string pointer before loading the next repeated pad byte
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // reload the pad string length before wrapping the repeated pad-byte index
    emitter.instruction("mov al, BYTE PTR [rax + r8]");                         // load the next repeated pad byte for the left-padding region
    emitter.instruction("mov BYTE PTR [r11], al");                              // store the repeated pad byte into the concat-buffer destination
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination after storing one left-padding byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining left-padding byte budget after emitting one byte
    emitter.instruction("add r8, 1");                                           // advance the repeated pad-byte index after consuming one pad byte
    emitter.instruction("cmp r8, rdx");                                         // has the repeated pad-byte index reached the end of the pad string?
    emitter.instruction("jb __rt_str_pad_left_loop_linux_x86_64");              // keep the current pad-byte index when more bytes remain in the pad string
    emitter.instruction("xor r8d, r8d");                                        // wrap the repeated pad-byte index back to zero when the pad string is exhausted
    emitter.instruction("jmp __rt_str_pad_left_loop_linux_x86_64");             // continue emitting the remaining left-padding bytes

    emitter.label("__rt_str_pad_input_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the input string pointer before copying the unmodified input bytes
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the input string length before copying the unmodified input bytes

    emitter.label("__rt_str_pad_input_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // have all input bytes been copied into the padded result?
    emitter.instruction("jz __rt_str_pad_right_linux_x86_64");                  // start emitting right-padding bytes once the full input string has been copied
    emitter.instruction("mov dl, BYTE PTR [rax]");                              // load the next byte from the input string
    emitter.instruction("mov BYTE PTR [r11], dl");                              // store the next input byte into the concat-buffer destination
    emitter.instruction("add rax, 1");                                          // advance the input-string cursor after copying one byte
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination after storing one input byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining input-byte count after copying one byte
    emitter.instruction("jmp __rt_str_pad_input_loop_linux_x86_64");            // continue copying input bytes until the full source string is emitted

    emitter.label("__rt_str_pad_right_linux_x86_64");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 80]");                       // reload the remaining right-padding byte budget before entering the right-padding loop
    emitter.instruction("xor r8d, r8d");                                        // start the pad-string byte index at zero for the right-padding loop

    emitter.label("__rt_str_pad_right_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // has the right-padding budget been fully emitted?
    emitter.instruction("jz __rt_str_pad_done_linux_x86_64");                   // finalize the padded string once the right-padding budget reaches zero
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the pad string pointer before loading the next repeated pad byte
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // reload the pad string length before wrapping the repeated pad-byte index
    emitter.instruction("mov al, BYTE PTR [rax + r8]");                         // load the next repeated pad byte for the right-padding region
    emitter.instruction("mov BYTE PTR [r11], al");                              // store the repeated pad byte into the concat-buffer destination
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination after storing one right-padding byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining right-padding byte budget after emitting one byte
    emitter.instruction("add r8, 1");                                           // advance the repeated pad-byte index after consuming one pad byte
    emitter.instruction("cmp r8, rdx");                                         // has the repeated pad-byte index reached the end of the pad string?
    emitter.instruction("jb __rt_str_pad_right_loop_linux_x86_64");             // keep the current pad-byte index when more bytes remain in the pad string
    emitter.instruction("xor r8d, r8d");                                        // wrap the repeated pad-byte index back to zero when the pad string is exhausted
    emitter.instruction("jmp __rt_str_pad_right_loop_linux_x86_64");            // continue emitting the remaining right-padding bytes

    emitter.label("__rt_str_pad_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // return the concat-buffer start pointer of the padded string in the primary x86_64 string result register
    emitter.instruction("mov rdx, r11");                                        // copy the concat-buffer end pointer so the final padded-string length can be derived
    emitter.instruction("sub rdx, rax");                                        // derive the padded-string length from the concat-buffer start/end pointers
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // reload the concat-offset symbol address before publishing the new write position
    emitter.instruction("mov r8, QWORD PTR [rcx]");                             // reload the old concat-buffer write offset before advancing it by the padded-string length
    emitter.instruction("add r8, rdx");                                         // advance the concat-buffer write offset by the emitted padded-string length
    emitter.instruction("mov QWORD PTR [rcx], r8");                             // publish the updated concat-buffer write offset after emitting the padded string
    emitter.instruction("add rsp, 96");                                         // release the str_pad() spill slots before returning the padded string
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the caller
    emitter.instruction("ret");                                                 // return the padded string in the standard x86_64 string result registers

    emitter.label("__rt_str_pad_noop_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the input string pointer before delegating the no-padding case to strcopy()
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the input string length before delegating the no-padding case to strcopy()
    emitter.instruction("call __rt_strcopy");                                   // copy the unmodified input string into concat storage so str_pad() still returns owned string storage
    emitter.instruction("add rsp, 96");                                         // release the str_pad() spill slots before returning the copied no-op result
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the caller
    emitter.instruction("ret");                                                 // return the copied input string in the standard x86_64 string result registers
}
