//! Purpose:
//! Stream get/copy scratch frames, bounds, and target loops.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;
use crate::codegen_support::runtime::data::{
    STREAM_COPY_NO_SEEK, STREAM_COPY_SEEK_FAILED_HEAD, STREAM_COPY_SEEK_FAILED_TAIL,
};

/// Reserves temporary storage for `stream_get_contents` stream and length operands.
pub(super) fn emit_stream_get_contents_frame_enter(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #32");                         // reserve aligned temporary storage for fd and length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 32");                             // reserve aligned temporary storage for fd and length
        }
    }
}

/// Releases temporary storage used by `stream_get_contents`.
pub(super) fn emit_stream_get_contents_frame_leave(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("add sp, sp, #32");                         // release stream_get_contents temporary storage
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("add rsp, 32");                             // release stream_get_contents temporary storage
        }
    }
}

/// Saves the opaque handle and currently loaded backend descriptor in the temporary frame.
pub(super) fn emit_stream_get_contents_save_fd(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x10, [sp, #0]");                       // save the opaque handle across length and offset evaluation
            ctx.emitter.instruction("str x0, [sp, #24]");                       // save the backend descriptor for optional seeking
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], r10");            // save the opaque handle across length and offset evaluation
            ctx.emitter.instruction("mov QWORD PTR [rsp + 24], rax");           // save the backend descriptor for optional seeking
        }
    }
}

/// Saves the currently loaded length value in the temporary frame.
pub(super) fn emit_stream_get_contents_save_length(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [sp, #8]");                        // save the requested byte count or unlimited sentinel
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // save the requested byte count or unlimited sentinel
        }
    }
}

/// Reloads the saved handle and releases the `stream_get_contents` temporary frame.
pub(super) fn lower_stream_get_contents_reload_fd_and_leave_frame(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the opaque stream handle for the read-all path
            ctx.emitter.instruction("ldr x1, [sp, #16]");                       // pass the state-owned read-loop chunk size
            emit_stream_get_contents_frame_leave(ctx);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // reload the opaque stream handle for the read-all path
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 16]");           // pass the state-owned read-loop chunk size
            emit_stream_get_contents_frame_leave(ctx);
        }
    }
}

