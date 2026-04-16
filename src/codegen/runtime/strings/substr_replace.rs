use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// substr_replace: replace portion of string.
/// Input: x1/x2=subject, x3/x4=replacement, x0=offset, x7=length (-1=to end).
/// Output: x1/x2=result.
pub fn emit_substr_replace(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_substr_replace_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: substr_replace ---");
    emitter.label_global("__rt_substr_replace");
    emitter.instruction("sub sp, sp, #16");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // set frame pointer

    // -- clamp offset --
    emitter.instruction("cmp x0, #0");                                          // check if offset is negative
    emitter.instruction("b.ge 1f");                                             // skip if non-negative
    emitter.instruction("add x0, x2, x0");                                      // offset = len + offset
    emitter.instruction("cmp x0, #0");                                          // clamp to 0
    emitter.instruction("csel x0, xzr, x0, lt");                                // if still negative, use 0
    emitter.raw("1:");
    emitter.instruction("cmp x0, x2");                                          // clamp offset to string length
    emitter.instruction("csel x0, x2, x0, gt");                                 // min(offset, len)

    // -- compute replace length --
    emitter.instruction("cmn x7, #1");                                          // check if length == -1 (sentinel)
    emitter.instruction("b.ne 2f");                                             // if not sentinel, use given length
    emitter.instruction("sub x7, x2, x0");                                      // length = remaining from offset
    emitter.raw("2:");
    emitter.instruction("cmp x7, #0");                                          // clamp negative length to 0
    emitter.instruction("csel x7, xzr, x7, lt");                                // max(0, length)
    emitter.instruction("add x8, x0, x7");                                      // end = offset + length
    emitter.instruction("cmp x8, x2");                                          // clamp end to string length
    emitter.instruction("csel x8, x2, x8, gt");                                 // min(end, len)

    // -- build result: prefix + replacement + suffix --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x12, x11, x10");                                   // destination pointer
    emitter.instruction("mov x13, x12");                                        // save result start

    // -- copy prefix: subject[0..offset] --
    emitter.instruction("mov x14, #0");                                         // copy index
    emitter.label("__rt_subrepl_pre");
    emitter.instruction("cmp x14, x0");                                         // copied offset bytes?
    emitter.instruction("b.ge __rt_subrepl_mid");                               // yes → copy replacement
    emitter.instruction("ldrb w15, [x1, x14]");                                 // load prefix byte
    emitter.instruction("strb w15, [x12], #1");                                 // store and advance
    emitter.instruction("add x14, x14, #1");                                    // next byte
    emitter.instruction("b __rt_subrepl_pre");                                  // continue

    // -- copy replacement --
    emitter.label("__rt_subrepl_mid");
    emitter.instruction("mov x14, #0");                                         // replacement copy index
    emitter.label("__rt_subrepl_rep");
    emitter.instruction("cmp x14, x4");                                         // all replacement bytes copied?
    emitter.instruction("b.ge __rt_subrepl_suf");                               // yes → copy suffix
    emitter.instruction("ldrb w15, [x3, x14]");                                 // load replacement byte
    emitter.instruction("strb w15, [x12], #1");                                 // store and advance
    emitter.instruction("add x14, x14, #1");                                    // next byte
    emitter.instruction("b __rt_subrepl_rep");                                  // continue

    // -- copy suffix: subject[end..len] --
    emitter.label("__rt_subrepl_suf");
    emitter.instruction("mov x14, x8");                                         // start from end position
    emitter.label("__rt_subrepl_suf_loop");
    emitter.instruction("cmp x14, x2");                                         // past end of subject?
    emitter.instruction("b.ge __rt_subrepl_done");                              // yes → done
    emitter.instruction("ldrb w15, [x1, x14]");                                 // load suffix byte
    emitter.instruction("strb w15, [x12], #1");                                 // store and advance
    emitter.instruction("add x14, x14, #1");                                    // next byte
    emitter.instruction("b __rt_subrepl_suf_loop");                             // continue

    emitter.label("__rt_subrepl_done");
    emitter.instruction("mov x1, x13");                                         // result pointer
    emitter.instruction("sub x2, x12, x13");                                    // result length
    emitter.instruction("ldr x10, [x9]");                                       // reload current offset
    emitter.instruction("add x10, x10, x2");                                    // advance by result length
    emitter.instruction("str x10, [x9]");                                       // store updated offset
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame
    emitter.instruction("add sp, sp, #16");                                     // deallocate
    emitter.instruction("ret");                                                 // return
}

