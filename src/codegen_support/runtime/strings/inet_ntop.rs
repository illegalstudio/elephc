//! Purpose:
//! Emits the `__rt_inet_ntop` runtime helper assembly for the inet_ntop builtin.
//! Renders a 4-byte IPv4 or 16-byte IPv6 binary address as its presentation string.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - IPv4 keeps the existing path: the four octets are packed into an integer and
//!   `__rt_long2ip` is tail-called to render `A.B.C.D`. Nothing about that was wrong, and it
//!   avoids a C call for the common case.
//! - IPv6 goes through the platform's own `inet_ntop(3)`, which owns the compression rules --
//!   the longest run of zero groups becomes `::`, an `::ffff:` prefix renders its tail as
//!   dotted quad -- and is where PHP's rendering comes from too (issue #692).
//! - The family is the INPUT LENGTH, exactly as php-src decides it: 4 or 16, anything else is
//!   `false`.
//! - The IPv6 rendering lands in a stack buffer first and is then copied into storage from
//!   `__rt_concat_reserve`, published with `__rt_concat_publish`. Writing its 45 bytes straight
//!   at the scratch cursor would run off the end of the 64 KiB buffer once an earlier result
//!   has pushed the cursor near it.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch, platform::Platform};

/// The `AF_INET6` value of the target's own headers; see `inet_pton`'s copy for why it matters.
fn af_inet6(platform: Platform) -> i64 {
    match platform {
        Platform::MacOS => 30,
        Platform::Linux => 10,
        Platform::Windows => 23,
    }
}

/// inet_ntop: render a 4-byte IPv4 or 16-byte IPv6 binary address as text.
/// Input:  x0 = binary pointer, x1 = binary length
/// Output: x1 = string pointer (0 when the length is neither 4 nor 16), x2 = length
pub fn emit_inet_ntop(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_inet_ntop_linux_x86_64(emitter);
        return;
    }

    let inet6 = af_inet6(emitter.platform);

    emitter.blank();
    emitter.comment("--- runtime: inet_ntop ---");
    emitter.label_global("__rt_inet_ntop");

    emitter.instruction("cmp x1, #16");                                         // a 16-byte address is IPv6
    emitter.instruction("b.eq __rt_inet_ntop_v6");                              // render it through the C formatter
    emitter.instruction("cmp x1, #4");                                          // a 4-byte address is IPv4
    emitter.instruction("b.ne __rt_inet_ntop_false");                           // reject any other input length
    emitter.instruction("ldrb w2, [x0]");                                       // load octet 0
    emitter.instruction("ldrb w3, [x0, #1]");                                   // load octet 1
    emitter.instruction("ldrb w4, [x0, #2]");                                   // load octet 2
    emitter.instruction("ldrb w5, [x0, #3]");                                   // load octet 3
    emitter.instruction("lsl x2, x2, #24");                                     // octet 0 to the high byte
    emitter.instruction("lsl x3, x3, #16");                                     // octet 1 to the second byte
    emitter.instruction("lsl x4, x4, #8");                                      // octet 2 to the third byte
    emitter.instruction("orr x2, x2, x3");                                      // merge octet 1
    emitter.instruction("orr x2, x2, x4");                                      // merge octet 2
    emitter.instruction("orr x0, x2, x5");                                      // merge octet 3 into the long2ip argument
    emitter.instruction("b __rt_long2ip");                                      // tail-call long2ip to format the address

    // Frame: [sp, #0..64) the presentation buffer `inet_ntop(3)` writes into,
    // #64 rendered length, #72 reserved destination, #80 saved frame/link.
    emitter.label("__rt_inet_ntop_v6");
    emitter.instruction("sub sp, sp, #96");                                     // allocate the presentation buffer and bookkeeping
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the helper frame pointer
    emitter.instruction("mov x1, x0");                                          // the packed address is inet_ntop's second argument
    emitter.instruction(&format!("mov x0, #{}", inet6));                        // AF_INET6 for this platform
    emitter.instruction("add x2, sp, #0");                                      // the stack buffer is its destination
    emitter.instruction("mov x3, #64");                                         // and its capacity
    emitter.emit_call_c("inet_ntop");                                           // render through the target-aware socket C ABI
    emitter.instruction("cbz x0, __rt_inet_ntop_v6_false");                     // a null answer means the family was refused

    // -- measure the rendering, then reserve exactly that much --
    emitter.instruction("mov x9, x0");                                          // presentation-string cursor
    emitter.instruction("mov x10, xzr");                                        // rendered byte count
    emitter.label("__rt_inet_ntop_v6_len");
    emitter.instruction("ldrb w11, [x9, x10]");                                 // read one rendered byte
    emitter.instruction("cbz w11, __rt_inet_ntop_v6_len_done");                 // the NUL terminates the rendering
    emitter.instruction("add x10, x10, #1");                                    // count it
    emitter.instruction("b __rt_inet_ntop_v6_len");                             // keep measuring
    emitter.label("__rt_inet_ntop_v6_len_done");
    emitter.instruction("str x10, [sp, #64]");                                  // save the rendered length across the reservation
    emitter.instruction("mov x0, x10");                                         // ask for exactly that many bytes
    abi::emit_call_label(emitter, "__rt_concat_reserve");                       // clobbers the caller-saved registers
    emitter.instruction("str x0, [sp, #72]");                                   // save the reserved destination

    // -- copy the rendering out of the stack buffer into it --
    emitter.instruction("add x9, sp, #0");                                      // the presentation buffer is in this frame
    emitter.instruction("ldr x10, [sp, #64]");                                  // reload the rendered length
    emitter.instruction("mov x12, x0");                                         // destination cursor
    emitter.label("__rt_inet_ntop_v6_copy");
    emitter.instruction("cbz x10, __rt_inet_ntop_v6_copied");                   // stop once every byte is copied
    emitter.instruction("ldrb w13, [x9], #1");                                  // read one rendered byte
    emitter.instruction("strb w13, [x12], #1");                                 // write it into the reserved destination
    emitter.instruction("sub x10, x10, #1");                                    // one fewer byte to copy
    emitter.instruction("b __rt_inet_ntop_v6_copy");                            // continue copying
    emitter.label("__rt_inet_ntop_v6_copied");
    emitter.instruction("ldr x1, [sp, #72]");                                   // the rendered pointer
    emitter.instruction("ldr x2, [sp, #64]");                                   // the rendered length
    abi::emit_call_label(emitter, "__rt_concat_publish");                       // charge the scratch and preserve x1/x2
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the frame
    emitter.instruction("ret");                                                 // return the presentation string

    emitter.label("__rt_inet_ntop_v6_false");
    emitter.instruction("mov x1, #0");                                          // a null pointer signals an invalid address
    emitter.instruction("mov x2, #0");                                          // zero length for the invalid case
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the frame
    emitter.instruction("ret");                                                 // return the invalid-address result

    emitter.label("__rt_inet_ntop_false");
    emitter.instruction("mov x1, #0");                                          // a null pointer signals an invalid address
    emitter.instruction("mov x2, #0");                                          // zero length for the invalid case
    emitter.instruction("ret");                                                 // return the invalid-address result
}

