//! Purpose:
//! Emits the `__rt_strrpos` runtime helper assembly for the last occurrence of a needle.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - The ABI matches `__rt_strpos`: x1/x2 = haystack pointer/length, x3/x4 = needle
//!   pointer/length on AArch64, and the SysV string pairs on x86_64.
//! - The scan starts at the rightmost valid candidate and walks left. It compares only the
//!   first needle byte per position and calls `memcmp` for the full needle only on a
//!   first-byte hit, so a late match is found without scanning the whole haystack.
//! - An empty needle returns the haystack length, and a needle longer than the haystack
//!   returns -1, exactly like php-src.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_strrpos` helper for the active target.
pub fn emit_strrpos(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strrpos_linux_x86_64(emitter);
        return;
    }
    emit_strrpos_aarch64(emitter);
}

/// Emits the AArch64 `__rt_strrpos` helper.
///
/// State registers: `x19`/`x20` = haystack, `x21`/`x22` = needle, `x23` = candidate
/// offset. The first needle byte stays in `w9` across the scan.
fn emit_strrpos_aarch64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: strrpos ---");
    e.label_global("__rt_strrpos");
    e.instruction("cbz x4, __rt_strrpos_empty");                                // an empty needle returns the haystack length
    e.instruction("cmp x4, x2");                                                // reject needles longer than the haystack
    e.instruction("b.gt __rt_strrpos_notfound_early");                          // a long needle cannot fit
    e.instruction("sub sp, sp, #64");                                           // reserve the search state and linkage pair
    e.instruction("stp x29, x30, [sp, #48]");                                   // preserve the caller frame and return address
    e.instruction("add x29, sp, #48");                                          // establish a stable frame base
    e.instruction("stp x19, x20, [sp, #0]");                                    // preserve the haystack pointer and length
    e.instruction("stp x21, x22, [sp, #16]");                                   // preserve the needle pointer and length
    e.instruction("str x23, [sp, #32]");                                        // preserve the candidate offset
    e.instruction("mov x19, x1");                                               // x19 = haystack pointer
    e.instruction("mov x20, x2");                                               // x20 = haystack length
    e.instruction("mov x21, x3");                                               // x21 = needle pointer
    e.instruction("mov x22, x4");                                               // x22 = needle length
    e.instruction("sub x23, x20, x22");                                         // start at the rightmost valid candidate
    e.instruction("ldrb w9, [x21]");                                            // cache the needle's first byte

    e.label("__rt_strrpos_vector");
    e.instruction("cmp x23, #15");                                              // does a full sixteen-byte block fit before the candidate?
    e.instruction("b.lo __rt_strrpos_tail");                                    // short tails use the scalar first-byte scan
    e.instruction("sub x12, x23, #15");                                         // start of the block ending at the candidate
    e.instruction("add x0, x19, x12");                                          // address the block inside the haystack
    e.instruction("ldr q0, [x0]");                                              // load sixteen candidate bytes
    e.instruction("dup v1.16b, w9");                                            // broadcast the first needle byte
    e.instruction("cmeq v0.16b, v0.16b, v1.16b");                               // compare every lane against the needle byte
    e.instruction("umaxv b2, v0.16b");                                          // collapse the comparison lanes
    e.instruction("umov w10, v2.b[0]");                                         // read the collapsed match flag
    e.instruction("cbz w10, __rt_strrpos_block_miss");                          // no lane matched in this block
    e.instruction("mov x13, x23");                                              // scan this block from its right edge
    e.label("__rt_strrpos_block_scan");
    e.instruction("ldrb w11, [x19, x13]");                                      // load the scalar first byte inside the block
    e.instruction("cmp w11, w9");                                               // does it match the needle's first byte?
    e.instruction("b.eq __rt_strrpos_candidate");                              // compare the full needle at this candidate
    e.instruction("cmp x13, x12");                                              // has the whole block been scanned?
    e.instruction("b.eq __rt_strrpos_block_miss");                             // move to the block before this one
    e.instruction("sub x13, x13, #1");                                          // move one byte left inside the block
    e.instruction("b __rt_strrpos_block_scan");                                // keep scanning the block

    e.label("__rt_strrpos_candidate");
    e.instruction("mov x23, x13");                                              // publish the candidate before the libc call
    e.instruction("add x0, x19, x23");                                          // compare the full needle at this candidate
    e.instruction("mov x1, x21");                                               // pass the needle pointer
    e.instruction("mov x2, x22");                                               // pass the needle length
    e.bl_c("memcmp");                                                           // compare the remaining needle bytes
    e.instruction("cbz w0, __rt_strrpos_found");                                // a zero result is a full match
    e.instruction("cbz x23, __rt_strrpos_notfound");                            // position zero was the final candidate
    e.instruction("sub x23, x23, #1");                                          // move one candidate to the left
    e.instruction("ldrb w9, [x21]");                                            // restore the first byte after the libc call
    e.instruction("b __rt_strrpos_vector");                                    // scan the next block

    e.label("__rt_strrpos_block_miss");
    e.instruction("cbz x12, __rt_strrpos_notfound");                            // no earlier block remains
    e.instruction("sub x23, x12, #1");                                          // continue just before this block
    e.instruction("ldrb w9, [x21]");                                            // restore the first byte after vector reduction
    e.instruction("b __rt_strrpos_vector");                                    // scan the previous block

    e.label("__rt_strrpos_tail");
    e.instruction("ldrb w10, [x19, x23]");                                      // load the candidate's first haystack byte
    e.instruction("cmp w10, w9");                                               // does it match the needle's first byte?
    e.instruction("b.ne __rt_strrpos_tail_prev");                              // only a first-byte hit needs the full comparison
    e.instruction("add x0, x19, x23");                                          // compare the full needle at this candidate
    e.instruction("mov x1, x21");                                               // pass the needle pointer
    e.instruction("mov x2, x22");                                               // pass the needle length
    e.bl_c("memcmp");                                                           // compare the remaining needle bytes
    e.instruction("cbz w0, __rt_strrpos_found");                                // a zero result is a full match
    e.label("__rt_strrpos_tail_prev");
    e.instruction("cbz x23, __rt_strrpos_notfound");                            // position zero was the final candidate
    e.instruction("sub x23, x23, #1");                                          // move one candidate to the left
    e.instruction("ldrb w9, [x21]");                                            // restore the first byte after the libc call
    e.instruction("b __rt_strrpos_tail");                                      // continue the scalar first-byte scan

    e.label("__rt_strrpos_found");
    e.instruction("mov x0, x23");                                               // return the last matching candidate
    e.instruction("b __rt_strrpos_done");                                       // restore the search frame
    e.label("__rt_strrpos_notfound");
    e.instruction("mov x0, #-1");                                               // no candidate matched
    e.label("__rt_strrpos_done");
    e.instruction("ldp x19, x20, [sp, #0]");                                    // restore the haystack pointer and length
    e.instruction("ldp x21, x22, [sp, #16]");                                   // restore the needle pointer and length
    e.instruction("ldr x23, [sp, #32]");                                        // restore the candidate offset
    e.instruction("ldp x29, x30, [sp, #48]");                                   // restore the caller frame and return address
    e.instruction("add sp, sp, #64");                                           // release the search frame
    e.instruction("ret");                                                       // return the last match offset
    e.label("__rt_strrpos_empty");
    e.instruction("mov x0, x2");                                                // an empty needle matches just after the last byte
    e.instruction("ret");                                                       // return the haystack length
    e.label("__rt_strrpos_notfound_early");
    e.instruction("mov x0, #-1");                                               // a needle longer than the haystack cannot match
    e.instruction("ret");                                                       // return the not-found sentinel
}