/// Applies the optional `stream_get_contents` seek before reading.
pub(super) fn lower_stream_get_contents_seek(
    ctx: &mut FunctionContext<'_>,
    skip_seek: &str,
    wrap_seek: &str,
    seek_failed: &str,
) {
    let seek_ok = ctx.next_label("sgc_seek_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // a negative offset means no seek is requested
            ctx.emitter.instruction(&format!("b.lt {}", skip_seek));            // keep the current position for negative offsets
            ctx.emitter.instruction("mov x1, x0");                              // pass offset as the second seek argument
            ctx.emitter.instruction("mov x2, #0");                              // pass SEEK_SET as the third seek argument
            ctx.emitter.instruction("ldr x0, [sp, #24]");                       // reload the opaque stream handle
            // The saved slot now holds the opaque handle, so resolve the backend
            // descriptor before the wrapper probe and lseek. Passing a handle to
            // lseek made every offset-seeking stream_get_contents report failure.
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the seek arguments across the resolve
            abi::emit_call_label(ctx.emitter, "__rt_stream_fd");
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the seek arguments
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether the handle is a synthetic wrapper fd
            ctx.emitter.instruction(&format!("b.ge {}", wrap_seek));            // dispatch synthetic handles to wrapper stream_seek
            ctx.emitter.syscall(199);
            if ctx.emitter.platform.needs_cmp_before_error_branch() {
                ctx.emitter.instruction("cmp x0, #0");                          // Linux reports lseek failure as a negative result
            }
            ctx.emitter.instruction(
                &ctx.emitter.platform.branch_on_syscall_success(&seek_ok)
            );                                                                  // continue only when lseek succeeded
            ctx.emitter.instruction(&format!("b {}", seek_failed));             // failed seek makes stream_get_contents return false
            ctx.emitter.label(wrap_seek);
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fseek");
            ctx.emitter.instruction("cmp x0, #0");                              // wrapper fseek returns zero on success
            ctx.emitter.instruction(&format!("b.ne {}", seek_failed));          // failed wrapper seek makes stream_get_contents return false
            // A SEEK invalidates the stream's read buffer: what it holds was read from wherever
            // the last read stopped, and this call has just moved somewhere else. Without this,
            // `stream_get_contents($h, 2)` followed by `stream_get_contents($h, -1, 4)` answered
            // the buffered remainder AND the bytes from offset 4 — "cdefef" where php says "ef".
            // Only the SEEKING path clears it: reaching `skip_seek` without a seek must keep
            // whatever the buffer holds.
            ctx.emitter.label(&seek_ok);
            ctx.emitter.instruction("ldr x0, [sp, #24]");                       // the opaque stream handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_pending_clear");
            ctx.emitter.label(skip_seek);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // a negative offset means no seek is requested
            ctx.emitter.instruction(&format!("jl {}", skip_seek));              // keep the current position for negative offsets
            ctx.emitter.instruction("mov rsi, rax");                            // pass offset as the second seek argument
            ctx.emitter.instruction("xor edx, edx");                            // pass SEEK_SET as the third seek argument
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 24]");           // reload the opaque stream handle
            // The saved slot now holds the opaque handle, so resolve the backend
            // descriptor before the wrapper probe and lseek. Passing a handle to
            // lseek made every offset-seeking stream_get_contents report failure.
            ctx.emitter.instruction("push rsi");                                // preserve the seek arguments across the resolve
            ctx.emitter.instruction("push rdx");
            abi::emit_call_label(ctx.emitter, "__rt_stream_fd");
            ctx.emitter.instruction("pop rdx");
            ctx.emitter.instruction("pop rsi");
            ctx.emitter.instruction("mov rdi, rax");                            // seek on the resolved descriptor
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rdi, r9");                             // test whether the handle is a synthetic wrapper fd
            ctx.emitter.instruction(&format!("jge {}", wrap_seek));             // dispatch synthetic handles to wrapper stream_seek
            ctx.emitter.instruction("call lseek");                              // seek the native stream descriptor
            ctx.emitter.instruction("cmp rax, 0");                              // test whether lseek returned a non-negative offset
            ctx.emitter.instruction(&format!("jl {}", seek_failed));            // failed seek makes stream_get_contents return false
            ctx.emitter.instruction(&format!("jmp {}", seek_ok));               // continue after a successful native seek
            ctx.emitter.label(wrap_seek);
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fseek");
            ctx.emitter.instruction("cmp rax, 0");                              // wrapper fseek returns zero on success
            ctx.emitter.instruction(&format!("jne {}", seek_failed));           // failed wrapper seek makes stream_get_contents return false
            // See the AArch64 counterpart: a seek invalidates the stream's read buffer.
            ctx.emitter.label(&seek_ok);
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 24]");           // the opaque stream handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_pending_clear");
            ctx.emitter.label(skip_seek);
        }
    }
}

/// Performs a finite bounded read or jumps to read-all for null/negative length.
pub(super) fn lower_stream_get_contents_bounded_or_all(
    ctx: &mut FunctionContext<'_>,
    stream: crate::ir::ValueId,
    read_all: &str,
    done: &str,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #8]");                        // reload the requested byte count
            emit_branch_if_unlimited_length(ctx, "x9", "x10", read_all);
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the opaque stream handle for bounded reading
            ctx.emitter.instruction("mov x1, x9");                              // pass the finite byte count to the bounded helper
            ctx.emitter.instruction("ldr x2, [sp, #16]");                       // pass the state-owned read-loop chunk size
            emit_stream_get_contents_frame_leave(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_stream_get_contents_bounded");
            super::stream_file_ops::emit_advance_wrapper_position(ctx, stream, "stream_get_contents")?;
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            ctx.emitter.instruction(&format!("b {}", done));                    // bounded read completed successfully
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 8]");             // reload the requested byte count
            emit_branch_if_unlimited_length(ctx, "r9", "r10", read_all);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // reload the opaque stream handle for bounded reading
            ctx.emitter.instruction("mov rdi, rax");                            // pass the opaque stream handle to the bounded helper
            ctx.emitter.instruction("mov rsi, r9");                             // pass the finite byte count to the bounded helper
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");           // pass the state-owned read-loop chunk size
            emit_stream_get_contents_frame_leave(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_stream_get_contents_bounded");
            super::stream_file_ops::emit_advance_wrapper_position(ctx, stream, "stream_get_contents")?;
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            ctx.emitter.instruction(&format!("jmp {}", done));                  // bounded read completed successfully
        }
    }
    Ok(())
}

