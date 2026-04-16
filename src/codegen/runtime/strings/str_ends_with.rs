use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// str_ends_with: check if haystack ends with needle.
pub fn emit_str_ends_with(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_ends_with_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_ends_with ---");
    emitter.label_global("__rt_str_ends_with");

    // -- check if needle fits in haystack --
    emitter.instruction("cmp x4, x2");                                          // compare needle length with haystack length
    emitter.instruction("b.gt __rt_str_ends_with_no");                          // needle longer than haystack, can't match
    emitter.instruction("sub x5, x2, x4");                                      // compute offset where suffix starts
    emitter.instruction("mov x6, #0");                                          // initialize comparison index

    // -- compare suffix bytes --
    emitter.label("__rt_str_ends_with_loop");
    emitter.instruction("cmp x6, x4");                                          // check if all needle bytes compared
    emitter.instruction("b.ge __rt_str_ends_with_yes");                         // all matched, haystack ends with needle
    emitter.instruction("add x7, x5, x6");                                      // compute haystack index = offset + idx
    emitter.instruction("ldrb w8, [x1, x7]");                                   // load haystack byte at suffix position
    emitter.instruction("ldrb w9, [x3, x6]");                                   // load needle byte at index
    emitter.instruction("cmp w8, w9");                                          // compare the two bytes
    emitter.instruction("b.ne __rt_str_ends_with_no");                          // mismatch, does not end with needle
    emitter.instruction("add x6, x6, #1");                                      // advance to next byte
    emitter.instruction("b __rt_str_ends_with_loop");                           // continue comparing

    // -- return results --
    emitter.label("__rt_str_ends_with_yes");
    emitter.instruction("mov x0, #1");                                          // return 1 (true: ends with needle)
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_str_ends_with_no");
    emitter.instruction("mov x0, #0");                                          // return 0 (false: does not end with)
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_str_ends_with_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_ends_with ---");
    emitter.label_global("__rt_str_ends_with");

    emitter.instruction("cmp rcx, rsi");                                        // reject suffixes whose byte length exceeds the haystack length
    emitter.instruction("jg __rt_str_ends_with_no_linux_x86_64");               // return false when the suffix cannot fit at the end of the haystack
    emitter.instruction("mov r8, rsi");                                         // copy the haystack length so the suffix start offset can be computed once
    emitter.instruction("sub r8, rcx");                                         // compute the haystack offset where the suffix comparison should begin
    emitter.instruction("xor r9d, r9d");                                        // start comparing bytes from the beginning of the suffix

    emitter.label("__rt_str_ends_with_loop_linux_x86_64");
    emitter.instruction("cmp r9, rcx");                                         // have all suffix bytes matched so far?
    emitter.instruction("jge __rt_str_ends_with_yes_linux_x86_64");             // return true once every suffix byte has matched
    emitter.instruction("mov r10, r8");                                         // copy the suffix start offset so the indexed haystack byte can be addressed
    emitter.instruction("add r10, r9");                                         // compute the absolute haystack byte offset for the current suffix byte
    emitter.instruction("movzx eax, BYTE PTR [rdi + r10]");                     // load the haystack byte at the current suffix offset
    emitter.instruction("movzx r11d, BYTE PTR [rdx + r9]");                     // load the suffix byte at the current suffix offset
    emitter.instruction("cmp eax, r11d");                                       // compare the haystack and suffix bytes at the current offset
    emitter.instruction("jne __rt_str_ends_with_no_linux_x86_64");              // return false on the first byte mismatch
    emitter.instruction("add r9, 1");                                           // advance to the next suffix byte after a successful comparison
    emitter.instruction("jmp __rt_str_ends_with_loop_linux_x86_64");            // continue checking the remaining suffix bytes

    emitter.label("__rt_str_ends_with_yes_linux_x86_64");
    emitter.instruction("mov eax, 1");                                          // return true when the haystack ends with the full suffix
    emitter.instruction("ret");                                                 // return the boolean success result to the caller

    emitter.label("__rt_str_ends_with_no_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // return false when the suffix does not match the end of the haystack
    emitter.instruction("ret");                                                 // return the boolean failure result to the caller
}
