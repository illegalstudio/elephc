use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// rtrim_mask: strip characters in mask from right of string.
/// Input: x1=str_ptr, x2=str_len, x3=mask_ptr, x4=mask_len
/// Output: x1=str_ptr (unchanged), x2=adjusted_len
pub fn emit_rtrim_mask(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_rtrim_mask_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: rtrim_mask ---");
    emitter.label_global("__rt_rtrim_mask");

    // -- loop: check last character against mask --
    emitter.label("__rt_rtrim_mask_loop");
    emitter.instruction("cbz x2, __rt_rtrim_mask_done");                        // if string is empty, nothing to trim
    emitter.instruction("sub x9, x2, #1");                                      // compute index of last character
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load last byte of string

    // -- inner loop: check if w10 is in mask --
    emitter.instruction("mov x11, #0");                                         // mask index = 0
    emitter.label("__rt_rtrim_mask_cmp");
    emitter.instruction("cmp x11, x4");                                         // check if we've exhausted the mask
    emitter.instruction("b.ge __rt_rtrim_mask_done");                           // char not in mask, stop trimming
    emitter.instruction("ldrb w12, [x3, x11]");                                 // load mask character at index x11
    emitter.instruction("cmp w10, w12");                                        // compare string char with mask char
    emitter.instruction("b.eq __rt_rtrim_mask_strip");                          // match found, strip this character
    emitter.instruction("add x11, x11, #1");                                    // advance mask index
    emitter.instruction("b __rt_rtrim_mask_cmp");                               // check next mask character

    // -- strip: shrink length and re-check --
    emitter.label("__rt_rtrim_mask_strip");
    emitter.instruction("sub x2, x2, #1");                                      // reduce length by 1
    emitter.instruction("b __rt_rtrim_mask_loop");                              // check new last character

    emitter.label("__rt_rtrim_mask_done");
    emitter.instruction("ret");                                                 // return with adjusted x2
}

fn emit_rtrim_mask_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: rtrim_mask ---");
    emitter.label_global("__rt_rtrim_mask");

    emitter.label("__rt_rtrim_mask_loop_linux_x86_64");
    emitter.instruction("test rdx, rdx");                                       // stop once the borrowed source string slice has become empty after trimming from the right
    emitter.instruction("jz __rt_rtrim_mask_done_linux_x86_64");                // return immediately when there are no bytes left to classify against the trim mask
    emitter.instruction("mov rcx, rdx");                                        // copy the borrowed source string length so rtrim_mask() can inspect the current trailing-byte index
    emitter.instruction("sub rcx, 1");                                          // compute the index of the current trailing source byte before scanning the trim mask
    emitter.instruction("movzx r8d, BYTE PTR [rax + rcx]");                     // load the current trailing source byte so rtrim_mask() can check whether the mask contains it
    emitter.instruction("xor rcx, rcx");                                        // reset the trim-mask index before scanning the mask bytes for a match with the current trailing source byte

    emitter.label("__rt_rtrim_mask_cmp_linux_x86_64");
    emitter.instruction("cmp rcx, rsi");                                        // have we exhausted the trim-mask bytes without matching the current trailing source byte?
    emitter.instruction("jae __rt_rtrim_mask_done_linux_x86_64");               // stop trimming once the current trailing source byte is not present in the trim mask
    emitter.instruction("movzx r9d, BYTE PTR [rdi + rcx]");                     // load one trim-mask byte so rtrim_mask() can compare it against the current trailing source byte
    emitter.instruction("cmp r8b, r9b");                                        // does the current trim-mask byte match the current trailing source byte?
    emitter.instruction("je __rt_rtrim_mask_strip_linux_x86_64");               // trim the current trailing source byte when the mask contains it
    emitter.instruction("add rcx, 1");                                          // advance to the next trim-mask byte after a non-matching mask comparison
    emitter.instruction("jmp __rt_rtrim_mask_cmp_linux_x86_64");                // continue scanning the trim-mask bytes until one matches or the mask is exhausted

    emitter.label("__rt_rtrim_mask_strip_linux_x86_64");
    emitter.instruction("sub rdx, 1");                                          // shrink the borrowed source string length after trimming one trailing byte that matched the trim mask
    emitter.instruction("jmp __rt_rtrim_mask_loop_linux_x86_64");               // continue trimming from the new end of the source string slice

    emitter.label("__rt_rtrim_mask_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return the adjusted borrowed source string slice in the standard x86_64 string result registers
}
