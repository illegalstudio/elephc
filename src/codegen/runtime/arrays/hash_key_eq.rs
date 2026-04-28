use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// hash_key_eq: compare two normalized associative-array keys.
/// Input:  x1=left_lo, x2=left_hi, x3=right_lo, x4=right_hi
/// Output: x0=1 when equal, 0 otherwise
pub fn emit_hash_key_eq(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_key_eq_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_key_eq ---");
    emitter.label_global("__rt_hash_key_eq");

    emitter.instruction("cmn x2, #1");                                          // check whether the left key is an integer key
    emitter.instruction("b.eq __rt_hash_key_eq_left_int");                      // integer keys compare by signed payload
    emitter.instruction("cmn x4, #1");                                          // check whether the right key is an integer key
    emitter.instruction("b.eq __rt_hash_key_eq_false");                         // string and integer keys are never equal after normalization
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the caller frame before delegating to string equality
    emitter.instruction("mov x29, sp");                                         // establish a minimal frame for the nested call
    emitter.instruction("bl __rt_str_eq");                                      // compare two normalized string keys byte-for-byte
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame after string equality returns
    emitter.instruction("ret");                                                 // return the string-key equality result

    emitter.label("__rt_hash_key_eq_left_int");
    emitter.instruction("cmn x4, #1");                                          // check whether the right key is also an integer key
    emitter.instruction("b.ne __rt_hash_key_eq_false");                         // integer keys cannot equal string keys
    emitter.instruction("cmp x1, x3");                                          // compare the signed integer key payloads
    emitter.instruction("cset x0, eq");                                         // return 1 only when the integer payloads are identical
    emitter.instruction("ret");                                                 // return the integer-key equality result

    emitter.label("__rt_hash_key_eq_false");
    emitter.instruction("mov x0, #0");                                          // materialize false for mismatched key kinds
    emitter.instruction("ret");                                                 // return false to the hash probe caller
}

fn emit_hash_key_eq_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_key_eq ---");
    emitter.label_global("__rt_hash_key_eq");

    emitter.instruction("cmp rsi, -1");                                         // check whether the left key is an integer key
    emitter.instruction("je __rt_hash_key_eq_left_int");                        // integer keys compare by signed payload
    emitter.instruction("cmp rcx, -1");                                         // check whether the right key is an integer key
    emitter.instruction("je __rt_hash_key_eq_false");                           // string and integer keys are never equal after normalization
    emitter.instruction("call __rt_str_eq");                                    // compare two normalized string keys byte-for-byte
    emitter.instruction("ret");                                                 // return the string-key equality result

    emitter.label("__rt_hash_key_eq_left_int");
    emitter.instruction("cmp rcx, -1");                                         // check whether the right key is also an integer key
    emitter.instruction("jne __rt_hash_key_eq_false");                          // integer keys cannot equal string keys
    emitter.instruction("cmp rdi, rdx");                                        // compare the signed integer key payloads
    emitter.instruction("sete al");                                             // encode the equality result in the low return byte
    emitter.instruction("movzx eax, al");                                       // zero-extend the boolean result to the full integer result register
    emitter.instruction("ret");                                                 // return the integer-key equality result

    emitter.label("__rt_hash_key_eq_false");
    emitter.instruction("xor eax, eax");                                        // materialize false for mismatched key kinds
    emitter.instruction("ret");                                                 // return false to the hash probe caller
}
