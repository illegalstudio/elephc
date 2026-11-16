//! Purpose:
//! Emits the `__rt_inet_pton` runtime helper assembly for the inet_pton builtin.
//! Packs a textual IPv4 or IPv6 address into its 4- or 16-byte network-order form.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - The parsing is the platform's own `inet_pton(3)`, which is where PHP's goes too. A
//!   hand-written IPv6 parser would have to reproduce `::` compression and the embedded-IPv4
//!   form, and be wrong in a different way on each target (issue #692).
//! - The family is chosen the way php-src chooses it: a `:` anywhere in the input selects
//!   `AF_INET6`, everything else `AF_INET`. `AF_INET6` differs by platform (30 on Darwin,
//!   10 on Linux); `AF_INET` is 2 everywhere.
//! - The destination comes from `__rt_concat_reserve` and is published with
//!   `__rt_concat_publish`, so a 16-byte result cannot run off the end of the 64 KiB scratch
//!   buffer and falls back to the heap when it no longer fits.
//! - `MAX_ADDRESS_BYTES` is a documented divergence: PHP passes a string of any length to
//!   `inet_pton(3)`. See "Known divergence: `inet_pton()` caps its argument at 255 bytes" in
//!   `docs/php/strings.md` for why the two cannot disagree on anything that is an address
//!   (issue #1160).

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// The longest textual address this helper accepts, not counting the NUL.
///
/// A full IPv6 address with an embedded IPv4 tail is 45 bytes. The rest is room for a zone
/// identifier, which the platform parser may or may not accept -- Darwin's takes
/// `fe80::1%eth0`, glibc's refuses it, and PHP inherits whichever one it was built against.
/// 255 leaves an order of magnitude of slack so the bound is reachable only by input that is
/// not an address, and such input is refused before the copy rather than truncated into one
/// that parses.
const MAX_ADDRESS_BYTES: i64 = 255;