/// Emits the x86_64 SysV `__rt_strrpos` helper.
///
/// Frame state: `[rbp-8]`/`[rbp-16]` = haystack, `[rbp-24]`/`[rbp-32]` = needle,
/// `[rbp-40]` = candidate offset, `[rbp-48]` = first needle byte.
fn emit_strrpos_linux_x86_64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: strrpos ---");
    e.label_global("__rt_strrpos");
    e.instruction("test rcx, rcx");                                             // an empty needle returns the haystack length
    e.instruction("jz __rt_strrpos_empty_linux_x86_64");                        // return the haystack length
    e.instruction("cmp rcx, rsi");                                              // reject needles longer than the haystack
    e.instruction("jg __rt_strrpos_notfound_early_linux_x86_64");               // a long needle cannot fit
    e.instruction("push rbp");                                                  // preserve the caller frame pointer
    e.instruction("mov rbp, rsp");                                              // establish a stable frame base
    e.instruction("sub rsp, 48");                                               // reserve the search state slots
    e.instruction("mov QWORD PTR [rbp - 8], rdi");                              // save the haystack pointer
    e.instruction("mov QWORD PTR [rbp - 16], rsi");                             // save the haystack length
    e.instruction("mov QWORD PTR [rbp - 24], rdx");                             // save the needle pointer
    e.instruction("mov QWORD PTR [rbp - 32], rcx");                             // save the needle length
    e.instruction("mov rax, rsi");                                              // copy the haystack length
    e.instruction("sub rax, rcx");                                              // start at the rightmost valid candidate
    e.instruction("mov QWORD PTR [rbp - 40], rax");                             // publish the candidate offset
    e.instruction("movzx eax, BYTE PTR [rdx]");                                 // load the needle's first byte
    e.instruction("mov QWORD PTR [rbp - 48], rax");                             // cache the first-byte search key

    e.label("__rt_strrpos_scan_linux_x86_64");
    e.instruction("mov r11, QWORD PTR [rbp - 40]");                             // reload the candidate offset
    e.instruction("mov r8, QWORD PTR [rbp - 8]");                               // reload the haystack pointer
    e.instruction("movzx eax, BYTE PTR [r8 + r11]");                            // load the candidate's first haystack byte
    e.instruction("cmp eax, DWORD PTR [rbp - 48]");                             // does it match the needle's first byte?
    e.instruction("jne __rt_strrpos_prev_linux_x86_64");                        // only a first-byte hit needs the full comparison
    e.instruction("mov rdi, r8");                                               // pass the haystack pointer
    e.instruction("add rdi, r11");                                              // point at the candidate bytes
    e.instruction("mov rsi, QWORD PTR [rbp - 24]");                             // pass the needle pointer
    e.instruction("mov rdx, QWORD PTR [rbp - 32]");                             // pass the needle length
    e.bl_c("memcmp");                                                           // compare the remaining needle bytes
    e.instruction("test eax, eax");                                             // did the full needle compare equal?
    e.instruction("jz __rt_strrpos_found_linux_x86_64");                        // a zero result is a full match
    e.label("__rt_strrpos_prev_linux_x86_64");
    e.instruction("cmp QWORD PTR [rbp - 40], 0");                               // was position zero the final candidate?
    e.instruction("je __rt_strrpos_notfound_linux_x86_64");                     // no candidate matched
    e.instruction("sub QWORD PTR [rbp - 40], 1");                               // move one candidate to the left
    e.instruction("jmp __rt_strrpos_scan_linux_x86_64");                        // retry from the previous candidate

    e.label("__rt_strrpos_found_linux_x86_64");
    e.instruction("mov rax, QWORD PTR [rbp - 40]");                             // return the last matching candidate
    e.instruction("leave");                                                     // restore the caller frame pointer
    e.instruction("ret");                                                       // return the last match offset
    e.label("__rt_strrpos_notfound_linux_x86_64");
    e.instruction("mov rax, -1");                                               // no candidate matched
    e.instruction("leave");                                                     // restore the caller frame pointer
    e.instruction("ret");                                                       // return the not-found sentinel
    e.label("__rt_strrpos_empty_linux_x86_64");
    e.instruction("mov rax, rsi");                                              // an empty needle matches just after the last byte
    e.instruction("ret");                                                       // return the haystack length
    e.label("__rt_strrpos_notfound_early_linux_x86_64");
    e.instruction("mov rax, -1");                                               // a needle longer than the haystack cannot match
    e.instruction("ret");                                                       // return the not-found sentinel
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Verifies both targets compare the first byte before the full memcmp.
    #[test]
    fn strrpos_emits_the_reverse_candidate_scan_on_all_targets() {
        for name in ["macos-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).expect("supported target");
            let mut emitter = Emitter::new(target);
            emit_strrpos(&mut emitter);
            let assembly = emitter.output();
            assert!(assembly.contains("memcmp"), "{name}: {assembly}");
            assert!(!assembly.contains("__rt_strrpos_outer"), "{name}: {assembly}");
        }
    }
}