/// Emits the Linux x86_64 string runtime helper for inet ntop.
fn emit_inet_ntop_linux_x86_64(emitter: &mut Emitter) {
    let inet6 = af_inet6(emitter.platform);

    emitter.blank();
    emitter.comment("--- runtime: inet_ntop ---");
    emitter.label_global("__rt_inet_ntop");

    emitter.instruction("cmp rsi, 16");                                         // a 16-byte address is IPv6
    emitter.instruction("je __rt_inet_ntop_v6_x86");                            // render it through the C formatter
    emitter.instruction("cmp rsi, 4");                                          // a 4-byte address is IPv4
    emitter.instruction("jne __rt_inet_ntop_false_x86");                        // reject any other input length
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // load octet 0
    emitter.instruction("movzx ecx, BYTE PTR [rdi + 1]");                       // load octet 1
    emitter.instruction("movzx edx, BYTE PTR [rdi + 2]");                       // load octet 2
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 3]");                       // load octet 3
    emitter.instruction("shl rax, 24");                                         // octet 0 to the high byte
    emitter.instruction("shl rcx, 16");                                         // octet 1 to the second byte
    emitter.instruction("shl rdx, 8");                                          // octet 2 to the third byte
    emitter.instruction("or rax, rcx");                                         // merge octet 1
    emitter.instruction("or rax, rdx");                                         // merge octet 2
    emitter.instruction("or rax, r8");                                          // merge octet 3
    emitter.instruction("mov rdi, rax");                                        // pass the packed address to long2ip
    emitter.instruction("jmp __rt_long2ip");                                    // tail-call long2ip to format the address

    // Frame: [rbp-64 .. rbp) the presentation buffer `inet_ntop(3)` writes into,
    // [rbp-72] rendered length, [rbp-80] reserved destination.
    emitter.label("__rt_inet_ntop_v6_x86");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 96");                                         // allocate the presentation buffer and bookkeeping
    emitter.instruction("mov rsi, rdi");                                        // the packed address is inet_ntop's second argument
    emitter.instruction(&format!("mov edi, {}", inet6));                        // AF_INET6 for this platform
    emitter.instruction("lea rdx, [rbp - 64]");                                 // the stack buffer is its destination
    emitter.instruction("mov ecx, 64");                                         // and its capacity
    emitter.instruction("xor eax, eax");                                        // no vector arguments in this call
    emitter.emit_call_c("inet_ntop");                                           // render through the target-aware socket C ABI
    emitter.instruction("test rax, rax");                                       // a null answer means the family was refused
    emitter.instruction("jz __rt_inet_ntop_v6_false_x86");                      // report it as an invalid address

    // -- measure the rendering, then reserve exactly that much --
    emitter.instruction("mov r8, rax");                                         // presentation-string cursor
    emitter.instruction("xor r9d, r9d");                                        // rendered byte count
    emitter.label("__rt_inet_ntop_v6_len_x86");
    emitter.instruction("movzx ecx, BYTE PTR [r8 + r9]");                       // read one rendered byte
    emitter.instruction("test ecx, ecx");                                       // the NUL terminates the rendering
    emitter.instruction("jz __rt_inet_ntop_v6_len_done_x86");                   // stop measuring
    emitter.instruction("add r9, 1");                                           // count it
    emitter.instruction("jmp __rt_inet_ntop_v6_len_x86");                       // keep measuring
    emitter.label("__rt_inet_ntop_v6_len_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 72], r9");                        // save the rendered length across the reservation
    emitter.instruction("mov rax, r9");                                         // ask for exactly that many bytes
    abi::emit_call_label(emitter, "__rt_concat_reserve");                       // clobbers the caller-saved registers
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save the reserved destination

    // -- copy the rendering out of the stack buffer into it --
    emitter.instruction("lea r8, [rbp - 64]");                                  // the presentation buffer is in this frame
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // reload the rendered length
    emitter.instruction("mov r10, rax");                                        // destination cursor
    emitter.label("__rt_inet_ntop_v6_copy_x86");
    emitter.instruction("test r9, r9");                                         // stop once every byte is copied
    emitter.instruction("jz __rt_inet_ntop_v6_copied_x86");                     // the copy is complete
    emitter.instruction("movzx ecx, BYTE PTR [r8]");                            // read one rendered byte
    emitter.instruction("mov BYTE PTR [r10], cl");                              // write it into the reserved destination
    emitter.instruction("add r8, 1");                                           // advance the source cursor
    emitter.instruction("add r10, 1");                                          // advance the destination cursor
    emitter.instruction("sub r9, 1");                                           // one fewer byte to copy
    emitter.instruction("jmp __rt_inet_ntop_v6_copy_x86");                      // continue copying
    emitter.label("__rt_inet_ntop_v6_copied_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // the rendered pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // the rendered length
    abi::emit_call_label(emitter, "__rt_concat_publish");                       // charge the scratch and preserve rax/rdx
    emitter.instruction("mov rsp, rbp");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the presentation string

    emitter.label("__rt_inet_ntop_v6_false_x86");
    emitter.instruction("xor eax, eax");                                        // a null pointer signals an invalid address
    emitter.instruction("xor edx, edx");                                        // zero length for the invalid case
    emitter.instruction("mov rsp, rbp");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the invalid-address result

    emitter.label("__rt_inet_ntop_false_x86");
    emitter.instruction("xor eax, eax");                                        // a null pointer signals an invalid address
    emitter.instruction("xor edx, edx");                                        // zero length for the invalid case
    emitter.instruction("ret");                                                 // return the invalid-address result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{AppleVariant, Platform, Target};

    /// Emits the helper for one target and returns its assembly.
    fn assembly_for(target: Target) -> String {
        let mut emitter = Emitter::new(target);
        emit_inet_ntop(&mut emitter);
        emitter.output()
    }

    /// The `AF_INET6` value is the target's, not the host's; see `inet_pton`'s twin for why a
    /// wrong one is invisible to a host-run test (issue #692).
    #[test]
    fn inet_ntop_selects_the_targets_own_ipv6_family() {
        for (target, expected) in [
            (Target::new(Platform::MacOS, Arch::AArch64), "mov x0, #30"),
            (
                Target::new_apple(Arch::AArch64, AppleVariant::IOS),
                "mov x0, #30",
            ),
            (
                Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
                "mov x0, #30",
            ),
            (Target::new(Platform::Linux, Arch::AArch64), "mov x0, #10"),
        ] {
            let asm = assembly_for(target);
            assert!(
                asm.contains(expected),
                "{:?} must select its own AF_INET6: {}",
                target,
                asm
            );
        }
        assert!(assembly_for(Target::new(Platform::Linux, Arch::X86_64)).contains("mov edi, 10"));
    }

    /// IPv4 keeps its own rendering: no C call, a tail jump into `__rt_long2ip`.
    ///
    /// The x86_64 side additionally has to MOVE the assembled address into the first SysV
    /// argument register. Dropping that move leaves `__rt_long2ip` reading the caller's packed
    /// string pointer as an IPv4 integer, which renders an unrelated address -- the kind of
    /// break only the other architecture's CI sees.
    #[test]
    fn inet_ntop_still_renders_ipv4_without_the_platform_formatter() {
        let arm = assembly_for(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(arm.contains("orr x0, x2, x5"));
        assert!(arm.contains("b __rt_long2ip"));
        let x86 = assembly_for(Target::new(Platform::Linux, Arch::X86_64));
        assert!(
            x86.contains("mov rdi, rax") && x86.contains("jmp __rt_long2ip"),
            "the assembled IPv4 address must reach long2ip's argument register: {}",
            x86
        );
    }

    /// The rendered text is copied into RESERVED storage, never written at the scratch cursor.
    ///
    /// See `inet_pton`'s twin: an unbounded write at `_concat_buf + _concat_off` runs past the
    /// 64 KiB buffer once an earlier result has pushed the cursor near it.
    #[test]
    fn inet_ntop_reserves_and_publishes_its_destination() {
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