/// inet_pton: pack a textual IPv4 or IPv6 address into its network-order binary form.
/// Input:  x0 = string pointer, x1 = string length
/// Output: x1 = binary pointer (0 when invalid), x2 = length (4 for IPv4, 16 for IPv6)
pub fn emit_inet_pton(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_inet_pton_linux_x86_64(emitter);
        return;
    }

    let inet6 = emitter.platform.af_inet6();

    emitter.blank();
    emitter.comment("--- runtime: inet_pton ---");
    emitter.label_global("__rt_inet_pton");

    // Frame: [sp, #0..256) NUL-terminated address copy, #256 packed destination,
    // #264 packed length, #272 address family, #288 saved frame/link.
    emitter.instruction("sub sp, sp, #320");                                    // allocate the address copy, mode slot and saved bookkeeping
    emitter.instruction("stp x29, x30, [sp, #304]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #304");                                   // establish the helper frame pointer
    emitter.instruction("str x2, [sp, #296]");                                  // preserve the requested result mode

    emitter.instruction("cbz x1, __rt_inet_pton_false");                        // an empty string is no address
    emitter.instruction(&format!("cmp x1, #{}", MAX_ADDRESS_BYTES));            // longer than any address can be?
    emitter.instruction("b.hi __rt_inet_pton_false");                           // refuse rather than truncate into something that parses

    // -- fast path: parse a canonical dotted-decimal IPv4 address directly --
    emitter.instruction("mov x9, x0");                                          // source cursor for the fast parser
    emitter.instruction("mov x10, x1");                                         // remaining byte count for the fast parser
    emitter.instruction("mov x11, xzr");                                        // octet index
    emitter.label("__rt_inet_pton_fast_octet");
    emitter.instruction("cmp x11, #4");                                         // have four octets been parsed?
    emitter.instruction("b.hs __rt_inet_pton_fast_end");                        // check for trailing input
    emitter.instruction("mov x13, #0");                                         // octet value starts at zero
    emitter.instruction("mov x14, #0");                                         // octet digit count starts at zero
    emitter.label("__rt_inet_pton_fast_digit");
    emitter.instruction("cbz x10, __rt_inet_pton_fast_octet_done");             // end of input finishes this octet
    emitter.instruction("ldrb w15, [x9]");                                      // load the next address byte
    emitter.instruction("sub w16, w15, #48");                                   // map an ASCII digit to 0..9
    emitter.instruction("cmp w16, #9");                                         // is this a decimal digit?
    emitter.instruction("b.hi __rt_inet_pton_fast_octet_done");                 // a non-digit ends the octet
    emitter.instruction("mov w17, #10");                                        // decimal radix
    emitter.instruction("madd w13, w13, w17, w16");                             // accumulate the next decimal digit
    emitter.instruction("cmp w13, #255");                                       // does the octet still fit a byte?
    emitter.instruction("b.hi __rt_inet_pton_generic");                         // out-of-range octets use libc validation
    emitter.instruction("add x14, x14, #1");                                    // count this digit
    emitter.instruction("cmp x14, #2");                                         // is this the second digit?
    emitter.instruction("b.ne __rt_inet_pton_fast_no_leading_zero");            // only a two-digit octet can hide a leading zero
    emitter.instruction("cmp x13, #10");                                        // a value below ten after two digits is a leading zero
    emitter.instruction("b.lo __rt_inet_pton_generic");                         // let the platform's parser decide whether it accepts one
    emitter.label("__rt_inet_pton_fast_no_leading_zero");
    emitter.instruction("cmp x14, #4");                                         // more than three digits?
    emitter.instruction("b.hs __rt_inet_pton_generic");                         // let libc accept or reject the long form
    emitter.instruction("add x9, x9, #1");                                      // advance the fast parser cursor
    emitter.instruction("sub x10, x10, #1");                                    // consume one input byte
    emitter.instruction("b __rt_inet_pton_fast_digit");                         // parse the next digit
    emitter.label("__rt_inet_pton_fast_octet_done");
    emitter.instruction("cbz x14, __rt_inet_pton_generic");                     // an empty octet is not an address
    emitter.instruction("add x15, sp, #280");                                   // address the packed fast-path result
    emitter.instruction("strb w13, [x15, x11]");                                // store this octet
    emitter.instruction("add x11, x11, #1");                                    // advance to the next octet
    emitter.instruction("cmp x11, #4");                                         // was that the fourth octet?
    emitter.instruction("b.eq __rt_inet_pton_fast_end");                        // check for trailing input
    emitter.instruction("cbz x10, __rt_inet_pton_generic");                     // a missing separator is not an address
    emitter.instruction("ldrb w15, [x9]");                                      // load the separator
    emitter.instruction("cmp w15, #46");                                        // is it a dot?
    emitter.instruction("b.ne __rt_inet_pton_generic");                         // anything else is not dotted decimal
    emitter.instruction("add x9, x9, #1");                                      // skip the dot
    emitter.instruction("sub x10, x10, #1");                                    // consume the separator
    emitter.instruction("b __rt_inet_pton_fast_octet");                         // parse the next octet
    emitter.label("__rt_inet_pton_fast_end");
    emitter.instruction("cbnz x10, __rt_inet_pton_generic");                    // trailing bytes are not an IPv4 address
    emitter.instruction("ldr x9, [sp, #296]");                                  // reload the requested result mode
    emitter.instruction("cbnz x9, __rt_inet_pton_fast_length");                 // length-only calls skip the reservation
    emitter.instruction("mov x0, #4");                                          // the packed IPv4 result is four bytes
    abi::emit_call_label(emitter, "__rt_concat_reserve");                       // reserve the four-byte result
    emitter.instruction("ldr w13, [sp, #280]");                                 // reload the packed octets
    emitter.instruction("str w13, [x0]");                                       // publish the four packed bytes
    emitter.instruction("mov x1, x0");                                          // return the packed result pointer
    emitter.instruction("mov x2, #4");                                          // return the IPv4 result length
    abi::emit_call_label(emitter, "__rt_concat_publish");                       // charge the scratch and preserve the result pair
    emitter.instruction("b __rt_inet_pton_return");                             // share the frame restore
    emitter.label("__rt_inet_pton_fast_length");
    emitter.instruction("mov x0, #4");                                          // an IPv4 address is four packed bytes
    emitter.instruction("b __rt_inet_pton_return");                             // return the packed length

    // -- generic path: copy into a NUL-terminated buffer `inet_pton(3)` can read --
    emitter.label("__rt_inet_pton_generic");
    emitter.instruction("mov x9, x0");                                          // source cursor
    emitter.instruction("mov x10, x1");                                         // remaining byte count
    emitter.instruction("mov x11, sp");                                         // destination cursor
    emitter.instruction("mov x12, xzr");                                        // saw a ':' -> the address is IPv6
    emitter.label("__rt_inet_pton_copy");
    emitter.instruction("cbz x10, __rt_inet_pton_copy_done");                   // stop once every byte is copied
    emitter.instruction("ldrb w13, [x9], #1");                                  // read one address byte
    emitter.instruction("strb w13, [x11], #1");                                 // write it into the NUL-terminated copy
    emitter.instruction("cmp w13, #58");                                        // is this byte a ':'?
    emitter.instruction("b.ne __rt_inet_pton_copy_next");                       // only a colon selects the IPv6 family
    emitter.instruction("mov x12, #1");                                         // remember that this is an IPv6 address
    emitter.label("__rt_inet_pton_copy_next");
    emitter.instruction("sub x10, x10, #1");                                    // one fewer byte to copy
    emitter.instruction("b __rt_inet_pton_copy");                               // continue copying
    emitter.label("__rt_inet_pton_copy_done");
    emitter.instruction("strb wzr, [x11]");                                     // terminate the copy for the C parser

    // -- select the family and the packed width it produces --
    emitter.instruction("mov x13, #2");                                         // AF_INET
    emitter.instruction("mov x14, #4");                                         // an IPv4 address packs into four bytes
    emitter.instruction("cbz x12, __rt_inet_pton_family_ready");                // no colon: keep the IPv4 family
    emitter.instruction(&format!("mov x13, #{}", inet6));                       // AF_INET6 for this platform
    emitter.instruction("mov x14, #16");                                        // an IPv6 address packs into sixteen bytes
    emitter.label("__rt_inet_pton_family_ready");
    emitter.instruction("str x13, [sp, #272]");                                 // save the family across the reservation
    emitter.instruction("str x14, [sp, #264]");                                 // save the packed length across the reservation

    // -- reserve the destination, so sixteen bytes cannot run off the scratch buffer --
    emitter.instruction("mov x0, x14");                                         // ask for exactly the packed width
    abi::emit_call_label(emitter, "__rt_concat_reserve");                       // clobbers the caller-saved registers
    emitter.instruction("str x0, [sp, #256]");                                  // save the reserved destination

    // -- parse straight into the reserved destination --
    emitter.instruction("mov x2, x0");                                          // inet_pton writes its answer there
    emitter.instruction("ldr x0, [sp, #272]");                                  // reload the selected family as the first C argument
    emitter.instruction("add x1, sp, #0");                                      // pass the NUL-terminated copy as the second
    emitter.bl_c("inet_pton");                                                  // parse the address for the selected family
    emitter.instruction("cmp w0, #1");                                          // 1 means the address parsed; 0 and -1 do not
    emitter.instruction("b.ne __rt_inet_pton_unparsed");                        // release the reservation before reporting false

    emitter.instruction("ldr x1, [sp, #256]");                                  // the packed pointer
    emitter.instruction("ldr x2, [sp, #264]");                                  // the packed length
    emitter.instruction("ldr x9, [sp, #296]");                                  // reload the requested result mode
    emitter.instruction("cbnz x9, __rt_inet_pton_generic_length");              // length-only calls skip publication
    abi::emit_call_label(emitter, "__rt_concat_publish");                       // charge the scratch and preserve x1/x2
    emitter.instruction("b __rt_inet_pton_return");                             // share the frame restore
    emitter.label("__rt_inet_pton_generic_length");
    emitter.instruction("mov x0, x2");                                          // return the packed length as an integer
    emitter.instruction("b __rt_inet_pton_return");                             // share the frame restore

    // A reservation that was never published still has to be released: `__rt_concat_reserve`
    // answers with an OWNED heap block once the scratch buffer can no longer fit the request,
    // and a well-formed-looking address that the parser then rejects would leak one per call.
    // `__rt_heap_free_safe` ignores a scratch pointer, which is the ordinary case.
    emitter.label("__rt_inet_pton_unparsed");
    emitter.instruction("ldr x0, [sp, #256]");                                  // the reservation this call never published
    abi::emit_call_label(emitter, "__rt_heap_free_safe");                       // a no-op for scratch, a free for the heap fallback
    emitter.instruction("b __rt_inet_pton_false");                              // report the invalid address

    emitter.label("__rt_inet_pton_false");
    emitter.instruction("ldr x9, [sp, #296]");                                  // reload the requested result mode
    emitter.instruction("cbnz x9, __rt_inet_pton_false_length");                // length-only calls return zero
    emitter.instruction("mov x1, #0");                                          // a null pointer signals an invalid address
    emitter.instruction("mov x2, #0");                                          // zero length for the invalid case
    emitter.instruction("b __rt_inet_pton_return");                             // share the frame restore
    emitter.label("__rt_inet_pton_false_length");
    emitter.instruction("mov x0, #0");                                          // strlen(false) is zero
    emitter.label("__rt_inet_pton_return");
    emitter.instruction("ldp x29, x30, [sp, #304]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #320");                                    // release the frame
    emitter.instruction("ret");                                                 // return the requested result
}

