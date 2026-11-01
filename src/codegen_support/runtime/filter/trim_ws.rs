//! Purpose:
//! Emits the `__rt_filter_trim_ws` runtime helper: trims the exact whitespace
//! set PHP's `ext/filter` validators accept (space, tab, LF, VT, CR — NOT form
//! feed) from both ends of a PHP string.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::filter`.
//! - `__rt_filter_validate_int`, `__rt_filter_validate_float`, and
//!   `__rt_filter_validate_bool_str` (all in this same `filter` runtime module).
//!
//! Key details:
//! - The accepted whitespace set is `{0x09, 0x0A, 0x0B, 0x0D, 0x20}` — a php-verified
//!   quirk: form feed (`0x0C`) is NOT trimmed by `filter_var()`, unlike C's `isspace()`.
//!   Verified with `php -n -r 'var_dump(filter_var("\x0c42", FILTER_VALIDATE_INT));'`
//!   (PHP 8.5.6 local): `\x0c` before/after a digit makes the whole string fail, while
//!   `\t`/`\n`/`\r`/`\x0b`/space are all trimmed transparently.
//! - This is a leaf helper (no nested calls), so it needs no frame setup.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits `__rt_filter_trim_ws` for the host target.
///
/// AArch64: input/output x1=ptr, x2=len (trimmed in place).
/// x86_64: input/output rax=ptr, rdx=len (trimmed in place).
pub fn emit_filter_trim_ws(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_filter_trim_ws_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: filter_trim_ws ---");
    emitter.label_global("__rt_filter_trim_ws");

    // -- trim leading whitespace --
    emitter.label("__rt_filter_trim_ws_lead");
    emitter.instruction("cbz x2, __rt_filter_trim_ws_lead_done");               // no bytes left to inspect
    emitter.instruction("ldrb w3, [x1]");                                       // load the current leading byte
    emitter.instruction("cmp w3, #0x20");                                       // is it an ASCII space?
    emitter.instruction("b.eq __rt_filter_trim_ws_lead_skip");                  // space is always trimmed
    emitter.instruction("sub w4, w3, #0x09");                                   // normalize into the 0x09-0x0D candidate range
    emitter.instruction("cmp w4, #4");                                          // is the byte within [0x09, 0x0D]?
    emitter.instruction("b.hi __rt_filter_trim_ws_lead_done");                  // outside the range: stop trimming
    emitter.instruction("cmp w3, #0x0C");                                       // form feed (0x0C) is excluded from the accepted set
    emitter.instruction("b.eq __rt_filter_trim_ws_lead_done");                  // form feed: stop trimming
    emitter.label("__rt_filter_trim_ws_lead_skip");
    emitter.instruction("add x1, x1, #1");                                      // advance past the trimmed leading byte
    emitter.instruction("sub x2, x2, #1");                                      // shrink the remaining length
    emitter.instruction("b __rt_filter_trim_ws_lead");                          // continue trimming leading whitespace
    emitter.label("__rt_filter_trim_ws_lead_done");

    // -- trim trailing whitespace --
    emitter.label("__rt_filter_trim_ws_trail");
    emitter.instruction("cbz x2, __rt_filter_trim_ws_done");                    // no bytes left to inspect
    emitter.instruction("sub x5, x2, #1");                                      // index of the last remaining byte
    emitter.instruction("ldrb w3, [x1, x5]");                                   // load the current trailing byte
    emitter.instruction("cmp w3, #0x20");                                       // is it an ASCII space?
    emitter.instruction("b.eq __rt_filter_trim_ws_trail_skip");                 // space is always trimmed
    emitter.instruction("sub w4, w3, #0x09");                                   // normalize into the 0x09-0x0D candidate range
    emitter.instruction("cmp w4, #4");                                          // is the byte within [0x09, 0x0D]?
    emitter.instruction("b.hi __rt_filter_trim_ws_done");                       // outside the range: stop trimming
    emitter.instruction("cmp w3, #0x0C");                                       // form feed (0x0C) is excluded from the accepted set
    emitter.instruction("b.eq __rt_filter_trim_ws_done");                       // form feed: stop trimming
    emitter.label("__rt_filter_trim_ws_trail_skip");
    emitter.instruction("sub x2, x2, #1");                                      // shrink the remaining length
    emitter.instruction("b __rt_filter_trim_ws_trail");                         // continue trimming trailing whitespace
    emitter.label("__rt_filter_trim_ws_done");
    emitter.instruction("ret");                                                 // return the trimmed ptr/len in x1/x2
}

