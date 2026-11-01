//! Purpose:
//! Emits the `__rt_hash_bytes` runtime helper: the string-key hash of PHP array buckets.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//! - `__rt_hash_key_hash`, its only caller, for every string key a hash operation looks up.
//!
//! Key details:
//! - The value only chooses a bucket: PHP iteration order comes from the insertion links, so the
//!   function can change freely as long as every table is built and probed with the same one. No
//!   table is laid out at compile time with it (callable tables and labels have their own FNV).
//! - Callers treat the helper as frameless with a fixed clobber set -- `x0`/`x1`/`x2`/`x9`/`x10` on
//!   AArch64, `rax`/`rdi`/`rsi`/`rcx`/`r8` on x86_64 -- which the byte-wise FNV-1a it replaced
//!   defined, so the body stays inside that set.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_hash_bytes`.
///
/// Consumes eight bytes per step (`h = (h ^ word) * K; h ^= h >> 29`), finishes the at most seven
/// remaining bytes one at a time, and ends with an avalanche so the LOW bits -- the ones a
/// power-of-two bucket index reads -- depend on every input byte. The byte-wise FNV-1a it replaced
/// spent six instructions and a serial multiply per byte; a 20-byte key now takes about 40% of that.
///
/// # ARM64 calling convention
/// - **Input**: `x1` = pointer to byte buffer, `x2` = byte length
/// - **Output**: `x0` = 64-bit hash value
///
/// # x86_64 Linux calling convention
/// - **Input**: `rdi` = pointer to byte buffer, `rsi` = byte length
/// - **Output**: `rax` = 64-bit hash value
pub fn emit_hash_bytes(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_bytes_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_bytes ---");
    emitter.label_global("__rt_hash_bytes");

    emitter.instruction("movz x9, #0x7c15");                                    // K = 0x9E3779B97F4A7C15, the odd golden-ratio multiplier
    emitter.instruction("movk x9, #0x7f4a, lsl #16");
    emitter.instruction("movk x9, #0x79b9, lsl #32");
    emitter.instruction("movk x9, #0x9e37, lsl #48");
    emitter.instruction("movz x0, #0x08d3");                                    // seed = 0x243F6A8885A308D3
    emitter.instruction("movk x0, #0x85a3, lsl #16");
    emitter.instruction("movk x0, #0x6a88, lsl #32");
    emitter.instruction("movk x0, #0x243f, lsl #48");
    emitter.instruction("eor x0, x0, x2");                                      // the length separates keys that share a prefix
    emitter.instruction("mul x0, x0, x9");

    emitter.label("__rt_hash_bytes_word");
    emitter.instruction("cmp x2, #8");                                          // a whole 8-byte word left?
    emitter.instruction("b.lo __rt_hash_bytes_tail");
    emitter.instruction("ldr x10, [x1], #8");                                   // next word, unaligned loads are fine here
    emitter.instruction("eor x0, x0, x10");
    emitter.instruction("mul x0, x0, x9");
    emitter.instruction("eor x0, x0, x0, lsr #29");                             // pull the product's high bits back down
    emitter.instruction("sub x2, x2, #8");
    emitter.instruction("b __rt_hash_bytes_word");

    emitter.label("__rt_hash_bytes_tail");
    emitter.instruction("cbz x2, __rt_hash_bytes_final");                       // no byte left
    emitter.instruction("ldrb w10, [x1], #1");                                  // one of at most seven trailing bytes
    emitter.instruction("eor x0, x0, x10");
    emitter.instruction("mul x0, x0, x9");
    emitter.instruction("sub x2, x2, #1");
    emitter.instruction("b __rt_hash_bytes_tail");

    emitter.label("__rt_hash_bytes_final");
    emitter.instruction("eor x0, x0, x0, lsr #32");                             // avalanche: every input bit reaches the low bits
    emitter.instruction("mul x0, x0, x9");
    emitter.instruction("eor x0, x0, x0, lsr #29");
    emitter.instruction("ret");
}

/// Emits the x86_64 Linux variant of `__rt_hash_bytes`, the same function over `rdi`/`rsi`.
fn emit_hash_bytes_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_bytes ---");
    emitter.label_global("__rt_hash_bytes");

    emitter.instruction("mov r8, 11400714819323198485");                        // K = 0x9E3779B97F4A7C15, the odd golden-ratio multiplier
    emitter.instruction("mov rax, 2611923443488327891");                         // seed = 0x243F6A8885A308D3
    emitter.instruction("xor rax, rsi");                                        // the length separates keys that share a prefix
    emitter.instruction("imul rax, r8");

    emitter.label("__rt_hash_bytes_word");
    emitter.instruction("cmp rsi, 8");                                          // a whole 8-byte word left?
    emitter.instruction("jb __rt_hash_bytes_tail");
    emitter.instruction("mov rcx, QWORD PTR [rdi]");                            // next word
    emitter.instruction("xor rax, rcx");
    emitter.instruction("imul rax, r8");
    emitter.instruction("mov rcx, rax");
    emitter.instruction("shr rcx, 29");
    emitter.instruction("xor rax, rcx");                                        // pull the product's high bits back down
    emitter.instruction("add rdi, 8");
    emitter.instruction("sub rsi, 8");
    emitter.instruction("jmp __rt_hash_bytes_word");

    emitter.label("__rt_hash_bytes_tail");
    emitter.instruction("test rsi, rsi");                                       // no byte left?
    emitter.instruction("jz __rt_hash_bytes_final");
    emitter.instruction("movzx ecx, BYTE PTR [rdi]");                           // one of at most seven trailing bytes
    emitter.instruction("xor rax, rcx");
    emitter.instruction("imul rax, r8");
    emitter.instruction("add rdi, 1");
    emitter.instruction("sub rsi, 1");
    emitter.instruction("jmp __rt_hash_bytes_tail");

    emitter.label("__rt_hash_bytes_final");
    emitter.instruction("mov rcx, rax");                                        // avalanche: every input bit reaches the low bits
    emitter.instruction("shr rcx, 32");
    emitter.instruction("xor rax, rcx");
    emitter.instruction("imul rax, r8");
    emitter.instruction("mov rcx, rax");
    emitter.instruction("shr rcx, 29");
    emitter.instruction("xor rax, rcx");
    emitter.instruction("ret");
}
