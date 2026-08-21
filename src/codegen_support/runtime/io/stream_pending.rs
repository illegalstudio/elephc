//! Purpose:
//! Emits `__rt_stream_pending_put` and `__rt_stream_pending_take`, the per-stream holding area for
//! bytes a read consumed but must not hand back yet.
//!
//! Called from:
//! - `crate::codegen_support::runtime::io::stream_get_line`, which fills it and drains it.
//! - `crate::codegen_support::runtime::io::{fread, fgets}`, which drain it before the descriptor.
//!
//! Key details:
//! - php's `stream_get_line()` answers `false` when it finds neither the delimiter nor the length
//!   cap and the stream is not at EOF — and the bytes it read STAY on the stream. Measured on
//!   `php -n` 8.5.6 over a non-blocking socket pair: writing "abc" then calling
//!   `stream_get_line($h, 100, "\n")` answers `false`, and after "def\n" arrives the next call
//!   answers "abcdef". elephc consumed the "abc" and handed it back, so the caller received a line
//!   that php never breaks.
//! - php holds those bytes in the stream's own read buffer, which every read function shares — a
//!   refused `stream_get_line()` is followed by an `fread()` that sees the same "abc". This is that
//!   buffer, narrowed to what makes the difference observable: only `stream_get_line()` fills it,
//!   and the three readers drain it before touching the descriptor. A program that never reaches
//!   the refusal pays one load and a branch per read.
//! - EOF is NOT this case. A blocking file whose last line has no delimiter still answers that
//!   line, because there the loop stopped at a genuine end of input; only the would-block exit
//!   pushes back.