/// Branches when a length value means "read until EOF".
pub(super) fn emit_branch_if_unlimited_length(
    ctx: &mut FunctionContext<'_>,
    length_reg: &str,
    scratch_reg: &str,
    target_label: &str,
) {
    abi::emit_load_int_immediate(ctx.emitter, scratch_reg, NULL_SENTINEL);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(
                &format!("cmp {}, {}", length_reg, scratch_reg)
            );                                                                  // test whether length is PHP null
            ctx.emitter.instruction(&format!("b.eq {}", target_label));         // null length means read until EOF
            ctx.emitter.instruction(&format!("cmp {}, #0", length_reg));        // test whether length is negative
            ctx.emitter.instruction(&format!("b.lt {}", target_label));         // negative length means read until EOF
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(
                &format!("cmp {}, {}", length_reg, scratch_reg)
            );                                                                  // test whether length is PHP null
            ctx.emitter.instruction(&format!("je {}", target_label));           // null length means read until EOF
            ctx.emitter.instruction(&format!("cmp {}, 0", length_reg));         // test whether length is negative
            ctx.emitter.instruction(&format!("jl {}", target_label));           // negative length means read until EOF
        }
    }
}

/// Creates the `stream_copy_to_stream` scratch frame after source-handle/destination-fd loading.
pub(super) fn emit_stream_copy_frame_enter(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // preserve the destination descriptor while restoring the source handle
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("sub sp, sp, #48");                         // reserve source, destination, total, chunk, and max-length slots
            ctx.emitter.instruction("str x0, [sp, #0]");                        // save the opaque source handle
            ctx.emitter.instruction("str x1, [sp, #8]");                        // save the destination descriptor
            ctx.emitter.instruction("str xzr, [sp, #16]");                      // initialize copied-byte total to zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // preserve the destination descriptor while restoring the source handle
            abi::emit_pop_reg(ctx.emitter, "rdi");
            ctx.emitter.instruction("sub rsp, 48");                             // reserve source, destination, total, chunk, and max-length slots
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rdi");            // save the opaque source handle
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rsi");            // save the destination descriptor
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], 0");             // initialize copied-byte total to zero
        }
    }
}

/// Releases the `stream_copy_to_stream` scratch frame.
pub(super) fn emit_stream_copy_frame_leave(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("add sp, sp, #48");                         // release stream_copy_to_stream temporary storage
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("add rsp, 48");                             // release stream_copy_to_stream temporary storage
        }
    }
}

/// Saves the optional `stream_copy_to_stream` length or the unlimited sentinel.
pub(super) fn materialize_stream_copy_length(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 3 {
        let length = expect_operand(inst, 2)?;
        // `-1`, not the null sentinel: this site's copier reads the SAME word the omitted-argument
        // branch below writes, and that word is `-1`.
        load_optional_int_to_result(ctx, length, "stream_copy_to_stream length", -1)?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), -1);
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [sp, #32]");                       // save requested max bytes or the unlimited sentinel
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp + 32], rax");           // save requested max bytes or the unlimited sentinel
        }
    }
    Ok(())
}

