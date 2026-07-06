//! Purpose:
//! Emits the `__rt_inet_ntop` runtime helper assembly for the inet_ntop builtin.
//! Formats a 4-byte IPv4 binary string as a dotted-quad presentation string.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Only IPv4 (4-byte) input is supported; the four octets are packed into an
//!   integer and `__rt_long2ip` is tail-called to render `A.B.C.D`.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// inet_ntop: render a 4-byte IPv4 binary string as `A.B.C.D`.
/// Input:  x0 = binary pointer, x1 = binary length
/// Output: x1 = string pointer (0 when the length is not 4), x2 = length
pub fn emit_inet_ntop(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_inet_ntop_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: inet_ntop ---");
    emitter.label_global("__rt_inet_ntop");

    emitter.instruction("cmp x1, #4");                                          // only IPv4 (4-byte) addresses are supported
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

    emitter.label("__rt_inet_ntop_false");
    emitter.instruction("mov x1, #0");                                          // a null pointer signals an invalid address
    emitter.instruction("mov x2, #0");                                          // zero length for the invalid case
    emitter.instruction("ret");                                                 // return the invalid-address result
}

/// Emits the Linux x86_64 string runtime helper for inet ntop.
fn emit_inet_ntop_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: inet_ntop ---");
    emitter.label_global("__rt_inet_ntop");

    emitter.instruction("cmp rsi, 4");                                          // only IPv4 (4-byte) addresses are supported
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

    emitter.label("__rt_inet_ntop_false_x86");
    emitter.instruction("xor eax, eax");                                        // a null pointer signals an invalid address
    emitter.instruction("xor edx, edx");                                        // zero length for the invalid case
    emitter.instruction("ret");                                                 // return the invalid-address result
}
