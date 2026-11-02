//! Purpose:
//! Emits the `__rt_filter_validate_ip6` runtime helper backing
//! `filter_var($v, FILTER_VALIDATE_IP, FILTER_FLAG_IPV6)` (and the
//! either-family IP validation path) on string input: an IPv6 literal check
//! via libc `inet_pton(AF_INET6, ...)`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::filter`.
//! - `crate::codegen::lower_inst::builtins::filter` (the `filter_var()` EIR lowering).
//!
//! Key details:
//! - Delegates to libc `inet_pton`, mirroring `__rt_filter_validate_ip4`
//!   (sibling module) and reusing the same `AF_INET6` family-value accessor
//!   (`Platform::af_inet6`, 30 on macOS / 10 on Linux) already relied on by
//!   `crate::codegen_support::runtime::io::inet6_pton`. Php-verified against
//!   `FILTER_VALIDATE_IP|FILTER_FLAG_IPV6` (`php -n -r
//!   'var_dump(filter_var(...));'`, PHP 8.5.6 local): `"::1"` and
//!   `"::ffff:192.168.1.1"` (embedded-IPv4 form) accept, an IPv4-only literal
//!   like `"192.168.1.1"` rejects.
//! - The SAME macOS `inet_pton` leading-zero-octet quirk documented in
//!   `crate::codegen_support::runtime::filter::validate_ip4` also applies to the
//!   embedded-IPv4 tail of an IPv6 literal (confirmed with a standalone C
//!   probe: `inet_pton(AF_INET6, "::ffff:192.168.01.1", ...)` -> `1` on this
//!   macOS host, but PHP's `_php_filter_validate_ipv6()` delegates the tail to
//!   `_php_filter_validate_ipv4()`, which rejects it). Before calling
//!   `inet_pton(AF_INET6, ...)`, this helper mirrors PHP's own embedded-IPv4
//!   detection (`ext/filter/logical_filters.c`: find the first `.`, then walk
//!   backward to the nearest preceding `:` or the start of the string — that
//!   is the embedded-IPv4 tail) and reuses `validate_ip4`'s
//!   `emit_reject_leading_zero_octet_scan_arm64`/`_x86_64` scoped to just that
//!   tail substring (a whole-string scan would false-positive on a legitimate
//!   hex group like `"0db8"`, since a lone `'0'` followed by a non-decimal hex
//!   digit is fine in a colon group but never in a decimal octet).
//! - Register convention mirrors `__rt_filter_validate_int`/`__rt_filter_validate_ip4`
//!   (NOT `__rt_inet6_pton`'s socket-call convention): the string arrives
//!   already in the ABI string-result registers, so the `filter_var()` Str
//!   lowering can call straight through with no shuffle.

use crate::codegen::{emit::Emitter, platform::Arch};

use super::validate_ip4::{
    emit_reject_leading_zero_octet_scan_arm64, emit_reject_leading_zero_octet_scan_x86_64,
};

/// Emits `__rt_filter_validate_ip6` for the host target.
///
/// AArch64: input x1=ptr, x2=len. Output: x0=1 valid IPv6 literal, x0=0 otherwise.
/// x86_64: input rax=ptr, rdx=len. Output: rax=1 valid IPv6 literal, rax=0 otherwise.
pub fn emit_filter_validate_ip6(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_filter_validate_ip6_linux_x86_64(emitter);
        return;
    }

    let af_inet6 = emitter.platform.af_inet6();
    emitter.blank();
    emitter.comment("--- runtime: filter_validate_ip6 ---");
    emitter.label_global("__rt_filter_validate_ip6");

    // Frame (32 bytes): [0..16) saved x29/x30, [16..32) scratch inet_pton output buffer.
    emitter.instruction("sub sp, sp, #32");                                     // frame for saved regs and the scratch output buffer
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    emit_reject_embedded_ipv4_leading_zero_arm64(emitter, "__rt_filter_validate_ip6_fail");

    // -- null-terminate the host slice (x1=ptr, x2=len already match __rt_cstr) --
    emitter.instruction("bl __rt_cstr");                                        // x0 = null-terminated host literal

    // -- inet_pton(AF_INET6, c_str, scratch_out) --
    emitter.instruction("mov x1, x0");                                          // c_str into argument 1 (src)
    emitter.instruction("add x2, sp, #16");                                     // scratch output buffer (discarded — validation only)
    emitter.instruction(&format!("mov x0, #{}", af_inet6));                     // family: AF_INET6 (30 on macOS, 10 on Linux)
    emitter.bl_c("inet_pton");                                                  // x0 = 1 success, 0 fail, -1 EAFNOSUPPORT

    // -- collapse libc result to 0/1 (any non-positive return means fail) --
    emitter.instruction("cmp x0, #1");                                          // did libc report exactly one successful conversion?
    emitter.instruction("cset x0, eq");                                         // x0 = 1 on success, 0 otherwise
    emitter.instruction("b __rt_filter_validate_ip6_done");                     // done

    emitter.label("__rt_filter_validate_ip6_fail");
    emitter.instruction("mov x0, #0");                                          // report failure (embedded-IPv4 leading-zero octet rejected pre-libc)

    emitter.label("__rt_filter_validate_ip6_done");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // return the success flag
}