/// Emits the x86_64 System V variant of `__rt_filter_trim_ws`.
fn emit_filter_trim_ws_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: filter_trim_ws ---");
    emitter.label_global("__rt_filter_trim_ws");

    // -- trim leading whitespace --
    emitter.label("__rt_filter_trim_ws_lead_x86_64");
    emitter.instruction("test rdx, rdx");                                       // no bytes left to inspect?
    emitter.instruction("je __rt_filter_trim_ws_lead_done_x86_64");             // stop once the string is exhausted
    emitter.instruction("movzx r8d, BYTE PTR [rax]");                           // load the current leading byte
    emitter.instruction("cmp r8d, 0x20");                                       // is it an ASCII space?
    emitter.instruction("je __rt_filter_trim_ws_lead_skip_x86_64");             // space is always trimmed
    emitter.instruction("mov r9d, r8d");                                        // copy the byte for range normalization
    emitter.instruction("sub r9d, 0x09");                                       // normalize into the 0x09-0x0D candidate range
    emitter.instruction("cmp r9d, 4");                                          // is the byte within [0x09, 0x0D]?
    emitter.instruction("ja __rt_filter_trim_ws_lead_done_x86_64");             // outside the range: stop trimming
    emitter.instruction("cmp r8d, 0x0C");                                       // form feed (0x0C) is excluded from the accepted set
    emitter.instruction("je __rt_filter_trim_ws_lead_done_x86_64");             // form feed: stop trimming
    emitter.label("__rt_filter_trim_ws_lead_skip_x86_64");
    emitter.instruction("add rax, 1");                                          // advance past the trimmed leading byte
    emitter.instruction("sub rdx, 1");                                          // shrink the remaining length
    emitter.instruction("jmp __rt_filter_trim_ws_lead_x86_64");                 // continue trimming leading whitespace
    emitter.label("__rt_filter_trim_ws_lead_done_x86_64");

    // -- trim trailing whitespace --
    emitter.label("__rt_filter_trim_ws_trail_x86_64");
    emitter.instruction("test rdx, rdx");                                       // no bytes left to inspect?
    emitter.instruction("je __rt_filter_trim_ws_done_x86_64");                  // stop once the string is exhausted
    emitter.instruction("mov r10, rdx");                                        // copy the remaining length
    emitter.instruction("sub r10, 1");                                          // index of the last remaining byte
    emitter.instruction("movzx r8d, BYTE PTR [rax + r10]");                     // load the current trailing byte
    emitter.instruction("cmp r8d, 0x20");                                       // is it an ASCII space?
    emitter.instruction("je __rt_filter_trim_ws_trail_skip_x86_64");            // space is always trimmed
    emitter.instruction("mov r9d, r8d");                                        // copy the byte for range normalization
    emitter.instruction("sub r9d, 0x09");                                       // normalize into the 0x09-0x0D candidate range
    emitter.instruction("cmp r9d, 4");                                          // is the byte within [0x09, 0x0D]?
    emitter.instruction("ja __rt_filter_trim_ws_done_x86_64");                  // outside the range: stop trimming
    emitter.instruction("cmp r8d, 0x0C");                                       // form feed (0x0C) is excluded from the accepted set
    emitter.instruction("je __rt_filter_trim_ws_done_x86_64");                  // form feed: stop trimming
    emitter.label("__rt_filter_trim_ws_trail_skip_x86_64");
    emitter.instruction("sub rdx, 1");                                          // shrink the remaining length
    emitter.instruction("jmp __rt_filter_trim_ws_trail_x86_64");                // continue trimming trailing whitespace
    emitter.label("__rt_filter_trim_ws_done_x86_64");
    emitter.instruction("ret");                                                 // return the trimmed ptr/len in rax/rdx
}