/// Applies the optional `stream_copy_to_stream` source seek before copying.
///
/// php-src guards the seek with `pos > 0`, so a ZERO offset — the documented default — copies
/// from the source's CURRENT position rather than rewinding it. MEASURED on `php -n` 8.5.6 with
/// the source parked at byte 4 of `"0123456789"`: `$offset` of `0`, `-1`, and the omitted
/// default all copy 6 bytes (`"456789"`), while `2` copies 8. Treating `0` as "seek to the
/// start" — what this used to do, in a lowering whose contract default was still `-1` — rewound
/// a partially consumed source.
pub(super) fn lower_stream_copy_seek(
    ctx: &mut FunctionContext<'_>,
    skip_seek: &str,
    wrap_seek: &str,
    seek_failed: &str,
) {
    let native_success = ctx.next_label("scs_native_seek_success");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // only a strictly positive offset seeks, like php-src's `pos > 0`
            ctx.emitter.instruction(&format!("b.lt {}", skip_seek));            // keep the current source position for zero and negative offsets
            ctx.emitter.instruction("str x0, [sp, #40]");                       // preserve the requested offset across handle resolution
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the opaque source handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_fd");
            ctx.emitter.instruction("ldr x1, [sp, #40]");                       // pass offset as the second seek argument
            ctx.emitter.instruction("mov x2, #0");                              // pass SEEK_SET as the third seek argument
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether the source is a synthetic wrapper fd
            ctx.emitter.instruction(&format!("b.lo {}", native_success));       // descriptors below the wrapper range use native lseek
            crate::codegen_support::runtime::io::emit_load_handles_cap(ctx.emitter, "x10");
            ctx.emitter.instruction("add x10, x9, x10");                        // wrapper range end = USER_WRAPPER_FD_BASE + handle capacity
            ctx.emitter.instruction("cmp x0, x10");                             // is the backend above the wrapper range?
            ctx.emitter.instruction(&format!("b.lo {}", wrap_seek));            // dispatch wrapper backends to stream_seek
            let native_done = ctx.next_label("scs_native_seek_done");
            ctx.emitter.label(&native_success);
            ctx.emitter.syscall(199);
            if ctx.emitter.platform.needs_cmp_before_error_branch() {
                ctx.emitter.instruction("cmp x0, #0");                          // Linux reports lseek failure as a negative result
            }
            ctx.emitter.instruction(
                &ctx.emitter.platform.branch_on_syscall_success(&native_done)
            );                                                                  // continue only when lseek succeeded
            ctx.emitter.instruction(&format!("b {}", seek_failed));             // failed native seek returns PHP false
            // The descriptor moved; the bytes the stream was still HOLDING did not. Draining them
            // first copied from the old position — MEASURED, a source read to 4 then copied from
            // offset 8 answered the six held bytes plus the two real ones. `fseek()` reconciles
            // the same two pieces of state after its own successful lseek, for the same reason.
            ctx.emitter.label(&native_done);
            ctx.emitter.instruction("str x0, [sp, #40]");                       // the absolute offset lseek landed on
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the opaque source handle
            ctx.emitter.instruction("ldr x1, [sp, #40]");                       // the position the program restarts from
            abi::emit_call_label(ctx.emitter, "__rt_stream_filtered_pos_set");
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the opaque source handle
            ctx.emitter.instruction("mov x1, #0");                              // a seek restarts reading: clear end-of-file
            abi::emit_call_label(ctx.emitter, "__rt_stream_eof_set");
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the opaque source handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_pending_clear");     // drop what the last read held
            ctx.emitter.instruction(&format!("b {}", skip_seek));               // skip the wrapper arm
            ctx.emitter.label(wrap_seek);
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fseek");
            ctx.emitter.instruction("cmp x0, #0");                              // wrapper fseek returns zero on success
            ctx.emitter.instruction(&format!("b.ne {}", seek_failed));          // failed wrapper seek returns PHP false
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the opaque source handle
            ctx.emitter.instruction("mov x1, #0");                              // the wrapper seek restarts reading too
            abi::emit_call_label(ctx.emitter, "__rt_stream_eof_set");
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the opaque source handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_pending_clear");     // drop what the last read held
            ctx.emitter.label(skip_seek);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // only a strictly positive offset seeks, like php-src's `pos > 0`
            ctx.emitter.instruction(&format!("jl {}", skip_seek));              // keep the current source position for zero and negative offsets
            ctx.emitter.instruction("mov QWORD PTR [rsp + 40], rax");           // preserve the requested offset across handle resolution
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the opaque source handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_fd");
            ctx.emitter.instruction("mov rdi, rax");                            // pass the resolved backend descriptor to seek
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 40]");           // pass offset as the second seek argument
            ctx.emitter.instruction("xor edx, edx");                            // pass SEEK_SET as the third seek argument
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rdi, r9");                             // test whether the source is a synthetic wrapper fd
            ctx.emitter.instruction(&format!("jb {}", native_success));         // descriptors below the wrapper range use native lseek
            crate::codegen_support::runtime::io::emit_load_handles_cap(ctx.emitter, "r10");
            ctx.emitter.instruction("add r10, r9");                             // wrapper range end = USER_WRAPPER_FD_BASE + handle capacity
            ctx.emitter.instruction("cmp rdi, r10");                            // is the backend above the wrapper range?
            ctx.emitter.instruction(&format!("jb {}", wrap_seek));              // dispatch wrapper backends to stream_seek
            ctx.emitter.label(&native_success);
            ctx.emitter.instruction("call lseek");                              // seek the native stream descriptor
            ctx.emitter.instruction("cmp rax, 0");                              // test whether lseek returned a non-negative offset
            ctx.emitter.instruction(&format!("jl {}", seek_failed));            // failed native seek returns PHP false
            // See the AArch64 arm: the descriptor moved, the held read-ahead did not.
            ctx.emitter.instruction("mov QWORD PTR [rsp + 40], rax");           // the absolute offset lseek landed on
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the opaque source handle
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 40]");           // the position the program restarts from
            abi::emit_call_label(ctx.emitter, "__rt_stream_filtered_pos_set");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the opaque source handle
            ctx.emitter.instruction("xor esi, esi");                            // a seek restarts reading: clear end-of-file
            abi::emit_call_label(ctx.emitter, "__rt_stream_eof_set");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the opaque source handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_pending_clear");     // drop what the last read held
            ctx.emitter.instruction(&format!("jmp {}", skip_seek));             // continue after a successful native seek
            ctx.emitter.label(wrap_seek);
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fseek");
            ctx.emitter.instruction("cmp rax, 0");                              // wrapper fseek returns zero on success
            ctx.emitter.instruction(&format!("jne {}", seek_failed));           // failed wrapper seek returns PHP false
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the opaque source handle
            ctx.emitter.instruction("xor esi, esi");                            // the wrapper seek restarts reading too
            abi::emit_call_label(ctx.emitter, "__rt_stream_eof_set");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the opaque source handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_pending_clear");     // drop what the last read held
            ctx.emitter.label(skip_seek);
        }
    }
}

