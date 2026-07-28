//! Purpose:
//! Emits the `__rt_gethostbyaddr` runtime helper for the PHP `gethostbyaddr`
//! builtin (reverse DNS).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - The dotted-quad argument is parsed with `__rt_ip2long`; a malformed
//!   address returns a null pointer so the builtin can box PHP `false`.
//! - The packed integer is byte-swapped into a network-order `in_addr` and
//!   passed to libc `gethostbyaddr`. PHP returns the address unchanged when no
//!   reverse record exists, so the no-record path returns the input string.
//! - `hostent.h_name` is a NUL-terminated C string; its length is measured
//!   inline and the name is persisted with `__rt_str_persist`.

use crate::codegen_support::{emit::Emitter, platform::Arch};

const AF_INET: i64 = 2;

/// gethostbyaddr: resolve an IPv4 dotted-quad string to a host name.
/// Input:  AArch64 x1/x2 = address string / x86_64 rax/rdx = address string
/// Output: the host name string, the address unchanged when no record exists,
///         or a null pointer when the address is malformed.
pub fn emit_gethostbyaddr(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_gethostbyaddr_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: gethostbyaddr ---");
    emitter.label_global("__rt_gethostbyaddr");

    // Frame (48 bytes): [0]=addr ptr [8]=addr len [16]=in_addr [24]=x29 [32]=x30.
    emitter.instruction("sub sp, sp, #48");                                     // frame for the saved address string
    emitter.instruction("stp x29, x30, [sp, #24]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #24");                                    // establish the helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the address string pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save the address string length

    // -- parse the dotted quad into a packed integer --
    emitter.instruction("mov x0, x1");                                          // address pointer into the ip2long pointer register
    emitter.instruction("mov x1, x2");                                          // address length into the ip2long length register
    emitter.instruction("bl __rt_ip2long");                                     // x0 = packed IPv4 integer or -1
    emitter.instruction("cmn x0, #1");                                          // is the address malformed?
    emitter.instruction("b.eq __rt_gethostbyaddr_false");                       // a malformed address boxes PHP false

    // -- reverse-resolve through libc gethostbyaddr --
    emitter.instruction("rev w0, w0");                                          // packed integer into network byte order
    emitter.instruction("str w0, [sp, #16]");                                   // materialize the in_addr on the stack
    emitter.instruction("add x0, sp, #16");                                     // pointer to the in_addr
    emitter.instruction("mov x1, #4");                                          // in_addr length in bytes
    emitter.instruction(&format!("mov x2, #{}", AF_INET));                      // address family AF_INET
    emitter.bl_c("gethostbyaddr");                                              // x0 = struct hostent* (null when no record)
    emitter.instruction("cbz x0, __rt_gethostbyaddr_input");                    // no record: return the address unchanged
    emitter.instruction("ldr x1, [x0]");                                        // hostent.h_name: NUL-terminated C string
    emitter.instruction("cbz x1, __rt_gethostbyaddr_input");                    // guard a missing host name

    // -- measure the host name and persist it --
    emitter.instruction("mov x2, #0");                                          // host-name length accumulator
    emitter.label("__rt_gethostbyaddr_strlen");
    emitter.instruction("ldrb w3, [x1, x2]");                                   // load the next host-name byte
    emitter.instruction("cbz w3, __rt_gethostbyaddr_persist");                  // stop at the NUL terminator
    emitter.instruction("add x2, x2, #1");                                      // count this byte
    emitter.instruction("b __rt_gethostbyaddr_strlen");                         // continue measuring
    emitter.label("__rt_gethostbyaddr_persist");
    emitter.instruction("bl __rt_str_persist");                                 // x1/x2 = owned heap copy of the host name
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the frame
    emitter.instruction("ret");                                                 // return the resolved host name

    emitter.label("__rt_gethostbyaddr_input");
    emitter.instruction("ldr x1, [sp, #0]");                                    // return the address string unchanged
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the saved address length
    emitter.instruction("bl __rt_str_persist");                                 // x1/x2 = owned heap copy of the address
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the frame
    emitter.instruction("ret");                                                 // return the unchanged address

    emitter.label("__rt_gethostbyaddr_false");
    emitter.instruction("mov x1, #0");                                          // null pointer signals PHP false
    emitter.instruction("mov x2, #0");                                          // no length for the false case
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the frame
    emitter.instruction("ret");                                                 // return the malformed-address result
}

