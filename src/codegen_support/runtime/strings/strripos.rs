//! Purpose:
//! Emits the `__rt_strripos` runtime helper assembly: the case-insensitive twin of
//! `__rt_strrpos`, scanning right to left for the last occurrence of a needle.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - The ABI is byte-for-byte identical to `__rt_strrpos`, so `lower_string_position` drives
//!   both spellings through the same `$offset` window trimming, `ValueError` guard, and match
//!   rebase. Only the per-byte comparison differs.
//! - Folding is ASCII-only (`A`-`Z` -> `a`-`z`), matching php-src's locale-independent
//!   `zend_tolower_ascii`; non-ASCII bytes are compared verbatim.

use crate::codegen_support::{abi, emit::Emitter};
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_strripos` runtime helper for AArch64.
///
/// ABI:
///   Input:  x1=haystack_ptr, x2=haystack_len, x3=needle_ptr, x4=needle_len
///   Output: x0 = byte offset of the last case-insensitive match, or -1 when absent
///
/// An empty needle returns the haystack length (the last valid starting position) and a
/// needle longer than the haystack returns the not-found sentinel immediately, exactly as
/// `__rt_strrpos` decides those two edge cases.
/// Dispatches to `emit_strripos_linux_x86_64` on x86_64.
pub fn emit_strripos(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strripos_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: strripos ---");
    emitter.label_global("__rt_strripos");

    // -- edge cases --
    emitter.instruction("cbz x4, __rt_strripos_empty");                         // empty needle returns last position
    emitter.instruction("cmp x4, x2");                                          // compare needle length with haystack length
    emitter.instruction("b.gt __rt_strripos_notfound");                         // needle longer than haystack, can't match
    emitter.instruction("sub x5, x2, x4");                                      // start searching from rightmost valid position

    // -- outer loop: try matching needle from right to left --
    abi::emit_symbol_address(emitter, "x12", "_ascii_lower");                  // ASCII lowercase lookup table
    emitter.label("__rt_strripos_outer");
    emitter.instruction("mov x6, #0");                                          // reset needle comparison index
    emitter.label("__rt_strripos_inner");
    emitter.instruction("cmp x6, x4");                                          // check if all needle bytes matched
    emitter.instruction("b.ge __rt_strripos_found");                            // all matched, found at position x5
    emitter.instruction("add x7, x5, x6");                                      // compute haystack index = pos + needle_idx
    emitter.instruction("ldrb w8, [x1, x7]");                                   // load haystack byte at computed index
    emitter.instruction("ldrb w8, [x12, x8]");                                  // fold it through the lowercase table
    emitter.instruction("ldrb w10, [x3, x6]");                                  // load needle byte at current index
    emitter.instruction("ldrb w10, [x12, x10]");                                // fold it through the lowercase table
    emitter.instruction("cmp w8, w10");                                         // compare the folded haystack and needle bytes
    emitter.instruction("b.ne __rt_strripos_prev");                             // mismatch, try previous position
    emitter.instruction("add x6, x6, #1");                                      // advance needle index
    emitter.instruction("b __rt_strripos_inner");                               // continue comparing

    // -- move to previous position (searching right to left) --
    emitter.label("__rt_strripos_prev");
    emitter.instruction("cbz x5, __rt_strripos_notfound");                      // if at position 0, nowhere left to search
    emitter.instruction("sub x5, x5, #1");                                      // decrement search position
    emitter.instruction("b __rt_strripos_outer");                               // retry from new position

    // -- return results --
    emitter.label("__rt_strripos_found");
    emitter.instruction("mov x0, x5");                                          // return last match position
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_strripos_empty");
    emitter.instruction("mov x0, x2");                                          // empty needle returns haystack length
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_strripos_notfound");
    emitter.instruction("mov x0, #-1");                                         // return -1 (not found)
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the `__rt_strripos` runtime helper for Linux x86_64.
///
/// ABI:
///   Input:  rdi=haystack_ptr, rsi=haystack_len, rdx=needle_ptr, rcx=needle_len
///   Output: rax = byte offset of the last case-insensitive match, or -1 when absent
///
/// Called exclusively from `emit_strripos` when `emitter.target.arch == Arch::X86_64`.
fn emit_strripos_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strripos ---");
    emitter.label_global("__rt_strripos");

    emitter.instruction("test rcx, rcx");                                       // empty needles match just after the last haystack byte
    emitter.instruction("jz __rt_strripos_empty_linux_x86_64");                 // return the haystack length when strripos() receives an empty needle
    emitter.instruction("cmp rcx, rsi");                                        // reject searches whose needle is longer than the haystack
    emitter.instruction("jg __rt_strripos_notfound_linux_x86_64");              // return the not-found sentinel when the needle cannot fit
    emitter.instruction("mov r9, rsi");                                         // copy the haystack length so the rightmost valid start offset can be computed once
    emitter.instruction("sub r9, rcx");                                         // compute the rightmost haystack offset where the full needle can still fit

    abi::emit_symbol_address(emitter, "rsi", "_ascii_lower");                  // ASCII lowercase lookup table
    emitter.label("__rt_strripos_outer_linux_x86_64");
    emitter.instruction("xor r10d, r10d");                                      // start the needle byte comparison from index zero for the current candidate

    emitter.label("__rt_strripos_inner_linux_x86_64");
    emitter.instruction("cmp r10, rcx");                                        // did every byte in the needle match at the current haystack offset?
    emitter.instruction("jge __rt_strripos_found_linux_x86_64");                // return the current haystack offset once the full needle matches
    emitter.instruction("mov r8, r9");                                          // copy the current candidate start offset so the indexed haystack byte can be addressed
    emitter.instruction("add r8, r10");                                         // compute the absolute haystack byte offset for the current needle byte
    emitter.instruction("movzx eax, BYTE PTR [rdi + r8]");                      // load the current haystack byte for the right-to-left candidate comparison
    emitter.instruction("movzx eax, BYTE PTR [rsi + rax]");                     // fold it through the lowercase table
    emitter.instruction("movzx r11d, BYTE PTR [rdx + r10]");                    // load the current needle byte for the right-to-left candidate comparison
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r11]");                    // fold it through the lowercase table
    emitter.instruction("cmp al, r11b");                                        // compare the folded haystack and needle bytes
    emitter.instruction("jne __rt_strripos_prev_linux_x86_64");                 // abandon this candidate start offset on the first mismatching byte
    emitter.instruction("add r10, 1");                                          // advance to the next byte within the current needle comparison
    emitter.instruction("jmp __rt_strripos_inner_linux_x86_64");                // continue matching bytes against the current right-to-left candidate start offset

    emitter.label("__rt_strripos_prev_linux_x86_64");
    emitter.instruction("test r9, r9");                                         // are we already at haystack offset zero with no further candidates left to test?
    emitter.instruction("jz __rt_strripos_notfound_linux_x86_64");              // return the not-found sentinel once the final candidate also mismatches
    emitter.instruction("sub r9, 1");                                           // move the candidate start offset one byte to the left
    emitter.instruction("jmp __rt_strripos_outer_linux_x86_64");                // retry the needle comparison from the next right-to-left haystack start offset

    emitter.label("__rt_strripos_found_linux_x86_64");
    emitter.instruction("mov rax, r9");                                         // return the last haystack offset whose bytes matched the full needle
    emitter.instruction("ret");                                                 // return the signed match offset to the caller

    emitter.label("__rt_strripos_empty_linux_x86_64");
    emitter.instruction("mov rax, rsi");                                        // empty needles match just after the final haystack byte
    emitter.instruction("ret");                                                 // return the empty-needle offset to the caller

    emitter.label("__rt_strripos_notfound_linux_x86_64");
    emitter.instruction("mov rax, -1");                                         // return the not-found sentinel when no haystack offset matches the needle
    emitter.instruction("ret");                                                 // return the not-found sentinel to the caller
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Verifies both targets fold ASCII case through the shared lookup table.
    #[test]
    fn strripos_emits_table_folding_on_all_targets() {
        for name in ["macos-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).expect("supported target");
            let mut emitter = Emitter::new(target);
            emit_strripos(&mut emitter);
            let assembly = emitter.output();
            assert!(assembly.contains("_ascii_lower"), "{name}: {assembly}");
            assert!(!assembly.contains("__rt_strripos_fold_needle"), "{name}: {assembly}");
        }
    }
}