/// Finds an embedded-IPv4 tail in `[x1, x1+x2)` (PHP's own detection: the
/// first `.`, walked back to the nearest preceding `:` or the string start)
/// and, if one exists, runs the leading-zero-octet scan scoped to just that
/// tail, branching to `fail_label` on a match. A no-op (falls through) when
/// the string has no `.` at all. Clobbers x3-x9 (the find/walk-back scan) and
/// x10-x11 (the tail ptr/len passed to the shared scan); leaves x1/x2
/// untouched for the caller's subsequent `inet_pton` call.
fn emit_reject_embedded_ipv4_leading_zero_arm64(emitter: &mut Emitter, fail_label: &str) {
    emitter.instruction("mov x3, #0");                                          // scan index for the '.' search
    emitter.label("__rt_filter_ipv6_finddot_loop");
    emitter.instruction("cmp x3, x2");                                          // reached the end of the string?
    emitter.instruction("b.ge __rt_filter_ipv6_finddot_none");                  // no '.' anywhere: no embedded IPv4 tail
    emitter.instruction("ldrb w4, [x1, x3]");                                   // byte[idx]
    emitter.instruction("cmp w4, #0x2E");                                       // is it '.'?
    emitter.instruction("b.eq __rt_filter_ipv6_finddot_found");                 // first '.' found
    emitter.instruction("add x3, x3, #1");                                      // advance to the next byte
    emitter.instruction("b __rt_filter_ipv6_finddot_loop");                     // keep scanning for '.'

    emitter.label("__rt_filter_ipv6_finddot_found");
    emitter.instruction("mov x5, x3");                                          // x5 = ipv4_start, starts at the dot's index
    emitter.label("__rt_filter_ipv6_walkback_loop");
    emitter.instruction("cbz x5, __rt_filter_ipv6_walkback_done");              // reached the start of the string: stop
    emitter.instruction("sub x6, x5, #1");                                      // x6 = ipv4_start-1
    emitter.instruction("ldrb w7, [x1, x6]");                                   // byte[ipv4_start-1]
    emitter.instruction("cmp w7, #0x3A");                                       // is the preceding byte ':'?
    emitter.instruction("b.eq __rt_filter_ipv6_walkback_done");                 // found the boundary: stop
    emitter.instruction("mov x5, x6");                                          // walk one byte further back
    emitter.instruction("b __rt_filter_ipv6_walkback_loop");

    emitter.label("__rt_filter_ipv6_walkback_done");
    emitter.instruction("add x10, x1, x5");                                     // x10 = tail pointer (str + ipv4_start)
    emitter.instruction("sub x11, x2, x5");                                     // x11 = tail length (len - ipv4_start)
    emit_reject_leading_zero_octet_scan_arm64(
        emitter,
        "x10",
        "x11",
        fail_label,
        "__rt_filter_ipv6_v4tail_lz_scan",
    );

    emitter.label("__rt_filter_ipv6_finddot_none");
}

