//! Purpose:
//! Emits php's Mersenne Twister: `__rt_mt_seed`, `__rt_mt_reload`, `__rt_mt_u32` and
//! `__rt_mt_rand_common`, the engine behind `mt_srand()` / `srand()`, `mt_rand()` / `rand()`,
//! `shuffle()` and `array_rand()`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - A transcription of php-src's `ext/random/engine_mt19937.c`: the Knuth seeding loop, the
//!   twist (`loBit(v)` for `MT_RAND_MT19937`, `loBit(u)` for the deprecated `MT_RAND_PHP`), and
//!   the tempering. php's two reload loops index `p[M]` and then `p[M - N]`, which is the single
//!   loop over `k` below with `(k + 397) mod 624` and `(k + 1) mod 624`, updated in place.
//! - Until a script seeds it, php seeds the engine from the CSPRNG on first use, so an unseeded
//!   draw here simply comes from `__rt_random_u32`. `_mt_mode` is 0 while unseeded, 1 for
//!   `MT_RAND_MT19937` and 2 for `MT_RAND_PHP`; `--web` clears it per request, as php's request
//!   globals are.
//! - `random_int()` stays on the CSPRNG chain: php never draws it from the Mersenne Twister.
//! - Register discipline: callers keep a live value in x86_64 `r9` and AArch64 `x19` across the
//!   sampler chain (the range lowering and `__rt_shuffle`), so neither is touched here.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// php's `MT_RAND_MT19937` mode word, also the value stored in `_mt_mode` once seeded.
pub const MT_MODE_MT19937: i64 = 1;
/// php's deprecated `MT_RAND_PHP` mode word as stored in `_mt_mode`.
pub const MT_MODE_PHP: i64 = 2;

/// Bytes of `_mt_state`: 624 32-bit words.
pub const MT_STATE_BYTES: usize = 624 * 4;

/// Emits the four Mersenne Twister helpers.
pub fn emit_mt19937(emitter: &mut Emitter) {
    emit_seed(emitter);
    emit_reload(emitter);
    emit_next(emitter);
    emit_rand_common(emitter);
}

