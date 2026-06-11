//! Purpose:
//! Emits the `__rt_inet_pton` runtime helper assembly for the inet_pton builtin.
//! Parses a dotted-quad IPv4 string into a 4-byte network-order binary string.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - Reuses `__rt_ip2long` for parsing/validation, then writes the four bytes
//!   into the concat buffer in network byte order.

use crate::codegen::abi::emit_symbol_address;
use crate::codegen::{emit::Emitter, platform::Arch};
use crate::codegen::abi;

/// inet_pton: parse a dotted-quad IPv4 string into a 4-byte binary string.
/// Input:  x0 = string pointer, x1 = string length
/// Output: x1 = binary pointer (0 when invalid), x2 = length (4)
pub fn emit_inet_pton(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_inet_pton_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: inet_pton ---");
    emitter.label_global("__rt_inet_pton");

    emitter.instruction("sub sp, sp, #16");                                     // frame for the ip2long call
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("bl __rt_ip2long");                                     // parse the address; x0 = integer or -1
    emitter.instruction("cmp x0, #0");                                          // did parsing report an invalid address?
    emitter.instruction("b.lt __rt_inet_pton_false");                           // a -1 sentinel means invalid

    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load the current concat-buffer offset
    emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x12, x11, x10");                                   // compute the binary write pointer
    emitter.instruction("lsr x13, x0, #24");                                    // extract octet 0
    emitter.instruction("strb w13, [x12]");                                     // write octet 0
    emitter.instruction("lsr x13, x0, #16");                                    // extract octet 1
    emitter.instruction("strb w13, [x12, #1]");                                 // write octet 1
    emitter.instruction("lsr x13, x0, #8");                                     // extract octet 2
    emitter.instruction("strb w13, [x12, #2]");                                 // write octet 2
    emitter.instruction("strb w0, [x12, #3]");                                  // write octet 3 (the low byte)
    emitter.instruction("add x10, x10, #4");                                    // the binary string is four bytes
    emitter.instruction("str x10, [x9]");                                       // publish the updated concat-buffer offset
    emitter.instruction("mov x1, x12");                                         // return the binary pointer
    emitter.instruction("mov x2, #4");                                          // return the four-byte length
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the frame
    emitter.instruction("ret");                                                 // return the packed address

    emitter.label("__rt_inet_pton_false");
    emitter.instruction("mov x1, #0");                                          // a null pointer signals an invalid address
    emitter.instruction("mov x2, #0");                                          // zero length for the invalid case
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the frame
    emitter.instruction("ret");                                                 // return the invalid-address result
}

/// Emits the Linux x86_64 string runtime helper for inet pton.
fn emit_inet_pton_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: inet_pton ---");
    emitter.label_global("__rt_inet_pton");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("call __rt_ip2long");                                   // parse the address; rax = integer or -1
    emitter.instruction("test rax, rax");                                       // did parsing report an invalid address?
    emitter.instruction("js __rt_inet_pton_false_x86");                         // a -1 sentinel means invalid

    abi::emit_load_symbol_to_reg(emitter, "r9", "_concat_off", 0);              // current concat-buffer offset
    abi::emit_symbol_address(emitter, "r10", "_concat_buf");                    // concat-buffer base address
    emitter.instruction("lea r11, [r10 + r9]");                                 // compute the binary write pointer
    emitter.instruction("mov rcx, rax");                                        // keep the packed address for shifting
    emitter.instruction("shr rcx, 24");                                         // extract octet 0
    emitter.instruction("mov BYTE PTR [r11], cl");                              // write octet 0
    emitter.instruction("mov rcx, rax");                                        // reload the packed address
    emitter.instruction("shr rcx, 16");                                         // extract octet 1
    emitter.instruction("mov BYTE PTR [r11 + 1], cl");                          // write octet 1
    emitter.instruction("mov rcx, rax");                                        // reload the packed address
    emitter.instruction("shr rcx, 8");                                          // extract octet 2
    emitter.instruction("mov BYTE PTR [r11 + 2], cl");                          // write octet 2
    emitter.instruction("mov BYTE PTR [r11 + 3], al");                          // write octet 3 (the low byte)
    emitter.instruction("add r9, 4");                                           // the binary string is four bytes
    abi::emit_store_reg_to_symbol(emitter, "r9", "_concat_off", 0);             // publish the updated offset
    emitter.instruction("mov rax, r11");                                        // return the binary pointer
    emitter.instruction("mov rdx, 4");                                          // return the four-byte length
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the packed address

    emitter.label("__rt_inet_pton_false_x86");
    emitter.instruction("xor eax, eax");                                        // a null pointer signals an invalid address
    emitter.instruction("xor edx, edx");                                        // zero length for the invalid case
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the invalid-address result
}
