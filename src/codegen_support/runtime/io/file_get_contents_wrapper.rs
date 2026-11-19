//! Purpose:
//! Emits `__rt_file_get_contents_wrapper`, the `file_get_contents()` read path for a
//! `scheme://` URL backed by a registered userspace stream wrapper.
//!
//! Called from:
//! - `crate::codegen_support::runtime::io::file_get_contents_url` before its filesystem
//!   fallback.
//!
//! Key details:
//! - Opens through `__rt_fopen` so the wrapper's `stream_open` runs with the active stream
//!   context, drains through `__rt_stream_get_contents`, then closes the synthetic fd with
//!   `__rt_user_wrapper_fclose`.
//! - Returns the same owned string pointer/length pair as `__rt_file_get_contents`; a null
//!   pointer means PHP `false` and emits the standard failure warning.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Prefix of PHP's path-carrying wrapper failure warning.
const WRAPPER_FAILED_PREFIX: &str = "Warning: file_get_contents(";
/// Suffix of PHP's path-carrying wrapper failure warning.
const WRAPPER_FAILED_SUFFIX: &str = "): Failed to open stream: operation failed\n";

/// Emits the wrapper-backed `file_get_contents()` helper for the active target.
pub fn emit_file_get_contents_wrapper(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }
    emit_aarch64(emitter);
}

/// AArch64 helper: x1/x2 = path, x1/x2 = owned string on return.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: file_get_contents_wrapper ---");
    emitter.label_global("__rt_file_get_contents_wrapper");

    // Frame: [0]=path ptr [8]=path len [16]=fd [24]=result ptr [32]=result len
    //        [48]=x29 [56]=x30
    emitter.instruction("sub sp, sp, #64");                                     // reserve path, descriptor, and result spill slots
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the wrapper path pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save the wrapper path length

    abi::emit_symbol_address(emitter, "x3", "_meta_mode_rb");                   // pass the shared "rb" read mode
    emitter.instruction("mov x4, #2");                                          // strlen("rb")
    emitter.instruction("bl __rt_fopen");                                       // x0 = synthetic wrapper fd, or -1 on failure
    emitter.instruction("cmp x0, #0");                                          // did the wrapper refuse the URL?
    emitter.instruction("b.lt __rt_fgcw_fail");                                 // return PHP false and warn
    emitter.instruction("str x0, [sp, #16]");                                   // save the synthetic fd across drain/close

    emitter.instruction("bl __rt_stream_get_contents");                         // x1/x2 = accumulated wrapper payload
    emitter.instruction("stp x1, x2, [sp, #24]");                               // preserve the result across close
    emitter.instruction("bl __rt_str_persist");                                 // adopt scratch bytes into an owned result
    emitter.instruction("stp x1, x2, [sp, #24]");                               // persist any moved pointer/length
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the synthetic fd
    emitter.instruction("bl __rt_user_wrapper_fclose");                         // run stream_close and release the handle slot
    emitter.instruction("ldp x1, x2, [sp, #24]");                               // return the owned wrapper payload
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the owned payload

    emitter.label("__rt_fgcw_fail");
    abi::emit_symbol_address(emitter, "x1", "_diag_fgc_wrapper_prefix");        // warning prefix "Warning: file_get_contents("
    emitter.instruction(&format!("mov x2, #{}", WRAPPER_FAILED_PREFIX.len()));  // prefix byte length
    emitter.instruction("bl __rt_diag_warning_fragment");                       // start the path-carrying warning
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the wrapper path pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the wrapper path length
    emitter.instruction("bl __rt_diag_warning_fragment");                       // append the failing URL
    abi::emit_symbol_address(emitter, "x1", "_diag_fgc_wrapper_suffix");        // warning suffix ": Failed to open stream: operation failed"
    emitter.instruction(&format!("mov x2, #{}", WRAPPER_FAILED_SUFFIX.len()));  // suffix byte length
    emitter.instruction("bl __rt_diag_warning");                                // finish and dispatch the warning
    emitter.instruction("mov x1, #0");                                          // null string pointer → PHP false
    emitter.instruction("mov x2, #0");                                          // zero failure length
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the failure pair
}

/// x86_64 helper: rax/rdx = path, rax/rdx = owned string on return.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: file_get_contents_wrapper ---");
    emitter.label_global("__rt_file_get_contents_wrapper");

    // Frame: [rbp-8]=path ptr [rbp-16]=path len [rbp-24]=fd
    //        [rbp-32]=result ptr [rbp-40]=result len
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve path, descriptor, and result spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the wrapper path pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the wrapper path length

    abi::emit_symbol_address(emitter, "rdi", "_meta_mode_rb");                  // pass the shared "rb" read mode
    emitter.instruction("mov rsi, 2");                                          // strlen("rb")
    emitter.instruction("call __rt_fopen");                                     // rax = synthetic wrapper fd, or -1 on failure
    emitter.instruction("cmp rax, 0");                                          // did the wrapper refuse the URL?
    emitter.instruction("jl __rt_fgcw_fail_x86");                               // return PHP false and warn
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the synthetic fd across drain/close

    emitter.instruction("mov rdi, rax");                                        // pass the synthetic fd to the drain helper
    emitter.instruction("call __rt_stream_get_contents");                       // rax/rdx = accumulated wrapper payload
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the result pointer across close
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // preserve the result length across close
    emitter.instruction("call __rt_str_persist");                               // adopt scratch bytes into an owned result
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // persist any moved pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // persist any moved length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the synthetic fd
    emitter.instruction("call __rt_user_wrapper_fclose");                       // run stream_close and release the handle slot
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the owned payload pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // return the owned payload length
    emitter.instruction("add rsp, 48");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the owned payload

    emitter.label("__rt_fgcw_fail_x86");
    abi::emit_symbol_address(emitter, "rdi", "_diag_fgc_wrapper_prefix");       // warning prefix "Warning: file_get_contents("
    emitter.instruction(&format!("mov esi, {}", WRAPPER_FAILED_PREFIX.len()));  // prefix byte length
    emitter.instruction("call __rt_diag_warning_fragment");                     // start the path-carrying warning
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the wrapper path pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the wrapper path length
    emitter.instruction("call __rt_diag_warning_fragment");                     // append the failing URL
    abi::emit_symbol_address(emitter, "rdi", "_diag_fgc_wrapper_suffix");       // warning suffix ": Failed to open stream: operation failed"
    emitter.instruction(&format!("mov esi, {}", WRAPPER_FAILED_SUFFIX.len()));  // suffix byte length
    emitter.instruction("call __rt_diag_warning");                              // finish and dispatch the warning
    emitter.instruction("xor eax, eax");                                        // null string pointer → PHP false
    emitter.instruction("xor edx, edx");                                        // zero failure length
    emitter.instruction("add rsp, 48");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure pair
}
