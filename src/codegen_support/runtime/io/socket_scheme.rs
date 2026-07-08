//! Purpose:
//! Emits the `__rt_addr_is_udp` runtime helper, which reports whether a
//! socket address string carries the `udp://` transport prefix.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - `__rt_stream_socket_client` / `__rt_stream_socket_server` consult it to
//!   pick `SOCK_DGRAM` instead of `SOCK_STREAM`.
//!
//! Key details:
//! - A plain `tcp://` prefix or no scheme at all yields 0 (stream socket).

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// addr_is_udp: report whether a socket address begins with `udp://`.
/// Input:  AArch64 x0 = string pointer, x1 = string length
///         x86_64  rdi = string pointer, rsi = string length
/// Output: 1 when the address uses the `udp://` transport, 0 otherwise
pub fn emit_addr_is_udp(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_addr_is_udp_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: addr_is_udp ---");
    emitter.label_global("__rt_addr_is_udp");

    emitter.instruction("cmp x1, #6");                                          // "udp://" needs at least six bytes
    emitter.instruction("b.lt __rt_addr_is_udp_no");                            // too short to carry the prefix
    emitter.instruction("ldrb w2, [x0, #0]");                                   // load byte 0
    emitter.instruction("cmp w2, #117");                                        // is it 'u'?
    emitter.instruction("b.ne __rt_addr_is_udp_no");                            // not the udp prefix
    emitter.instruction("ldrb w2, [x0, #1]");                                   // load byte 1
    emitter.instruction("cmp w2, #100");                                        // is it 'd'?
    emitter.instruction("b.ne __rt_addr_is_udp_no");                            // not the udp prefix
    emitter.instruction("ldrb w2, [x0, #2]");                                   // load byte 2
    emitter.instruction("cmp w2, #112");                                        // is it 'p'?
    emitter.instruction("b.ne __rt_addr_is_udp_no");                            // not the udp prefix
    emitter.instruction("ldrb w2, [x0, #3]");                                   // load byte 3
    emitter.instruction("cmp w2, #58");                                         // is it ':'?
    emitter.instruction("b.ne __rt_addr_is_udp_no");                            // not the udp prefix
    emitter.instruction("ldrb w2, [x0, #4]");                                   // load byte 4
    emitter.instruction("cmp w2, #47");                                         // is it '/'?
    emitter.instruction("b.ne __rt_addr_is_udp_no");                            // not the udp prefix
    emitter.instruction("ldrb w2, [x0, #5]");                                   // load byte 5
    emitter.instruction("cmp w2, #47");                                         // is it '/'?
    emitter.instruction("b.ne __rt_addr_is_udp_no");                            // not the udp prefix
    emitter.instruction("mov x0, #1");                                          // the address uses the udp transport
    emitter.instruction("ret");                                                 // return the udp result
    emitter.label("__rt_addr_is_udp_no");
    emitter.instruction("mov x0, #0");                                          // the address is not a udp transport
    emitter.instruction("ret");                                                 // return the non-udp result
}

/// Emits the Linux x86_64 stream runtime helper for addr is udp.
fn emit_addr_is_udp_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: addr_is_udp ---");
    emitter.label_global("__rt_addr_is_udp");

    emitter.instruction("cmp rsi, 6");                                          // "udp://" needs at least six bytes
    emitter.instruction("jl __rt_addr_is_udp_no");                              // too short to carry the prefix
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // load byte 0
    emitter.instruction("cmp eax, 117");                                        // is it 'u'?
    emitter.instruction("jne __rt_addr_is_udp_no");                             // not the udp prefix
    emitter.instruction("movzx eax, BYTE PTR [rdi + 1]");                       // load byte 1
    emitter.instruction("cmp eax, 100");                                        // is it 'd'?
    emitter.instruction("jne __rt_addr_is_udp_no");                             // not the udp prefix
    emitter.instruction("movzx eax, BYTE PTR [rdi + 2]");                       // load byte 2
    emitter.instruction("cmp eax, 112");                                        // is it 'p'?
    emitter.instruction("jne __rt_addr_is_udp_no");                             // not the udp prefix
    emitter.instruction("movzx eax, BYTE PTR [rdi + 3]");                       // load byte 3
    emitter.instruction("cmp eax, 58");                                         // is it ':'?
    emitter.instruction("jne __rt_addr_is_udp_no");                             // not the udp prefix
    emitter.instruction("movzx eax, BYTE PTR [rdi + 4]");                       // load byte 4
    emitter.instruction("cmp eax, 47");                                         // is it '/'?
    emitter.instruction("jne __rt_addr_is_udp_no");                             // not the udp prefix
    emitter.instruction("movzx eax, BYTE PTR [rdi + 5]");                       // load byte 5
    emitter.instruction("cmp eax, 47");                                         // is it '/'?
    emitter.instruction("jne __rt_addr_is_udp_no");                             // not the udp prefix
    emitter.instruction("mov eax, 1");                                          // the address uses the udp transport
    emitter.instruction("ret");                                                 // return the udp result
    emitter.label("__rt_addr_is_udp_no");
    emitter.instruction("xor eax, eax");                                        // the address is not a udp transport
    emitter.instruction("ret");                                                 // return the non-udp result
}
