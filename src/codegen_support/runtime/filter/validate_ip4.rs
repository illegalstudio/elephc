//! Purpose:
//! Emits the `__rt_filter_validate_ip4` runtime helper backing
//! `filter_var($v, FILTER_VALIDATE_IP, FILTER_FLAG_IPV4)` (and the
//! either-family IP validation path) on string input: a strict IPv4
//! dotted-quad check via libc `inet_pton(AF_INET, ...)`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::filter`.
//! - `crate::codegen::lower_inst::builtins::filter` (the `filter_var()` EIR lowering).
//! - `crate::codegen_support::runtime::filter::validate_ip6` reuses this file's
//!   `emit_reject_leading_zero_octet_scan_arm64`/`_x86_64` for the embedded-
//!   IPv4-in-IPv6 tail (the same libc quirk applies there too).
//!
//! Key details:
//! - Delegates to libc `inet_pton`, matching the existing `__rt_inet6_pton`
//!   helper's approach (see `crate::codegen_support::runtime::io::inet6_pton`) rather
//!   than hand-rolling a second dotted-quad grammar. `inet_pton(AF_INET, ...)`
//!   is strict RFC 791 decimal dotted-quad (no partial forms, no leading/
//!   trailing whitespace) — php-verified to match `FILTER_VALIDATE_IP` on the
//!   common/adversarial cases exercised by this feature (`php -n -r
//!   'var_dump(filter_var(...));'`, PHP 8.5.6 local): `"192.168.1.1"`
//!   accepts, `" 192.168.1.1"`/`"192.168.1.1 "` (whitespace) and
//!   `"300.1.1.1"` (out-of-range octet) reject.
//! - ONE divergence from libc needed hand patching: macOS's `inet_pton`
//!   ACCEPTS a leading-zero octet (`"192.168.1.01"` -> success), but PHP's own
//!   `_php_filter_validate_ipv4()` (`ext/filter/logical_filters.c`) explicitly
//!   rejects it ("don't allow a leading 0; that introduces octal numbers,
//!   which we don't support") — confirmed with a standalone C probe
//!   (`inet_pton(AF_INET, "192.168.1.01", ...)` -> `1` on this macOS host).
//!   Since this is a libc/platform quirk rather than a guaranteed-uniform
//!   POSIX behavior, this helper runs an explicit leading-zero-octet scan
//!   BEFORE calling `inet_pton` on every target, rather than trusting the
//!   platform C library to already match PHP here.
//! - `AF_INET` is `2` on both macOS and Linux (POSIX-standard, unlike
//!   `AF_INET6`'s platform-dependent value) — already hardcoded the same way
//!   in `crate::codegen_support::runtime::io::gethostbyaddr` and the stream-socket
//!   client helpers, so no `Platform` accessor is needed here.
//! - Register convention mirrors `__rt_filter_validate_int`/`__rt_filter_validate_float`
//!   (NOT `__rt_inet6_pton`'s socket-call convention): the string arrives
//!   already in the ABI string-result registers, so the `filter_var()` Str
//!   lowering can call straight through with no shuffle.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits `__rt_filter_validate_ip4` for the host target.
///
/// AArch64: input x1=ptr, x2=len. Output: x0=1 valid IPv4 literal, x0=0 otherwise.
/// x86_64: input rax=ptr, rdx=len. Output: rax=1 valid IPv4 literal, rax=0 otherwise.
pub fn emit_filter_validate_ip4(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_filter_validate_ip4_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: filter_validate_ip4 ---");
    emitter.label_global("__rt_filter_validate_ip4");

    // Frame (32 bytes): [0..16) saved x29/x30, [16..32) scratch inet_pton output buffer.
    emitter.instruction("sub sp, sp, #32");                                     // frame for saved regs and the scratch output buffer
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    emit_reject_leading_zero_octet_scan_arm64(
        emitter,
        "x1",
        "x2",
        "__rt_filter_validate_ip4_fail",
        "__rt_filter_ipv4_lz_scan",
    );

    // -- null-terminate the host slice (x1=ptr, x2=len already match __rt_cstr) --
    emitter.instruction("bl __rt_cstr");                                        // x0 = null-terminated host literal

    // -- inet_pton(AF_INET, c_str, scratch_out) --
    emitter.instruction("mov x1, x0");                                          // c_str into argument 1 (src)
    emitter.instruction("add x2, sp, #16");                                     // scratch output buffer (discarded — validation only)
    emitter.instruction("mov x0, #2");                                          // family: AF_INET (2 on both macOS and Linux)
    emitter.bl_c("inet_pton");                                                  // x0 = 1 success, 0 fail, -1 EAFNOSUPPORT

    // -- collapse libc result to 0/1 (any non-positive return means fail) --
    emitter.instruction("cmp x0, #1");                                          // did libc report exactly one successful conversion?
    emitter.instruction("cset x0, eq");                                         // x0 = 1 on success, 0 otherwise
    emitter.instruction("b __rt_filter_validate_ip4_done");                     // done

    emitter.label("__rt_filter_validate_ip4_fail");
    emitter.instruction("mov x0, #0");                                          // report failure (leading-zero octet rejected pre-libc)

    emitter.label("__rt_filter_validate_ip4_done");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // return the success flag
}