fn emit_substr_replace_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: substr_replace ---");
    emitter.label_global("__rt_substr_replace");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving substr_replace() spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved subject, replacement, and slice bounds
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for the input strings plus concat-buffer bookkeeping
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the subject string pointer across offset clamping and concat-buffer copying
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the subject string length across offset clamping and concat-buffer copying
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // preserve the replacement string pointer across the concat-buffer copy loops
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // preserve the replacement string length across the concat-buffer copy loops
    emitter.instruction("mov r9, rcx");                                         // start clamping from the requested replacement offset
    emitter.instruction("cmp r9, 0");                                           // check whether the requested replacement offset is negative
    emitter.instruction("jge __rt_substr_replace_off_ready_linux_x86_64");      // skip the tail-relative offset fixup when the requested offset is already non-negative
    emitter.instruction("add r9, QWORD PTR [rbp - 16]");                        // convert the negative offset into a tail-relative byte index
    emitter.instruction("cmp r9, 0");                                           // check whether the tail-relative replacement offset still points before the string start
    emitter.instruction("mov rcx, 0");                                          // materialize zero for the final negative-offset clamp
    emitter.instruction("cmovl r9, rcx");                                       // clamp the adjusted replacement offset back to zero when it still underflows
    emitter.label("__rt_substr_replace_off_ready_linux_x86_64");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the full subject-string length before clamping offsets past the end
    emitter.instruction("cmp r9, rcx");                                         // compare the requested replacement offset against the full subject-string length
    emitter.instruction("cmovg r9, rcx");                                       // clamp the replacement offset to the end of the subject string when needed
    emitter.instruction("mov r10, r8");                                         // start from the requested replacement length before sentinel and bounds clamping
    emitter.instruction("cmp r10, -1");                                         // check whether the caller omitted the optional replacement length
    emitter.instruction("jne __rt_substr_replace_len_known_linux_x86_64");      // skip the sentinel expansion when the caller supplied an explicit replacement length
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the subject-string length before deriving the tail replacement span
    emitter.instruction("sub r10, r9");                                         // replace the remainder of the subject string when the optional length is omitted
    emitter.label("__rt_substr_replace_len_known_linux_x86_64");
    emitter.instruction("cmp r10, 0");                                          // check whether the requested replacement length is negative
    emitter.instruction("mov rcx, 0");                                          // materialize zero for the negative-length clamp
    emitter.instruction("cmovl r10, rcx");                                      // clamp negative replacement lengths back to zero
    emitter.instruction("mov r11, r9");                                         // seed the suffix start from the clamped replacement offset
    emitter.instruction("add r11, r10");                                        // compute the byte offset immediately after the replaced slice
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the full subject-string length before clamping the suffix start
    emitter.instruction("cmp r11, rcx");                                        // compare the suffix start against the subject-string end
    emitter.instruction("cmovg r11, rcx");                                      // clamp the suffix start to the end of the subject string when the slice overruns
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // preserve the clamped replacement offset for the prefix copy loop
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // preserve the clamped suffix start for the suffix copy loop
    crate::codegen::abi::emit_symbol_address(emitter, "rcx", "_concat_off");
    emitter.instruction("mov r8, QWORD PTR [rcx]");                             // load the current concat-buffer write offset before emitting the replacement result
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r8, [r10 + r8]");                                  // compute the concat-buffer destination pointer where the replaced string begins
    emitter.instruction("mov QWORD PTR [rbp - 56], r8");                        // preserve the replaced-string start pointer for the final x86_64 string return pair
    emitter.instruction("mov QWORD PTR [rbp - 64], rcx");                       // preserve the concat-offset symbol address so the helper can publish the new write position
    emitter.instruction("xor rcx, rcx");                                        // start the prefix copy loop from byte offset zero

    emitter.label("__rt_substr_replace_prefix_linux_x86_64");
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 40]");                       // have we already copied every prefix byte before the replacement offset?
    emitter.instruction("jge __rt_substr_replace_replacement_linux_x86_64");    // jump to the replacement payload copy once the full prefix is emitted
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the subject string pointer before copying the next prefix byte
    emitter.instruction("mov al, BYTE PTR [r10 + rcx]");                        // load the current prefix byte from the subject string
    emitter.instruction("mov BYTE PTR [r8], al");                               // store the current prefix byte into the concat-buffer destination
    emitter.instruction("add r8, 1");                                           // advance the concat-buffer destination after storing one prefix byte
    emitter.instruction("add rcx, 1");                                          // advance to the next prefix byte before repeating the loop
    emitter.instruction("jmp __rt_substr_replace_prefix_linux_x86_64");         // continue copying prefix bytes until the replacement offset is reached

    emitter.label("__rt_substr_replace_replacement_linux_x86_64");
    emitter.instruction("xor rcx, rcx");                                        // start copying the replacement payload from byte offset zero

    emitter.label("__rt_substr_replace_replacement_loop_linux_x86_64");
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 32]");                       // have we already copied every byte of the replacement string?
    emitter.instruction("jge __rt_substr_replace_suffix_linux_x86_64");         // jump to the suffix copy once the full replacement string is emitted
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the replacement string pointer before copying the next replacement byte
    emitter.instruction("mov al, BYTE PTR [r10 + rcx]");                        // load the current replacement byte from the replacement string
    emitter.instruction("mov BYTE PTR [r8], al");                               // store the current replacement byte into the concat-buffer destination
    emitter.instruction("add r8, 1");                                           // advance the concat-buffer destination after storing one replacement byte
    emitter.instruction("add rcx, 1");                                          // advance to the next replacement byte before repeating the loop
    emitter.instruction("jmp __rt_substr_replace_replacement_loop_linux_x86_64"); // continue copying replacement bytes until the full replacement string is emitted

    emitter.label("__rt_substr_replace_suffix_linux_x86_64");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // start the suffix copy from the clamped byte offset immediately after the replaced slice

    emitter.label("__rt_substr_replace_suffix_loop_linux_x86_64");
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // have we already copied every suffix byte through the end of the subject string?
    emitter.instruction("jge __rt_substr_replace_done_linux_x86_64");           // finalize the returned string once the suffix is fully emitted
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the subject string pointer before copying the next suffix byte
    emitter.instruction("mov al, BYTE PTR [r10 + rcx]");                        // load the current suffix byte from the subject string
    emitter.instruction("mov BYTE PTR [r8], al");                               // store the current suffix byte into the concat-buffer destination
    emitter.instruction("add r8, 1");                                           // advance the concat-buffer destination after storing one suffix byte
    emitter.instruction("add rcx, 1");                                          // advance to the next suffix byte before repeating the loop
    emitter.instruction("jmp __rt_substr_replace_suffix_loop_linux_x86_64");    // continue copying suffix bytes until the subject-string end is reached

    emitter.label("__rt_substr_replace_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // return the concat-buffer start pointer of the replaced string in the primary x86_64 string result register
    emitter.instruction("mov rdx, r8");                                         // copy the concat-buffer end pointer so the final replaced-string length can be derived
    emitter.instruction("sub rdx, rax");                                        // derive the replaced-string length from the concat-buffer start/end pointers
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // reload the concat-offset symbol address before publishing the new write position
    emitter.instruction("mov r9, QWORD PTR [rcx]");                             // reload the old concat-buffer write offset before advancing it by the replaced-string length
    emitter.instruction("add r9, rdx");                                         // advance the concat-buffer write offset by the emitted replaced-string length
    emitter.instruction("mov QWORD PTR [rcx], r9");                             // publish the updated concat-buffer write offset after emitting the replaced string
    emitter.instruction("add rsp, 64");                                         // release the substr_replace() spill slots before returning the replaced string
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the caller
    emitter.instruction("ret");                                                 // return the replaced string in the standard x86_64 string result registers
}
