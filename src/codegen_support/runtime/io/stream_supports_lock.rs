//! Purpose:
//! Emits `__rt_stream_supports_lock`, which answers `stream_supports_lock()` from the
//! wrapper a stream was opened through.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The `stream_supports_lock()` lowering.
//!
//! Key details:
//! - php-src answers from the stream's ops: a descriptor-backed stream carries the lock
//!   option, the memory and output wrappers do not. Measured against `php -n` 8.5.6, that
//!   makes exactly `php://memory`, `php://temp`, `php://output` and `php://input` answer
//!   false, while a file, `tmpfile()`, `php://stdout` and `STDIN` answer true.
//! - Those four are told apart the way `stream_type_name` tells them apart: wrapper id
//!   `php` plus the byte after `php://`. A descriptor test cannot do it, because elephc
//!   backs `php://memory` with a real temporary descriptor.

use crate::codegen_support::runtime::resources::layout::{
    STREAM_URI_LEN_OFFSET, STREAM_URI_PTR_OFFSET, STREAM_WRAPPER_ID_OFFSET,
};
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Wrapper id recorded for a `php://` stream.
const WRAPPER_ID_PHP: u64 = 6;
/// The `data://` wrapper id, which reports no lock option and is not local.
const WRAPPER_ID_DATA: u64 = 7;

/// Emits `__rt_stream_supports_lock(handle) -> 1 lockable / 0 not`.
pub fn emit_stream_supports_lock(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 form.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_supports_lock ---");
    emitter.label_global("__rt_stream_supports_lock");
    emitter.instruction("sub sp, sp, #16");                                     // frame for the saved linkage
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("bl __rt_stream_state");                                // resolve the owning stream state
    emitter.instruction("cbz x0, __rt_ssl_yes");                                // no state: keep the permissive answer
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_WRAPPER_ID_OFFSET}]")); // which wrapper opened it
    emitter.instruction(&format!("cmp x9, #{WRAPPER_ID_DATA}"));
    emitter.instruction("b.eq __rt_ssl_no");                                    // data:// carries its payload in the URI: nothing to lock
    emitter.instruction(&format!("cmp x9, #{WRAPPER_ID_PHP}"));
    emitter.instruction("b.ne __rt_ssl_yes");                                   // otherwise only the php:// wrappers lack the lock option
    emitter.instruction(&format!("ldr x10, [x0, #{STREAM_URI_PTR_OFFSET}]"));   // the recorded URI
    emitter.instruction(&format!("ldr x11, [x0, #{STREAM_URI_LEN_OFFSET}]"));   // and its length
    emitter.instruction("cbz x10, __rt_ssl_yes");                               // no URI to classify
    emitter.instruction("cmp x11, #7");                                         // "php://" plus the naming byte
    emitter.instruction("b.lt __rt_ssl_yes");
    emitter.instruction("ldrb w12, [x10, #6]");                                 // the first byte of the php:// wrapper name
    emitter.instruction("cmp w12, #0x6D");                                      // 'm' as in memory
    emitter.instruction("b.eq __rt_ssl_no");
    emitter.instruction("cmp w12, #0x74");                                      // 't' as in temp
    emitter.instruction("b.eq __rt_ssl_no");
    emitter.instruction("cmp w12, #0x6F");                                      // 'o' as in output
    emitter.instruction("b.eq __rt_ssl_no");
    emitter.instruction("cmp w12, #0x69");                                      // 'i' as in input
    emitter.instruction("b.eq __rt_ssl_no");
    emitter.label("__rt_ssl_yes");
    emitter.instruction("mov x0, #1");                                          // stdin/stdout/stderr, fd, and every other wrapper lock
    emitter.instruction("b __rt_ssl_ret");
    emitter.label("__rt_ssl_no");
    emitter.instruction("mov x0, #0");                                          // the memory and output wrappers do not
    emitter.label("__rt_ssl_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");
}

/// Emits the Linux x86_64 form.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_supports_lock ---");
    emitter.label_global("__rt_stream_supports_lock");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("call __rt_stream_state");                              // resolve the owning stream state
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_ssl_yes_x86");                                 // no state: keep the permissive answer
    emitter.instruction(&format!(
        "mov r9, QWORD PTR [rax + {STREAM_WRAPPER_ID_OFFSET}]"
    ));                                                                         // which wrapper opened it
    emitter.instruction(&format!("cmp r9, {WRAPPER_ID_DATA}"));
    emitter.instruction("je __rt_ssl_no_x86");                                  // data:// carries its payload in the URI: nothing to lock
    emitter.instruction(&format!("cmp r9, {WRAPPER_ID_PHP}"));
    emitter.instruction("jne __rt_ssl_yes_x86");                                // only the php:// wrappers lack the lock option
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {STREAM_URI_PTR_OFFSET}]"
    ));                                                                         // the recorded URI
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [rax + {STREAM_URI_LEN_OFFSET}]"
    ));                                                                         // and its length
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_ssl_yes_x86");                                 // no URI to classify
    emitter.instruction("cmp r11, 7");                                          // "php://" plus the naming byte
    emitter.instruction("jl __rt_ssl_yes_x86");
    emitter.instruction("movzx r9d, BYTE PTR [r10 + 6]");                       // the first byte of the php:// wrapper name
    emitter.instruction("cmp r9d, 0x6D");                                       // 'm' as in memory
    emitter.instruction("je __rt_ssl_no_x86");
    emitter.instruction("cmp r9d, 0x74");                                       // 't' as in temp
    emitter.instruction("je __rt_ssl_no_x86");
    emitter.instruction("cmp r9d, 0x6F");                                       // 'o' as in output
    emitter.instruction("je __rt_ssl_no_x86");
    emitter.instruction("cmp r9d, 0x69");                                       // 'i' as in input
    emitter.instruction("je __rt_ssl_no_x86");
    emitter.label("__rt_ssl_yes_x86");
    emitter.instruction("mov eax, 1");                                          // stdin/stdout/stderr, fd, and every other wrapper lock
    emitter.instruction("jmp __rt_ssl_ret_x86");
    emitter.label("__rt_ssl_no_x86");
    emitter.instruction("xor eax, eax");                                        // the memory and output wrappers do not
    emitter.label("__rt_ssl_ret_x86");
    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}
