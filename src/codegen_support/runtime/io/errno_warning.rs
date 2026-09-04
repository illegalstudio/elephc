//! Purpose:
//! Emits `__rt_errno_warning`, the diagnostic shape PHP uses when a builtin names itself and the
//! reason a syscall gave, and nothing else.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The `disk_free_space()` and `disk_total_space()` lowerings, on the failing branch.
//!
//! Key details:
//! - `disk_free_space(): No such file or directory` carries neither the path nor a fixed middle,
//!   so `__rt_open_failed_warning` cannot serve it: that composer is built around a path.
//! - The line goes out in fragments through `__rt_diag_warning`, which is what honours `@`, since
//!   only the reason is known at run time.
//! - The reason comes from libc `strerror` directly rather than from `__rt_socket_strerror`. That
//!   one answers a pending resolver message ahead of any `errno`, so a failed DNS lookup earlier
//!   in the program would supply the reason for a later filesystem error.

use crate::codegen_support::runtime::data::DIAG_NEWLINE;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_errno_warning(prefix_ptr, prefix_len, errno)`.
///
/// AArch64 takes `x0`/`x1`/`x2`; x86_64 takes `rdi`/`rsi`/`rdx`. A zero `errno`, or a code
/// `strerror` does not recognise, prints the prefix and the newline with nothing between them.
pub fn emit_errno_warning(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_errno_warning_aarch64(emitter),
        Arch::X86_64 => emit_errno_warning_x86_64(emitter),
    }
}

/// Emits the AArch64 form.
fn emit_errno_warning_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: errno warning ---");
    emitter.label_global("__rt_errno_warning");
    emitter.instruction("sub sp, sp, #32");                                     // frame for the errno and the saved linkage
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame pointer
    emitter.instruction("str x2, [sp, #0]");                                    // hold the error number across the calls

    emitter.instruction("mov x2, x1");                                          // the diagnostic helper takes the length in x2
    emitter.instruction("mov x1, x0");                                          // and the pointer in x1
    emitter.instruction("bl __rt_diag_warning");                                // warnings honour the @ suppression depth

    emitter.instruction("ldr x0, [sp, #0]");                                    // the error number the failing syscall gave
    emitter.instruction("cbz x0, __rt_ew_newline");                             // nothing to describe
    emitter.bl_c("strerror");                                                   // x0 = static NUL-terminated reason
    emitter.instruction("cbz x0, __rt_ew_newline");                             // strerror answers NULL for an unknown code
    emitter.instruction("mov x1, x0");                                          // the reason text
    emitter.instruction("mov x9, #0");                                          // measured length
    emitter.label("__rt_ew_scan");
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load the next reason byte
    emitter.instruction("cbz w10, __rt_ew_scanned");                            // reached the terminator
    emitter.instruction("add x9, x9, #1");                                      // keep measuring
    emitter.instruction("b __rt_ew_scan");                                      // continue
    emitter.label("__rt_ew_scanned");
    emitter.instruction("mov x2, x9");                                          // the measured byte length
    emitter.instruction("bl __rt_diag_warning");

    emitter.label("__rt_ew_newline");
    abi::emit_symbol_address(emitter, "x1", "_diag_newline");
    emitter.instruction(&format!("mov x2, #{}", DIAG_NEWLINE.len()));
    emitter.instruction("bl __rt_diag_warning");                                // close the line

    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");
}

/// Emits the x86_64 form.
fn emit_errno_warning_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: errno warning ---");
    emitter.label_global("__rt_errno_warning");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 16");                                         // reserve the errno slot
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                        // hold the error number across the calls

    emitter.instruction("call __rt_diag_warning");                              // prefix already sits in rdi/rsi

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the error number the failing syscall gave
    emitter.instruction("test rdi, rdi");
    emitter.instruction("jz __rt_ew_newline_x86");                              // nothing to describe
    emitter.bl_c("strerror");                                                   // rax = static NUL-terminated reason
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_ew_newline_x86");                              // strerror answers NULL for an unknown code
    emitter.instruction("mov rdi, rax");                                        // the reason text
    emitter.instruction("xor r9, r9");                                          // measured length
    emitter.label("__rt_ew_scan_x86");
    emitter.instruction("movzx r10d, BYTE PTR [rdi + r9]");                     // load the next reason byte
    emitter.instruction("test r10b, r10b");                                     // reached the terminator?
    emitter.instruction("jz __rt_ew_scanned_x86");
    emitter.instruction("add r9, 1");                                           // keep measuring
    emitter.instruction("jmp __rt_ew_scan_x86");                                // continue
    emitter.label("__rt_ew_scanned_x86");
    emitter.instruction("mov rsi, r9");                                         // the measured byte length
    emitter.instruction("call __rt_diag_warning");

    emitter.label("__rt_ew_newline_x86");
    abi::emit_symbol_address(emitter, "rdi", "_diag_newline");
    emitter.instruction(&format!("mov rsi, {}", DIAG_NEWLINE.len()));
    emitter.instruction("call __rt_diag_warning");                              // close the line

    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}
