//! Purpose:
//! Emits the `__rt_dirname_levels`, `__rt_dirname_levels_fail` runtime helper assembly for dirname levels.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen::{emit::Emitter, platform::Arch};

use super::super::data::DIRNAME_LEVELS_MSG;

/// Emits the `__rt_dirname_levels` runtime helper.
/// Applies `dirname()` repeatedly `levels` times to the path in x1/x2.
/// Inputs: x1=path pointer, x2=path length, x3=levels
/// Output: x1/x2 = parent directory after `levels` applications
/// Fatal: exits with diagnostic if levels < 1 (PHP requires levels >= 1).
pub fn emit_dirname_levels(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_dirname_levels_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment after preceding runtime literals
    emitter.comment("--- runtime: dirname levels ---");
    emitter.label_global("__rt_dirname_levels");

    emitter.instruction("sub sp, sp, #32");                                     // reserve a small frame for the loop counter and return address
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across dirname calls
    emitter.instruction("add x29, sp, #16");                                    // establish a stable frame pointer for the loop frame
    emitter.instruction("cmp x3, #1");                                          // PHP requires dirname() levels to be at least 1
    emitter.instruction("b.lt __rt_dirname_levels_fail");                       // reject invalid dynamic levels with a fatal runtime diagnostic
    emitter.instruction("str x3, [sp, #0]");                                    // store the requested parent depth as the remaining loop count

    emitter.label("__rt_dirname_levels_loop");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the remaining dirname applications
    emitter.instruction("cmp x9, #0");                                          // have all requested levels been consumed?
    emitter.instruction("b.le __rt_dirname_levels_done");                       // zero or negative levels leave the current path unchanged
    emitter.instruction("bl __rt_dirname");                                     // replace the current path with its parent directory
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the remaining level count after dirname clobbered scratch regs
    emitter.instruction("sub x9, x9, #1");                                      // account for the dirname application just performed
    emitter.instruction("str x9, [sp, #0]");                                    // persist the decremented remaining level count
    emitter.instruction("b __rt_dirname_levels_loop");                          // continue until the requested number of levels has been applied

    emitter.label("__rt_dirname_levels_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the dirname-levels frame
    emitter.instruction("ret");                                                 // return the repeated dirname result in x1/x2

    emitter.label("__rt_dirname_levels_fail");
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_dirname_levels_msg"); // load the dirname levels fatal diagnostic text
    emitter.instruction(&format!("mov x2, #{}", DIRNAME_LEVELS_MSG.len()));     // pass the exact dirname levels diagnostic length to write()
    emitter.instruction("mov x0, #2");                                          // write dirname diagnostics to stderr
    emitter.syscall(4);
    emitter.instruction("mov x0, #1");                                          // use a failing process exit status for invalid dirname levels
    emitter.syscall(1);
}

/// Emits the x86_64 Linux variant of `__rt_dirname_levels`.
/// ABI: rdi=path_ptr, rdx=path_len, rsi=levels. Returns rax/rdx.
/// Fatal: exits with diagnostic if levels < 1 (PHP requires levels >= 1).
fn emit_dirname_levels_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: dirname levels ---");
    emitter.label_global("__rt_dirname_levels");

    // ABI: rax=path_ptr, rdx=path_len, rdi=levels. Returns rax/rdx.
    emitter.instruction("push rbp");                                            // preserve caller frame pointer before the loop helper uses a spill slot
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the remaining level count
    emitter.instruction("sub rsp, 16");                                         // reserve aligned spill space for the remaining level count
    emitter.instruction("cmp rdi, 1");                                          // PHP requires dirname() levels to be at least 1
    emitter.instruction("jl __rt_dirname_levels_fail_x86");                     // reject invalid dynamic levels with a fatal runtime diagnostic
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested parent depth as the remaining loop count

    emitter.label("__rt_dirname_levels_loop_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the remaining dirname applications
    emitter.instruction("test r8, r8");                                         // have all requested levels been consumed?
    emitter.instruction("jle __rt_dirname_levels_done_x86");                    // zero or negative levels leave the current path unchanged
    emitter.instruction("call __rt_dirname");                                   // replace the current path with its parent directory
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the remaining level count after dirname clobbered scratch regs
    emitter.instruction("sub r8, 1");                                           // account for the dirname application just performed
    emitter.instruction("mov QWORD PTR [rbp - 8], r8");                         // persist the decremented remaining level count
    emitter.instruction("jmp __rt_dirname_levels_loop_x86");                    // continue until the requested number of levels has been applied

    emitter.label("__rt_dirname_levels_done_x86");
    emitter.instruction("add rsp, 16");                                         // release the dirname-levels spill space
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the repeated dirname result in rax/rdx

    emitter.label("__rt_dirname_levels_fail_x86");
    crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_dirname_levels_msg"); // load the dirname levels fatal diagnostic text
    emitter.instruction(&format!("mov edx, {}", DIRNAME_LEVELS_MSG.len()));     // pass the exact dirname levels diagnostic length to write()
    emitter.instruction("mov edi, 2");                                          // write dirname diagnostics to stderr
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 writes the diagnostic bytes
    emitter.instruction("syscall");                                             // emit the invalid dirname levels diagnostic
    emitter.instruction("mov edi, 1");                                          // use a failing process exit status for invalid dirname levels
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 exits the process
    emitter.instruction("syscall");                                             // terminate after the fatal dirname diagnostic
}
