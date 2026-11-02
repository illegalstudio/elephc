//! Purpose:
//! Emits the `__rt_substr_compare` runtime helper assembly for the PHP `substr_compare` builtin.
//! Compares an already-sliced haystack window against the needle, optionally folding ASCII case.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - The helper receives the WINDOW, not the original subject: `substr_compare()`'s `$offset`
//!   is normalized (and both of its `ValueError`s raised) in the backend lowering, which then
//!   passes `haystack + offset`, `strlen($haystack) - offset`, and the resolved comparison
//!   length. That keeps the catchable diagnostics out of the runtime, where a fatal could not
//!   be caught.
//! - The return value is php-src's, not a normalized sign. `zend_binary_strncmp()` hands back
//!   `memcmp()`'s raw UNSIGNED-BYTE difference at the first mismatch (`substr_compare("a","z",0)`
//!   is `-25`, measured on php 8.5.10), and only the equal-prefix tiebreak is the `-1`/`0`/`1`
//!   of `ZEND_THREEWAY_COMPARE`. Callers testing `=== 0` and callers testing `< 0` both exist,
//!   so neither half may be approximated by the other.
//! - Case folding is ASCII-only (`zend_tolower_ascii`), and that is a DELIBERATE, MEASURED
//!   divergence from php's default on this host. php routes the `$case_insensitive` path
//!   through `zend_binary_strncasecmp_l`, whose fold is the C library's LOCALE-dependent
//!   `tolower()`: with `setlocale(LC_CTYPE, "C")` php answers
//!   `substr_compare("\xE9","\xC9",0,1,true) === 32` (no fold, what this does), and under any
//!   real locale -- and under macOS's own startup rune table, which is what a php process that
//!   never calls `setlocale` gets here -- it answers `0`, folding all of Latin-1 `0xC0`-`0xDE`
//!   except `0xD7`. `strcasecmp()`/`strncasecmp()` are ASCII-only in php 8 and are NOT affected,
//!   which is why they must not share a fold table with this. A compiled binary has no locale
//!   to consult, so elephc pins the deterministic ASCII answer; the only inputs that can tell
//!   the two apart are RAW Latin-1 high bytes, never UTF-8 (a UTF-8 lead byte folds identically
//!   on both sides of the comparison and cancels out).
//! - The tiebreak compares the TRUNCATED lengths `min($cmp_len, $len)`, never the raw ones.
//! - The helper allocates nothing and calls nothing, so it needs no frame and no concat
//!   scratch reservation.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_substr_compare` runtime helper for the `substr_compare` builtin.
///
/// ABI (AArch64):
///   Input:  `x1` = window pointer, `x2` = window length, `x3` = needle pointer,
///           `x4` = needle length, `x5` = comparison length, `x6` = case-insensitive flag.
///   Output: `x0` = php's comparison result.
///
/// ABI (x86_64 System V):
///   Input:  `rdi` = window pointer, `rsi` = window length, `rdx` = needle pointer,
///           `rcx` = needle length, `r8` = comparison length, `r9` = case-insensitive flag.
///   Output: `rax` = php's comparison result.
///
/// Every length is already known to be non-negative, so the truncating minimums are taken with
/// unsigned compares exactly like `__rt_strncasecmp` does.
pub fn emit_substr_compare(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_substr_compare_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: substr_compare ---");
    emitter.label_global("__rt_substr_compare");

    // -- truncate both operands to the requested comparison length --
    emitter.instruction("cmp x2, x5");                                          // compare the window length against the requested bound
    emitter.instruction("csel x9, x2, x5, lo");                                  // x9 = min(window length, cmp length), the first effective length
    emitter.instruction("cmp x4, x5");                                          // compare the needle length against the requested bound
    emitter.instruction("csel x10, x4, x5, lo");                                 // x10 = min(needle length, cmp length), the second effective length
    emitter.instruction("cmp x9, x10");                                         // compare both effective lengths
    emitter.instruction("csel x11, x9, x10, lo");                                // x11 = shared prefix actually compared byte by byte
    emitter.instruction("mov x7, #0");                                          // start comparing at byte offset zero

    emitter.label("__rt_substr_compare_loop");
    emitter.instruction("cmp x7, x11");                                         // has the shared prefix been fully compared?
    emitter.instruction("b.hs __rt_substr_compare_len");                        // fall back to the effective-length tiebreak
    emitter.instruction("ldrb w12, [x1, x7]");                                  // load the current window byte
    emitter.instruction("ldrb w13, [x3, x7]");                                  // load the current needle byte
    emitter.instruction("cbz x6, __rt_substr_compare_cmp");                     // a case-sensitive call compares the raw bytes

    // -- ASCII-fold the window byte --
    emitter.instruction("cmp w12, #65");                                        // is the window byte at or above 'A'?
    emitter.instruction("b.lt __rt_substr_compare_fold_b");                     // bytes below 'A' are compared unchanged
    emitter.instruction("cmp w12, #90");                                        // is the window byte at or below 'Z'?
    emitter.instruction("b.gt __rt_substr_compare_fold_b");                     // bytes above 'Z' are compared unchanged
    emitter.instruction("add w12, w12, #32");                                   // fold the uppercase ASCII letter to lowercase

    // -- ASCII-fold the needle byte --
    emitter.label("__rt_substr_compare_fold_b");
    emitter.instruction("cmp w13, #65");                                        // is the needle byte at or above 'A'?
    emitter.instruction("b.lt __rt_substr_compare_cmp");                        // bytes below 'A' are compared unchanged
    emitter.instruction("cmp w13, #90");                                        // is the needle byte at or below 'Z'?
    emitter.instruction("b.gt __rt_substr_compare_cmp");                        // bytes above 'Z' are compared unchanged
    emitter.instruction("add w13, w13, #32");                                   // fold the uppercase ASCII letter to lowercase

    emitter.label("__rt_substr_compare_cmp");
    emitter.instruction("cmp w12, w13");                                        // compare the two (optionally folded) bytes
    emitter.instruction("b.ne __rt_substr_compare_diff");                       // report the byte difference on the first mismatch
    emitter.instruction("add x7, x7, #1");                                      // advance to the next shared-prefix byte
    emitter.instruction("b __rt_substr_compare_loop");                          // keep comparing the shared prefix

    emitter.label("__rt_substr_compare_diff");
    emitter.instruction("sub x0, x12, x13");                                    // php returns memcmp()'s RAW unsigned-byte difference here
    emitter.instruction("ret");                                                 // hand the byte difference back to the caller

    emitter.label("__rt_substr_compare_len");
    emitter.instruction("cmp x9, x10");                                         // tiebreak on the TRUNCATED lengths, never the raw ones
    emitter.instruction("cset x0, hi");                                         // 1 when the window side is the longer one
    emitter.instruction("csinv x0, x0, xzr, hs");                               // -1 when the needle side is longer: ZEND_THREEWAY_COMPARE, not a difference
    emitter.instruction("ret");                                                 // hand the normalized ordering back to the caller
}

