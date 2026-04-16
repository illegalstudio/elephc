use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// ltrim_mask: strip characters in mask from left of string.
/// Input: x1=str_ptr, x2=str_len, x3=mask_ptr, x4=mask_len
/// Output: x1=adjusted_ptr, x2=adjusted_len
pub fn emit_ltrim_mask(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ltrim_mask_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ltrim_mask ---");
    emitter.label_global("__rt_ltrim_mask");

    // -- loop: check first character against mask --
    emitter.label("__rt_ltrim_mask_loop");
    emitter.instruction("cbz x2, __rt_ltrim_mask_done");                        // if string is empty, nothing to trim
    emitter.instruction("ldrb w10, [x1]");                                      // load first byte of string

    // -- inner loop: check if w10 is in mask --
    emitter.instruction("mov x11, #0");                                         // mask index = 0
    emitter.label("__rt_ltrim_mask_cmp");
    emitter.instruction("cmp x11, x4");                                         // check if we've exhausted the mask
    emitter.instruction("b.ge __rt_ltrim_mask_done");                           // char not in mask, stop trimming
    emitter.instruction("ldrb w12, [x3, x11]");                                 // load mask character at index x11
    emitter.instruction("cmp w10, w12");                                        // compare string char with mask char
    emitter.instruction("b.eq __rt_ltrim_mask_skip");                           // match found, skip this character
    emitter.instruction("add x11, x11, #1");                                    // advance mask index
    emitter.instruction("b __rt_ltrim_mask_cmp");                               // check next mask character

    // -- skip: advance pointer, shrink length, re-check --
    emitter.label("__rt_ltrim_mask_skip");
    emitter.instruction("add x1, x1, #1");                                      // advance string pointer past matched char
    emitter.instruction("sub x2, x2, #1");                                      // decrement string length
    emitter.instruction("b __rt_ltrim_mask_loop");                              // check next first character

    emitter.label("__rt_ltrim_mask_done");
    emitter.instruction("ret");                                                 // return with adjusted x1 and x2
}

fn emit_ltrim_mask_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ltrim_mask ---");
    emitter.label_global("__rt_ltrim_mask");

    emitter.label("__rt_ltrim_mask_loop_linux_x86_64");
    emitter.instruction("test rdx, rdx");                                       // stop once the borrowed source string slice has become empty after trimming from the left
    emitter.instruction("jz __rt_ltrim_mask_done_linux_x86_64");                // return immediately when there are no bytes left to classify against the trim mask
    emitter.instruction("movzx ecx, BYTE PTR [rax]");                           // load the current leading source byte so ltrim_mask() can check whether the mask contains it
    emitter.instruction("xor r8, r8");                                          // reset the trim-mask index before scanning the mask bytes for a match with the current leading source byte

    emitter.label("__rt_ltrim_mask_cmp_linux_x86_64");
    emitter.instruction("cmp r8, rsi");                                         // have we exhausted the trim-mask bytes without matching the current leading source byte?
    emitter.instruction("jae __rt_ltrim_mask_done_linux_x86_64");               // stop trimming once the current leading source byte is not present in the trim mask
    emitter.instruction("movzx r9d, BYTE PTR [rdi + r8]");                      // load one trim-mask byte so ltrim_mask() can compare it against the current leading source byte
    emitter.instruction("cmp cl, r9b");                                         // does the current trim-mask byte match the current leading source byte?
    emitter.instruction("je __rt_ltrim_mask_skip_linux_x86_64");                // trim the current leading source byte when the mask contains it
    emitter.instruction("add r8, 1");                                           // advance to the next trim-mask byte after a non-matching mask comparison
    emitter.instruction("jmp __rt_ltrim_mask_cmp_linux_x86_64");                // continue scanning the trim-mask bytes until one matches or the mask is exhausted

    emitter.label("__rt_ltrim_mask_skip_linux_x86_64");
    emitter.instruction("add rax, 1");                                          // advance the borrowed source string pointer past one leading byte that matched the trim mask
    emitter.instruction("sub rdx, 1");                                          // shrink the borrowed source string length after trimming one leading byte
    emitter.instruction("jmp __rt_ltrim_mask_loop_linux_x86_64");               // continue trimming from the new front of the source string slice

    emitter.label("__rt_ltrim_mask_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return the adjusted borrowed source string slice in the standard x86_64 string result registers
}
