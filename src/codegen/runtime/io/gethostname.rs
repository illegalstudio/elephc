//! Purpose:
//! Emits the `__rt_gethostname` runtime helper assembly for the gethostname
//! builtin.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - macOS reads the host name through `sysctl({CTL_KERN, KERN_HOSTNAME})`;
//!   Linux reads it from the `nodename` field of `uname`'s `struct utsname`.
//!   The result is written into the shared concat buffer.

use crate::codegen::abi::emit_symbol_address;
use crate::codegen::{emit::Emitter, platform::Arch, platform::Platform};
use crate::codegen::abi;

/// gethostname: return the system host name.
/// Input:  (none)
/// Output: x1 = string pointer (in concat_buf), x2 = length
pub fn emit_gethostname(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_gethostname_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: gethostname ---");
    emitter.label_global("__rt_gethostname");

    if matches!(emitter.platform, Platform::MacOS) {
        // macOS: sysctl({CTL_KERN, KERN_HOSTNAME}, 2, buf, &len, NULL, 0).
        // Frame: [0..8) mib (two 32-bit ints), [8..16) length, [16..24) result.
        emitter.instruction("sub sp, sp, #32");                                 // scratch for the mib, length, and result pointer
        emitter.instruction("mov w9, #1");                                      // CTL_KERN
        emitter.instruction("str w9, [sp, #0]");                                // mib[0]
        emitter.instruction("mov w9, #10");                                     // KERN_HOSTNAME
        emitter.instruction("str w9, [sp, #4]");                                // mib[1]
        emitter.instruction("mov x9, #256");                                    // available buffer space
        emitter.instruction("str x9, [sp, #8]");                                // sysctl length in/out parameter
        emit_symbol_address(emitter, "x9", "_concat_off");
        emitter.instruction("ldr x10, [x9]");                                   // current concat-buffer offset
        emit_symbol_address(emitter, "x11", "_concat_buf");
        emitter.instruction("add x12, x11, x10");                               // result write pointer
        emitter.instruction("str x12, [sp, #16]");                              // save the result pointer
        emitter.instruction("add x0, sp, #0");                                  // &mib
        emitter.instruction("mov x1, #2");                                      // mib element count
        emitter.instruction("mov x2, x12");                                     // oldp = result pointer
        emitter.instruction("add x3, sp, #8");                                  // &length
        emitter.instruction("mov x4, #0");                                      // newp = NULL
        emitter.instruction("mov x5, #0");                                      // newlen = 0
        emitter.syscall(202);
        emitter.instruction("cmn x0, #1");                                      // did sysctl fail (-1)?
        emitter.instruction("b.eq __rt_gethostname_fail");                      // report an empty name on failure
        emitter.instruction("ldr x2, [sp, #8]");                                // returned length, including the NUL
        emitter.instruction("sub x2, x2, #1");                                  // drop the trailing NUL
        emit_symbol_address(emitter, "x9", "_concat_off");
        emitter.instruction("ldr x10, [x9]");                                   // concat-buffer offset
        emitter.instruction("add x10, x10, x2");                                // reserve the host-name bytes
        emitter.instruction("str x10, [x9]");                                   // publish the updated offset
        emitter.instruction("ldr x1, [sp, #16]");                               // result pointer
        emitter.instruction("add sp, sp, #32");                                 // release the scratch
        emitter.instruction("ret");                                             // return the host name

        emitter.label("__rt_gethostname_fail");
        emitter.instruction("ldr x1, [sp, #16]");                               // a valid (empty) concat-buffer pointer
        emitter.instruction("mov x2, #0");                                      // zero-length name on failure
        emitter.instruction("add sp, sp, #32");                                 // release the scratch
        emitter.instruction("ret");                                             // return the empty name
    } else {
        // Linux: uname(&utsname); the host name is nodename at offset 65.
        emitter.instruction("sub sp, sp, #416");                                // scratch for the 390-byte struct utsname
        emitter.instruction("add x0, sp, #16");                                 // &utsname
        emitter.syscall(160);
        emit_symbol_address(emitter, "x9", "_concat_off");
        emitter.instruction("ldr x10, [x9]");                                   // current concat-buffer offset
        emit_symbol_address(emitter, "x11", "_concat_buf");
        emitter.instruction("add x12, x11, x10");                               // result write pointer
        emitter.instruction("add x13, sp, #16");                                // utsname base
        emitter.instruction("add x13, x13, #65");                               // nodename field offset
        emitter.instruction("mov x2, #0");                                      // copied-byte count
        emitter.label("__rt_gethostname_copy");
        emitter.instruction("ldrb w14, [x13, x2]");                             // a nodename byte
        emitter.instruction("cbz w14, __rt_gethostname_copy_done");             // stop at the terminating NUL
        emitter.instruction("strb w14, [x12, x2]");                             // append the byte to the result
        emitter.instruction("add x2, x2, #1");                                  // advance the copy count
        emitter.instruction("b __rt_gethostname_copy");                         // continue copying
        emitter.label("__rt_gethostname_copy_done");
        emitter.instruction("ldr x10, [x9]");                                   // concat-buffer offset (x9 still holds _concat_off)
        emitter.instruction("add x10, x10, x2");                                // reserve the host-name bytes
        emitter.instruction("str x10, [x9]");                                   // publish the updated offset
        emitter.instruction("mov x1, x12");                                     // result pointer
        emitter.instruction("add sp, sp, #416");                                // release the scratch
        emitter.instruction("ret");                                             // return the host name
    }
}

/// Emits the Linux x86_64 stream runtime helper for gethostname.
fn emit_gethostname_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: gethostname ---");
    emitter.label_global("__rt_gethostname");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 416");                                        // scratch for the 390-byte struct utsname
    emitter.instruction("lea rdi, [rbp - 400]");                                // &utsname
    emitter.instruction("mov eax, 63");                                         // Linux x86_64 syscall 63 = uname
    emitter.instruction("syscall");                                             // read the system information
    abi::emit_load_symbol_to_reg(emitter, "r9", "_concat_off", 0);              // current concat-buffer offset
    abi::emit_symbol_address(emitter, "r10", "_concat_buf");                    // concat-buffer base address
    emitter.instruction("lea r11, [r10 + r9]");                                 // result write pointer
    emitter.instruction("lea r8, [rbp - 400]");                                 // utsname base
    emitter.instruction("add r8, 65");                                          // nodename field offset
    emitter.instruction("xor edx, edx");                                        // copied-byte count
    emitter.label("__rt_gethostname_copy_x86");
    emitter.instruction("movzx ecx, BYTE PTR [r8 + rdx]");                      // a nodename byte
    emitter.instruction("test cl, cl");                                         // is it the terminating NUL?
    emitter.instruction("jz __rt_gethostname_copy_done_x86");                   // stop at the NUL
    emitter.instruction("mov BYTE PTR [r11 + rdx], cl");                        // append the byte to the result
    emitter.instruction("inc rdx");                                             // advance the copy count
    emitter.instruction("jmp __rt_gethostname_copy_x86");                       // continue copying
    emitter.label("__rt_gethostname_copy_done_x86");
    emitter.instruction("add r9, rdx");                                         // reserve the host-name bytes
    abi::emit_store_reg_to_symbol(emitter, "r9", "_concat_off", 0);             // publish the updated offset
    emitter.instruction("mov rax, r11");                                        // result pointer
    emitter.instruction("add rsp, 416");                                        // release the scratch
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the host name
}
