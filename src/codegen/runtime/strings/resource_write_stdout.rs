//! Purpose:
//! Emits the `__rt_resource_write_stdout`, `__rt_itoa` runtime helper assembly for resource write stdout.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - Resource helpers format or write runtime resource identifiers without claiming ownership of external descriptors.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_resource_write_stdout` runtime helper.
///
/// Writes `"Resource id #<id>"` to stdout, where `<id>` is the 1-based display
/// form of the resource index passed in `x0` (0-based internally, converted to
/// 1-based for display). Calls `__rt_itoa` to format the decimal id. Falls through
/// to the ARM64 default after the x86_64 Linux branch.
pub fn emit_resource_write_stdout(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_resource_write_stdout_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: resource_write_stdout ---");
    emitter.label_global("__rt_resource_write_stdout");

    emitter.instruction("sub sp, sp, #32");                                     // reserve a small frame for the saved return address and resource payload
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address before nested helper calls
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the native resource payload while writing the prefix
    abi::emit_symbol_address(emitter, "x1", "_resource_id_prefix");
    emitter.instruction("mov x2, #13");                                         // pass the byte length of "Resource id #"
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the native resource payload for display-id formatting
    emitter.instruction("add x0, x0, #1");                                      // present resources as 1-based ids like PHP's display form
    abi::emit_call_label(emitter, "__rt_itoa");                                 // convert the resource display id into decimal text
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address after the writes
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the `__rt_resource_write_stdout` runtime helper for Linux x86_64.
fn emit_resource_write_stdout_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resource_write_stdout ---");
    emitter.label_global("__rt_resource_write_stdout");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before allocating locals
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame pointer for the helper body
    emitter.instruction("sub rsp, 16");                                         // reserve aligned local storage for the native resource payload
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the native resource payload while writing the prefix
    abi::emit_symbol_address(emitter, "rsi", "_resource_id_prefix");
    emitter.instruction("mov edx, 13");                                         // pass the byte length of "Resource id #"
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // write the resource prefix to stdout
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the native resource payload for display-id formatting
    emitter.instruction("add rax, 1");                                          // present resources as 1-based ids like PHP's display form
    abi::emit_call_label(emitter, "__rt_itoa");                                 // convert the resource display id into decimal text
    emitter.instruction("mov rsi, rax");                                        // move the formatted id pointer into the write buffer register
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // write the resource id digits to stdout
    emitter.instruction("add rsp, 16");                                         // release the aligned local storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}
