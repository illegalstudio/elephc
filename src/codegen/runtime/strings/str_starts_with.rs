use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// str_starts_with: check if haystack starts with needle.
/// Input: x1/x2=haystack, x3/x4=needle
/// Output: x0 = 1 if starts with, 0 otherwise
pub fn emit_str_starts_with(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_starts_with_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_starts_with ---");
    emitter.label_global("__rt_str_starts_with");

    // -- check if needle fits in haystack --
    emitter.instruction("cmp x4, x2");                                          // compare needle length with haystack length
    emitter.instruction("b.gt __rt_str_starts_with_no");                        // needle longer than haystack, can't match
    emitter.instruction("mov x5, #0");                                          // initialize comparison index

    // -- compare prefix bytes --
    emitter.label("__rt_str_starts_with_loop");
    emitter.instruction("cmp x5, x4");                                          // check if all needle bytes compared
    emitter.instruction("b.ge __rt_str_starts_with_yes");                       // all matched, haystack starts with needle
    emitter.instruction("ldrb w6, [x1, x5]");                                   // load haystack byte at index
    emitter.instruction("ldrb w7, [x3, x5]");                                   // load needle byte at index
    emitter.instruction("cmp w6, w7");                                          // compare the two bytes
    emitter.instruction("b.ne __rt_str_starts_with_no");                        // mismatch, does not start with needle
    emitter.instruction("add x5, x5, #1");                                      // advance to next byte
    emitter.instruction("b __rt_str_starts_with_loop");                         // continue comparing

    // -- return results --
    emitter.label("__rt_str_starts_with_yes");
    emitter.instruction("mov x0, #1");                                          // return 1 (true: starts with needle)
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_str_starts_with_no");
    emitter.instruction("mov x0, #0");                                          // return 0 (false: does not start with)
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_str_starts_with_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_starts_with ---");
    emitter.label_global("__rt_str_starts_with");

    emitter.instruction("cmp rcx, rsi");                                        // reject prefixes whose byte length exceeds the haystack length
    emitter.instruction("jg __rt_str_starts_with_no_linux_x86_64");             // return false when the prefix cannot fit at the start of the haystack
    emitter.instruction("xor r8d, r8d");                                        // start comparing bytes from offset zero

    emitter.label("__rt_str_starts_with_loop_linux_x86_64");
    emitter.instruction("cmp r8, rcx");                                         // have all prefix bytes matched so far?
    emitter.instruction("jge __rt_str_starts_with_yes_linux_x86_64");           // return true once every prefix byte has matched
    emitter.instruction("movzx eax, BYTE PTR [rdi + r8]");                      // load the haystack byte at the current prefix offset
    emitter.instruction("movzx r9d, BYTE PTR [rdx + r8]");                      // load the prefix byte at the current prefix offset
    emitter.instruction("cmp eax, r9d");                                        // compare the haystack and prefix bytes at the current offset
    emitter.instruction("jne __rt_str_starts_with_no_linux_x86_64");            // return false on the first byte mismatch
    emitter.instruction("add r8, 1");                                           // advance to the next prefix byte after a successful comparison
    emitter.instruction("jmp __rt_str_starts_with_loop_linux_x86_64");          // continue checking the remaining prefix bytes

    emitter.label("__rt_str_starts_with_yes_linux_x86_64");
    emitter.instruction("mov eax, 1");                                          // return true when the haystack begins with the full prefix
    emitter.instruction("ret");                                                 // return the boolean success result to the caller

    emitter.label("__rt_str_starts_with_no_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // return false when the prefix does not match the start of the haystack
    emitter.instruction("ret");                                                 // return the boolean failure result to the caller
}
