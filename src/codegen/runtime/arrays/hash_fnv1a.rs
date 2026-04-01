use crate::codegen::emit::Emitter;

/// hash_fnv1a: FNV-1a 64-bit hash function.
/// Input:  x1=ptr, x2=len
/// Output: x0=hash (64-bit)
pub fn emit_hash_fnv1a(emitter: &mut Emitter) {
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
