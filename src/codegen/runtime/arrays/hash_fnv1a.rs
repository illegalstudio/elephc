use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// hash_fnv1a: FNV-1a 64-bit hash function.
/// Input:  x1=ptr, x2=len
/// Output: x0=hash (64-bit)
pub fn emit_hash_fnv1a(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_fnv1a_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_fnv1a ---");
    emitter.label_global("__rt_hash_fnv1a");

    // -- load FNV offset basis into x0 (0xcbf29ce484222325) --
    emitter.instruction("movz x0, #0x2325");                                    // hash[15:0] = 0x2325
    emitter.instruction("movk x0, #0x8422, lsl #16");                           // hash[31:16] = 0x8422
    emitter.instruction("movk x0, #0x9ce4, lsl #32");                           // hash[47:32] = 0x9ce4
    emitter.instruction("movk x0, #0xcbf2, lsl #48");                           // hash[63:48] = 0xcbf2

    // -- load FNV prime into x9 (0x00000100000001B3) --
    emitter.instruction("movz x9, #0x01B3");                                    // prime[15:0] = 0x01B3
    emitter.instruction("movk x9, #0x0000, lsl #16");                           // prime[31:16] = 0x0000
    emitter.instruction("movk x9, #0x0100, lsl #32");                           // prime[47:32] = 0x0100
    emitter.instruction("movk x9, #0x0000, lsl #48");                           // prime[63:48] = 0x0000

    // -- hash each byte: hash = (hash ^ byte) * prime --
    emitter.label("__rt_hash_fnv1a_loop");
    emitter.instruction("cbz x2, __rt_hash_fnv1a_done");                        // if no bytes remain, return hash
    emitter.instruction("ldrb w10, [x1], #1");                                  // load next byte from string, advance pointer
    emitter.instruction("eor x0, x0, x10");                                     // hash ^= byte
    emitter.instruction("mul x0, x0, x9");                                      // hash *= FNV prime
    emitter.instruction("sub x2, x2, #1");                                      // decrement remaining byte count
    emitter.instruction("b __rt_hash_fnv1a_loop");                              // continue to next byte

    // -- return hash in x0 --
    emitter.label("__rt_hash_fnv1a_done");
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_hash_fnv1a_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_fnv1a ---");
    emitter.label_global("__rt_hash_fnv1a");

    emitter.instruction("mov rax, 14695981039346656037");                       // load the 64-bit FNV-1a offset basis into the return register
    emitter.instruction("mov r8, 1099511628211");                               // load the 64-bit FNV prime into a caller-saved scratch register

    emitter.label("__rt_hash_fnv1a_loop");
    emitter.instruction("test rsi, rsi");                                       // stop once every input byte has been folded into the hash
    emitter.instruction("je __rt_hash_fnv1a_done");                             // return immediately when the remaining byte count reaches zero
    emitter.instruction("movzx ecx, BYTE PTR [rdi]");                           // load the next input byte and zero-extend it for the xor step
    emitter.instruction("xor rax, rcx");                                        // fold the next byte into the running FNV-1a hash state
    emitter.instruction("imul rax, r8");                                        // multiply by the fixed FNV prime to advance the hash state
    emitter.instruction("add rdi, 1");                                          // advance the source pointer to the next byte
    emitter.instruction("sub rsi, 1");                                          // decrement the remaining byte count after consuming one input byte
    emitter.instruction("jmp __rt_hash_fnv1a_loop");                            // continue hashing until the input buffer is exhausted

    emitter.label("__rt_hash_fnv1a_done");
    emitter.instruction("ret");                                                 // return the completed 64-bit hash in rax
}
