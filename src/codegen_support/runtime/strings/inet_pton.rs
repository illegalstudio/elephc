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

use crate::codegen_support::{abi, emit::Emitter, platform::Arch, platform::Platform};

/// The longest textual address this helper accepts, not counting the NUL.
///
/// A full IPv6 address with an embedded IPv4 tail is 45 bytes. The rest is room for a zone
/// identifier, which the platform parser may or may not accept -- Darwin's takes
/// `fe80::1%eth0`, glibc's refuses it, and PHP inherits whichever one it was built against.
/// 255 leaves an order of magnitude of slack so the bound is reachable only by input that is
/// not an address, and such input is refused before the copy rather than truncated into one
/// that parses.
const MAX_ADDRESS_BYTES: i64 = 255;

/// The `AF_INET6` value of the target's own headers.
///
/// Darwin and Linux disagree, and passing the wrong one makes `inet_pton(3)` answer `-1`
/// (`EAFNOSUPPORT`) for every IPv6 address rather than parsing it.
fn af_inet6(platform: Platform) -> i64 {
    match platform {
        Platform::MacOS => 30,
        Platform::Linux => 10,
        Platform::Windows => 23,
    }
}

/// inet_pton: pack a textual IPv4 or IPv6 address into its network-order binary form.
/// Input:  x0 = string pointer, x1 = string length
/// Output: x1 = binary pointer (0 when invalid), x2 = length (4 for IPv4, 16 for IPv6)
pub fn emit_inet_pton(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_inet_pton_linux_x86_64(emitter);
        return;
    }

    let inet6 = af_inet6(emitter.platform);

    emitter.blank();
    emitter.comment("--- runtime: inet_pton ---");
    emitter.label_global("__rt_inet_pton");

    // Frame: [sp, #0..256) NUL-terminated address copy, #256 packed destination,
    // #264 packed length, #272 address family, #288 saved frame/link.
    emitter.instruction("sub sp, sp, #304");                                    // allocate the address copy and the saved bookkeeping
    emitter.instruction("stp x29, x30, [sp, #288]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #288");                                   // establish the helper frame pointer

    emitter.instruction("cbz x1, __rt_inet_pton_false");                        // an empty string is no address
    emitter.instruction(&format!("cmp x1, #{}", MAX_ADDRESS_BYTES));            // longer than any address can be?
    emitter.instruction("b.hi __rt_inet_pton_false");                           // refuse rather than truncate into something that parses

    // -- copy the borrowed bytes into a NUL-terminated buffer `inet_pton(3)` can read --
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
    abi::emit_call_label(emitter, "__rt_concat_publish");                       // charge the scratch and preserve x1/x2
    emitter.instruction("ldp x29, x30, [sp, #288]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #304");                                    // release the frame
    emitter.instruction("ret");                                                 // return the packed address

    // A reservation that was never published still has to be released: `__rt_concat_reserve`
    // answers with an OWNED heap block once the scratch buffer can no longer fit the request,
    // and a well-formed-looking address that the parser then rejects would leak one per call.
    // `__rt_heap_free_safe` ignores a scratch pointer, which is the ordinary case.
    emitter.label("__rt_inet_pton_unparsed");
    emitter.instruction("ldr x0, [sp, #256]");                                  // the reservation this call never published
    abi::emit_call_label(emitter, "__rt_heap_free_safe");                       // a no-op for scratch, a free for the heap fallback
    emitter.instruction("b __rt_inet_pton_false");                              // report the invalid address

    emitter.label("__rt_inet_pton_false");
    emitter.instruction("mov x1, #0");                                          // a null pointer signals an invalid address
    emitter.instruction("mov x2, #0");                                          // zero length for the invalid case
    emitter.instruction("ldp x29, x30, [sp, #288]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #304");                                    // release the frame
    emitter.instruction("ret");                                                 // return the invalid-address result
}

/// Emits the Linux x86_64 string runtime helper for inet pton.
fn emit_inet_pton_linux_x86_64(emitter: &mut Emitter) {
    let inet6 = af_inet6(emitter.platform);

    emitter.blank();
    emitter.comment("--- runtime: inet_pton ---");
    emitter.label_global("__rt_inet_pton");

    // Frame: [rbp-304 .. rbp-48) NUL-terminated address copy, [rbp-40] packed destination,
    // [rbp-32] packed length, [rbp-24] address family.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 304");                                        // allocate the address copy and the saved bookkeeping

    emitter.instruction("test rsi, rsi");                                       // an empty string is no address
    emitter.instruction("jz __rt_inet_pton_false_x86");                         // refuse it before the copy
    emitter.instruction(&format!("cmp rsi, {}", MAX_ADDRESS_BYTES));            // longer than any address can be?
    emitter.instruction("ja __rt_inet_pton_false_x86");                         // refuse rather than truncate into something that parses

    // -- copy the borrowed bytes into a NUL-terminated buffer `inet_pton(3)` can read --
    emitter.instruction("mov r8, rdi");                                         // source cursor
    emitter.instruction("mov r9, rsi");                                         // remaining byte count
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

    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // the packed pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // the packed length
    abi::emit_call_label(emitter, "__rt_concat_publish");                       // charge the scratch and preserve rax/rdx
    emitter.instruction("mov rsp, rbp");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the packed address

    // See the AArch64 twin: an unpublished reservation may be an owned heap block.
    emitter.label("__rt_inet_pton_unparsed_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // the reservation this call never published
    abi::emit_call_label(emitter, "__rt_heap_free_safe");                       // a no-op for scratch, a free for the heap fallback
    emitter.instruction("jmp __rt_inet_pton_false_x86");                        // report the invalid address

    emitter.label("__rt_inet_pton_false_x86");
    emitter.instruction("xor eax, eax");                                        // a null pointer signals an invalid address
    emitter.instruction("xor edx, edx");                                        // zero length for the invalid case
    emitter.instruction("mov rsp, rbp");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the invalid-address result
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