/// Copies source chunks into the destination and boxes the int-or-false result.
pub(super) fn lower_stream_copy_loop_and_box(
    ctx: &mut FunctionContext<'_>,
    seek_failed: &str,
    boxed_done: &str,
) {
    let loop_label = ctx.next_label("scs_loop");
    let done_label = ctx.next_label("scs_done");
    let length_unlimited = ctx.next_label("scs_length_unlimited");
    let after_length_check = ctx.next_label("scs_after_length_check");
    let request_unlimited = ctx.next_label("scs_request_unlimited");
    let after_request = ctx.next_label("scs_after_request");
    let chunk_unlimited = ctx.next_label("scs_chunk_unlimited");
    let after_chunk = ctx.next_label("scs_after_chunk");
    ctx.emitter.label(&loop_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_stream_copy_loop_aarch64(
            ctx,
            &loop_label,
            &done_label,
            &length_unlimited,
            &after_length_check,
            &request_unlimited,
            &after_request,
            &chunk_unlimited,
            &after_chunk,
        ),
        Arch::X86_64 => lower_stream_copy_loop_x86_64(
            ctx,
            &loop_label,
            &done_label,
            &length_unlimited,
            &after_length_check,
            &request_unlimited,
            &after_request,
            &chunk_unlimited,
            &after_chunk,
        ),
    }
    // php answers `false`, not a byte count, when a read filter REFUSED the data with
    // PSFS_ERR_FATAL: `stream_copy_to_stream()` on such a stream is `bool(false)` where elephc
    // reported `int(0)`, which reads as "nothing to copy" rather than "the copy failed". The
    // code is the one the last brigade dispatch published; the copy resets it to PSFS_PASS_ON
    // before its first read, so it can only be FATAL because a filter said so during THIS copy.
    // The frame has already been left by the loop, so this path boxes false directly rather than
    // sharing the seek-failure label, which leaves the frame itself.
    let filter_ok = ctx.next_label("scs_filter_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_user_filter_last_psfs");
            ctx.emitter.instruction("ldr x9, [x9]");                            // the code the last dispatch published
            ctx.emitter.instruction("cmp x9, #0");                              // PSFS_ERR_FATAL
            ctx.emitter.instruction(&format!("b.ne {}", filter_ok));            // no filter refused: report the byte count
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_user_filter_last_psfs");
            ctx.emitter.instruction("mov r9, QWORD PTR [r9]");                  // the code the last dispatch published
            ctx.emitter.instruction("cmp r9, 0");                               // PSFS_ERR_FATAL
            ctx.emitter.instruction(&format!("jne {}", filter_ok));             // no filter refused: report the byte count
        }
    }
    emit_bool_result(ctx, false);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("b {}", boxed_done)),
        Arch::X86_64 => ctx.emitter.instruction(&format!("jmp {}", boxed_done)),
    }
    ctx.emitter.label(&filter_ok);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", boxed_done));              // successful copy skips the seek-failure false result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", boxed_done));            // successful copy skips the seek-failure false result
        }
    }
    ctx.emitter.label(seek_failed);
    emit_stream_copy_seek_refusal(ctx);
    emit_stream_copy_frame_leave(ctx);
    emit_bool_result(ctx, false);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    ctx.emitter.label(boxed_done);
}

