//! Purpose:
//! Emits `__rt_random_u64` and `__rt_random_uniform64`, the 64-bit-wide counterparts of
//! `__rt_random_u32` / `__rt_random_uniform`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - `__rt_random_uniform` takes its bound in `w0`/`edi` — THIRTY-TWO BITS. The range lowering
//!   computed a 64-bit width and passed it there, so any span that was a multiple of 2^32
//!   arrived as zero and the helper answered `min` every single time. `random_int(0, PHP_INT_MAX)`
//!   returned `0` on every call; `random_int(0, 4294967294)` was fine and `random_int(0,
//!   4294967295)` was not, which is the signature of the truncation rather than of a weak
//!   generator.
//! - The bound convention here is INCLUSIVE (`umax = max - min`), never `max - min + 1`.
//!   The exclusive form overflows to zero for `random_int(PHP_INT_MIN, PHP_INT_MAX)`, which is a
//!   second, independent way to reach the same stuck-at-`min` behaviour.
//! - Mirrors php-src's `php_random_range64` (ext/random/random.c): the full-width span is
//!   returned raw, a power-of-two span is masked, and anything else is rejection-sampled against
//!   `UINT64_MAX - (UINT64_MAX % count) - 1`.
//! - Spans that fit in 32 bits DELEGATE to the existing 32-bit sampler, so `mt_rand()`,
//!   `array_rand()` and `shuffle()` keep exactly the cost they had. That delegation is what
//!   php-src's `php_random_range()` does too.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_random_u64() -> uint64`, drawing two 32-bit words from the CSPRNG-backed source.
///
/// ABI: no arguments; result in `x0` (AArch64) or `rax` (x86_64).
pub fn emit_random_u64(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_random_u64_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: random_u64 ---");
    emitter.label_global("__rt_random_u64");
    emitter.instruction("sub sp, sp, #32");                                     // frame for the saved pair and the low word
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a frame pointer
    emitter.instruction("bl __rt_random_u32");                                  // draw the low half
    emitter.instruction("str w0, [sp, #0]");                                    // stash it across the second draw
    emitter.instruction("bl __rt_random_u32");                                  // draw the high half
    emitter.instruction("ldr w9, [sp, #0]");                                    // reload the low half, zero-extended
    emitter.instruction("orr x0, x9, x0, lsl #32");                             // combine into one 64-bit word
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // return the random uint64
}

/// Emits `__rt_random_uniform64(umax) -> uint64` in `[0, umax]`, INCLUSIVE of `umax`.
///
/// ABI: `umax` in `x0` (AArch64) or `rdi` (x86_64); result in `x0`/`rax`.
pub fn emit_random_uniform64(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_random_uniform64_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: random_uniform64 ---");
    emitter.label_global("__rt_random_uniform64");
    // Frame: 48 bytes. [0..16) saved x29/x30, [16] umax, [24] count, [32] rejection limit.
    emitter.instruction("sub sp, sp, #48");                                     // frame for the saved pair and three locals
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish a frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the inclusive upper bound
    emitter.instruction("cbz x0, __rt_ru64_zero");                              // a single-value range is always its own answer

    emitter.instruction("mov w9, #-1");                                         // w9 = 0xFFFFFFFF, zero-extended into x9
    emitter.instruction("cmp x0, x9");                                          // does the span still fit the 32-bit sampler?
    emitter.instruction("b.hs __rt_ru64_wide");                                 // no — take the 64-bit path
    emitter.instruction("add w0, w0, #1");                                      // exclusive bound = umax + 1, which fits in u32
    emitter.instruction("bl __rt_random_uniform");                              // reuse the existing sampler at its existing cost
    emitter.instruction("mov w0, w0");                                          // zero-extend the 32-bit result
    emitter.instruction("b __rt_ru64_done");                                    // done

    emitter.label("__rt_ru64_wide");
    emitter.instruction("cmn x0, #1");                                          // is the span the entire 64-bit range?
    emitter.instruction("b.eq __rt_ru64_raw");                                  // yes — every draw is already in range
    emitter.instruction("add x10, x0, #1");                                     // count = umax + 1
    emitter.instruction("str x10, [sp, #24]");                                  // save the count
    emitter.instruction("and x11, x10, x0");                                    // count & umax is zero only for a power of two
    emitter.instruction("cbz x11, __rt_ru64_mask");                             // power of two — mask instead of dividing
    emitter.instruction("mov x12, #-1");                                        // x12 = UINT64_MAX
    emitter.instruction("udiv x13, x12, x10");                                  // quotient of UINT64_MAX / count
    emitter.instruction("msub x14, x13, x10, x12");                             // remainder = UINT64_MAX % count
    emitter.instruction("sub x15, x12, x14");                                   // UINT64_MAX - remainder
    emitter.instruction("sub x15, x15, #1");                                    // limit, above which a draw is biased
    emitter.instruction("str x15, [sp, #32]");                                  // save the rejection limit

    emitter.label("__rt_ru64_loop");
    emitter.instruction("bl __rt_random_u64");                                  // draw a fresh 64-bit candidate
    emitter.instruction("ldr x15, [sp, #32]");                                  // reload the rejection limit
    emitter.instruction("cmp x0, x15");                                         // is the candidate inside the unbiased prefix?
    emitter.instruction("b.hi __rt_ru64_loop");                                 // no — discard and resample
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the count
    emitter.instruction("udiv x13, x0, x10");                                   // quotient = candidate / count
    emitter.instruction("msub x0, x13, x10, x0");                               // remainder = candidate % count
    emitter.instruction("b __rt_ru64_done");                                    // done

    emitter.label("__rt_ru64_mask");
    emitter.instruction("bl __rt_random_u64");                                  // a power-of-two span needs no rejection
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload umax, which is the mask
    emitter.instruction("and x0, x0, x9");                                      // keep the low bits
    emitter.instruction("b __rt_ru64_done");                                    // done

    emitter.label("__rt_ru64_raw");
    emitter.instruction("bl __rt_random_u64");                                  // the full range needs no reduction at all
    emitter.instruction("b __rt_ru64_done");                                    // done

    emitter.label("__rt_ru64_zero");
    emitter.instruction("mov x0, #0");                                          // degenerate range maps to offset zero

    emitter.label("__rt_ru64_done");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the frame
    emitter.instruction("ret");                                                 // return the sampled offset
}

