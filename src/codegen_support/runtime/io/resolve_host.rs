//! Purpose:
//! Emits the `__rt_resolve_host` runtime helper, which resolves a host name to
//! a packed IPv4 address for the socket address parser.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - `__rt_inet_addr_parse` when the address slice is not a numeric dotted quad.
//!
//! Key details:
//! - Resolution goes through libc `gethostbyname`; the resulting `in_addr` is
//!   in network byte order and is byte-swapped to match `__rt_ip2long`'s
//!   host-readable packed integer so both feed the socket builders identically.
//! - The `hostent` `h_addr_list` field sits at offset 24 on every supported
//!   LP64 target (macOS and Linux, AArch64 and x86_64).

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// resolve_host: resolve a host-name slice to a packed IPv4 integer.
/// Input:  AArch64 x0 = host pointer, x1 = host length
///         x86_64  rdi = host pointer, rsi = host length
/// Output: packed IPv4 integer in `__rt_ip2long` form, or -1 when unresolved.
pub fn emit_resolve_host(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_resolve_host_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: resolve_host ---");
    emitter.label_global("__rt_resolve_host");

    emitter.instruction("sub sp, sp, #16");                                     // frame for the saved registers
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // -- null-terminate the host slice for the libc resolver --
    emitter.instruction("mov x2, x1");                                          // host length into the __rt_cstr length register
    emitter.instruction("mov x1, x0");                                          // host pointer into the __rt_cstr pointer register
    emitter.instruction("bl __rt_cstr");                                        // x0 = null-terminated host name

    // -- resolve the name through libc gethostbyname --
    emitter.bl_c("gethostbyname");                                              // x0 = struct hostent* (null when unresolved)
    emitter.instruction("cbz x0, __rt_resolve_host_fail");                      // a null hostent means resolution failed
    emitter.instruction("ldr x0, [x0, #24]");                                   // hostent.h_addr_list
    emitter.instruction("cbz x0, __rt_resolve_host_fail");                      // guard a missing address list
    emitter.instruction("ldr x0, [x0]");                                        // h_addr_list[0]: pointer to the in_addr
    emitter.instruction("cbz x0, __rt_resolve_host_fail");                      // guard an empty address list
    emitter.instruction("ldr w0, [x0]");                                        // load the 4-byte network-order IPv4
    emitter.instruction("rev w0, w0");                                          // byte-swap into __rt_ip2long packed form

    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the frame
    emitter.instruction("ret");                                                 // return the packed IPv4 integer

    emitter.label("__rt_resolve_host_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 signals an unresolvable host name
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the frame
    emitter.instruction("ret");                                                 // return the failure result
}

/// Emits the Linux x86_64 stream runtime helper for resolve host.
fn emit_resolve_host_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resolve_host ---");
    emitter.label_global("__rt_resolve_host");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer

    // -- null-terminate the host slice for the libc resolver --
    emitter.instruction("mov rax, rdi");                                        // host pointer into the __rt_cstr pointer register
    emitter.instruction("mov rdx, rsi");                                        // host length into the __rt_cstr length register
    emitter.instruction("call __rt_cstr");                                      // rax = null-terminated host name

    // -- resolve the name through libc gethostbyname --
    emitter.instruction("mov rdi, rax");                                        // host name into the gethostbyname argument register
    emitter.emit_call_c("gethostbyname");                                       // rax = struct hostent* (null when unresolved)
    emitter.instruction("test rax, rax");                                       // did resolution fail?
    emitter.instruction("jz __rt_resolve_host_fail_x86");                       // a null hostent means resolution failed
    emitter.instruction("mov rax, QWORD PTR [rax + 24]");                       // hostent.h_addr_list
    emitter.instruction("test rax, rax");                                       // guard a missing address list
    emitter.instruction("jz __rt_resolve_host_fail_x86");                       // bail when there is no address list
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // h_addr_list[0]: pointer to the in_addr
    emitter.instruction("test rax, rax");                                       // guard an empty address list
    emitter.instruction("jz __rt_resolve_host_fail_x86");                       // bail when the address list is empty
    emitter.instruction("mov eax, DWORD PTR [rax]");                            // load the 4-byte network-order IPv4
    emitter.instruction("bswap eax");                                           // byte-swap into __rt_ip2long packed form

    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the packed IPv4 integer

    emitter.label("__rt_resolve_host_fail_x86");
    emitter.instruction("mov rax, -1");                                         // -1 signals an unresolvable host name
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}