/// Says why `stream_copy_to_stream()` refused, in php's two lines.
///
/// The copier's own message is unconditional on a failed seek and names the offset it could not
/// reach. The line before it belongs to `_php_stream_seek` and appears only when the stream has no
/// seek operation AT ALL — for a userspace wrapper, when the class declares no `stream_seek`. php
/// discovers that by CALLING the method and finding it absent, then marks the stream unseekable
/// and falls through to the refusal; `__rt_user_wrapper_lacks_seek` asks the vtable the same
/// question directly, and answers no for every other kind of stream, whose seek failed on its own
/// terms. MEASURED on `php -n` 8.5.6 with two wrappers differing in nothing but that method.
///
/// Emitted while the copy frame is still up, because the offset it prints lives in it.
fn emit_stream_copy_seek_refusal(ctx: &mut FunctionContext<'_>) {
    let has_seek_op = ctx.next_label("scs_has_seek_op");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // the opaque source handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_fd");
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_lacks_seek");
            ctx.emitter.instruction(&format!("cbz x0, {}", has_seek_op));       // the stream has a seek that simply failed
            abi::emit_symbol_address(ctx.emitter, "x1", "_scts_no_seek");
            ctx.emitter.instruction(&format!("mov x2, #{}", STREAM_COPY_NO_SEEK.len()));
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            ctx.emitter.label(&has_seek_op);
            abi::emit_symbol_address(ctx.emitter, "x1", "_scts_seek_head");
            ctx.emitter.instruction(&format!("mov x2, #{}", STREAM_COPY_SEEK_FAILED_HEAD.len()));
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            ctx.emitter.instruction("ldr x0, [sp, #40]");                       // the offset the seek was asked for
            abi::emit_call_label(ctx.emitter, "__rt_itoa");                     // x1 = digits, x2 = length
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_symbol_address(ctx.emitter, "x1", "_scts_seek_tail");
            ctx.emitter.instruction(&format!("mov x2, #{}", STREAM_COPY_SEEK_FAILED_TAIL.len()));
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");             // the newline writes the composed line out
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // the opaque source handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_fd");
            ctx.emitter.instruction("mov rdi, rax");
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_lacks_seek");
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction(&format!("jz {}", has_seek_op));            // the stream has a seek that simply failed
            abi::emit_symbol_address(ctx.emitter, "rdi", "_scts_no_seek");
            ctx.emitter.instruction(&format!("mov rsi, {}", STREAM_COPY_NO_SEEK.len()));
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            ctx.emitter.label(&has_seek_op);
            abi::emit_symbol_address(ctx.emitter, "rdi", "_scts_seek_head");
            ctx.emitter.instruction(&format!("mov rsi, {}", STREAM_COPY_SEEK_FAILED_HEAD.len()));
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 40]");           // the offset the seek was asked for
            abi::emit_call_label(ctx.emitter, "__rt_itoa");                     // rax = digits, rdx = length
            ctx.emitter.instruction("mov rdi, rax");
            ctx.emitter.instruction("mov rsi, rdx");
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_symbol_address(ctx.emitter, "rdi", "_scts_seek_tail");
            ctx.emitter.instruction(&format!("mov rsi, {}", STREAM_COPY_SEEK_FAILED_TAIL.len()));
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");             // the newline writes the composed line out
        }
    }
}