/// Emits the Linux x86_64 implementation of `__rt_random_u64`.
fn emit_random_u64_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: random_u64 ---");
    emitter.label_global("__rt_random_u64");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a frame pointer
    emitter.instruction("sub rsp, 16");                                         // scratch for the low word, keeping rsp aligned
    emitter.instruction("call __rt_random_u32");                                // draw the low half
    emitter.instruction("mov DWORD PTR [rbp - 8], eax");                        // stash it across the second draw
    emitter.instruction("call __rt_random_u32");                                // draw the high half
    emitter.instruction("shl rax, 32");                                         // move it into the high word
    emitter.instruction("mov ecx, DWORD PTR [rbp - 8]");                        // reload the low half, zero-extended
    emitter.instruction("or rax, rcx");                                         // combine into one 64-bit word
    emitter.instruction("add rsp, 16");                                         // release the scratch
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the random uint64
}

/// Emits the Linux x86_64 implementation of `__rt_random_uniform64`.
fn emit_random_uniform64_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: random_uniform64 ---");
    emitter.label_global("__rt_random_uniform64");
    // Frame: [rbp-8] umax, [rbp-16] count, [rbp-24] rejection limit.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a frame pointer
    emitter.instruction("sub rsp, 32");                                         // three locals, keeping rsp aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the inclusive upper bound
    emitter.instruction("test rdi, rdi");                                       // is the range a single value?
    emitter.instruction("jz __rt_ru64_zero_x86");                               // yes — it is always its own answer
    emitter.instruction("mov rcx, 0xFFFFFFFF");                                 // the widest span the 32-bit sampler can take
    emitter.instruction("cmp rdi, rcx");                                        // does the span still fit it?
    emitter.instruction("jae __rt_ru64_wide_x86");                              // no — take the 64-bit path
    emitter.instruction("add edi, 1");                                          // exclusive bound = umax + 1, which fits in u32
    emitter.instruction("call __rt_random_uniform");                            // reuse the existing sampler at its existing cost
    emitter.instruction("mov eax, eax");                                        // zero-extend the 32-bit result
    emitter.instruction("jmp __rt_ru64_done_x86");                              // done

    emitter.label("__rt_ru64_wide_x86");
    emitter.instruction("cmp rdi, -1");                                         // is the span the entire 64-bit range?
    emitter.instruction("je __rt_ru64_raw_x86");                                // yes — every draw is already in range
    emitter.instruction("mov r8, rdi");                                         // copy umax
    emitter.instruction("add r8, 1");                                           // count = umax + 1
    emitter.instruction("mov QWORD PTR [rbp - 16], r8");                        // save the count
    emitter.instruction("mov r11, r8");                                         // copy the count
    emitter.instruction("and r11, rdi");                                        // count & umax is zero only for a power of two
    emitter.instruction("jz __rt_ru64_mask_x86");                               // power of two — mask instead of dividing
    emitter.instruction("mov rax, -1");                                         // rax = UINT64_MAX
    emitter.instruction("xor edx, edx");                                        // clear the high half of the dividend
    emitter.instruction("div r8");                                              // rdx = UINT64_MAX % count
    emitter.instruction("mov r10, -1");                                         // r10 = UINT64_MAX
    emitter.instruction("sub r10, rdx");                                        // UINT64_MAX - remainder
    emitter.instruction("sub r10, 1");                                          // limit, above which a draw is biased
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the rejection limit

    emitter.label("__rt_ru64_loop_x86");
    emitter.instruction("call __rt_random_u64");                                // draw a fresh 64-bit candidate
    emitter.instruction("cmp rax, QWORD PTR [rbp - 24]");                       // is the candidate inside the unbiased prefix?
    emitter.instruction("ja __rt_ru64_loop_x86");                               // no — discard and resample
    emitter.instruction("xor edx, edx");                                        // clear the high half of the dividend
    emitter.instruction("div QWORD PTR [rbp - 16]");                            // rdx = candidate % count
    emitter.instruction("mov rax, rdx");                                        // the remainder is the sampled offset
    emitter.instruction("jmp __rt_ru64_done_x86");                              // done

    emitter.label("__rt_ru64_mask_x86");
    emitter.instruction("call __rt_random_u64");                                // a power-of-two span needs no rejection
    emitter.instruction("and rax, QWORD PTR [rbp - 8]");                        // umax is the mask
    emitter.instruction("jmp __rt_ru64_done_x86");                              // done

    emitter.label("__rt_ru64_raw_x86");
    emitter.instruction("call __rt_random_u64");                                // the full range needs no reduction at all
    emitter.instruction("jmp __rt_ru64_done_x86");                              // done

    emitter.label("__rt_ru64_zero_x86");
    emitter.instruction("xor eax, eax");                                        // degenerate range maps to offset zero

    emitter.label("__rt_ru64_done_x86");
    emitter.instruction("add rsp, 32");                                         // release the locals
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the sampled offset
}
