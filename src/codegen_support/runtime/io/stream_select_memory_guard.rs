//! Purpose:
//! Emits `__rt_stream_select_memory_guard`, which refuses a `php://memory` stream the way php's
//! `stream_select()` does.
//!
//! Called from:
//! - `crate::codegen_support::runtime::io::stream_select`, once per entry while the pollfd array
//!   is built.
//!
//! Key details:
//! - A MEMORY stream is bytes in the heap: there is no operating-system descriptor to poll. php
//!   says so and drops the entry — `Warning: stream_select(): Cannot represent a stream of type
//!   MEMORY as a select()able descriptor` — and the `ValueError: No stream arrays were passed`
//!   follows when nothing selectable is left. elephc polled its backing descriptor instead and
//!   reported the stream ready, so a select loop over a memory stream span forever on php and
//!   returned immediately here.
//! - Only MEMORY is refused. Measured on `php -n` 8.5.6, `php://temp` selects fine — it is backed
//!   by a real file — and so do `data:`, a plain file, and the standard streams.
//! - The type is recognised through `__rt_stream_type_name`, the same helper
//!   `stream_get_meta_data()` uses, so the two can never disagree about what a stream IS. The
//!   comparison is on the returned POINTER: every recorded name is one interned literal, and
//!   `_meta_stype_memory` is that literal for exactly this case.

use crate::codegen_support::runtime::data::SELECT_CAST_UNREPRESENTABLE_MEMORY;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_stream_select_memory_guard(handle) -> handle`, or `-1` after warning.
pub fn emit_stream_select_memory_guard(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// The AArch64 guard.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: refuse a php://memory stream in stream_select ---");
    emitter.label_global("__rt_stream_select_memory_guard");
    emitter.instruction("sub sp, sp, #32");                                     // reserve the guard frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the guard frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // keep the handle for the pass-through

    emitter.instruction("bl __rt_stream_type_name");                            // x0 = the recorded name, or 0
    emitter.instruction("cbz x0, __rt_ssmg_keep");                              // nothing recorded: not a php:// stream
    abi::emit_symbol_address(emitter, "x9", "_meta_stype_memory");
    emitter.instruction("cmp x0, x9");                                          // is it the MEMORY literal itself?
    emitter.instruction("b.ne __rt_ssmg_keep");                                 // any other type stays selectable

    abi::emit_symbol_address(emitter, "x1", "_select_cast_unrepresentable_memory");
    emitter.instruction(&format!(
        "mov x2, #{}",
        SELECT_CAST_UNREPRESENTABLE_MEMORY.len()
    ));
    emitter.instruction("bl __rt_diag_warning");                                // warnings honour the @ suppression depth
    emitter.instruction("mov x0, #-1");                                         // refuse the entry, as php drops it
    emitter.instruction("b __rt_ssmg_done");

    emitter.label("__rt_ssmg_keep");
    emitter.instruction("ldr x0, [sp, #0]");                                    // hand the handle back unchanged
    emitter.label("__rt_ssmg_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the guard frame
    emitter.instruction("ret");
}

/// The x86_64 guard.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: refuse a php://memory stream in stream_select ---");
    emitter.label_global("__rt_stream_select_memory_guard");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the guard frame
    emitter.instruction("sub rsp, 16");                                         // reserve the handle slot
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // keep the handle for the pass-through

    emitter.instruction("call __rt_stream_type_name");                          // rax = the recorded name, or 0
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_ssmg_keep_x");                                 // nothing recorded: not a php:// stream
    emitter.instruction("lea r10, [rip + _meta_stype_memory]");
    emitter.instruction("cmp rax, r10");                                        // is it the MEMORY literal itself?
    emitter.instruction("jne __rt_ssmg_keep_x");                                // any other type stays selectable

    emitter.instruction("lea rdi, [rip + _select_cast_unrepresentable_memory]");
    emitter.instruction(&format!(
        "mov rsi, {}",
        SELECT_CAST_UNREPRESENTABLE_MEMORY.len()
    ));
    emitter.instruction("call __rt_diag_warning");                              // warnings honour the @ suppression depth
    emitter.instruction("mov rax, -1");                                         // refuse the entry, as php drops it
    emitter.instruction("jmp __rt_ssmg_done_x");

    emitter.label("__rt_ssmg_keep_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // hand the handle back unchanged
    emitter.label("__rt_ssmg_done_x");
    emitter.instruction("add rsp, 16");                                         // release the guard frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}