/// `__rt_mt_seed(seed, mode)`: seeds the state with the low 32 bits of `seed` and stores `mode`
/// (`MT_MODE_MT19937` or `MT_MODE_PHP`), then reloads, exactly as `php_random_mt19937_seed32`.
fn emit_seed(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mt19937 seed ---");
    emitter.label_global("__rt_mt_seed");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x29, x30, [sp, #-16]!");                   // save the frame pair before the nested reload
            emitter.instruction("mov x29, sp");                                 // establish the frame pointer
            abi::emit_symbol_address(emitter, "x10", "_mt_mode");
            emitter.instruction("str x1, [x10]");                               // remember which twist the reloads use
            abi::emit_symbol_address(emitter, "x9", "_mt_state");
            emitter.instruction("mov w11, w0");                                 // php truncates the seed to 32 bits
            emitter.instruction("str w11, [x9]");                               // state[0] = seed
            emitter.instruction("mov x12, #1");                                 // i = 1
            emitter.instruction("mov w13, #0x8965");                            // low half of Knuth's multiplier 1812433253
            emitter.instruction("movk w13, #0x6c07, lsl #16");                  // high half of the multiplier
            emitter.label("__rt_mt_seed_loop");
            emitter.instruction("cmp x12, #624");                               // every state word filled?
            emitter.instruction("b.hs __rt_mt_seed_done");                      // yes: twist the fresh state once
            emitter.instruction("eor w14, w11, w11, lsr #30");                  // prev ^ (prev >> 30)
            emitter.instruction("madd w11, w14, w13, w12");                     // state[i] = multiplier * that + i, wrapped to 32 bits
            emitter.instruction("str w11, [x9, x12, lsl #2]");                  // store state[i]
            emitter.instruction("add x12, x12, #1");                            // i += 1
            emitter.instruction("b __rt_mt_seed_loop");                         // fill the next word
            emitter.label("__rt_mt_seed_done");
            emitter.instruction("bl __rt_mt_reload");                           // php reloads right after seeding
            emitter.instruction("ldp x29, x30, [sp], #16");                     // restore the frame pair
            emitter.instruction("ret");                                         // the engine is seeded
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // keep the stack aligned for the nested reload
            emitter.instruction("mov rbp, rsp");                                // establish the frame pointer
            abi::emit_symbol_address(emitter, "r10", "_mt_mode");
            emitter.instruction("mov QWORD PTR [r10], rsi");                    // remember which twist the reloads use
            abi::emit_symbol_address(emitter, "r8", "_mt_state");
            emitter.instruction("mov eax, edi");                                // php truncates the seed to 32 bits
            emitter.instruction("mov DWORD PTR [r8], eax");                     // state[0] = seed
            emitter.instruction("mov ecx, 1");                                  // i = 1
            emitter.label("__rt_mt_seed_loop");
            emitter.instruction("cmp ecx, 624");                                // every state word filled?
            emitter.instruction("jae __rt_mt_seed_done");                       // yes: twist the fresh state once
            emitter.instruction("mov edx, eax");                                // copy prev
            emitter.instruction("shr edx, 30");                                 // prev >> 30
            emitter.instruction("xor eax, edx");                                // prev ^ (prev >> 30)
            emitter.instruction("imul eax, eax, 1812433253");                   // times Knuth's multiplier, wrapped to 32 bits
            emitter.instruction("add eax, ecx");                                // + i
            emitter.instruction("mov DWORD PTR [r8 + rcx*4], eax");             // store state[i]
            emitter.instruction("inc ecx");                                     // i += 1
            emitter.instruction("jmp __rt_mt_seed_loop");                       // fill the next word
            emitter.label("__rt_mt_seed_done");
            emitter.instruction("call __rt_mt_reload");                         // php reloads right after seeding
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // the engine is seeded
        }
    }
}

