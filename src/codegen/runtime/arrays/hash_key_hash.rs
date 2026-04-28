use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// hash_key_hash: hash a normalized associative-array key.
/// Input:  x1=key_lo, x2=key_hi (-1 means integer key)
/// Output: x0=hash
pub fn emit_hash_key_hash(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_key_hash_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_key_hash ---");
    emitter.label_global("__rt_hash_key_hash");

    emitter.instruction("cmn x2, #1");                                          // check whether key_hi is the integer-key sentinel
    emitter.instruction("b.eq __rt_hash_key_hash_int");                         // integer keys use a scalar hash path
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the caller frame before delegating to the string hash
    emitter.instruction("mov x29, sp");                                         // establish a minimal frame for the nested call
    emitter.instruction("bl __rt_hash_fnv1a");                                  // hash the string key using the existing byte-wise FNV-1a helper
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame after the nested helper returns
    emitter.instruction("ret");                                                 // return the string hash to the caller

    emitter.label("__rt_hash_key_hash_int");
    emitter.instruction("mov x0, x1");                                          // seed the hash from the signed integer key payload
    emitter.instruction("eor x0, x0, x0, lsr #33");                             // mix high integer bits into low bits before multiplication
    emitter.instruction("movz x9, #0x7c15");                                    // materialize the low 16 bits of the integer hash multiplier
    emitter.instruction("movk x9, #0x7f4a, lsl #16");                           // materialize multiplier bits 31:16
    emitter.instruction("movk x9, #0x79b9, lsl #32");                           // materialize multiplier bits 47:32
    emitter.instruction("movk x9, #0x9e37, lsl #48");                           // materialize multiplier bits 63:48
    emitter.instruction("mul x0, x0, x9");                                      // multiply by an odd 64-bit constant to spread nearby integer keys
    emitter.instruction("ret");                                                 // return the integer-key hash to the caller
}

fn emit_hash_key_hash_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_key_hash ---");
    emitter.label_global("__rt_hash_key_hash");

    emitter.instruction("cmp rsi, -1");                                         // check whether key_hi is the integer-key sentinel
    emitter.instruction("je __rt_hash_key_hash_int");                           // integer keys use a scalar hash path
    emitter.instruction("call __rt_hash_fnv1a");                                // hash the string key using the existing byte-wise FNV-1a helper
    emitter.instruction("ret");                                                 // return the string hash to the caller

    emitter.label("__rt_hash_key_hash_int");
    emitter.instruction("mov rax, rdi");                                        // seed the hash from the signed integer key payload
    emitter.instruction("mov rcx, rax");                                        // copy the key so high bits can be mixed into the running hash
    emitter.instruction("shr rcx, 33");                                         // isolate high integer bits for the xor-mix step
    emitter.instruction("xor rax, rcx");                                        // mix high integer bits into low bits before multiplication
    emitter.instruction("mov rcx, -7046029254386353131");                       // load an odd 64-bit multiplier for nearby integer-key dispersion
    emitter.instruction("imul rax, rcx");                                       // spread nearby integer keys across the hash table probe space
    emitter.instruction("ret");                                                 // return the integer-key hash to the caller
}
