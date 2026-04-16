use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// strrpos: find last occurrence of needle. Returns position or -1.
pub fn emit_strrpos(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strrpos_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: strrpos ---");
    emitter.label_global("__rt_strrpos");

    // -- edge cases --
    emitter.instruction("cbz x4, __rt_strrpos_empty");                          // empty needle returns last position
    emitter.instruction("cmp x4, x2");                                          // compare needle length with haystack length
    emitter.instruction("b.gt __rt_strrpos_notfound");                          // needle longer than haystack, can't match
    emitter.instruction("sub x5, x2, x4");                                      // start searching from rightmost valid position

    // -- outer loop: try matching needle from right to left --
    emitter.label("__rt_strrpos_outer");
    emitter.instruction("mov x6, #0");                                          // reset needle comparison index
    emitter.label("__rt_strrpos_inner");
    emitter.instruction("cmp x6, x4");                                          // check if all needle bytes matched
    emitter.instruction("b.ge __rt_strrpos_found");                             // all matched, found at position x5
    emitter.instruction("add x7, x5, x6");                                      // compute haystack index = pos + needle_idx
    emitter.instruction("ldrb w8, [x1, x7]");                                   // load haystack byte at computed index
    emitter.instruction("ldrb w9, [x3, x6]");                                   // load needle byte at current index
    emitter.instruction("cmp w8, w9");                                          // compare haystack and needle bytes
    emitter.instruction("b.ne __rt_strrpos_prev");                              // mismatch, try previous position
    emitter.instruction("add x6, x6, #1");                                      // advance needle index
    emitter.instruction("b __rt_strrpos_inner");                                // continue comparing

    // -- move to previous position (searching right to left) --
    emitter.label("__rt_strrpos_prev");
    emitter.instruction("cbz x5, __rt_strrpos_notfound");                       // if at position 0, nowhere left to search
    emitter.instruction("sub x5, x5, #1");                                      // decrement search position
    emitter.instruction("b __rt_strrpos_outer");                                // retry from new position

    // -- return results --
    emitter.label("__rt_strrpos_found");
    emitter.instruction("mov x0, x5");                                          // return last match position
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_strrpos_empty");
    emitter.instruction("sub x0, x2, #0");                                      // empty needle returns haystack length
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_strrpos_notfound");
    emitter.instruction("mov x0, #-1");                                         // return -1 (not found)
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_strrpos_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strrpos ---");
    emitter.label_global("__rt_strrpos");

    emitter.instruction("test rcx, rcx");                                       // empty needles match just after the last haystack byte
    emitter.instruction("jz __rt_strrpos_empty_linux_x86_64");                  // return the haystack length when strrpos() receives an empty needle
    emitter.instruction("cmp rcx, rsi");                                        // reject searches whose needle is longer than the haystack
    emitter.instruction("jg __rt_strrpos_notfound_linux_x86_64");               // return the not-found sentinel when the needle cannot fit
    emitter.instruction("mov r9, rsi");                                         // copy the haystack length so the rightmost valid start offset can be computed once
    emitter.instruction("sub r9, rcx");                                         // compute the rightmost haystack offset where the full needle can still fit

    emitter.label("__rt_strrpos_outer_linux_x86_64");
    emitter.instruction("xor r10d, r10d");                                      // start the needle byte comparison from index zero for the current candidate

    emitter.label("__rt_strrpos_inner_linux_x86_64");
    emitter.instruction("cmp r10, rcx");                                        // did every byte in the needle match at the current haystack offset?
    emitter.instruction("jge __rt_strrpos_found_linux_x86_64");                 // return the current haystack offset once the full needle matches
    emitter.instruction("mov r8, r9");                                          // copy the current candidate start offset so the indexed haystack byte can be addressed
    emitter.instruction("add r8, r10");                                         // compute the absolute haystack byte offset for the current needle byte
    emitter.instruction("movzx eax, BYTE PTR [rdi + r8]");                      // load the current haystack byte for the right-to-left candidate comparison
    emitter.instruction("movzx r11d, BYTE PTR [rdx + r10]");                    // load the current needle byte for the right-to-left candidate comparison
    emitter.instruction("cmp eax, r11d");                                       // compare the haystack and needle bytes at the current candidate position
    emitter.instruction("jne __rt_strrpos_prev_linux_x86_64");                  // abandon this candidate start offset on the first mismatching byte
    emitter.instruction("add r10, 1");                                          // advance to the next byte within the current needle comparison
    emitter.instruction("jmp __rt_strrpos_inner_linux_x86_64");                 // continue matching bytes against the current right-to-left candidate start offset

    emitter.label("__rt_strrpos_prev_linux_x86_64");
    emitter.instruction("test r9, r9");                                         // are we already at haystack offset zero with no further candidates left to test?
    emitter.instruction("jz __rt_strrpos_notfound_linux_x86_64");               // return the not-found sentinel once the final candidate also mismatches
    emitter.instruction("sub r9, 1");                                           // move the candidate start offset one byte to the left
    emitter.instruction("jmp __rt_strrpos_outer_linux_x86_64");                 // retry the needle comparison from the next right-to-left haystack start offset

    emitter.label("__rt_strrpos_found_linux_x86_64");
    emitter.instruction("mov rax, r9");                                         // return the last haystack offset whose bytes matched the full needle
    emitter.instruction("ret");                                                 // return the signed match offset to the caller

    emitter.label("__rt_strrpos_empty_linux_x86_64");
    emitter.instruction("mov rax, rsi");                                        // empty needles match just after the final haystack byte
    emitter.instruction("ret");                                                 // return the empty-needle offset to the caller

    emitter.label("__rt_strrpos_notfound_linux_x86_64");
    emitter.instruction("mov rax, -1");                                         // return the not-found sentinel when no haystack offset matches the needle
    emitter.instruction("ret");                                                 // return the not-found sentinel to the caller
}