/// Emits the AArch64 stream-copy read/write loop.
pub(super) fn lower_stream_copy_loop_aarch64(
    ctx: &mut FunctionContext<'_>,
    loop_label: &str,
    done_label: &str,
    length_unlimited: &str,
    after_length_check: &str,
    request_unlimited: &str,
    after_request: &str,
    chunk_unlimited: &str,
    after_chunk: &str,
) {
    ctx.emitter.instruction("ldr x9, [sp, #16]");                               // load copied-byte total so far
    ctx.emitter.instruction("ldr x10, [sp, #32]");                              // load requested max byte count
    emit_branch_if_unlimited_length(ctx, "x10", "x11", length_unlimited);
    ctx.emitter.instruction("cmp x9, x10");                                     // test whether requested byte count is complete
    ctx.emitter.instruction(&format!("b.ge {}", done_label));                   // finish once length bytes have been copied
    ctx.emitter.instruction(&format!("b {}", after_length_check));              // continue with a finite request
    ctx.emitter.label(length_unlimited);
    ctx.emitter.label(after_length_check);
    ctx.emitter.instruction("ldr x0, [sp, #0]");                                // reload the opaque source handle
    ctx.emitter.instruction("mov x1, #4096");                                   // request up to 4096 bytes by default
    ctx.emitter.instruction("ldr x10, [sp, #32]");                              // load requested max byte count
    emit_branch_if_unlimited_length(ctx, "x10", "x11", request_unlimited);
    ctx.emitter.instruction("ldr x9, [sp, #16]");                               // reload copied-byte total so far
    ctx.emitter.instruction("sub x10, x10, x9");                                // compute remaining finite bytes
    ctx.emitter.instruction("cmp x10, x1");                                     // check whether remaining bytes are below the chunk cap
    ctx.emitter.instruction("csel x1, x10, x1, lt");                            // clamp the read request to remaining bytes
    ctx.emitter.instruction(&format!("b {}", after_request));                   // finite request size is ready
    ctx.emitter.label(request_unlimited);
    ctx.emitter.label(after_request);
    abi::emit_call_label(ctx.emitter, "__rt_fread");
    ctx.emitter.instruction(&format!("cbz x2, {}", done_label));                // stop when the source returns an empty chunk
    ctx.emitter.instruction("ldr x10, [sp, #32]");                              // load requested max byte count
    emit_branch_if_unlimited_length(ctx, "x10", "x11", chunk_unlimited);
    ctx.emitter.instruction("ldr x9, [sp, #16]");                               // reload copied-byte total so far
    ctx.emitter.instruction("sub x10, x10, x9");                                // compute remaining finite bytes
    ctx.emitter.instruction("cmp x2, x10");                                     // check whether the read returned too many bytes
    ctx.emitter.instruction("csel x2, x2, x10, ls");                            // clamp wrapper chunks to remaining bytes
    ctx.emitter.instruction(&format!("b {}", after_chunk));                     // finite chunk length is ready
    ctx.emitter.label(chunk_unlimited);
    ctx.emitter.label(after_chunk);
    ctx.emitter.instruction("str x1, [sp, #24]");                               // save the owned chunk pointer for release
    ctx.emitter.instruction("ldr x9, [sp, #16]");                               // load copied-byte total so far
    ctx.emitter.instruction("add x9, x9, x2");                                  // add this chunk length to the total
    ctx.emitter.instruction("str x9, [sp, #16]");                               // store updated copied-byte total
    ctx.emitter.instruction("ldr x0, [sp, #8]");                                // reload the destination descriptor
    abi::emit_call_label(ctx.emitter, "__rt_fwrite");
    ctx.emitter.instruction("ldr x0, [sp, #24]");                               // reload the owned chunk pointer
    abi::emit_call_label(ctx.emitter, "__rt_decref_any");
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // copy the next chunk
    ctx.emitter.label(done_label);
    ctx.emitter.instruction("ldr x0, [sp, #16]");                               // return the copied-byte total
    emit_stream_copy_frame_leave(ctx);
}