/// `__rt_mt_reload()`: twists all 624 words in place and rewinds the draw index.
fn emit_reload(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mt19937 reload ---");
    emitter.label_global("__rt_mt_reload");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x10", "_mt_mode");
            emitter.instruction("ldr x10, [x10]");                              // the mode picks loBit(v) or php's legacy loBit(u)
            abi::emit_symbol_address(emitter, "x9", "_mt_state");
            emitter.instruction("mov x11, #0");                                 // k = 0
            emitter.instruction("mov w15, #0xb0df");                            // low half of the twist matrix 0x9908b0df
            emitter.instruction("movk w15, #0x9908, lsl #16");                  // high half of the twist matrix
            emitter.label("__rt_mt_reload_loop");
            emitter.instruction("cmp x11, #624");                               // every word twisted?
            emitter.instruction("b.hs __rt_mt_reload_done");                    // yes: rewind the draw index
            emitter.instruction("add x12, x11, #1");                            // k + 1
            emitter.instruction("cmp x12, #624");                               // past the last word?
            emitter.instruction("csel x12, xzr, x12, eq");                      // wrap to word 0 (already twisted, as in php)
            emitter.instruction("add x13, x11, #397");                          // k + M
            emitter.instruction("subs x14, x13, #624");                         // k + M - N
            emitter.instruction("csel x13, x14, x13, hs");                      // wrap past the end of the state
            emitter.instruction("ldr w1, [x9, x11, lsl #2]");                   // u = state[k]
            emitter.instruction("ldr w2, [x9, x12, lsl #2]");                   // v = state[k + 1]
            emitter.instruction("ldr w3, [x9, x13, lsl #2]");                   // m = state[k + M]
            emitter.instruction("and w4, w1, #0x80000000");                     // hiBit(u)
            emitter.instruction("and w5, w2, #0x7fffffff");                     // loBits(v)
            emitter.instruction("orr w4, w4, w5");                              // mixBits(u, v)
            emitter.instruction("eor w3, w3, w4, lsr #1");                      // m ^ (mixBits >> 1)
            emitter.instruction(&format!("cmp x10, #{MT_MODE_PHP}"));           // legacy mode twists on u's low bit
            emitter.instruction("csel w6, w1, w2, eq");                         // pick the word whose low bit selects the matrix
            emitter.instruction("tst w6, #1");                                  // is that low bit set?
            emitter.instruction("csel w7, w15, wzr, ne");                       // the matrix, or nothing
            emitter.instruction("eor w3, w3, w7");                              // finish the twist
            emitter.instruction("str w3, [x9, x11, lsl #2]");                   // state[k] = twisted word
            emitter.instruction("add x11, x11, #1");                            // k += 1
            emitter.instruction("b __rt_mt_reload_loop");                       // twist the next word
            emitter.label("__rt_mt_reload_done");
            abi::emit_symbol_address(emitter, "x9", "_mt_count");
            emitter.instruction("str xzr, [x9]");                               // draws restart at word 0
            emitter.instruction("ret");                                         // the state is reloaded
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r10", "_mt_mode");
            emitter.instruction("mov r10, QWORD PTR [r10]");                    // the mode picks loBit(v) or php's legacy loBit(u)
            abi::emit_symbol_address(emitter, "r8", "_mt_state");
            emitter.instruction("xor ecx, ecx");                                // k = 0
            emitter.label("__rt_mt_reload_loop");
            emitter.instruction("cmp ecx, 624");                                // every word twisted?
            emitter.instruction("jae __rt_mt_reload_done");                     // yes: rewind the draw index
            emitter.instruction("mov eax, DWORD PTR [r8 + rcx*4]");             // u = state[k]
            emitter.instruction("lea edx, [rcx + 1]");                          // k + 1
            emitter.instruction("xor esi, esi");                                // word 0, for the wrap
            emitter.instruction("cmp edx, 624");                                // past the last word?
            emitter.instruction("cmove edx, esi");                              // wrap to word 0 (already twisted, as in php)
            emitter.instruction("mov edi, DWORD PTR [r8 + rdx*4]");             // v = state[k + 1]
            emitter.instruction("lea edx, [rcx + 397]");                        // k + M
            emitter.instruction("lea esi, [rcx - 227]");                        // k + M - N
            emitter.instruction("cmp edx, 624");                                // past the end of the state?
            emitter.instruction("cmovae edx, esi");                             // wrap it
            emitter.instruction("mov r11d, DWORD PTR [r8 + rdx*4]");            // m = state[k + M]
            emitter.instruction("mov edx, eax");                                // copy u
            emitter.instruction("and edx, 0x80000000");                         // hiBit(u)
            emitter.instruction("mov esi, edi");                                // copy v
            emitter.instruction("and esi, 0x7fffffff");                         // loBits(v)
            emitter.instruction("or edx, esi");                                 // mixBits(u, v)
            emitter.instruction("shr edx, 1");                                  // mixBits >> 1
            emitter.instruction("xor r11d, edx");                               // m ^ (mixBits >> 1)
            emitter.instruction("mov edx, edi");                                // the low bit normally comes from v
            emitter.instruction(&format!("cmp r10, {MT_MODE_PHP}"));            // legacy mode twists on u's low bit
            emitter.instruction("cmove edx, eax");                              // pick u instead
            emitter.instruction("and edx, 1");                                  // isolate the low bit
            emitter.instruction("neg edx");                                     // all ones when it is set
            emitter.instruction("and edx, 0x9908b0df");                         // the matrix, or nothing
            emitter.instruction("xor r11d, edx");                               // finish the twist
            emitter.instruction("mov DWORD PTR [r8 + rcx*4], r11d");            // state[k] = twisted word
            emitter.instruction("inc ecx");                                     // k += 1
            emitter.instruction("jmp __rt_mt_reload_loop");                     // twist the next word
            emitter.label("__rt_mt_reload_done");
            abi::emit_symbol_address(emitter, "r8", "_mt_count");
            emitter.instruction("mov QWORD PTR [r8], 0");                       // draws restart at word 0
            emitter.instruction("ret");                                         // the state is reloaded
        }
    }
}

