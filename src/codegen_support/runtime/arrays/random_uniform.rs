//! Purpose:
//! Emits `__rt_<source>_uniform`, a uniform integer below a 32-bit bound, once per random source:
//! `__rt_random_uniform` over the CSPRNG and `__rt_mt_uniform` over the Mersenne Twister.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The reduction is php-src's `php_random_range32` (ext/random/random.c), draw for draw, so a
//!   seeded `shuffle()` or `array_rand()` consumes the twister exactly as php does: one draw
//!   first, even for a one-value range; a power-of-two bound masks; any other bound redraws while
//!   the draw is above `UINT32_MAX - (UINT32_MAX % bound) - 1` and then takes the remainder.
//! - A bound of zero stands for the full 2^32 range (php's `umax == UINT32_MAX`) and returns the
//!   draw unreduced.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_<source>_uniform(bound) -> [0, bound)`, drawing from `__rt_<source>_u32`.
///
/// ABI: bound in `w0` (ARM64) or `edi` (x86_64); result in `x0` / `rax`. Only the draws'
/// own scratch registers are clobbered, so x86_64 `r9` survives the call.
pub fn emit_random_uniform(emitter: &mut Emitter, source: &str) {
    if emitter.target.arch == Arch::X86_64 {
        emit_random_uniform_linux_x86_64(emitter, source);
        return;
    }
    let name = format!("__rt_{source}_uniform");
    let draw = format!("__rt_{source}_u32");
    emitter.blank();
    emitter.comment(&format!("--- runtime: {source}_uniform ---"));
    emitter.label_global(&name);
    emitter.instruction("sub sp, sp, #32");                                     // bound, rejection limit, frame pair
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a frame pointer
    emitter.instruction("str w0, [sp, #0]");                                    // save the exclusive upper bound
    emitter.instruction(&format!("bl {draw}"));                                 // php draws before looking at the bound
    emitter.instruction("ldr w1, [sp, #0]");                                    // reload the bound
    emitter.instruction(&format!("cbz w1, {name}_done"));                       // zero stands for the full 32-bit range
    emitter.instruction("sub w2, w1, #1");                                      // bound - 1
    emitter.instruction("tst w1, w2");                                          // is the bound a power of two?
    emitter.instruction(&format!("b.ne {name}_limit"));                         // no: compute the rejection limit
    emitter.instruction("and w0, w0, w2");                                      // a power of two only masks
    emitter.instruction(&format!("b {name}_done"));                             // done
    emitter.label(&format!("{name}_limit"));
    emitter.instruction("mov w3, #-1");                                         // UINT32_MAX
    emitter.instruction("udiv w4, w3, w1");                                     // UINT32_MAX / bound
    emitter.instruction("msub w4, w4, w1, w3");                                 // UINT32_MAX % bound
    emitter.instruction("sub w4, w3, w4");                                      // UINT32_MAX - remainder
    emitter.instruction("sub w4, w4, #1");                                      // limit, above which a draw is biased
    emitter.instruction("str w4, [sp, #8]");                                    // save the limit across redraws
    emitter.label(&format!("{name}_check"));
    emitter.instruction("ldr w4, [sp, #8]");                                    // reload the limit
    emitter.instruction("cmp w0, w4");                                          // is the draw at or under the limit?
    emitter.instruction(&format!("b.ls {name}_accept"));                        // yes: reduce it
    emitter.instruction(&format!("bl {draw}"));                                 // no: draw again, as php does
    emitter.instruction(&format!("b {name}_check"));                            // and test the new draw
    emitter.label(&format!("{name}_accept"));
    emitter.instruction("ldr w1, [sp, #0]");                                    // reload the bound
    emitter.instruction("udiv w2, w0, w1");                                     // draw / bound
    emitter.instruction("msub w0, w2, w1, w0");                                 // draw % bound
    emitter.label(&format!("{name}_done"));
    emitter.instruction("mov w0, w0");                                          // zero-extend the 32-bit result
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // return the sampled offset
}

/// Emits the x86_64 Linux implementation of `__rt_<source>_uniform`.
///
/// System V ABI: bound in `edi`, result in `eax`; `[rbp - 4]` keeps the bound and `[rbp - 8]`
/// the rejection limit across the draws.
fn emit_random_uniform_linux_x86_64(emitter: &mut Emitter, source: &str) {
    let name = format!("__rt_{source}_uniform");
    let draw = format!("__rt_{source}_u32");
    emitter.blank();
    emitter.comment(&format!("--- runtime: {source}_uniform ---"));
    emitter.label_global(&name);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a frame pointer
    emitter.instruction("sub rsp, 16");                                         // bound and rejection limit, keeping rsp aligned
    emitter.instruction("mov DWORD PTR [rbp - 4], edi");                        // save the exclusive upper bound
    emitter.instruction(&format!("call {draw}"));                               // php draws before looking at the bound
    emitter.instruction("mov ecx, DWORD PTR [rbp - 4]");                        // reload the bound
    emitter.instruction("test ecx, ecx");                                       // zero stands for the full 32-bit range
    emitter.instruction(&format!("jz {name}_done"));                            // return the draw unreduced
    emitter.instruction("lea edx, [rcx - 1]");                                  // bound - 1
    emitter.instruction("test ecx, edx");                                       // is the bound a power of two?
    emitter.instruction(&format!("jnz {name}_limit"));                          // no: compute the rejection limit
    emitter.instruction("and eax, edx");                                        // a power of two only masks
    emitter.instruction(&format!("jmp {name}_done"));                           // done
    emitter.label(&format!("{name}_limit"));
    emitter.instruction("mov r8d, eax");                                        // keep the draw across the division
    emitter.instruction("mov eax, 0xffffffff");                                 // UINT32_MAX
    emitter.instruction("xor edx, edx");                                        // clear the high half of the dividend
    emitter.instruction("div ecx");                                             // edx = UINT32_MAX % bound
    emitter.instruction("mov eax, 0xffffffff");                                 // UINT32_MAX
    emitter.instruction("sub eax, edx");                                        // UINT32_MAX - remainder
    emitter.instruction("sub eax, 1");                                          // limit, above which a draw is biased
    emitter.instruction("mov DWORD PTR [rbp - 8], eax");                        // save the limit across redraws
    emitter.instruction("mov eax, r8d");                                        // restore the draw
    emitter.label(&format!("{name}_check"));
    emitter.instruction("cmp eax, DWORD PTR [rbp - 8]");                        // is the draw at or under the limit?
    emitter.instruction(&format!("jbe {name}_accept"));                         // yes: reduce it
    emitter.instruction(&format!("call {draw}"));                               // no: draw again, as php does
    emitter.instruction(&format!("jmp {name}_check"));                          // and test the new draw
    emitter.label(&format!("{name}_accept"));
    emitter.instruction("xor edx, edx");                                        // clear the high half of the dividend
    emitter.instruction("div DWORD PTR [rbp - 4]");                             // edx = draw % bound
    emitter.instruction("mov eax, edx");                                        // the remainder is the sampled offset
    emitter.label(&format!("{name}_done"));
    emitter.instruction("mov eax, eax");                                        // zero-extend the 32-bit result
    emitter.instruction("add rsp, 16");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the sampled offset
}
