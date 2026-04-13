use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// str_eq: compare two strings for equality.
/// Input:  x1=ptr_a, x2=len_a, x3=ptr_b, x4=len_b
/// Output: x0 = 1 if equal, 0 if not
pub fn emit_str_eq(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_eq_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_eq ---");
    emitter.label_global("__rt_str_eq");

    // -- quick length check: different lengths means not equal --
    emitter.instruction("cmp x2, x4");                                          // compare lengths of both strings
    emitter.instruction("b.ne __rt_str_eq_false");                              // if lengths differ, strings can't be equal

    // -- byte-by-byte comparison --
    emitter.instruction("cbz x2, __rt_str_eq_true");                            // if both empty (len=0), they're equal
    emitter.label("__rt_str_eq_loop");
    emitter.instruction("ldrb w5, [x1], #1");                                   // load byte from string A, advance pointer
    emitter.instruction("ldrb w6, [x3], #1");                                   // load byte from string B, advance pointer
    emitter.instruction("cmp w5, w6");                                          // compare the two bytes
    emitter.instruction("b.ne __rt_str_eq_false");                              // mismatch found, strings not equal
    emitter.instruction("sub x2, x2, #1");                                      // decrement remaining byte count
    emitter.instruction("cbnz x2, __rt_str_eq_loop");                           // if bytes remain, continue comparing

    // -- strings are equal --
    emitter.label("__rt_str_eq_true");
    emitter.instruction("mov x0, #1");                                          // return 1 (true: strings are equal)
    emitter.instruction("ret");                                                 // return to caller

    // -- strings are not equal --
    emitter.label("__rt_str_eq_false");
    emitter.instruction("mov x0, #0");                                          // return 0 (false: strings differ)
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_str_eq_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_eq ---");
    emitter.label_global("__rt_str_eq");

    emitter.instruction("cmp rsi, rcx");                                        // compare both string lengths before touching the byte payloads
    emitter.instruction("jne __rt_str_eq_false");                               // unequal lengths cannot represent equal strings
    emitter.instruction("test rsi, rsi");                                       // are both strings empty after the length comparison?
    emitter.instruction("je __rt_str_eq_true");                                 // zero-length strings are equal without entering the byte loop

    emitter.label("__rt_str_eq_loop");
    emitter.instruction("movzx r8d, BYTE PTR [rdi]");                           // load the next byte from the left-hand string
    emitter.instruction("movzx r9d, BYTE PTR [rdx]");                           // load the next byte from the right-hand string
    emitter.instruction("cmp r8b, r9b");                                        // compare the current byte pair
    emitter.instruction("jne __rt_str_eq_false");                               // return false as soon as a mismatching byte is found
    emitter.instruction("add rdi, 1");                                          // advance the left-hand string pointer to the next byte
    emitter.instruction("add rdx, 1");                                          // advance the right-hand string pointer to the next byte
    emitter.instruction("sub rsi, 1");                                          // decrement the remaining byte count after one successful comparison
    emitter.instruction("jne __rt_str_eq_loop");                                // continue until every byte has matched

    emitter.label("__rt_str_eq_true");
    emitter.instruction("mov rax, 1");                                          // return 1 to signal that both strings are equal
    emitter.instruction("ret");                                                 // return to the caller after the equality fast path or completed byte loop

    emitter.label("__rt_str_eq_false");
    emitter.instruction("xor rax, rax");                                        // return 0 to signal that the strings differ
    emitter.instruction("ret");                                                 // return immediately after the mismatch or length check failure
}
