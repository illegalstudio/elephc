use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// strcasecmp: case-insensitive string comparison.
pub fn emit_strcasecmp(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strcasecmp_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: strcasecmp ---");
    emitter.label_global("__rt_strcasecmp");

    // -- determine minimum length for comparison --
    emitter.instruction("cmp x2, x4");                                          // compare lengths of both strings
    emitter.instruction("csel x5, x2, x4, lt");                                 // x5 = min(len_a, len_b)
    emitter.instruction("mov x6, #0");                                          // initialize byte index to 0

    // -- compare bytes (case-insensitive) --
    emitter.label("__rt_strcasecmp_loop");
    emitter.instruction("cmp x6, x5");                                          // check if we've compared all min-length bytes
    emitter.instruction("b.ge __rt_strcasecmp_len");                            // if done, compare by string lengths
    emitter.instruction("ldrb w7, [x1, x6]");                                   // load byte from string A at index
    emitter.instruction("ldrb w8, [x3, x6]");                                   // load byte from string B at index

    // -- convert byte A to lowercase if uppercase --
    emitter.instruction("cmp w7, #65");                                         // compare with 'A'
    emitter.instruction("b.lt __rt_strcasecmp_b");                              // if below 'A', skip conversion
    emitter.instruction("cmp w7, #90");                                         // compare with 'Z'
    emitter.instruction("b.gt __rt_strcasecmp_b");                              // if above 'Z', skip conversion
    emitter.instruction("add w7, w7, #32");                                     // convert A-Z to a-z

    // -- convert byte B to lowercase if uppercase --
    emitter.label("__rt_strcasecmp_b");
    emitter.instruction("cmp w8, #65");                                         // compare with 'A'
    emitter.instruction("b.lt __rt_strcasecmp_cmp");                            // if below 'A', skip conversion
    emitter.instruction("cmp w8, #90");                                         // compare with 'Z'
    emitter.instruction("b.gt __rt_strcasecmp_cmp");                            // if above 'Z', skip conversion
    emitter.instruction("add w8, w8, #32");                                     // convert A-Z to a-z

    // -- compare lowered bytes --
    emitter.label("__rt_strcasecmp_cmp");
    emitter.instruction("cmp w7, w8");                                          // compare the two lowered bytes
    emitter.instruction("b.ne __rt_strcasecmp_diff");                           // if different, return their difference
    emitter.instruction("add x6, x6, #1");                                      // advance to next byte
    emitter.instruction("b __rt_strcasecmp_loop");                              // continue comparing

    // -- bytes differ: return difference --
    emitter.label("__rt_strcasecmp_diff");
    emitter.instruction("sub x0, x7, x8");                                      // return lowered_a - lowered_b
    emitter.instruction("ret");                                                 // return to caller

    // -- all shared bytes equal: compare by length --
    emitter.label("__rt_strcasecmp_len");
    emitter.instruction("sub x0, x2, x4");                                      // return len_a - len_b as tiebreaker
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_strcasecmp_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strcasecmp ---");
    emitter.label_global("__rt_strcasecmp");

    emitter.instruction("mov r8, rsi");                                         // seed the shared-length comparison bound from the first string length
    emitter.instruction("cmp rsi, rcx");                                        // compare both string lengths to determine the shorter shared prefix
    emitter.instruction("cmovg r8, rcx");                                       // clamp the shared-length comparison bound to the shorter string length
    emitter.instruction("xor r9d, r9d");                                        // start comparing bytes from offset zero

    emitter.label("__rt_strcasecmp_loop_linux_x86_64");
    emitter.instruction("cmp r9, r8");                                          // have we compared every byte in the shared prefix?
    emitter.instruction("jge __rt_strcasecmp_len_linux_x86_64");                // fall back to comparing string lengths once the shared prefix is exhausted
    emitter.instruction("movzx rax, BYTE PTR [rdi + r9]");                      // load the current byte from the first string
    emitter.instruction("movzx r10, BYTE PTR [rdx + r9]");                      // load the current byte from the second string
    emitter.instruction("cmp al, 65");                                          // is the first string byte an uppercase ASCII letter at or above 'A'?
    emitter.instruction("jb __rt_strcasecmp_second_linux_x86_64");              // skip lowercasing when the first string byte is below 'A'
    emitter.instruction("cmp al, 90");                                          // is the first string byte above the uppercase ASCII range?
    emitter.instruction("ja __rt_strcasecmp_second_linux_x86_64");              // skip lowercasing when the first string byte is above 'Z'
    emitter.instruction("add al, 32");                                          // lowercase the uppercase ASCII byte from the first string

    emitter.label("__rt_strcasecmp_second_linux_x86_64");
    emitter.instruction("cmp r10b, 65");                                        // is the second string byte an uppercase ASCII letter at or above 'A'?
    emitter.instruction("jb __rt_strcasecmp_cmp_linux_x86_64");                 // skip lowercasing when the second string byte is below 'A'
    emitter.instruction("cmp r10b, 90");                                        // is the second string byte above the uppercase ASCII range?
    emitter.instruction("ja __rt_strcasecmp_cmp_linux_x86_64");                 // skip lowercasing when the second string byte is above 'Z'
    emitter.instruction("add r10b, 32");                                        // lowercase the uppercase ASCII byte from the second string

    emitter.label("__rt_strcasecmp_cmp_linux_x86_64");
    emitter.instruction("cmp rax, r10");                                        // compare the lowercased bytes from both strings
    emitter.instruction("jne __rt_strcasecmp_diff_linux_x86_64");               // return the byte difference on the first mismatch
    emitter.instruction("add r9, 1");                                           // advance to the next shared-prefix byte after an equal comparison
    emitter.instruction("jmp __rt_strcasecmp_loop_linux_x86_64");               // continue comparing the remaining shared-prefix bytes

    emitter.label("__rt_strcasecmp_diff_linux_x86_64");
    emitter.instruction("sub rax, r10");                                        // return the signed lowercased-byte difference for the first mismatching character pair
    emitter.instruction("ret");                                                 // return the byte-difference result to the caller

    emitter.label("__rt_strcasecmp_len_linux_x86_64");
    emitter.instruction("mov rax, rsi");                                        // seed the length-tiebreak result from the first string length
    emitter.instruction("sub rax, rcx");                                        // return the signed length difference once the shared prefix is equal
    emitter.instruction("ret");                                                 // return the length-difference result to the caller
}
