//! Purpose:
//! Emits the `__rt_random_uniform`, `__rt_random_uniform_calc` runtime helper assembly for random uniform.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Random helpers produce bounded integer values used by array_rand and shuffle without modulo bias where possible.
//! - `__rt_random_uniform` is a 32-bit sampler: its bound lives in `edi`/`w0`. It
//!   backs array_rand and shuffle, whose bound is an element count. `random_int`,
//!   `rand` and `mt_rand` accept the full PHP integer range, so they go through
//!   `__rt_random_uniform64`, which takes an *inclusive* width and delegates to the
//!   32-bit sampler whenever the range fits — mirroring php-src's
//!   `php_random_range()` dispatch between `php_random_range32`/`php_random_range64`
//!   (ext/random/random.c).

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_random_uniform` runtime helper.
///
/// Dispatches to the x86_64 Linux implementation or generates a portable ARM64
/// rejection-sampling implementation. Returns a bias-free uniform integer in the
/// range [0, bound) where bound is passed in `w0` (ARM64) or `edi` (x86_64).
/// The result is returned in `x0` (ARM64) or `eax` (x86_64).
///
/// # Algorithm
/// Uses rejection sampling to avoid the modulo bias that would arise from naively
/// scaling a uint32 to a smaller range. The rejection threshold is `2^32 % bound`,
/// which discards the top portion of the 32-bit space that would otherwise cause
/// uneven distribution.
///
/// # ABI
/// - ARM64: bound in `w0`, result in `w0`
/// - x86_64: bound in `edi`, result in `eax`
pub fn emit_random_uniform(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_random_uniform_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: random_uniform ---");
    emitter.label_global("__rt_random_uniform");
    emitter.instruction("sub sp, sp, #32");                                     // allocate stack space for locals and saved frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a frame pointer
    emitter.instruction("str w0, [sp, #0]");                                    // save the exclusive upper bound as a uint32
    emitter.instruction("cmp w0, #1");                                          // is the bound 0 or 1?
    emitter.instruction("b.hi __rt_random_uniform_calc");                       // no — continue with rejection sampling
    emitter.instruction("mov x0, #0");                                          // degenerate ranges always map to zero
    emitter.instruction("b __rt_random_uniform_done");                          // skip the sampling loop

    emitter.label("__rt_random_uniform_calc");
    emitter.instruction("neg w9, w0");                                          // compute 2^32 - bound modulo 2^32
    emitter.instruction("udiv w10, w9, w0");                                    // quotient = floor((2^32 - bound) / bound)
    emitter.instruction("msub w9, w10, w0, w9");                                // threshold = (2^32 % bound)
    emitter.instruction("str w9, [sp, #4]");                                    // save the rejection threshold

    emitter.label("__rt_random_uniform_loop");
    emitter.instruction("bl __rt_random_u32");                                  // generate a fresh uint32 candidate
    emitter.instruction("ldr w9, [sp, #4]");                                    // reload the rejection threshold
    emitter.instruction("cmp w0, w9");                                          // candidate below the biased prefix?
    emitter.instruction("b.lo __rt_random_uniform_loop");                       // yes — discard and resample
    emitter.instruction("ldr w1, [sp, #0]");                                    // reload the exclusive upper bound
    emitter.instruction("udiv w10, w0, w1");                                    // quotient = candidate / bound
    emitter.instruction("msub w0, w10, w1, w0");                                // remainder = candidate % bound

    emitter.label("__rt_random_uniform_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the temporary stack frame
    emitter.instruction("ret");                                                 // return the unbiased random value
}

/// Emits the x86_64 Linux implementation of `__rt_random_uniform`.
///
/// Uses x86_64 System V ABI: bound passed in `edi`, result returned in `eax`.
/// Stack-allocates two uint32 scratch slots (bound and rejection threshold) at
/// `[rbp - 4]` and `[rbp - 8]` relative to the frame pointer.
///
/// # Algorithm
/// Rejection sampling identical to the ARM64 path. Computes `threshold = 2^32 % bound`
/// then loops generating `__rt_random_u32` candidates and discarding those below
/// the threshold before computing `candidate % bound`.
fn emit_random_uniform_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: random_uniform ---");
    emitter.label_global("__rt_random_uniform");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving x86_64 rejection-sampling scratch slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved bound and rejection threshold
    emitter.instruction("sub rsp, 16");                                         // reserve aligned stack space for the uint32 bound and rejection threshold across helper calls
    emitter.instruction("mov DWORD PTR [rbp - 4], edi");                        // preserve the exclusive upper bound as a uint32 across the rejection-sampling loop
    emitter.instruction("cmp edi, 1");                                          // detect degenerate bounds that only admit the zero result
    emitter.instruction("ja __rt_random_uniform_calc_x86");                     // continue with rejection sampling only when the exclusive upper bound exceeds one
    emitter.instruction("xor eax, eax");                                        // degenerate ranges always map to the scalar zero result
    emitter.instruction("jmp __rt_random_uniform_done_x86");                    // skip the rejection-sampling loop on degenerate bounds

    emitter.label("__rt_random_uniform_calc_x86");
    emitter.instruction("xor eax, eax");                                        // seed the threshold dividend from zero before subtracting the exclusive upper bound modulo 2^32
    emitter.instruction("sub eax, DWORD PTR [rbp - 4]");                        // compute 2^32 - bound modulo 2^32 using 32-bit wraparound arithmetic
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before computing the uint32 rejection threshold remainder
    emitter.instruction("div DWORD PTR [rbp - 4]");                             // divide the wrapped dividend by the bound so edx becomes the rejection threshold
    emitter.instruction("mov DWORD PTR [rbp - 8], edx");                        // preserve the uint32 rejection threshold across random_u32 helper calls

    emitter.label("__rt_random_uniform_loop_x86");
    emitter.instruction("call __rt_random_u32");                                // generate a fresh uint32 candidate for the rejection-sampling loop
    emitter.instruction("cmp eax, DWORD PTR [rbp - 8]");                        // compare the candidate against the rejection threshold that removes modulo bias
    emitter.instruction("jb __rt_random_uniform_loop_x86");                     // discard biased candidates that fall below the rejection threshold
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before reducing the unbiased uint32 candidate modulo the bound
    emitter.instruction("div DWORD PTR [rbp - 4]");                             // divide the unbiased uint32 candidate by the bound so edx becomes candidate % bound
    emitter.instruction("mov eax, edx");                                        // return the unbiased remainder as the sampled scalar value in [0, bound)

    emitter.label("__rt_random_uniform_done_x86");
    emitter.instruction("add rsp, 16");                                         // release the rejection-sampling scratch slots before returning to the caller
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the x86_64 rejection-sampling helper completes
    emitter.instruction("ret");                                                 // return the unbiased random value in eax
}

/// Emits the `__rt_random_u64` and `__rt_random_uniform64` runtime helpers.
///
/// `__rt_random_uniform64` takes an **inclusive** range width `umax` and returns a
/// uniform value in `[0, umax]`. The inclusive convention is what makes the full
/// PHP integer range representable: `random_int(PHP_INT_MIN, PHP_INT_MAX)` has
/// `max - min + 1 == 2^64`, which is zero in 64 bits, whereas `max - min` is
/// `UINT64_MAX` and needs no wider arithmetic.
///
/// This mirrors php-src `php_random_range()` (ext/random/random.c), which selects
/// `php_random_range32` when `umax <= UINT32_MAX` and `php_random_range64`
/// otherwise, and which special-cases `umax == UINT64_MAX` before incrementing.
///
/// # ABI
/// - AArch64: `umax` in `x0`, result in `x0`
/// - x86_64: `umax` in `rdi`, result in `rax`
pub fn emit_random_uniform64(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_random_uniform64_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: random_u64 ---");
    emitter.label_global("__rt_random_u64");
    emitter.instruction("sub sp, sp, #32");                                     // reserve a frame so the two nested u32 draws can preserve state
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across the helper calls
    emitter.instruction("add x29, sp, #16");                                    // establish a frame pointer for the composed draw
    emitter.instruction("bl __rt_random_u32");                                  // draw the low half from the platform CSPRNG
    emitter.instruction("str w0, [sp, #0]");                                    // stash the low half while the second draw runs
    emitter.instruction("bl __rt_random_u32");                                  // draw the high half from the platform CSPRNG
    emitter.instruction("ldr w9, [sp, #0]");                                    // reload the low half as a zero-extended uint32
    emitter.instruction("lsl x0, x0, #32");                                     // move the second draw into the high 32 bits
    emitter.instruction("orr x0, x0, x9");                                      // compose both independent draws into one uint64
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the temporary stack frame
    emitter.instruction("ret");                                                 // return the composed random uint64

    emitter.blank();
    emitter.comment("--- runtime: random_uniform64 ---");
    emitter.label_global("__rt_random_uniform64");
    emitter.instruction("sub sp, sp, #32");                                     // reserve slots for the element count and rejection limit
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #16");                                    // establish a frame pointer for the sampling paths
    emitter.instruction("cmn x0, #1");                                          // is the inclusive width UINT64_MAX (compare against -1)?
    emitter.instruction("b.ne __rt_random_uniform64_bounded");                  // no — the width can be incremented without wrapping
    emitter.instruction("bl __rt_random_u64");                                  // the whole uint64 space is admissible, so draw once
    emitter.instruction("b __rt_random_uniform64_done");                        // and return it unreduced

    emitter.label("__rt_random_uniform64_bounded");
    emitter.instruction("add x0, x0, #1");                                      // convert the inclusive width into an exclusive element count
    emitter.instruction("lsr x9, x0, #32");                                     // does the count need more than 32 bits?
    emitter.instruction("cbnz x9, __rt_random_uniform64_wide");                 // yes — sample across the full uint64 space
    emitter.instruction("bl __rt_random_uniform");                              // no — reuse the 32-bit rejection sampler with the count in w0
    emitter.instruction("b __rt_random_uniform64_done");                        // its result is already in [0, count)

    emitter.label("__rt_random_uniform64_wide");
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the element count across the sampling loop
    emitter.instruction("mov x10, #-1");                                        // load UINT64_MAX as the rejection-limit dividend
    emitter.instruction("udiv x11, x10, x0");                                   // quotient = UINT64_MAX / count
    emitter.instruction("msub x11, x11, x0, x10");                              // remainder = UINT64_MAX % count
    emitter.instruction("sub x11, x10, x11");                                   // UINT64_MAX - (UINT64_MAX % count)
    emitter.instruction("sub x11, x11, #1");                                    // limit = the largest value that reduces without bias
    emitter.instruction("str x11, [sp, #8]");                                   // preserve the rejection limit across the sampling loop

    emitter.label("__rt_random_uniform64_loop");
    emitter.instruction("bl __rt_random_u64");                                  // draw a fresh uint64 candidate
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the rejection limit
    emitter.instruction("cmp x0, x11");                                         // candidate inside the unbiased prefix?
    emitter.instruction("b.hi __rt_random_uniform64_loop");                     // no — discard and resample
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the element count
    emitter.instruction("udiv x10, x0, x9");                                    // quotient = candidate / count
    emitter.instruction("msub x0, x10, x9, x0");                                // remainder = candidate % count

    emitter.label("__rt_random_uniform64_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the temporary stack frame
    emitter.instruction("ret");                                                 // return the unbiased value in [0, umax]
}

/// Emits the x86_64 `__rt_random_u64` and `__rt_random_uniform64` helpers.
///
/// Same algorithm as the AArch64 path. `umax` arrives in `rdi` and the result is
/// returned in `rax`; the element count and rejection limit are stack-allocated at
/// `[rbp - 8]` and `[rbp - 16]` so they survive the nested draw calls.
fn emit_random_uniform64_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: random_u64 ---");
    emitter.label_global("__rt_random_u64");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before composing two draws
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the stashed low half
    emitter.instruction("sub rsp, 16");                                         // reserve aligned scratch space across the nested helper calls
    emitter.instruction("call __rt_random_u32");                                // draw the low half from the platform CSPRNG
    emitter.instruction("mov DWORD PTR [rbp - 4], eax");                        // stash the low half while the second draw runs
    emitter.instruction("call __rt_random_u32");                                // draw the high half from the platform CSPRNG
    emitter.instruction("mov eax, eax");                                        // zero-extend the second draw into the full 64-bit register
    emitter.instruction("shl rax, 32");                                         // move the second draw into the high 32 bits
    emitter.instruction("mov ecx, DWORD PTR [rbp - 4]");                        // reload the low half as a zero-extended uint32
    emitter.instruction("or rax, rcx");                                         // compose both independent draws into one uint64
    emitter.instruction("add rsp, 16");                                         // release the scratch space before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the composed random uint64

    emitter.blank();
    emitter.comment("--- runtime: random_uniform64 ---");
    emitter.label_global("__rt_random_uniform64");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving sampling scratch
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the count and rejection limit
    emitter.instruction("sub rsp, 32");                                         // reserve aligned space for the element count and rejection limit
    emitter.instruction("cmp rdi, -1");                                         // is the inclusive width UINT64_MAX?
    emitter.instruction("jne __rt_random_uniform64_bounded_x86");               // no — the width can be incremented without wrapping
    emitter.instruction("call __rt_random_u64");                                // the whole uint64 space is admissible, so draw once
    emitter.instruction("jmp __rt_random_uniform64_done_x86");                  // and return it unreduced

    emitter.label("__rt_random_uniform64_bounded_x86");
    emitter.instruction("add rdi, 1");                                          // convert the inclusive width into an exclusive element count
    emitter.instruction("mov rcx, rdi");                                        // copy the count so the high half can be tested
    emitter.instruction("shr rcx, 32");                                         // does the count need more than 32 bits?
    emitter.instruction("jnz __rt_random_uniform64_wide_x86");                  // yes — sample across the full uint64 space
    emitter.instruction("call __rt_random_uniform");                            // no — reuse the 32-bit rejection sampler with the count in edi
    emitter.instruction("mov eax, eax");                                        // zero-extend its uint32 result into the 64-bit return register
    emitter.instruction("jmp __rt_random_uniform64_done_x86");                  // its result is already in [0, count)

    emitter.label("__rt_random_uniform64_wide_x86");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the element count across the sampling loop
    emitter.instruction("mov rax, -1");                                         // load UINT64_MAX as the rejection-limit dividend
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before the 64-bit division
    emitter.instruction("div rdi");                                             // rdx = UINT64_MAX % count
    emitter.instruction("mov rax, -1");                                         // reload UINT64_MAX to subtract the remainder from it
    emitter.instruction("sub rax, rdx");                                        // UINT64_MAX - (UINT64_MAX % count)
    emitter.instruction("sub rax, 1");                                          // limit = the largest value that reduces without bias
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the rejection limit across the sampling loop

    emitter.label("__rt_random_uniform64_loop_x86");
    emitter.instruction("call __rt_random_u64");                                // draw a fresh uint64 candidate
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // candidate inside the unbiased prefix?
    emitter.instruction("ja __rt_random_uniform64_loop_x86");                   // no — discard and resample
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before reducing modulo the count
    emitter.instruction("div QWORD PTR [rbp - 8]");                             // rdx = candidate % count
    emitter.instruction("mov rax, rdx");                                        // return the unbiased remainder

    emitter.label("__rt_random_uniform64_done_x86");
    emitter.instruction("add rsp, 32");                                         // release the sampling scratch slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the unbiased value in [0, umax]
}