use crate::codegen_support::runtime::resources::layout::{
    STREAM_PENDING_LEN_OFFSET, STREAM_PENDING_POS_OFFSET, STREAM_PENDING_PTR_OFFSET,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_stream_pending_put(handle, ptr, len)`, which retains a copy of `len` bytes.
///
/// Replaces whatever was held: the only caller drained the buffer into its own accumulation first,
/// so what it hands back already contains the previous contents.
pub fn emit_stream_pending_put(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_put_aarch64(emitter),
        Arch::X86_64 => emit_put_x86_64(emitter),
    }
}

/// Emits `__rt_stream_pending_take(handle, dest, max) -> n`, which moves up to `max` held bytes.
///
/// Advances the read cursor and releases the block once it is drained, so a stream that is not in
/// the middle of a refused `stream_get_line()` holds nothing.
pub fn emit_stream_pending_take(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_take_aarch64(emitter),
        Arch::X86_64 => emit_take_x86_64(emitter),
    }
}

/// The AArch64 writer.
fn emit_put_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: retain unconsumed stream bytes ---");
    emitter.label_global("__rt_stream_pending_put");
    // Frame: [0]=state [8]=source ptr [16]=source len.
    emitter.instruction("sub sp, sp, #48");                                     // reserve the writer frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x1, [sp, #8]");                                    // the bytes to retain
    emitter.instruction("str x2, [sp, #16]");                                   // and how many
    emitter.instruction("cbz x2, __rt_spp_done");                               // nothing to retain
    emitter.instruction("bl __rt_stream_state");                                // resolve the owning stream state
    emitter.instruction("cbz x0, __rt_spp_done");                               // a stale handle retains nothing
    emitter.instruction("str x0, [sp, #0]");                                    // the state outlives the allocation

    // -- release anything already held, then take a fresh copy --
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_PENDING_PTR_OFFSET}]"));
    emitter.instruction("cbz x9, __rt_spp_alloc");
    emitter.instruction("mov x0, x9");
    emitter.instruction("bl __rt_heap_free");                                   // the caller already drained it
    emitter.label("__rt_spp_alloc");
    emitter.instruction("ldr x0, [sp, #16]");                                   // the byte count
    emitter.instruction("bl __rt_heap_alloc");
    emitter.instruction("cbz x0, __rt_spp_done");                               // out of memory: retain nothing
    emitter.instruction("ldr x9, [sp, #0]");                                    // the state again
    emitter.instruction(&format!("str x0, [x9, #{STREAM_PENDING_PTR_OFFSET}]"));
    emitter.instruction("ldr x10, [sp, #16]");
    emitter.instruction(&format!("str x10, [x9, #{STREAM_PENDING_LEN_OFFSET}]"));
    emitter.instruction(&format!("str xzr, [x9, #{STREAM_PENDING_POS_OFFSET}]"));
    emitter.instruction("ldr x11, [sp, #8]");                                   // the source bytes
    emitter.instruction("mov x12, #0");                                         // copy cursor
    emitter.label("__rt_spp_copy");
    emitter.instruction("cmp x12, x10");
    emitter.instruction("b.ge __rt_spp_done");
    emitter.instruction("ldrb w13, [x11, x12]");
    emitter.instruction("strb w13, [x0, x12]");
    emitter.instruction("add x12, x12, #1");
    emitter.instruction("b __rt_spp_copy");

    emitter.label("__rt_spp_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the writer frame
    emitter.instruction("ret");
}

/// The AArch64 reader.
fn emit_take_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: drain retained stream bytes ---");
    emitter.label_global("__rt_stream_pending_take");
    // Frame: [0]=state [8]=dest [16]=max [24]=moved.
    emitter.instruction("sub sp, sp, #48");                                     // reserve the reader frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x1, [sp, #8]");                                    // where the bytes go
    emitter.instruction("str x2, [sp, #16]");                                   // and how many at most
    emitter.instruction("str xzr, [sp, #24]");                                  // nothing moved yet
    emitter.instruction("cbz x2, __rt_spt_done");                               // room for nothing
    emitter.instruction("bl __rt_stream_state");                                // resolve the owning stream state
    emitter.instruction("cbz x0, __rt_spt_done");                               // a stale handle holds nothing
    emitter.instruction("str x0, [sp, #0]");
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_PENDING_PTR_OFFSET}]"));
    emitter.instruction("cbz x9, __rt_spt_done");                               // the common case: nothing held
    emitter.instruction(&format!("ldr x10, [x0, #{STREAM_PENDING_LEN_OFFSET}]"));
    emitter.instruction(&format!("ldr x11, [x0, #{STREAM_PENDING_POS_OFFSET}]"));
    emitter.instruction("sub x12, x10, x11");                                   // bytes still held
    emitter.instruction("cbz x12, __rt_spt_release");                           // drained: let the block go
    emitter.instruction("ldr x13, [sp, #16]");                                  // the caller's room
    emitter.instruction("cmp x12, x13");
    emitter.instruction("csel x12, x13, x12, gt");                              // move MIN(held, room)
    emitter.instruction("str x12, [sp, #24]");                                  // that is the answer
    emitter.instruction("ldr x14, [sp, #8]");                                   // the destination
    emitter.instruction("add x9, x9, x11");                                     // the first byte still held
    emitter.instruction("mov x15, #0");                                         // copy cursor
    emitter.label("__rt_spt_copy");
    emitter.instruction("cmp x15, x12");
    emitter.instruction("b.ge __rt_spt_copied");
    emitter.instruction("ldrb w16, [x9, x15]");
    emitter.instruction("strb w16, [x14, x15]");
    emitter.instruction("add x15, x15, #1");
    emitter.instruction("b __rt_spt_copy");
    emitter.label("__rt_spt_copied");
    emitter.instruction("ldr x0, [sp, #0]");                                    // the state
    emitter.instruction("add x11, x11, x12");                                   // advance the read cursor
    emitter.instruction(&format!("str x11, [x0, #{STREAM_PENDING_POS_OFFSET}]"));
    emitter.instruction(&format!("ldr x10, [x0, #{STREAM_PENDING_LEN_OFFSET}]"));
    emitter.instruction("cmp x11, x10");
    emitter.instruction("b.lt __rt_spt_done");                                  // more still held for the next read
    emitter.label("__rt_spt_release");
    emitter.instruction("ldr x0, [sp, #0]");                                    // the state
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_PENDING_PTR_OFFSET}]"));
    emitter.instruction(&format!("str xzr, [x0, #{STREAM_PENDING_PTR_OFFSET}]"));
    emitter.instruction(&format!("str xzr, [x0, #{STREAM_PENDING_LEN_OFFSET}]"));
    emitter.instruction(&format!("str xzr, [x0, #{STREAM_PENDING_POS_OFFSET}]"));
    emitter.instruction("cbz x9, __rt_spt_done");
    emitter.instruction("mov x0, x9");
    emitter.instruction("bl __rt_heap_free");                                   // the holding area is empty again
    emitter.label("__rt_spt_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // how many bytes moved
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the reader frame
    emitter.instruction("ret");
}

/// The x86_64 writer.
fn emit_put_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: retain unconsumed stream bytes ---");
    emitter.label_global("__rt_stream_pending_put");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the writer frame
    emitter.instruction("sub rsp, 48");                                         // reserve the state and source slots
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // the bytes to retain
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // and how many
    emitter.instruction("test rdx, rdx");
    emitter.instruction("jz __rt_spp_done_x86");                                // nothing to retain
    emitter.instruction("call __rt_stream_state");                              // resolve the owning stream state
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_spp_done_x86");                                // a stale handle retains nothing
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // the state outlives the allocation

    emitter.instruction(&format!(
        "mov r9, QWORD PTR [rax + {STREAM_PENDING_PTR_OFFSET}]"
    ));
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_spp_alloc_x86");
    emitter.instruction("mov rax, r9");                                         // __rt_heap_free takes its pointer in rax
    emitter.instruction("call __rt_heap_free");                                 // the caller already drained it
    emitter.label("__rt_spp_alloc_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // the byte count
    emitter.instruction("call __rt_heap_alloc");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_spp_done_x86");                                // out of memory: retain nothing
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // the state again
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {STREAM_PENDING_PTR_OFFSET}], rax"
    ));
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {STREAM_PENDING_LEN_OFFSET}], r10"
    ));
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {STREAM_PENDING_POS_OFFSET}], 0"
    ));
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // the source bytes
    emitter.instruction("xor rcx, rcx");                                        // copy cursor
    emitter.label("__rt_spp_copy_x86");
    emitter.instruction("cmp rcx, r10");
    emitter.instruction("jge __rt_spp_done_x86");
    emitter.instruction("movzx edi, BYTE PTR [r11 + rcx]");
    emitter.instruction("mov BYTE PTR [rax + rcx], dil");
    emitter.instruction("inc rcx");
    emitter.instruction("jmp __rt_spp_copy_x86");

    emitter.label("__rt_spp_done_x86");
    emitter.instruction("add rsp, 48");                                         // release the writer frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}

/// The x86_64 reader.
fn emit_take_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: drain retained stream bytes ---");
    emitter.label_global("__rt_stream_pending_take");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the reader frame
    emitter.instruction("sub rsp, 48");                                         // reserve the state, destination and tally slots
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // where the bytes go
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // and how many at most
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // nothing moved yet
    emitter.instruction("test rdx, rdx");
    emitter.instruction("jz __rt_spt_done_x86");                                // room for nothing
    emitter.instruction("call __rt_stream_state");                              // resolve the owning stream state
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_spt_done_x86");                                // a stale handle holds nothing
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");
    emitter.instruction(&format!(
        "mov r9, QWORD PTR [rax + {STREAM_PENDING_PTR_OFFSET}]"
    ));
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_spt_done_x86");                                // the common case: nothing held
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {STREAM_PENDING_LEN_OFFSET}]"
    ));
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [rax + {STREAM_PENDING_POS_OFFSET}]"
    ));
    emitter.instruction("mov rcx, r10");
    emitter.instruction("sub rcx, r11");                                        // bytes still held
    emitter.instruction("test rcx, rcx");
    emitter.instruction("jz __rt_spt_release_x86");                             // drained: let the block go
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // the caller's room
    emitter.instruction("cmp rcx, rdx");
    emitter.instruction("cmovg rcx, rdx");                                      // move MIN(held, room)
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // that is the answer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // the destination
    emitter.instruction("add r9, r11");                                         // the first byte still held
    emitter.instruction("xor r8, r8");                                          // copy cursor
    emitter.label("__rt_spt_copy_x86");
    emitter.instruction("cmp r8, rcx");
    emitter.instruction("jge __rt_spt_copied_x86");
    emitter.instruction("movzx edi, BYTE PTR [r9 + r8]");
    emitter.instruction("mov BYTE PTR [rsi + r8], dil");
    emitter.instruction("inc r8");
    emitter.instruction("jmp __rt_spt_copy_x86");
    emitter.label("__rt_spt_copied_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the state
    emitter.instruction("add r11, rcx");                                        // advance the read cursor
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {STREAM_PENDING_POS_OFFSET}], r11"
    ));
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {STREAM_PENDING_LEN_OFFSET}]"
    ));
    emitter.instruction("cmp r11, r10");
    emitter.instruction("jl __rt_spt_done_x86");                                // more still held for the next read
    emitter.label("__rt_spt_release_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the state
    emitter.instruction(&format!(
        "mov r9, QWORD PTR [rax + {STREAM_PENDING_PTR_OFFSET}]"
    ));
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {STREAM_PENDING_PTR_OFFSET}], 0"
    ));
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {STREAM_PENDING_LEN_OFFSET}], 0"
    ));
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {STREAM_PENDING_POS_OFFSET}], 0"
    ));
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_spt_done_x86");
    emitter.instruction("mov rax, r9");                                         // __rt_heap_free takes its pointer in rax
    emitter.instruction("call __rt_heap_free");                                 // the holding area is empty again
    emitter.label("__rt_spt_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // how many bytes moved
    emitter.instruction("add rsp, 48");                                         // release the reader frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}

/// Silences the unused-import warning when neither arm references the ABI helper.
#[allow(dead_code)]
fn _abi_used(emitter: &mut Emitter) {
    let _ = abi::int_result_reg(emitter);
}