/// Emits the Linux x86_64 string runtime helper for inet pton.
fn emit_inet_pton_linux_x86_64(emitter: &mut Emitter) {
    let inet6 = emitter.platform.af_inet6();

    emitter.blank();
    emitter.comment("--- runtime: inet_pton ---");
    emitter.label_global("__rt_inet_pton");

    // Frame: [rbp-304 .. rbp-48) NUL-terminated address copy, [rbp-40] packed destination,
    // [rbp-32] packed length, [rbp-24] address family.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 304");                                        // allocate the address copy and the saved bookkeeping
    emitter.instruction("mov QWORD PTR [rbp - 8], r8");                         // preserve the requested result mode

    emitter.instruction("test rsi, rsi");                                       // an empty string is no address
    emitter.instruction("jz __rt_inet_pton_false_x86");                         // refuse it before the copy
    emitter.instruction(&format!("cmp rsi, {}", MAX_ADDRESS_BYTES));            // longer than any address can be?
    emitter.instruction("ja __rt_inet_pton_false_x86");                         // refuse rather than truncate into something that parses

    // -- fast path: parse a canonical dotted-decimal IPv4 address directly --
    emitter.instruction("mov r8, rax");                                         // source cursor for the fast parser
    emitter.instruction("mov r9, rdx");                                         // remaining byte count for the fast parser
    emitter.instruction("xor r10d, r10d");                                      // octet index
    emitter.label("__rt_inet_pton_fast_octet_x86");
    emitter.instruction("cmp r10, 4");                                          // have four octets been parsed?
    emitter.instruction("jae __rt_inet_pton_fast_end_x86");                     // check for trailing input
    emitter.instruction("xor r11d, r11d");                                      // octet value starts at zero
    emitter.instruction("xor ecx, ecx");                                        // octet digit count starts at zero
    emitter.label("__rt_inet_pton_fast_digit_x86");
    emitter.instruction("test r9, r9");                                         // end of input finishes this octet
    emitter.instruction("jz __rt_inet_pton_fast_octet_done_x86");               // finish the current octet
    emitter.instruction("movzx esi, BYTE PTR [r8]");                            // load the next address byte
    emitter.instruction("lea edi, [rsi - 48]");                                 // map an ASCII digit to 0..9
    emitter.instruction("cmp edi, 9");                                          // is this a decimal digit?
    emitter.instruction("ja __rt_inet_pton_fast_octet_done_x86");               // a non-digit ends the octet
    emitter.instruction("imul r11d, r11d, 10");                                 // shift the accumulated value by one decimal place
    emitter.instruction("add r11d, edi");                                       // add the next decimal digit
    emitter.instruction("cmp r11d, 255");                                       // does the octet still fit a byte?
    emitter.instruction("ja __rt_inet_pton_generic_x86");                       // out-of-range octets use libc validation
    emitter.instruction("inc ecx");                                             // count this digit
    emitter.instruction("cmp ecx, 2");                                          // is this the second digit?
    emitter.instruction("jne __rt_inet_pton_fast_no_leading_zero_x86");         // only a two-digit octet can hide a leading zero
    emitter.instruction("cmp r11d, 10");                                        // a value below ten after two digits is a leading zero
    emitter.instruction("jb __rt_inet_pton_generic_x86");                       // let the platform's parser decide whether it accepts one
    emitter.label("__rt_inet_pton_fast_no_leading_zero_x86");
    emitter.instruction("cmp ecx, 4");                                          // more than three digits?
    emitter.instruction("jae __rt_inet_pton_generic_x86");                      // let libc accept or reject the long form
    emitter.instruction("inc r8");                                              // advance the fast parser cursor
    emitter.instruction("dec r9");                                              // consume one input byte
    emitter.instruction("jmp __rt_inet_pton_fast_digit_x86");                   // parse the next digit
    emitter.label("__rt_inet_pton_fast_octet_done_x86");
    emitter.instruction("test ecx, ecx");                                       // an empty octet is not an address
    emitter.instruction("jz __rt_inet_pton_generic_x86");                       // fall back to libc for validation
    emitter.instruction("mov BYTE PTR [rbp - 16 + r10], r11b");                 // store this octet
    emitter.instruction("inc r10");                                             // advance to the next octet
    emitter.instruction("cmp r10, 4");                                          // was that the fourth octet?
    emitter.instruction("je __rt_inet_pton_fast_end_x86");                      // check for trailing input
    emitter.instruction("test r9, r9");                                         // a missing separator is not an address
    emitter.instruction("jz __rt_inet_pton_generic_x86");                       // fall back to libc for validation
    emitter.instruction("cmp BYTE PTR [r8], 46");                               // is the separator a dot?
    emitter.instruction("jne __rt_inet_pton_generic_x86");                      // anything else is not dotted decimal
    emitter.instruction("inc r8");                                              // skip the dot
    emitter.instruction("dec r9");                                              // consume the separator
    emitter.instruction("jmp __rt_inet_pton_fast_octet_x86");                   // parse the next octet
    emitter.label("__rt_inet_pton_fast_end_x86");
    emitter.instruction("test r9, r9");                                         // trailing bytes are not an IPv4 address
    emitter.instruction("jnz __rt_inet_pton_generic_x86");                      // fall back to libc for validation
    emitter.instruction("cmp QWORD PTR [rbp - 8], 0");                          // was a length-only result requested?
    emitter.instruction("jne __rt_inet_pton_fast_length_x86");                  // length-only calls skip the reservation
    emitter.instruction("mov eax, 4");                                          // the packed IPv4 result is four bytes
    abi::emit_call_label(emitter, "__rt_concat_reserve");                      // reserve the four-byte result
    emitter.instruction("mov r11d, DWORD PTR [rbp - 16]");                      // reload the packed octets
    emitter.instruction("mov DWORD PTR [rax], r11d");                           // publish the four packed bytes
    emitter.instruction("mov rdx, 4");                                          // return the IPv4 result length
    abi::emit_call_label(emitter, "__rt_concat_publish");                      // charge the scratch and preserve the result pair
    emitter.instruction("jmp __rt_inet_pton_return_x86");                       // share the frame restore
    emitter.label("__rt_inet_pton_fast_length_x86");
    emitter.instruction("mov eax, 4");                                          // an IPv4 address is four packed bytes
    emitter.instruction("jmp __rt_inet_pton_return_x86");                       // return the packed length

    // -- generic path: copy into a NUL-terminated buffer `inet_pton(3)` can read --
    emitter.label("__rt_inet_pton_generic_x86");
    emitter.instruction("mov r8, rax");                                         // restore the source pointer after the fast parser clobbered rdi
    emitter.instruction("mov r9, rdx");                                         // restore the remaining byte count after the fast parser clobbered rsi
    emitter.instruction("lea r10, [rbp - 304]");                                // destination cursor
    emitter.instruction("xor r11d, r11d");                                      // saw a ':' -> the address is IPv6
    emitter.label("__rt_inet_pton_copy_x86");
    emitter.instruction("test r9, r9");                                         // stop once every byte is copied
    emitter.instruction("jz __rt_inet_pton_copy_done_x86");                     // the copy is complete
    emitter.instruction("movzx ecx, BYTE PTR [r8]");                            // read one address byte
    emitter.instruction("mov BYTE PTR [r10], cl");                              // write it into the NUL-terminated copy
    emitter.instruction("cmp ecx, 58");                                         // is this byte a ':'?
    emitter.instruction("jne __rt_inet_pton_copy_next_x86");                    // only a colon selects the IPv6 family
    emitter.instruction("mov r11d, 1");                                         // remember that this is an IPv6 address
    emitter.label("__rt_inet_pton_copy_next_x86");
    emitter.instruction("add r8, 1");                                           // advance the source cursor
    emitter.instruction("add r10, 1");                                          // advance the destination cursor
    emitter.instruction("sub r9, 1");                                           // one fewer byte to copy
    emitter.instruction("jmp __rt_inet_pton_copy_x86");                         // continue copying
    emitter.label("__rt_inet_pton_copy_done_x86");
    emitter.instruction("mov BYTE PTR [r10], 0");                               // terminate the copy for the C parser

    // -- select the family and the packed width it produces --
    emitter.instruction("mov rcx, 2");                                          // AF_INET
    emitter.instruction("mov rdx, 4");                                          // an IPv4 address packs into four bytes
    emitter.instruction("test r11d, r11d");                                     // did the address contain a colon?
    emitter.instruction("jz __rt_inet_pton_family_ready_x86");                  // no colon: keep the IPv4 family
    emitter.instruction(&format!("mov rcx, {}", inet6));                        // AF_INET6 for this platform
    emitter.instruction("mov rdx, 16");                                         // an IPv6 address packs into sixteen bytes
    emitter.label("__rt_inet_pton_family_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the family across the reservation
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save the packed length across the reservation

    // -- reserve the destination, so sixteen bytes cannot run off the scratch buffer --
    emitter.instruction("mov rax, rdx");                                        // ask for exactly the packed width
    abi::emit_call_label(emitter, "__rt_concat_reserve");                       // clobbers the caller-saved registers
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the reserved destination

    // -- parse straight into the reserved destination --
    emitter.instruction("mov rdx, rax");                                        // inet_pton writes its answer there
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the selected family as the first argument
    emitter.instruction("lea rsi, [rbp - 304]");                                // pass the NUL-terminated copy as the second
    emitter.instruction("xor eax, eax");                                        // no vector arguments in this call
    emitter.bl_c("inet_pton");                                                  // parse the address for the selected family
    emitter.instruction("cmp eax, 1");                                          // 1 means the address parsed; 0 and -1 do not
    emitter.instruction("jne __rt_inet_pton_unparsed_x86");                     // release the reservation before reporting false

    emitter.instruction("cmp QWORD PTR [rbp - 8], 0");                          // was a length-only result requested?
    emitter.instruction("jne __rt_inet_pton_generic_length_x86");               // length-only calls skip publication
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // the packed pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // the packed length
    abi::emit_call_label(emitter, "__rt_concat_publish");                       // charge the scratch and preserve rax/rdx
    emitter.instruction("jmp __rt_inet_pton_return_x86");                       // share the frame restore
    emitter.label("__rt_inet_pton_generic_length_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the packed length as an integer
    emitter.instruction("jmp __rt_inet_pton_return_x86");                       // share the frame restore

    // See the AArch64 twin: an unpublished reservation may be an owned heap block.
    emitter.label("__rt_inet_pton_unparsed_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // the reservation this call never published
    abi::emit_call_label(emitter, "__rt_heap_free_safe");                       // a no-op for scratch, a free for the heap fallback
    emitter.instruction("jmp __rt_inet_pton_false_x86");                        // report the invalid address

    emitter.label("__rt_inet_pton_false_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 8], 0");                          // was a length-only result requested?
    emitter.instruction("jne __rt_inet_pton_false_length_x86");                 // length-only calls return zero
    emitter.instruction("xor eax, eax");                                        // a null pointer signals an invalid address
    emitter.instruction("xor edx, edx");                                        // zero length for the invalid case
    emitter.instruction("jmp __rt_inet_pton_return_x86");                       // share the frame restore
    emitter.label("__rt_inet_pton_false_length_x86");
    emitter.instruction("xor eax, eax");                                        // strlen(false) is zero
    emitter.label("__rt_inet_pton_return_x86");
    emitter.instruction("mov rsp, rbp");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the requested result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{AppleVariant, Platform, Target};

    /// Emits the helper for one target and returns its assembly.
    fn assembly_for(target: Target) -> String {
        let mut emitter = Emitter::new(target);
        emit_inet_pton(&mut emitter);
        emitter.output()
    }

    /// The length-only mode skips destination reservation and publication on both targets.
    #[test]
    fn inet_pton_length_mode_returns_the_packed_width() {
        let arm = assembly_for(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(arm.contains("str x2, [sp, #296]"), "{arm}");
        assert!(arm.contains("cbnz x9, __rt_inet_pton_fast_length"), "{arm}");
        assert!(arm.contains("__rt_inet_pton_generic_length:"), "{arm}");
        assert!(arm.contains("__rt_inet_pton_false_length:"), "{arm}");

        let x64 = assembly_for(Target::new(Platform::Linux, Arch::X86_64));
        assert!(x64.contains("mov QWORD PTR [rbp - 8], r8"), "{x64}");
        assert!(x64.contains("jne __rt_inet_pton_fast_length_x86"), "{x64}");
        assert!(x64.contains("__rt_inet_pton_generic_length_x86:"), "{x64}");
        assert!(x64.contains("__rt_inet_pton_false_length_x86:"), "{x64}");
    }

    /// The `AF_INET6` value is the target's, not the host's.
    ///
    /// Darwin spells it 30 and Linux 10, and `inet_pton(3)` answers `EAFNOSUPPORT` -- reported
    /// as `false` here -- for every IPv6 address when it is handed the wrong one. A
    /// cross-compiled binary would therefore reject addresses the same source accepts natively,
    /// which no host-run test can see (issue #692).
    #[test]
    fn inet_pton_selects_the_targets_own_ipv6_family() {
        for (target, expected) in [
            (Target::new(Platform::MacOS, Arch::AArch64), "mov x13, #30"),
            (
                Target::new_apple(Arch::AArch64, AppleVariant::IOS),
                "mov x13, #30",
            ),
            (
                Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
                "mov x13, #30",
            ),
            (Target::new(Platform::Linux, Arch::AArch64), "mov x13, #10"),
        ] {
            let asm = assembly_for(target);
            assert!(
                asm.contains(expected),
                "{:?} must select its own AF_INET6: {}",
                target,
                asm
            );
        }
        assert!(assembly_for(Target::new(Platform::Linux, Arch::X86_64)).contains("mov rcx, 10"));
    }

    /// `AF_INET` is 2 on every supported target, so the IPv4 path carries no platform split.
    #[test]
    fn inet_pton_uses_the_same_ipv4_family_everywhere() {
        assert!(assembly_for(Target::new(Platform::MacOS, Arch::AArch64)).contains("mov x13, #2"));
        assert!(assembly_for(Target::new(Platform::Linux, Arch::AArch64)).contains("mov x13, #2"));
        assert!(assembly_for(Target::new(Platform::Linux, Arch::X86_64)).contains("mov rcx, 2"));
    }

    /// The parse is the platform's, so every target has to actually call it.
    #[test]
    fn inet_pton_calls_the_platform_parser() {
        assert!(assembly_for(Target::new(Platform::MacOS, Arch::AArch64)).contains("bl _inet_pton"));
        assert!(assembly_for(Target::new(Platform::Linux, Arch::AArch64)).contains("bl inet_pton"));
        assert!(assembly_for(Target::new(Platform::Linux, Arch::X86_64)).contains("call inet_pton"));
    }

    /// A reservation the parser rejects is released rather than leaked.
    ///
    /// `__rt_concat_reserve` answers with an OWNED heap block once the scratch buffer can no
    /// longer fit the request, so an address that looks well-formed enough to be reserved for
    /// and is then refused would leak one allocation per call.
    #[test]
    fn inet_pton_releases_a_reservation_it_never_publishes() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let asm = assembly_for(target);
            assert!(
                asm.contains("__rt_heap_free_safe"),
                "{:?} must release an unpublished reservation: {}",
                target,
                asm
            );
        }
    }

    /// The destination is RESERVED, never taken straight from the scratch cursor.
    ///
    /// Writing at `_concat_buf + _concat_off` without asking runs sixteen bytes off the end of
    /// the 64 KiB buffer once an earlier result has pushed the cursor near it, and silently
    /// corrupts whatever follows. `__rt_concat_reserve` bounds the request and falls back to the
    /// heap; `__rt_concat_publish` is what charges the scratch afterwards.
    #[test]
    fn inet_pton_reserves_and_publishes_its_destination() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let asm = assembly_for(target);
            assert!(
                asm.contains("__rt_concat_reserve") && asm.contains("__rt_concat_publish"),
                "{:?} must reserve and publish: {}",
                target,
                asm
            );
            assert!(
                !asm.contains("_concat_off"),
                "{:?} must not touch the scratch cursor directly: {}",
                target,
                asm
            );
        }
    }
}