/// `__rt_mt_u32()`: the next tempered 32-bit word, or a CSPRNG word while unseeded.
fn emit_next(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mt19937 next ---");
    emitter.label_global("__rt_mt_u32");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x10", "_mt_mode");
            emitter.instruction("ldr x10, [x10]");                              // has the script seeded the engine?
            emitter.instruction("cbnz x10, __rt_mt_u32_seeded");                // yes: draw from the twister
            emitter.instruction("b __rt_random_u32");                           // no: php's auto-seed is as random as the CSPRNG
            emitter.label("__rt_mt_u32_seeded");
            emitter.instruction("stp x29, x30, [sp, #-16]!");                   // save the frame pair before a possible reload
            emitter.instruction("mov x29, sp");                                 // establish the frame pointer
            abi::emit_symbol_address(emitter, "x11", "_mt_count");
            emitter.instruction("ldr x12, [x11]");                              // index of the next untempered word
            emitter.instruction("cmp x12, #624");                               // every word drawn?
            emitter.instruction("b.lo __rt_mt_u32_ready");                      // no: draw from the current state
            emitter.instruction("bl __rt_mt_reload");                           // twist a fresh block, as php's generate() does
            emitter.instruction("mov x12, #0");                                 // and start from its first word
            emitter.label("__rt_mt_u32_ready");
            abi::emit_symbol_address(emitter, "x9", "_mt_state");
            emitter.instruction("ldr w0, [x9, x12, lsl #2]");                   // s1 = state[count]
            emitter.instruction("add x12, x12, #1");                            // count += 1
            abi::emit_symbol_address(emitter, "x11", "_mt_count");
            emitter.instruction("str x12, [x11]");                              // publish the advanced index
            emitter.instruction("eor w0, w0, w0, lsr #11");                     // s1 ^= s1 >> 11
            emitter.instruction("mov w13, #0x5680");                            // low half of 0x9d2c5680
            emitter.instruction("movk w13, #0x9d2c, lsl #16");                  // high half of 0x9d2c5680
            emitter.instruction("and w14, w13, w0, lsl #7");                    // (s1 << 7) & 0x9d2c5680
            emitter.instruction("eor w0, w0, w14");                             // s1 ^= that
            emitter.instruction("mov w13, #0xefc60000");                        // 0xefc60000 is a single shifted half
            emitter.instruction("and w14, w13, w0, lsl #15");                   // (s1 << 15) & 0xefc60000
            emitter.instruction("eor w0, w0, w14");                             // s1 ^= that
            emitter.instruction("eor w0, w0, w0, lsr #18");                     // s1 ^ (s1 >> 18), zero-extended into x0
            emitter.instruction("ldp x29, x30, [sp], #16");                     // restore the frame pair
            emitter.instruction("ret");                                         // return the tempered word
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r10", "_mt_mode");
            emitter.instruction("mov r10, QWORD PTR [r10]");                    // has the script seeded the engine?
            emitter.instruction("test r10, r10");                               // zero while unseeded
            emitter.instruction("jz __rt_random_u32");                          // php's auto-seed is as random as the CSPRNG
            emitter.instruction("push rbp");                                    // keep the stack aligned for a possible reload
            emitter.instruction("mov rbp, rsp");                                // establish the frame pointer
            abi::emit_symbol_address(emitter, "r11", "_mt_count");
            emitter.instruction("mov rcx, QWORD PTR [r11]");                    // index of the next untempered word
            emitter.instruction("cmp rcx, 624");                                // every word drawn?
            emitter.instruction("jb __rt_mt_u32_ready");                        // no: draw from the current state
            emitter.instruction("call __rt_mt_reload");                         // twist a fresh block, as php's generate() does
            emitter.instruction("xor ecx, ecx");                                // and start from its first word
            emitter.label("__rt_mt_u32_ready");
            abi::emit_symbol_address(emitter, "r8", "_mt_state");
            emitter.instruction("mov eax, DWORD PTR [r8 + rcx*4]");             // s1 = state[count]
            emitter.instruction("inc rcx");                                     // count += 1
            abi::emit_symbol_address(emitter, "r11", "_mt_count");
            emitter.instruction("mov QWORD PTR [r11], rcx");                    // publish the advanced index
            emitter.instruction("mov edx, eax");                                // copy s1
            emitter.instruction("shr edx, 11");                                 // s1 >> 11
            emitter.instruction("xor eax, edx");                                // s1 ^= s1 >> 11
            emitter.instruction("mov edx, eax");                                // copy s1
            emitter.instruction("shl edx, 7");                                  // s1 << 7
            emitter.instruction("and edx, 0x9d2c5680");                         // masked
            emitter.instruction("xor eax, edx");                                // s1 ^= that
            emitter.instruction("mov edx, eax");                                // copy s1
            emitter.instruction("shl edx, 15");                                 // s1 << 15
            emitter.instruction("and edx, 0xefc60000");                         // masked
            emitter.instruction("xor eax, edx");                                // s1 ^= that
            emitter.instruction("mov edx, eax");                                // copy s1
            emitter.instruction("shr edx, 18");                                 // s1 >> 18
            emitter.instruction("xor eax, edx");                                // the tempered word, zero-extended into rax
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // return the tempered word
        }
    }
}