/// Emits the x86_64 stream-copy read/write loop.
pub(super) fn lower_stream_copy_loop_x86_64(
    ctx: &mut FunctionContext<'_>,
    loop_label: &str,
    done_label: &str,
    length_unlimited: &str,
    after_length_check: &str,
    request_unlimited: &str,
    after_request: &str,
    chunk_unlimited: &str,
    after_chunk: &str,
) {
    ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 16]");                    // load copied-byte total so far
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 32]");                    // load requested max byte count
    emit_branch_if_unlimited_length(ctx, "r9", "r10", length_unlimited);
    ctx.emitter.instruction("cmp r8, r9");                                      // test whether requested byte count is complete
    ctx.emitter.instruction(&format!("jge {}", done_label));                    // finish once length bytes have been copied
    ctx.emitter.instruction(&format!("jmp {}", after_length_check));            // continue with a finite request
    ctx.emitter.label(length_unlimited);
    ctx.emitter.label(after_length_check);
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                    // reload the opaque source handle
    ctx.emitter.instruction("mov rsi, 4096");                                   // request up to 4096 bytes by default
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 32]");                    // load requested max byte count
    emit_branch_if_unlimited_length(ctx, "r9", "r10", request_unlimited);
    ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 16]");                    // reload copied-byte total so far
    ctx.emitter.instruction("sub r9, r8");                                      // compute remaining finite bytes
    ctx.emitter.instruction("cmp r9, rsi");                                     // check whether remaining bytes are below the chunk cap
    ctx.emitter.instruction("cmovl rsi, r9");                                   // clamp the read request to remaining bytes
    ctx.emitter.instruction(&format!("jmp {}", after_request));                 // finite request size is ready
    ctx.emitter.label(request_unlimited);
    ctx.emitter.label(after_request);
    abi::emit_call_label(ctx.emitter, "__rt_fread");
    ctx.emitter.instruction("test rdx, rdx");                                   // check whether the source returned an empty chunk
    ctx.emitter.instruction(&format!("jz {}", done_label));                     // stop when the source returns an empty chunk
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 32]");                    // load requested max byte count
    emit_branch_if_unlimited_length(ctx, "r9", "r10", chunk_unlimited);
    ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 16]");                    // reload copied-byte total so far
    ctx.emitter.instruction("sub r9, r8");                                      // compute remaining finite bytes
    ctx.emitter.instruction("cmp rdx, r9");                                     // check whether the read returned too many bytes
    ctx.emitter.instruction("cmova rdx, r9");                                   // clamp wrapper chunks to remaining bytes
    ctx.emitter.instruction(&format!("jmp {}", after_chunk));                   // finite chunk length is ready
    ctx.emitter.label(chunk_unlimited);
    ctx.emitter.label(after_chunk);
    ctx.emitter.instruction("mov QWORD PTR [rsp + 24], rax");                   // save the owned chunk pointer for release
    ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 16]");                    // load copied-byte total so far
    ctx.emitter.instruction("add r8, rdx");                                     // add this chunk length to the total
    ctx.emitter.instruction("mov QWORD PTR [rsp + 16], r8");                    // store updated copied-byte total
    ctx.emitter.instruction("mov rsi, rax");                                    // pass the chunk pointer to fwrite
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                    // reload the destination descriptor
    abi::emit_call_label(ctx.emitter, "__rt_fwrite");
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 24]");                   // reload the owned chunk pointer
    abi::emit_call_label(ctx.emitter, "__rt_decref_any");
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // copy the next chunk
    ctx.emitter.label(done_label);
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                   // return the copied-byte total
    emit_stream_copy_frame_leave(ctx);
}