/// Emits the Linux x86_64 stream runtime helper for gethostbyaddr.
fn emit_gethostbyaddr_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: gethostbyaddr ---");
    emitter.label_global("__rt_gethostbyaddr");

    // Frame (rbp-relative): [-8]=addr ptr [-16]=addr len [-24]=in_addr.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // frame for the saved address string
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the address string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the address string length

    // -- parse the dotted quad into a packed integer --
    emitter.instruction("mov rdi, rax");                                        // address pointer into the ip2long argument
    emitter.instruction("mov rsi, rdx");                                        // address length into the ip2long argument
    emitter.instruction("call __rt_ip2long");                                   // rax = packed IPv4 integer or -1
    emitter.instruction("cmp rax, -1");                                         // is the address malformed?
    emitter.instruction("je __rt_gethostbyaddr_false_x86");                     // a malformed address boxes PHP false

    // -- reverse-resolve through libc gethostbyaddr --
    emitter.instruction("bswap eax");                                           // packed integer into network byte order
    emitter.instruction("mov DWORD PTR [rbp - 24], eax");                       // materialize the in_addr on the stack
    emitter.instruction("lea rdi, [rbp - 24]");                                 // pointer to the in_addr
    emitter.instruction("mov rsi, 4");                                          // in_addr length in bytes
    emitter.instruction(&format!("mov rdx, {}", AF_INET));                      // address family AF_INET
    emitter.emit_call_c("gethostbyaddr");                                       // rax = struct hostent* (null when no record)
    emitter.instruction("test rax, rax");                                       // did the reverse lookup find a record?
    emitter.instruction("jz __rt_gethostbyaddr_input_x86");                     // no record: return the address unchanged
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // hostent.h_name: NUL-terminated C string
    emitter.instruction("test rax, rax");                                       // guard a missing host name
    emitter.instruction("jz __rt_gethostbyaddr_input_x86");                     // bail when there is no host name

    // -- measure the host name and persist it --
    emitter.instruction("xor edx, edx");                                        // host-name length accumulator
    emitter.label("__rt_gethostbyaddr_strlen_x86");
    emitter.instruction("movzx ecx, BYTE PTR [rax + rdx]");                     // load the next host-name byte
    emitter.instruction("test cl, cl");                                         // is it the NUL terminator?
    emitter.instruction("jz __rt_gethostbyaddr_persist_x86");                   // stop at the NUL terminator
    emitter.instruction("inc rdx");                                             // count this byte
    emitter.instruction("jmp __rt_gethostbyaddr_strlen_x86");                   // continue measuring
    emitter.label("__rt_gethostbyaddr_persist_x86");
    emitter.instruction("call __rt_str_persist");                               // rax/rdx = owned heap copy of the host name
    emitter.instruction("add rsp, 32");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the resolved host name

    emitter.label("__rt_gethostbyaddr_input_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the address string unchanged
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the saved address length
    emitter.instruction("call __rt_str_persist");                               // rax/rdx = owned heap copy of the address
    emitter.instruction("add rsp, 32");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the unchanged address

    emitter.label("__rt_gethostbyaddr_false_x86");
    emitter.instruction("xor eax, eax");                                        // null pointer signals PHP false
    emitter.instruction("xor edx, edx");                                        // no length for the false case
    emitter.instruction("add rsp, 32");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the malformed-address result
}