/// `__rt_mt_rand_common(min, max)`: php's `php_mt_rand_common` for a non-inverted range. The
/// default mode draws `min + range(max - min)` through the Mersenne Twister sampler chain; the
/// deprecated `MT_RAND_PHP` mode scales one 31-bit draw with php's floating-point formula.
fn emit_rand_common(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mt19937 rand range ---");
    emitter.label_global("__rt_mt_rand_common");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x29, x30, [sp, #-32]!");                   // frame pair plus the two bounds
            emitter.instruction("mov x29, sp");                                 // establish the frame pointer
            emitter.instruction("stp x0, x1, [sp, #16]");                       // save min and max across the draws
            abi::emit_symbol_address(emitter, "x10", "_mt_mode");
            emitter.instruction("ldr x10, [x10]");                              // which range formula does the mode use?
            emitter.instruction(&format!("cmp x10, #{MT_MODE_PHP}"));           // the legacy mode scales instead
            emitter.instruction("b.eq __rt_mt_rand_common_legacy");             // take php's RAND_RANGE_BADSCALING path
            emitter.instruction("sub x0, x1, x0");                              // umax = max - min, unsigned
            emitter.instruction("bl __rt_mt_uniform64");                        // php_random_range over the twister
            emitter.instruction("ldr x9, [sp, #16]");                           // reload min
            emitter.instruction("add x0, x0, x9");                              // shift the offset back into the range
            emitter.instruction("b __rt_mt_rand_common_done");                  // done
            emitter.label("__rt_mt_rand_common_legacy");
            emitter.instruction("bl __rt_mt_u32");                              // one 32-bit draw
            emitter.instruction("lsr w0, w0, #1");                              // r = draw >> 1, as php does
            emitter.instruction("ucvtf d0, x0");                                // r as a double
            emitter.instruction("mov x9, #0x41e0000000000000");                 // 2147483648.0 = PHP_MT_RAND_MAX + 1.0
            emitter.instruction("fmov d1, x9");                                 // as a double
            emitter.instruction("fdiv d0, d0, d1");                             // r / (PHP_MT_RAND_MAX + 1.0)
            emitter.instruction("ldp x9, x10, [sp, #16]");                      // reload min and max
            emitter.instruction("scvtf d1, x10");                               // (double) max
            emitter.instruction("scvtf d2, x9");                                // (double) min
            emitter.instruction("fsub d1, d1, d2");                             // (double) max - min
            emitter.instruction("fmov d2, #1.0");                               // + 1.0
            emitter.instruction("fadd d1, d1, d2");                             // the width as php computes it
            emitter.instruction("fmul d0, d1, d0");                             // the scaled offset
            emitter.instruction("fcvtzu x0, d0");                               // truncated to an unsigned offset
            emitter.instruction("add x0, x0, x9");                              // offset + min
            emitter.label("__rt_mt_rand_common_done");
            emitter.instruction("ldp x29, x30, [sp], #32");                     // restore the frame pair
            emitter.instruction("ret");                                         // return the sampled integer
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish the frame pointer
            emitter.instruction("sub rsp, 16");                                 // min and max, keeping rsp aligned
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // save min across the draws
            emitter.instruction("mov QWORD PTR [rbp - 16], rsi");               // save max across the draws
            abi::emit_symbol_address(emitter, "r10", "_mt_mode");
            emitter.instruction("mov r10, QWORD PTR [r10]");                    // which range formula does the mode use?
            emitter.instruction(&format!("cmp r10, {MT_MODE_PHP}"));            // the legacy mode scales instead
            emitter.instruction("je __rt_mt_rand_common_legacy");               // take php's RAND_RANGE_BADSCALING path
            emitter.instruction("mov rdi, rsi");                                // copy max
            emitter.instruction("sub rdi, QWORD PTR [rbp - 8]");                // umax = max - min, unsigned
            emitter.instruction("call __rt_mt_uniform64");                      // php_random_range over the twister
            emitter.instruction("add rax, QWORD PTR [rbp - 8]");                // shift the offset back into the range
            emitter.instruction("jmp __rt_mt_rand_common_done");                // done
            emitter.label("__rt_mt_rand_common_legacy");
            emitter.instruction("call __rt_mt_u32");                            // one 32-bit draw
            emitter.instruction("shr eax, 1");                                  // r = draw >> 1, as php does
            emitter.instruction("cvtsi2sd xmm0, rax");                          // r as a double (it fits in 31 bits)
            emitter.instruction("mov rax, 0x41e0000000000000");                 // 2147483648.0 = PHP_MT_RAND_MAX + 1.0
            emitter.instruction("movq xmm1, rax");                              // as a double
            emitter.instruction("divsd xmm0, xmm1");                            // r / (PHP_MT_RAND_MAX + 1.0)
            emitter.instruction("cvtsi2sd xmm1, QWORD PTR [rbp - 16]");         // (double) max
            emitter.instruction("cvtsi2sd xmm2, QWORD PTR [rbp - 8]");          // (double) min
            emitter.instruction("subsd xmm1, xmm2");                            // (double) max - min
            emitter.instruction("mov rax, 0x3ff0000000000000");                 // 1.0
            emitter.instruction("movq xmm2, rax");                              // as a double
            emitter.instruction("addsd xmm1, xmm2");                            // the width as php computes it
            emitter.instruction("mulsd xmm1, xmm0");                            // the scaled offset
            emitter.instruction("cvttsd2si rax, xmm1");                         // truncated to an integer offset
            emitter.instruction("add rax, QWORD PTR [rbp - 8]");                // offset + min
            emitter.label("__rt_mt_rand_common_done");
            emitter.instruction("add rsp, 16");                                 // release the bounds
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // return the sampled integer
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// The engine falls back to the CSPRNG while unseeded, never touches the registers callers
    /// keep live across the sampler chain, and twists with php's matrix on every target.
    #[test]
    fn mt19937_engine_keeps_caller_registers_on_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_mt19937(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_random_u32"), "{target}: unseeded fallback");
            if target == "linux-x86_64" {
                assert!(asm.contains("0x9908b0df"), "{target}");
                assert!(!asm.contains("r9"), "{target}: r9 carries the caller's minimum");
            } else {
                assert!(asm.contains("#0x9908, lsl #16"), "{target}");
                for reg in ["x19", "x20", "x21", "x22"] {
                    assert!(!asm.contains(reg), "{target}: {reg} is the caller's");
                }
            }
        }
    }
}
