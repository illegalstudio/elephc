//! Purpose:
//! Emits the `__rt_resource_write_stdout`, `__rt_itoa` runtime helper assembly for resource write stdout.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - Resource helpers format or write runtime resource identifiers without claiming ownership of external descriptors.
//! - Both writes (the prefix and the decimal id) route through `__rt_stdout_write` so `--web` output capture applies.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_resource_write_stdout` runtime helper.
///
/// Writes `"Resource id #<id>"` to stdout, where `<id>` is the 1-based display
/// form of the resource index passed in `x0` (0-based internally, converted to
/// 1-based for display). Calls `__rt_itoa` to format the decimal id. Both the
/// prefix and the id are emitted through `__rt_stdout_write` so the `--web`
/// capture indirection sees the bytes. Falls through to the ARM64 default after
/// the x86_64 Linux branch.
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
    emitter.instruction("mov x2, #13");                                         // byte length of "Resource id #"
    emitter.instruction("mov x0, x1");                                          // capture-aware write: prefix pointer → x0
    emitter.instruction("mov x1, x2");                                          // prefix length → x1 per __rt_stdout_write's ABI
    emitter.instruction("bl __rt_stdout_write");                                // route the prefix through the capture indirection
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the native resource payload for display-id formatting
    emitter.instruction("add x0, x0, #1");                                      // present resources as 1-based ids like PHP's display form
    abi::emit_call_label(emitter, "__rt_itoa");                                 // convert the resource display id into decimal text (x1/x2)
    emitter.instruction("mov x0, x1");                                          // capture-aware write: id pointer → x0
    emitter.instruction("mov x1, x2");                                          // id length → x1 per __rt_stdout_write's ABI
    emitter.instruction("bl __rt_stdout_write");                                // route the id digits through the capture indirection
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
    emitter.instruction("mov edx, 13");                                         // byte length of "Resource id #"
    emitter.instruction("mov rdi, rsi");                                        // capture-aware write: prefix pointer → first arg register
    emitter.instruction("mov rsi, rdx");                                        // prefix length → second arg register
    emitter.instruction("call __rt_stdout_write");                              // route the prefix through the capture indirection
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the native resource payload for display-id formatting
    emitter.instruction("add rax, 1");                                          // present resources as 1-based ids like PHP's display form
    abi::emit_call_label(emitter, "__rt_itoa");                                 // convert the resource display id into decimal text (rax ptr, rdx len)
    emitter.instruction("mov rdi, rax");                                        // capture-aware write: id pointer → first arg register
    emitter.instruction("mov rsi, rdx");                                        // id length → second arg register
    emitter.instruction("call __rt_stdout_write");                              // route the id digits through the capture indirection
    emitter.instruction("add rsp, 16");                                         // release the aligned local storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}
