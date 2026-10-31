//! Purpose:
//! Emits the `__rt_stripos` runtime helper assembly: the case-insensitive twin of
//! `__rt_strpos`, scanning left to right for the first occurrence of a needle.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - The ABI is byte-for-byte identical to `__rt_strpos`, so `lower_string_position`
//!   drives both spellings through the same `$offset` normalization, `ValueError` guard,
//!   and match rebase. Only the candidate scan and comparison differ.
//! - Folding is ASCII-only (`A`-`Z` -> `a`-`z`), matching php-src's locale-independent
//!   `zend_tolower_ascii`. That is why `stripos("Été", "é")` is 3 rather than 1: the two
//!   non-ASCII lead bytes are compared verbatim.
//! - The first needle byte is scanned with libc `memchr` for both ASCII case variants,
//!   so a 100-byte haystack costs one vectorized scan instead of one branchy comparison
//!   per byte. Only real first-byte candidates enter the folded comparison loop.

use crate::codegen_support::{abi, emit::Emitter};
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_stripos` runtime helper for the active target.
///
/// ABI (AArch64): x1/x2 = haystack pointer/length, x3/x4 = needle pointer/length.
/// ABI (x86_64): rdi/rsi = haystack pointer/length, rdx/rcx = needle pointer/length.
/// Output: match offset, or -1 when the needle is absent.
pub fn emit_stripos(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stripos_linux_x86_64(emitter);
        return;
    }
    emit_stripos_aarch64(emitter);
}

/// Emits the AArch64 `__rt_stripos` helper.
///
/// State registers:
/// - `x19`/`x20` = haystack pointer/length
/// - `x21`/`x22` = needle pointer/length
/// - `x23` = last valid candidate start
/// - `x24` = current candidate offset
/// - `x25`/`x26` = first needle byte, original and folded
/// - `x27` = accepted candidate offset
/// - `x28` = needle comparison index
fn emit_stripos_aarch64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: stripos ---");
    e.label_global("__rt_stripos");
    e.instruction("cbz x4, __rt_stripos_empty");                                // an empty needle matches at offset zero
    e.instruction("cmp x4, x2");                                                // reject needles longer than the haystack
    e.instruction("b.gt __rt_stripos_notfound_early");                          // a long needle cannot fit
    e.instruction("sub sp, sp, #112");                                          // reserve the search state and linkage pair
    e.instruction("stp x29, x30, [sp, #96]");                                   // preserve the caller frame and return address
    e.instruction("add x29, sp, #96");                                          // establish a stable frame base
    e.instruction("stp x19, x20, [sp, #0]");                                    // preserve the haystack pointer and length
    e.instruction("stp x21, x22, [sp, #16]");                                   // preserve the needle pointer and length
    e.instruction("stp x23, x24, [sp, #32]");                                   // preserve the last candidate and current offset
    e.instruction("stp x25, x26, [sp, #48]");                                   // preserve both first-byte case variants
    e.instruction("stp x27, x28, [sp, #64]");                                   // preserve the candidate and comparison index
    e.instruction("mov x19, x1");                                               // x19 = haystack pointer
    e.instruction("mov x20, x2");                                               // x20 = haystack length
    e.instruction("mov x21, x3");                                               // x21 = needle pointer
    e.instruction("mov x22, x4");                                               // x22 = needle length
    e.instruction("sub x23, x20, x22");                                         // last valid candidate start
    e.instruction("mov x24, xzr");                                              // scan from the first candidate
    e.instruction("ldrb w25, [x21]");                                           // cache the needle's first byte
    e.instruction("mov w26, w25");                                              // start the lowercase scan key at the original byte
    e.instruction("cmp w26, #65");                                              // is the first byte at or above 'A'?
    e.instruction("b.lt __rt_stripos_lower_ready");                             // bytes below 'A' are already lowercase
    e.instruction("cmp w26, #90");                                              // is the first byte at or below 'Z'?
    e.instruction("b.gt __rt_stripos_lower_ready");                             // bytes above 'Z' are already lowercase
    e.instruction("add w26, w26, #32");                                         // fold the uppercase first byte to lowercase
    e.label("__rt_stripos_lower_ready");
    e.instruction("mov w27, w25");                                              // start the uppercase scan key at the original byte
    e.instruction("cmp w27, #97");                                              // is the first byte at or above 'a'?
    e.instruction("b.lt __rt_stripos_upper_ready");                             // bytes below 'a' are already uppercase
    e.instruction("cmp w27, #122");                                             // is the first byte at or below 'z'?
    e.instruction("b.gt __rt_stripos_upper_ready");                             // bytes above 'z' are already uppercase
    e.instruction("sub w27, w27, #32");                                         // fold the lowercase first byte to uppercase
    e.label("__rt_stripos_upper_ready");
    e.instruction("cmp w26, w27");                                              // does this first byte have two case variants?
    e.instruction("b.eq __rt_stripos_single_scan");                             // non-letters need only one memchr pass
    e.instruction("mov w25, w26");                                              // scan for the lowercase variant first
    e.instruction("mov w26, w27");                                              // scan for the uppercase variant second

    // -- candidate scan: take the nearest of the two first-byte variants --
    e.label("__rt_stripos_dual_scan");
    e.instruction("add x0, x19, x24");                                          // search from the current candidate offset
    e.instruction("mov w1, w25");                                               // pass the original first needle byte
    e.instruction("sub x2, x20, x24");                                          // remaining haystack bytes from the candidate
    e.bl_c("memchr");                                                           // find the next original-case candidate
    e.instruction("mov x27, x0");                                               // preserve the original-case candidate
    e.instruction("add x0, x19, x24");                                          // restart at the same candidate offset
    e.instruction("mov w1, w26");                                               // pass the folded first needle byte
    e.instruction("sub x2, x20, x24");                                          // the folded scan covers the same window
    e.bl_c("memchr");                                                           // find the next folded-case candidate
    e.instruction("cbz x27, __rt_stripos_dual_second");                         // no original-case candidate was found
    e.instruction("cbz x0, __rt_stripos_dual_have");                            // no folded-case candidate was found
    e.instruction("cmp x27, x0");                                               // which candidate starts first?
    e.instruction("csel x27, x27, x0, lo");                                     // keep the earliest candidate
    e.instruction("b __rt_stripos_dual_have");                                  // compare the chosen candidate
    e.label("__rt_stripos_dual_second");
    e.instruction("cbz x0, __rt_stripos_notfound");                             // neither case variant remains
    e.instruction("mov x27, x0");                                               // the folded candidate is the only one
    e.label("__rt_stripos_dual_have");
    e.instruction("sub x27, x27, x19");                                         // convert the candidate pointer into an offset
    e.instruction("mov x28, #1");                                               // remember the dual-scan mode
    e.instruction("str x28, [sp, #80]");                                        // publish the scan mode for the retry path
    e.instruction("b __rt_stripos_compare");                                    // compare the full needle at this candidate

    // -- candidate scan for a first byte with no second case variant --
    e.label("__rt_stripos_single_scan");
    e.instruction("add x0, x19, x24");                                          // search from the current candidate offset
    e.instruction("mov w1, w25");                                               // pass the only first-byte variant
    e.instruction("sub x2, x20, x24");                                          // remaining haystack bytes from the candidate
    e.bl_c("memchr");                                                           // find the next first-byte candidate
    e.instruction("cbz x0, __rt_stripos_notfound");                             // no first-byte candidate remains
    e.instruction("sub x27, x0, x19");                                          // convert the candidate pointer into an offset
    e.instruction("str xzr, [sp, #80]");                                        // remember the single-scan mode

    // -- folded comparison of the full needle at the accepted candidate --
    e.label("__rt_stripos_compare");
    e.instruction("cmp x27, x23");                                              // does the full needle still fit here?
    e.instruction("b.gt __rt_stripos_notfound");                                // a candidate past the window cannot match
    e.instruction("mov x28, xzr");                                              // start the needle comparison at index zero
    abi::emit_symbol_address(e, "x15", "_ascii_lower");                        // ASCII lowercase lookup table
    e.label("__rt_stripos_compare_loop");
    e.instruction("cmp x28, x22");                                              // has the whole needle matched?
    e.instruction("b.hs __rt_stripos_found");                                   // all needle bytes matched
    e.instruction("add x9, x27, x28");                                          // compute the haystack index of this needle byte
    e.instruction("ldrb w10, [x19, x9]");                                       // load the haystack byte
    e.instruction("ldrb w10, [x15, x10]");                                      // fold it through the lowercase table
    e.instruction("ldrb w11, [x21, x28]");                                      // load the needle byte
    e.instruction("ldrb w11, [x15, x11]");                                      // fold it through the lowercase table
    e.instruction("cmp w10, w11");                                              // compare the folded haystack and needle bytes
    e.instruction("b.ne __rt_stripos_next");                                    // abandon this candidate on the first mismatch
    e.instruction("add x28, x28, #1");                                          // advance to the next needle byte
    e.instruction("b __rt_stripos_compare_loop");                               // keep comparing the current candidate

    // -- advance past the failed candidate and choose the scan mode again --
    e.label("__rt_stripos_next");
    e.instruction("add x24, x27, #1");                                          // advance to the byte after the failed candidate
    e.instruction("cmp x24, x23");                                              // is another candidate still in range?
    e.instruction("b.gt __rt_stripos_notfound");                                // no candidate can start past the window
    e.instruction("ldr x9, [sp, #80]");                                         // reload the scan mode
    e.instruction("cbnz x9, __rt_stripos_dual_scan");                           // a letter first byte needs both case variants
    e.instruction("b __rt_stripos_single_scan");                                // a non-letter first byte needs one scan

    // -- result epilogue --
    e.label("__rt_stripos_found");
    e.instruction("mov x0, x27");                                               // return the matching candidate offset
    e.instruction("b __rt_stripos_done");                                       // restore the search frame
    e.label("__rt_stripos_notfound");
    e.instruction("mov x0, #-1");                                               // return the not-found sentinel
    e.label("__rt_stripos_done");
    e.instruction("ldp x19, x20, [sp, #0]");                                    // restore the haystack pointer and length
    e.instruction("ldp x21, x22, [sp, #16]");                                   // restore the needle pointer and length
    e.instruction("ldp x23, x24, [sp, #32]");                                   // restore the last candidate and current offset
    e.instruction("ldp x25, x26, [sp, #48]");                                   // restore both first-byte case variants
    e.instruction("ldp x27, x28, [sp, #64]");                                   // restore the candidate and comparison index
    e.instruction("ldp x29, x30, [sp, #96]");                                   // restore the caller frame and return address
    e.instruction("add sp, sp, #112");                                          // release the search frame
    e.instruction("ret");                                                       // return the match offset or -1
    e.label("__rt_stripos_empty");
    e.instruction("mov x0, #0");                                                // empty needles match at offset zero
    e.instruction("ret");                                                       // return the empty-needle offset
    e.label("__rt_stripos_notfound_early");
    e.instruction("mov x0, #-1");                                               // a needle longer than the haystack cannot match
    e.instruction("ret");                                                       // return the not-found sentinel
}

/// Emits the x86_64 Linux `__rt_stripos` helper.
///
/// The search state lives in the frame because libc `memchr` may clobber every
/// caller-saved register:
/// - `[rbp-8]`/`[rbp-16]` = haystack pointer/length
/// - `[rbp-24]`/`[rbp-32]` = needle pointer/length
/// - `[rbp-40]` = last valid candidate start
/// - `[rbp-48]` = current candidate offset
/// - `[rbp-56]`/`[rbp-64]` = first needle byte, original and folded
/// - `[rbp-72]` = accepted candidate offset
/// - `[rbp-80]` = needle comparison index
/// - `[rbp-88]` = scan mode (0 single, 1 dual)
/// - `[rbp-96]` = first memchr result
fn emit_stripos_linux_x86_64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: stripos ---");
    e.label_global("__rt_stripos");
    e.instruction("test rcx, rcx");                                             // an empty needle matches at offset zero
    e.instruction("jz __rt_stripos_empty_linux_x86_64");                        // return zero for an empty needle
    e.instruction("cmp rcx, rsi");                                              // reject needles longer than the haystack
    e.instruction("jg __rt_stripos_notfound_early_linux_x86_64");               // a long needle cannot fit
    e.instruction("push rbp");                                                  // preserve the caller frame pointer
    e.instruction("mov rbp, rsp");                                              // establish a stable frame base
    e.instruction("sub rsp, 96");                                               // reserve the search state slots
    e.instruction("mov QWORD PTR [rbp - 8], rdi");                              // save the haystack pointer
    e.instruction("mov QWORD PTR [rbp - 16], rsi");                             // save the haystack length
    e.instruction("mov QWORD PTR [rbp - 24], rdx");                             // save the needle pointer
    e.instruction("mov QWORD PTR [rbp - 32], rcx");                             // save the needle length
    e.instruction("mov rax, rsi");                                              // copy the haystack length
    e.instruction("sub rax, rcx");                                              // last valid candidate start
    e.instruction("mov QWORD PTR [rbp - 40], rax");                             // publish the last candidate start
    e.instruction("mov QWORD PTR [rbp - 48], 0");                               // scan from the first candidate
    e.instruction("movzx eax, BYTE PTR [rdx]");                                 // load the needle's first byte
    e.instruction("mov r10, rax");                                              // preserve the original first byte
    e.instruction("mov QWORD PTR [rbp - 56], rax");                             // start the lowercase scan key at the original byte
    e.instruction("mov QWORD PTR [rbp - 64], rax");                             // start the uppercase scan key at the original byte
    e.instruction("cmp al, 65");                                                // is the first byte at or above 'A'?
    e.instruction("jb __rt_stripos_lower_ready_linux_x86_64");                  // bytes below 'A' are already lowercase
    e.instruction("cmp al, 90");                                                // is the first byte at or below 'Z'?
    e.instruction("ja __rt_stripos_lower_ready_linux_x86_64");                  // bytes above 'Z' are already lowercase
    e.instruction("add al, 32");                                                // fold the uppercase first byte to lowercase
    e.instruction("movzx eax, al");                                             // clear the high byte after folding
    e.instruction("mov QWORD PTR [rbp - 56], rax");                             // save the lowercase scan key
    e.label("__rt_stripos_lower_ready_linux_x86_64");
    e.instruction("mov eax, r10d");                                             // restore the original first byte for the uppercase fold
    e.instruction("cmp al, 97");                                                // is the first byte at or above 'a'?
    e.instruction("jb __rt_stripos_upper_ready_linux_x86_64");                  // bytes below 'a' are already uppercase
    e.instruction("cmp al, 122");                                               // is the first byte at or below 'z'?
    e.instruction("ja __rt_stripos_upper_ready_linux_x86_64");                  // bytes above 'z' are already uppercase
    e.instruction("sub al, 32");                                                // fold the lowercase first byte to uppercase
    e.instruction("movzx eax, al");                                             // clear the high byte after folding
    e.instruction("mov QWORD PTR [rbp - 64], rax");                             // save the uppercase scan key
    e.label("__rt_stripos_upper_ready_linux_x86_64");
    e.instruction("mov r10, QWORD PTR [rbp - 56]");                             // reload the lowercase scan key
    e.instruction("cmp r10, QWORD PTR [rbp - 64]");                             // does this first byte have two case variants?
    e.instruction("je __rt_stripos_single_scan_linux_x86_64");                  // non-letters need only one memchr pass
    e.instruction("mov QWORD PTR [rbp - 88], 1");                               // remember the dual-scan mode

    // -- candidate scan: take the nearest of the two first-byte variants --
    e.label("__rt_stripos_dual_scan_linux_x86_64");
    e.instruction("mov rdi, QWORD PTR [rbp - 8]");                              // reload the haystack pointer
    e.instruction("add rdi, QWORD PTR [rbp - 48]");                             // search from the current candidate offset
    e.instruction("mov esi, DWORD PTR [rbp - 56]");                             // pass the original first needle byte
    e.instruction("mov rdx, QWORD PTR [rbp - 16]");                             // reload the haystack length
    e.instruction("sub rdx, QWORD PTR [rbp - 48]");                             // remaining haystack bytes from the candidate
    e.bl_c("memchr");                                                           // find the next original-case candidate
    e.instruction("mov QWORD PTR [rbp - 96], rax");                             // preserve the original-case candidate
    e.instruction("mov rdi, QWORD PTR [rbp - 8]");                              // restart at the same candidate offset
    e.instruction("add rdi, QWORD PTR [rbp - 48]");                             // recompute the search base
    e.instruction("mov esi, DWORD PTR [rbp - 64]");                             // pass the folded first needle byte
    e.instruction("mov rdx, QWORD PTR [rbp - 16]");                             // reload the haystack length
    e.instruction("sub rdx, QWORD PTR [rbp - 48]");                             // the folded scan covers the same window
    e.bl_c("memchr");                                                           // find the next folded-case candidate
    e.instruction("mov r11, QWORD PTR [rbp - 96]");                             // reload the original-case candidate
    e.instruction("test r11, r11");                                             // was an original-case candidate found?
    e.instruction("jz __rt_stripos_dual_second_linux_x86_64");                  // use the folded candidate when it is alone
    e.instruction("test rax, rax");                                             // was a folded-case candidate found?
    e.instruction("jz __rt_stripos_dual_have_linux_x86_64");                    // keep the original candidate when it is alone
    e.instruction("cmp r11, rax");                                              // which candidate starts first?
    e.instruction("cmovg r11, rax");                                            // keep the earliest candidate
    e.instruction("jmp __rt_stripos_dual_have_linux_x86_64");                   // compare the chosen candidate
    e.label("__rt_stripos_dual_second_linux_x86_64");
    e.instruction("test rax, rax");                                             // neither case variant remains?
    e.instruction("jz __rt_stripos_notfound_linux_x86_64");                     // report the not-found sentinel
    e.instruction("mov r11, rax");                                              // the folded candidate is the only one
    e.label("__rt_stripos_dual_have_linux_x86_64");
    e.instruction("sub r11, QWORD PTR [rbp - 8]");                              // convert the candidate pointer into an offset
    e.instruction("mov QWORD PTR [rbp - 72], r11");                             // publish the accepted candidate
    e.instruction("jmp __rt_stripos_compare_linux_x86_64");                     // compare the full needle

    // -- candidate scan for a first byte with no second case variant --
    e.label("__rt_stripos_single_scan_linux_x86_64");
    e.instruction("mov QWORD PTR [rbp - 88], 0");                               // remember the single-scan mode
    e.instruction("mov rdi, QWORD PTR [rbp - 8]");                              // reload the haystack pointer
    e.instruction("add rdi, QWORD PTR [rbp - 48]");                             // search from the current candidate offset
    e.instruction("mov esi, DWORD PTR [rbp - 56]");                             // pass the only first-byte variant
    e.instruction("mov rdx, QWORD PTR [rbp - 16]");                             // reload the haystack length
    e.instruction("sub rdx, QWORD PTR [rbp - 48]");                             // remaining haystack bytes from the candidate
    e.bl_c("memchr");                                                           // find the next first-byte candidate
    e.instruction("test rax, rax");                                             // did memchr find a candidate?
    e.instruction("jz __rt_stripos_notfound_linux_x86_64");                     // no first-byte candidate remains
    e.instruction("sub rax, QWORD PTR [rbp - 8]");                              // convert the candidate pointer into an offset
    e.instruction("mov QWORD PTR [rbp - 72], rax");                             // publish the accepted candidate

    // -- folded comparison of the full needle at the accepted candidate --
    e.label("__rt_stripos_compare_linux_x86_64");
    e.instruction("mov r11, QWORD PTR [rbp - 72]");                             // reload the accepted candidate
    e.instruction("cmp r11, QWORD PTR [rbp - 40]");                             // does the full needle still fit here?
    e.instruction("jg __rt_stripos_notfound_linux_x86_64");                     // a candidate past the window cannot match
    e.instruction("mov QWORD PTR [rbp - 80], 0");                               // start the needle comparison at index zero
    abi::emit_symbol_address(e, "rsi", "_ascii_lower");                        // ASCII lowercase lookup table
    e.label("__rt_stripos_compare_loop_linux_x86_64");
    e.instruction("mov r10, QWORD PTR [rbp - 80]");                             // reload the comparison index
    e.instruction("cmp r10, QWORD PTR [rbp - 32]");                             // has the whole needle matched?
    e.instruction("jge __rt_stripos_found_linux_x86_64");                       // all needle bytes matched
    e.instruction("mov r8, QWORD PTR [rbp - 8]");                               // reload the haystack pointer
    e.instruction("add r8, r11");                                               // advance to the accepted candidate
    e.instruction("add r8, r10");                                               // index the current haystack byte
    e.instruction("movzx eax, BYTE PTR [r8]");                                  // load the haystack byte
    e.instruction("movzx eax, BYTE PTR [rsi + rax]");                           // fold it through the lowercase table
    e.instruction("mov r9, QWORD PTR [rbp - 24]");                              // reload the needle pointer
    e.instruction("add r9, r10");                                               // index the current needle byte
    e.instruction("movzx edx, BYTE PTR [r9]");                                  // load the needle byte
    e.instruction("movzx edx, BYTE PTR [rsi + rdx]");                           // fold it through the lowercase table
    e.instruction("cmp al, dl");                                                // compare the folded haystack and needle bytes
    e.instruction("jne __rt_stripos_next_linux_x86_64");                        // abandon this candidate on the first mismatch
    e.instruction("add QWORD PTR [rbp - 80], 1");                               // advance to the next needle byte
    e.instruction("jmp __rt_stripos_compare_loop_linux_x86_64");                // keep comparing the current candidate

    // -- advance past the failed candidate and choose the scan mode again --
    e.label("__rt_stripos_next_linux_x86_64");
    e.instruction("mov r11, QWORD PTR [rbp - 72]");                             // reload the failed candidate offset
    e.instruction("add r11, 1");                                                // advance to the byte after it
    e.instruction("mov QWORD PTR [rbp - 48], r11");                             // publish the next candidate offset
    e.instruction("cmp r11, QWORD PTR [rbp - 40]");                             // is another candidate still in range?
    e.instruction("jg __rt_stripos_notfound_linux_x86_64");                     // no candidate can start past the window
    e.instruction("cmp QWORD PTR [rbp - 88], 0");                               // which scan mode was active?
    e.instruction("jne __rt_stripos_dual_scan_linux_x86_64");                   // a letter first byte needs both case variants
    e.instruction("jmp __rt_stripos_single_scan_linux_x86_64");                 // a non-letter first byte needs one scan

    // -- result epilogue --
    e.label("__rt_stripos_found_linux_x86_64");
    e.instruction("mov rax, QWORD PTR [rbp - 72]");                             // return the matching candidate offset
    e.instruction("leave");                                                     // restore the caller frame pointer
    e.instruction("ret");                                                       // return the match offset
    e.label("__rt_stripos_notfound_linux_x86_64");
    e.instruction("mov rax, -1");                                               // return the not-found sentinel
    e.instruction("leave");                                                     // restore the caller frame pointer
    e.instruction("ret");                                                       // return the not-found sentinel
    e.label("__rt_stripos_empty_linux_x86_64");
    e.instruction("xor eax, eax");                                              // empty needles match at offset zero
    e.instruction("ret");                                                       // return the empty-needle offset
    e.label("__rt_stripos_notfound_early_linux_x86_64");
    e.instruction("mov rax, -1");                                               // a needle longer than the haystack cannot match
    e.instruction("ret");                                                       // return the not-found sentinel
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Verifies both targets use the memchr candidate scan rather than the old byte loop.
    #[test]
    fn stripos_emits_the_candidate_scan_on_all_targets() {
        for name in ["macos-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).expect("supported target");
            let mut emitter = Emitter::new(target);
            emit_stripos(&mut emitter);
            let assembly = emitter.output();
            assert!(assembly.contains("memchr"), "{name}: {assembly}");
            assert!(assembly.contains("_ascii_lower"), "{name}: {assembly}");
            assert!(!assembly.contains("__rt_stripos_outer"), "{name}: {assembly}");
        }
    }
}