/// Emits `__rt_substr_compare` for x86_64 Linux using the System V ABI.
///
/// `rsi` is repurposed as the shared-prefix bound and `r8` as the byte index once the raw
/// lengths have been truncated into `r10`/`r11`, so the helper needs no callee-saved register
/// and no stack frame.
fn emit_substr_compare_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: substr_compare ---");
    emitter.label_global("__rt_substr_compare");

    // -- truncate both operands to the requested comparison length --
    emitter.instruction("mov r10, rsi");                                        // seed the first effective length from the window length
    emitter.instruction("cmp r10, r8");                                         // compare the window length against the requested bound
    emitter.instruction("cmova r10, r8");                                       // r10 = min(window length, cmp length)
    emitter.instruction("mov r11, rcx");                                        // seed the second effective length from the needle length
    emitter.instruction("cmp r11, r8");                                         // compare the needle length against the requested bound
    emitter.instruction("cmova r11, r8");                                       // r11 = min(needle length, cmp length)
    emitter.instruction("mov rsi, r10");                                        // seed the shared prefix bound from the first effective length
    emitter.instruction("cmp rsi, r11");                                        // compare both effective lengths
    emitter.instruction("cmova rsi, r11");                                      // rsi = shared prefix actually compared byte by byte
    emitter.instruction("xor r8d, r8d");                                        // start comparing at byte offset zero

    emitter.label("__rt_substr_compare_loop_linux_x86_64");
    emitter.instruction("cmp r8, rsi");                                         // has the shared prefix been fully compared?
    emitter.instruction("jae __rt_substr_compare_len_linux_x86_64");            // fall back to the effective-length tiebreak
    emitter.instruction("movzx eax, BYTE PTR [rdi + r8]");                      // load the current window byte
    emitter.instruction("movzx ecx, BYTE PTR [rdx + r8]");                      // load the current needle byte
    emitter.instruction("test r9, r9");                                         // was this call asked to ignore case?
    emitter.instruction("jz __rt_substr_compare_cmp_linux_x86_64");             // a case-sensitive call compares the raw bytes

    // -- ASCII-fold the window byte --
    emitter.instruction("cmp al, 65");                                          // is the window byte at or above 'A'?
    emitter.instruction("jb __rt_substr_compare_fold_b_linux_x86_64");          // bytes below 'A' are compared unchanged
    emitter.instruction("cmp al, 90");                                          // is the window byte at or below 'Z'?
    emitter.instruction("ja __rt_substr_compare_fold_b_linux_x86_64");          // bytes above 'Z' are compared unchanged
    emitter.instruction("add al, 32");                                          // fold the uppercase ASCII letter to lowercase

    // -- ASCII-fold the needle byte --
    emitter.label("__rt_substr_compare_fold_b_linux_x86_64");
    emitter.instruction("cmp cl, 65");                                          // is the needle byte at or above 'A'?
    emitter.instruction("jb __rt_substr_compare_cmp_linux_x86_64");             // bytes below 'A' are compared unchanged
    emitter.instruction("cmp cl, 90");                                          // is the needle byte at or below 'Z'?
    emitter.instruction("ja __rt_substr_compare_cmp_linux_x86_64");             // bytes above 'Z' are compared unchanged
    emitter.instruction("add cl, 32");                                          // fold the uppercase ASCII letter to lowercase

    emitter.label("__rt_substr_compare_cmp_linux_x86_64");
    emitter.instruction("cmp al, cl");                                          // compare the two (optionally folded) bytes
    emitter.instruction("jne __rt_substr_compare_diff_linux_x86_64");           // report the byte difference on the first mismatch
    emitter.instruction("add r8, 1");                                           // advance to the next shared-prefix byte
    emitter.instruction("jmp __rt_substr_compare_loop_linux_x86_64");           // keep comparing the shared prefix

    emitter.label("__rt_substr_compare_diff_linux_x86_64");
    emitter.instruction("movzx eax, al");                                       // widen the window byte back to a full unsigned value
    emitter.instruction("movzx ecx, cl");                                       // widen the needle byte back to a full unsigned value
    emitter.instruction("sub rax, rcx");                                        // php returns memcmp()'s RAW unsigned-byte difference here
    emitter.instruction("ret");                                                 // hand the byte difference back to the caller

    emitter.label("__rt_substr_compare_len_linux_x86_64");
    emitter.instruction("cmp r10, r11");                                        // tiebreak on the TRUNCATED lengths, never the raw ones
    emitter.instruction("ja __rt_substr_compare_len_greater_linux_x86_64");     // the window side is the longer one
    emitter.instruction("mov rax, -1");                                         // the needle side is longer: ZEND_THREEWAY_COMPARE yields -1
    emitter.instruction("jb __rt_substr_compare_len_done_linux_x86_64");        // keep the -1 only when the window side is strictly shorter
    emitter.instruction("xor eax, eax");                                        // equal truncated lengths compare equal
    emitter.instruction("ret");                                                 // hand the tie back to the caller

    emitter.label("__rt_substr_compare_len_greater_linux_x86_64");
    emitter.instruction("mov rax, 1");                                          // ZEND_THREEWAY_COMPARE yields 1, never a length difference

    emitter.label("__rt_substr_compare_len_done_linux_x86_64");
    emitter.instruction("ret");                                                 // hand the normalized ordering back to the caller
}
