//! Purpose:
//! Emits the native runtime helper for gethostname.
//!
//! Called from:
//! - Direct lowering and the eval runtime dispatcher.
//!
//! Key details:
//! - Darwin uses sysctl, Linux selects nodename from uname.
//! - The bounded native result is staged before Memoir copies it.

use crate::codegen_support::{emit::Emitter, platform::{Arch, Platform}};
use elephc_memoir::strings::staged_concat::emit_copy_staged_result;

/// Dispatches the native hostname query to the target implementation.
pub fn emit_gethostname(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: gethostname ---");
    emitter.label_global("__rt_gethostname");
    match (emitter.target.arch, emitter.platform) {
        (Arch::X86_64, _) => emit_linux_x86_64(emitter),
        (Arch::AArch64, Platform::MacOS) => emit_darwin(emitter),
        (Arch::AArch64, _) => emit_linux_aarch64(emitter),
    }
}

/// Stages the full sysctl buffer, then copies the returned bytes without the NUL.
fn emit_darwin(emitter: &mut Emitter) {
    emitter.instruction("sub sp, sp, #288");                                    // reserve the selectors, 256-byte output and frame footer
    emitter.instruction("stp x29, x30, [sp, #272]");                            // preserve the caller frame and return link
    emitter.instruction("add x29, sp, #272");                                   // anchor the frame above the host buffer
    emitter.instruction("mov w9, #1");                                          // CTL_KERN
    emitter.instruction("str w9, [sp]");                                        // mib first selector
    emitter.instruction("mov w9, #10");                                         // KERN_HOSTNAME
    emitter.instruction("str w9, [sp, #4]");                                    // mib second selector
    emitter.instruction("mov x9, #256");                                        // capacity including the terminator
    emitter.instruction("str x9, [sp, #8]");                                    // sysctl length in/out parameter
    emitter.instruction("mov x0, sp");                                          // pass the mib address
    emitter.instruction("mov x1, #2");                                          // two integer selectors
    emitter.instruction("add x2, sp, #16");                                     // sysctl's private output buffer
    emitter.instruction("add x3, sp, #8");                                      // pass the length address
    emitter.instruction("mov x4, xzr");                                         // no input value
    emitter.instruction("mov x5, xzr");                                         // no input bytes
    emitter.syscall(202);
    emitter.instruction("b.cs __rt_gethostname_empty");                         // a failed query produces an empty string
    emitter.instruction("ldr x2, [sp, #8]");                                    // read the actual copied size
    emitter.instruction("cbz x2, __rt_gethostname_ready");                      // keep an empty result at zero length
    emitter.instruction("cmp x2, #256");                                        // the returned count must fit the supplied buffer
    emitter.instruction("b.hi __rt_gethostname_empty");                         // reject an invalid byte count
    emitter.instruction("sub x2, x2, #1");                                      // exclude the terminating NUL
    emitter.instruction("b __rt_gethostname_ready");                            // make the measured bytes available for copying
    emitter.label("__rt_gethostname_empty");
    emitter.instruction("mov x2, xzr");                                         // return zero bytes
    emitter.label("__rt_gethostname_ready");
    emitter.instruction("add x1, sp, #16");                                     // expose the staged slice to Memoir
    emit_copy_staged_result(emitter, "__rt_gethostname_copied");
    emitter.instruction("ldp x29, x30, [sp, #272]");                            // restore the enclosing frame and return link
    emitter.instruction("add sp, sp, #288");                                    // release staged native storage
    emitter.instruction("ret");                                                 // return the hostname slice
}

/// Measures Linux's 65-byte nodename field on AArch64.
fn emit_linux_aarch64(emitter: &mut Emitter) {
    emitter.instruction("sub sp, sp, #416");                                    // reserve the utsname structure and saved frame
    emitter.instruction("stp x29, x30, [sp]");                                  // preserve the enclosing frame and return link
    emitter.instruction("mov x29, sp");                                         // anchor the current frame
    emitter.instruction("add x0, sp, #16");                                     // pass the utsname destination
    emitter.syscall(160);
    emitter.instruction("add x1, sp, #81");                                     // select nodename at utsname offset 65
    emitter.instruction("mov x2, xzr");                                         // count bytes from the beginning of the field
    emitter.instruction("cmp x0, #0");                                          // did uname fill the structure?
    emitter.instruction("b.lt __rt_gethostname_ready");                         // no output bytes on failure
    emitter.label("__rt_gethostname_measure");
    emitter.instruction("cmp x2, #65");                                         // stop at the end of nodename even without a terminator
    emitter.instruction("b.hs __rt_gethostname_ready");                         // avoid reading past the field
    emitter.instruction("ldrb w9, [x1, x2]");                                   // inspect the next hostname byte
    emitter.instruction("cbz w9, __rt_gethostname_ready");                      // do not include the terminator
    emitter.instruction("add x2, x2, #1");                                      // count the accepted byte
    emitter.instruction("b __rt_gethostname_measure");                          // keep measuring the field
    emitter.label("__rt_gethostname_ready");
    emit_copy_staged_result(emitter, "__rt_gethostname_copied");
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore the enclosing frame and return link
    emitter.instruction("add sp, sp, #416");                                    // release the utsname storage
    emitter.instruction("ret");                                                 // return the hostname slice
}

/// Measures Linux's 65-byte nodename field on x86-64.
fn emit_linux_x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // anchor the staged native structure
    emitter.instruction("sub rsp, 416");                                        // reserve aligned utsname storage
    emitter.instruction("lea rdi, [rbp - 400]");                                // pass the utsname destination
    emitter.instruction("mov eax, 63");                                         // select Linux uname
    emitter.instruction("syscall");                                             // fill the utsname structure
    emitter.instruction("xor edx, edx");                                        // start at zero bytes
    emitter.instruction("test rax, rax");                                       // did uname fill the structure?
    emitter.instruction("lea rax, [rbp - 335]");                                // select nodename without altering flags
    emitter.instruction("js __rt_gethostname_ready");                           // no output bytes on failure
    emitter.label("__rt_gethostname_measure");
    emitter.instruction("cmp rdx, 65");                                         // stop at the field boundary
    emitter.instruction("jae __rt_gethostname_ready");                          // avoid reading past the field
    emitter.instruction("cmp BYTE PTR [rax + rdx], 0");                         // inspect the next hostname byte
    emitter.instruction("je __rt_gethostname_ready");                           // do not include the terminator
    emitter.instruction("inc rdx");                                             // count the accepted byte
    emitter.instruction("jmp __rt_gethostname_measure");                        // keep measuring the field
    emitter.label("__rt_gethostname_ready");
    emit_copy_staged_result(emitter, "__rt_gethostname_copied");
    emitter.instruction("leave");                                               // release the structure and restore the caller frame
    emitter.instruction("ret");                                                 // return the hostname slice
}