/// Emits `__rt_filter_validate_ip6` for the Linux x86_64 target.
fn emit_filter_validate_ip6_linux_x86_64(emitter: &mut Emitter) {
    let af_inet6 = emitter.platform.af_inet6();
    emitter.blank();
    emitter.comment("--- runtime: filter_validate_ip6 ---");
    emitter.label_global("__rt_filter_validate_ip6");

    // Frame (16 bytes, rbp-relative): [-16..0) scratch inet_pton output buffer.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve the scratch output buffer

    emit_reject_embedded_ipv4_leading_zero_x86_64(emitter, "__rt_filter_validate_ip6_fail_x86_64");

    // -- null-terminate the host slice (rax=ptr, rdx=len already match __rt_cstr) --
    emitter.instruction("call __rt_cstr");                                      // rax = null-terminated host literal

    // -- inet_pton(AF_INET6, c_str, scratch_out) --
    emitter.instruction("mov rsi, rax");                                        // c_str into argument 1 (src)
    emitter.instruction("lea rdx, [rbp - 16]");                                 // scratch output buffer (discarded — validation only)
    emitter.instruction(&format!("mov edi, {}", af_inet6));                     // family: AF_INET6 (30 on macOS, 10 on Linux)
    emitter.instruction("call inet_pton");                                      // rax = 1 success, 0 fail, -1 EAFNOSUPPORT

    // -- collapse libc result to 0/1 (any non-positive return means fail) --
    emitter.instruction("cmp eax, 1");                                          // did libc report exactly one successful conversion?
    emitter.instruction("sete al");                                             // al = 1 on success, 0 otherwise
    emitter.instruction("movzx eax, al");                                       // widen the success flag to a full word
    emitter.instruction("jmp __rt_filter_validate_ip6_done_x86_64");            // done

    emitter.label("__rt_filter_validate_ip6_fail_x86_64");
    emitter.instruction("xor eax, eax");                                        // report failure (embedded-IPv4 leading-zero octet rejected pre-libc)

    emitter.label("__rt_filter_validate_ip6_done_x86_64");
    emitter.instruction("mov rsp, rbp");                                        // release the scratch output buffer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the success flag
}

/// x86_64 counterpart of `emit_reject_embedded_ipv4_leading_zero_arm64`.
/// Clobbers r8-r11 (the find/walk-back scan) and rcx/rsi (the tail ptr/len
/// passed to the shared scan — deliberately caller-saved registers, so this
/// leaf helper needs no save/restore); leaves rax/rdx untouched for the
/// caller's subsequent `inet_pton` call.
fn emit_reject_embedded_ipv4_leading_zero_x86_64(emitter: &mut Emitter, fail_label: &str) {
    emitter.instruction("xor r8, r8");                                          // scan index for the '.' search
    emitter.label("__rt_filter_ipv6_finddot_loop_x86_64");
    emitter.instruction("cmp r8, rdx");                                         // reached the end of the string?
    emitter.instruction("jge __rt_filter_ipv6_finddot_none_x86_64");            // no '.' anywhere: no embedded IPv4 tail
    emitter.instruction("movzx r9d, BYTE PTR [rax + r8]");                      // byte[idx]
    emitter.instruction("cmp r9d, 0x2E");                                       // is it '.'?
    emitter.instruction("je __rt_filter_ipv6_finddot_found_x86_64");            // first '.' found
    emitter.instruction("inc r8");                                              // advance to the next byte
    emitter.instruction("jmp __rt_filter_ipv6_finddot_loop_x86_64");            // keep scanning for '.'

    emitter.label("__rt_filter_ipv6_finddot_found_x86_64");
    emitter.instruction("mov r10, r8");                                         // r10 = ipv4_start, starts at the dot's index
    emitter.label("__rt_filter_ipv6_walkback_loop_x86_64");
    emitter.instruction("test r10, r10");                                       // reached the start of the string?
    emitter.instruction("jz __rt_filter_ipv6_walkback_done_x86_64");            // stop
    emitter.instruction("mov r11, r10");                                        // r11 = ipv4_start-1
    emitter.instruction("dec r11");
    emitter.instruction("movzx r9d, BYTE PTR [rax + r11]");                     // byte[ipv4_start-1]
    emitter.instruction("cmp r9d, 0x3A");                                       // is the preceding byte ':'?
    emitter.instruction("je __rt_filter_ipv6_walkback_done_x86_64");            // found the boundary: stop
    emitter.instruction("mov r10, r11");                                        // walk one byte further back
    emitter.instruction("jmp __rt_filter_ipv6_walkback_loop_x86_64");

    emitter.label("__rt_filter_ipv6_walkback_done_x86_64");
    emitter.instruction("mov rcx, rax");                                        // rcx = tail pointer (str + ipv4_start)
    emitter.instruction("add rcx, r10");
    emitter.instruction("mov rsi, rdx");                                        // rsi = tail length (len - ipv4_start)
    emitter.instruction("sub rsi, r10");
    emit_reject_leading_zero_octet_scan_x86_64(
        emitter,
        "rcx",
        "rsi",
        fail_label,
        "__rt_filter_ipv6_v4tail_lz_scan_x86_64",
    );

    emitter.label("__rt_filter_ipv6_finddot_none_x86_64");
}
