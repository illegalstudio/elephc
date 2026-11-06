//! Purpose:
//! Materializes PHP_BINARY from the running executable on Linux and Apple targets.
//!
//! Called from:
//! - Typed EIR executable-path calls and native constant inventory/registration.
//!
//! Key details:
//! - The returned PHP string owns its heap allocation.
//! - Linux reads procfs; Apple uses dyld followed by realpath, including iOS.
//! - All scratch path bytes live in the current activation's aligned stack frame.

use crate::codegen_support::{abi, emit::Emitter, platform::{Arch, Platform}};

/// Emits an owned executable path without capturing the compiler process's location.
pub(crate) fn emit_php_binary(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: current executable path ---");
    emitter.label_global("__rt_php_binary");
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
    emitter.raw(".p2align 3");
    emitter.label("__rt_php_binary_proc_path");
    emitter.raw("    .asciz \"/proc/self/exe\"");
    emitter.raw(".p2align 2");
}

/// Uses the platform's executable-path API and persists the activation-local bytes.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // preserve the caller frame and return address
    emitter.instruction("mov x29, sp");                                         // anchor scratch metadata outside the path buffers
    emitter.instruction("sub sp, sp, #2, lsl #12");                             // reserve independent raw and canonical path buffers
    if emitter.target.platform == Platform::MacOS {
        emitter.instruction("mov w9, #4096");                                   // bound the dyld path buffer
        emitter.instruction("str w9, [x29, #16]");                              // publish the mutable dyld capacity
        emitter.instruction("mov x0, sp");                                      // pass the raw executable path buffer
        emitter.instruction("add x1, x29, #16");                                // pass the in/out capacity word
        emitter.bl_c("_NSGetExecutablePath");
        emitter.instruction("cbnz w0, __rt_php_binary_empty");                  // an unavailable path produces an empty string
        emitter.instruction("mov x0, sp");                                      // canonicalize the path supplied by dyld
        emitter.instruction("add x1, sp, #1, lsl #12");                         // keep canonical bytes separate from the input
        emitter.bl_c("realpath");
        emitter.instruction("cbnz x0, __rt_php_binary_have_path");              // prefer the canonical executable path
        emitter.instruction("mov x0, sp");                                      // preserve dyld's path if canonicalization fails
        emitter.label("__rt_php_binary_have_path");
        emitter.instruction("str x0, [x29, #24]");                              // preserve the selected path across strlen
        emitter.bl_c("strlen");
        emitter.instruction("mov x2, x0");                                      // return the selected byte length
        emitter.instruction("ldr x1, [x29, #24]");                              // recover the selected path bytes
        emitter.instruction("b __rt_php_binary_persist");                       // copy the path before releasing stack storage
    } else {
        abi::emit_symbol_address(emitter, "x0", "__rt_php_binary_proc_path");
        emitter.instruction("mov x1, sp");                                      // read the running executable's procfs symlink
        emitter.instruction("mov x2, #4095");                                   // bound the read to the activation-local buffer
        emitter.bl_c("readlink");
        emitter.instruction("tbnz x0, #63, __rt_php_binary_empty");             // a failed read has no path bytes
        emitter.instruction("mov x2, x0");                                      // readlink returns the byte length without a terminator
        emitter.instruction("mov x1, sp");                                      // expose the populated path bytes
        emitter.instruction("b __rt_php_binary_persist");                       // persist successful path reads
    }
    emitter.label("__rt_php_binary_empty");
    emitter.instruction("mov x1, sp");                                          // an empty slice needs no initialized path bytes
    emitter.instruction("mov x2, #0");                                          // return an empty path when the OS cannot identify it
    emitter.label("__rt_php_binary_persist");
    abi::emit_call_label(emitter, "__rt_str_persist");
    emitter.instruction("add sp, sp, #2, lsl #12");                             // release the path buffers after copying
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore the caller frame and return address
    emitter.instruction("ret");                                                 // return an owned PHP path string
}

/// Reads procfs on Linux x86_64 while preserving System V stack alignment.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // preserve the caller's frame pointer
    emitter.instruction("mov rbp, rsp");                                        // anchor the frame across libc and heap calls
    emitter.instruction("sub rsp, 8192");                                       // reserve path scratch with call alignment intact
    abi::emit_symbol_address(emitter, "rdi", "__rt_php_binary_proc_path");
    emitter.instruction("mov rsi, rsp");                                        // read the running executable's procfs symlink
    emitter.instruction("mov rdx, 4095");                                       // bound the read to the scratch buffer
    emitter.bl_c("readlink");
    emitter.instruction("test rax, rax");                                       // a negative result reports an unavailable path
    emitter.instruction("jns __rt_php_binary_have_length");                     // successful reads already carry a length
    emitter.instruction("xor eax, eax");                                        // expose an empty string on a failed read
    emitter.label("__rt_php_binary_have_length");
    emitter.instruction("mov rdx, rax");                                        // use readlink's byte length without requiring a terminator
    emitter.instruction("mov rax, rsp");                                        // persist the activation-local path bytes
    abi::emit_call_label(emitter, "__rt_str_persist");
    emitter.instruction("leave");                                               // release scratch and restore the caller's frame
    emitter.instruction("ret");                                                 // return an owned PHP path string
}