/// Scans `[ptr_reg, ptr_reg + len_reg)` for a decimal octet that starts with
/// `'0'` and is immediately followed by another decimal digit (a leading-zero
/// octet PHP's `_php_filter_validate_ipv4()` rejects but macOS's `inet_pton`
/// silently accepts — see the module doc) and branches to `fail_label` when
/// found. An "octet start" is byte index 0 or any byte immediately after a
/// `.`; this is safe to run unconditionally before the libc call because a
/// non-decimal byte elsewhere in the string (a `:` from a colon-shaped
/// non-IPv4 input, for instance) never matches `'0'` followed by a digit at
/// an octet-start position, so it cannot mis-reject a genuinely-invalid input
/// libc would have rejected anyway. Clobbers x3-x9; leaves `ptr_reg`/`len_reg`
/// untouched for the caller's subsequent use. `label_prefix` must be unique
/// per call site within the emitted assembly (this module and
/// `crate::codegen_support::runtime::filter::validate_ip6` both call it, for the
/// whole-string IPv4 case and the embedded-IPv4-in-IPv6 tail respectively).
pub(super) fn emit_reject_leading_zero_octet_scan_arm64(
    emitter: &mut Emitter,
    ptr_reg: &str,
    len_reg: &str,
    fail_label: &str,
    label_prefix: &str,
) {
    let loop_label = format!("{label_prefix}_loop");
    let next_label = format!("{label_prefix}_next");
    let at_start_label = format!("{label_prefix}_at_start");
    let done_label = format!("{label_prefix}_done");

    emitter.instruction("mov x3, #0");                                          // scan index
    emitter.label(&loop_label);
    emitter.instruction(&format!("cmp x3, {}", len_reg));                       // reached the end of the scanned range?
    emitter.instruction(&format!("b.ge {}", done_label));                       // no more bytes to check
    emitter.instruction(&format!("ldrb w4, [{}, x3]", ptr_reg));                // byte[idx]
    emitter.instruction("cmp w4, #0x30");                                       // is it '0'?
    emitter.instruction(&format!("b.ne {}", next_label));                       // not a candidate leading digit
    emitter.instruction(&format!("cbz x3, {}", at_start_label));                // idx==0: this '0' starts the string (and an octet)
    emitter.instruction("sub x6, x3, #1");                                      // x6 = idx-1
    emitter.instruction(&format!("ldrb w5, [{}, x6]", ptr_reg));                // byte[idx-1]
    emitter.instruction("cmp w5, #0x2E");                                       // is the previous byte '.'?
    emitter.instruction(&format!("b.ne {}", next_label));                       // '0' mid-octet (not the first digit): not a leading zero
    emitter.label(&at_start_label);
    emitter.instruction("add x7, x3, #1");                                      // x7 = idx+1 (candidate next-digit position)
    emitter.instruction(&format!("cmp x7, {}", len_reg));                       // is there a byte after this '0'?
    emitter.instruction(&format!("b.ge {}", next_label));                       // '0' is the last byte: just "0", not a leading zero
    emitter.instruction(&format!("ldrb w8, [{}, x7]", ptr_reg));                // byte[idx+1]
    emitter.instruction("sub w9, w8, #0x30");                                   // normalize to a candidate decimal digit
    emitter.instruction("cmp w9, #9");                                          // is byte[idx+1] a decimal digit (0-9)?
    emitter.instruction(&format!("b.hi {}", next_label));                       // "0." or "0" before non-digit: not a leading zero
    emitter.instruction(&format!("b {}", fail_label));                          // "0" followed by another digit: reject
    emitter.label(&next_label);
    emitter.instruction("add x3, x3, #1");                                      // advance to the next byte
    emitter.instruction(&format!("b {}", loop_label));                          // keep scanning
    emitter.label(&done_label);
}

/// x86_64 counterpart of `emit_reject_leading_zero_octet_scan_arm64` — same
/// scan, same clobber/label-uniqueness contract (clobbers r8-r11; leaves
/// `ptr_reg`/`len_reg` untouched).
pub(super) fn emit_reject_leading_zero_octet_scan_x86_64(
    emitter: &mut Emitter,
    ptr_reg: &str,
    len_reg: &str,
    fail_label: &str,
    label_prefix: &str,
) {
    let loop_label = format!("{label_prefix}_loop");
    let next_label = format!("{label_prefix}_next");
    let at_start_label = format!("{label_prefix}_at_start");
    let done_label = format!("{label_prefix}_done");

    emitter.instruction("xor r8, r8");                                          // scan index
    emitter.label(&loop_label);
    emitter.instruction(&format!("cmp r8, {}", len_reg));                       // reached the end of the scanned range?
    emitter.instruction(&format!("jge {}", done_label));                        // no more bytes to check
    emitter.instruction(&format!("movzx r9d, BYTE PTR [{} + r8]", ptr_reg));    // byte[idx]
    emitter.instruction("cmp r9d, 0x30");                                       // is it '0'?
    emitter.instruction(&format!("jne {}", next_label));                        // not a candidate leading digit
    emitter.instruction("test r8, r8");                                         // idx==0?
    emitter.instruction(&format!("jz {}", at_start_label));                     // this '0' starts the string (and an octet)
    emitter.instruction("mov r10, r8");                                         // r10 = idx-1
    emitter.instruction("dec r10");
    emitter.instruction(&format!("movzx r9d, BYTE PTR [{} + r10]", ptr_reg));   // byte[idx-1]
    emitter.instruction("cmp r9d, 0x2E");                                       // is the previous byte '.'?
    emitter.instruction(&format!("jne {}", next_label));                        // '0' mid-octet (not the first digit): not a leading zero
    emitter.label(&at_start_label);
    emitter.instruction("mov r10, r8");                                         // r10 = idx+1 (candidate next-digit position)
    emitter.instruction("inc r10");
    emitter.instruction(&format!("cmp r10, {}", len_reg));                      // is there a byte after this '0'?
    emitter.instruction(&format!("jge {}", next_label));                        // '0' is the last byte: just "0", not a leading zero
    emitter.instruction(&format!("movzx r9d, BYTE PTR [{} + r10]", ptr_reg));   // byte[idx+1]
    emitter.instruction("sub r9d, 0x30");                                       // normalize to a candidate decimal digit
    emitter.instruction("cmp r9d, 9");                                          // is byte[idx+1] a decimal digit (0-9)?
    emitter.instruction(&format!("ja {}", next_label));                         // "0." or "0" before non-digit: not a leading zero
    emitter.instruction(&format!("jmp {}", fail_label));                        // "0" followed by another digit: reject
    emitter.label(&next_label);
    emitter.instruction("inc r8");                                              // advance to the next byte
    emitter.instruction(&format!("jmp {}", loop_label));                        // keep scanning
    emitter.label(&done_label);
}

/// Emits `__rt_filter_validate_ip4` for the Linux x86_64 target.
fn emit_filter_validate_ip4_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: filter_validate_ip4 ---");
    emitter.label_global("__rt_filter_validate_ip4");

    // Frame (16 bytes, rbp-relative): [-16..0) scratch inet_pton output buffer.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve the scratch output buffer

    emit_reject_leading_zero_octet_scan_x86_64(
        emitter,
        "rax",
        "rdx",
        "__rt_filter_validate_ip4_fail_x86_64",
        "__rt_filter_ipv4_lz_scan_x86_64",
    );

    // -- null-terminate the host slice (rax=ptr, rdx=len already match __rt_cstr) --
    emitter.instruction("call __rt_cstr");                                      // rax = null-terminated host literal

    // -- inet_pton(AF_INET, c_str, scratch_out) --
    emitter.instruction("mov rsi, rax");                                        // c_str into argument 1 (src)
    emitter.instruction("lea rdx, [rbp - 16]");                                 // scratch output buffer (discarded — validation only)
    emitter.instruction("mov edi, 2");                                          // family: AF_INET (2 on both macOS and Linux)
    emitter.instruction("call inet_pton");                                      // rax = 1 success, 0 fail, -1 EAFNOSUPPORT

    // -- collapse libc result to 0/1 (any non-positive return means fail) --
    emitter.instruction("cmp eax, 1");                                          // did libc report exactly one successful conversion?
    emitter.instruction("sete al");                                             // al = 1 on success, 0 otherwise
    emitter.instruction("movzx eax, al");                                       // widen the success flag to a full word
    emitter.instruction("jmp __rt_filter_validate_ip4_done_x86_64");            // done

    emitter.label("__rt_filter_validate_ip4_fail_x86_64");
    emitter.instruction("xor eax, eax");                                        // report failure (leading-zero octet rejected pre-libc)

    emitter.label("__rt_filter_validate_ip4_done_x86_64");
    emitter.instruction("mov rsp, rbp");                                        // release the scratch output buffer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the success flag
}
