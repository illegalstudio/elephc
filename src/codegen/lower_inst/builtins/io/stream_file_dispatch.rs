//! Purpose:
//! Passthrough, seek, sync, locking, and fd runtime dispatch.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Emits native or userspace-wrapper streaming for a loaded `fpassthru()` handle.
///
/// Three byte sources, one drain loop. A native descriptor with no read filter streams through
/// `__rt_fpassthru`, which `read()`s the descriptor straight into the output sink. Anything the
/// descriptor cannot answer for — a userspace wrapper, or a stream carrying a read-filter chain
/// — goes through `__rt_fread` instead, because that is the only helper that runs the chain.
/// Reading the descriptor on a filtered stream passed the RAW bytes through, silently.
pub(super) fn emit_fpassthru_dispatch(ctx: &mut FunctionContext<'_>) {
    let wrapper_label = ctx.next_label("fpt_wrapper");
    let loop_label = ctx.next_label("fpt_loop");
    let release_eof_label = ctx.next_label("fpt_release_eof");
    let wrapper_done_label = ctx.next_label("fpt_done");
    let done_label = ctx.next_label("fpt_after");
    let native_label = ctx.next_label("fpt_native");
    let drain_label = ctx.next_label("fpt_drain");
    let probed_label = ctx.next_label("fpt_probed");
    let unfiltered_label = ctx.next_label("fpt_unfiltered");
    let head = crate::codegen_support::runtime::resources::layout::STREAM_READ_FILTER_HEAD_OFFSET;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            // The push reserves 16 bytes for one value, so its high half is free: park the
            // read-filter answer there rather than probe again after the descriptor lookup.
            abi::emit_call_label(ctx.emitter, "__rt_stream_state");
            ctx.emitter.instruction(&format!("cbz x0, {}", unfiltered_label));  // no state: nothing can be attached to it
            ctx.emitter.instruction(&format!("ldr x9, [x0, #{head}]"));         // read-direction chain head
            ctx.emitter.instruction("str x9, [sp, #8]");                        // remember whether the chain exists
            ctx.emitter.instruction(&format!("b {}", probed_label));
            ctx.emitter.label(&unfiltered_label);
            ctx.emitter.instruction("str xzr, [sp, #8]");                       // an unfiltered stream may use the descriptor
            ctx.emitter.label(&probed_label);
            ctx.emitter.instruction("ldr x9, [sp, #8]");                        // the read-direction chain head
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // the opaque handle
            // The chain is asked about BEFORE the backend, because it outranks it: a filtered
            // stream drains through `__rt_fread` whether its backend is a descriptor or a
            // userspace wrapper, and `__rt_fread_raw` resolves that difference itself.
            ctx.emitter.instruction(&format!("cbnz x9, {}", native_label));     // filtered: the handle is its own read handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_fd");
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether the backend is below the wrapper range
            ctx.emitter.instruction(&format!("b.lo {}", native_label));         // native descriptors use the state-aware passthru helper
            crate::codegen_support::runtime::io::emit_load_handles_cap(ctx.emitter, "x10");
            ctx.emitter.instruction("add x10, x9, x10");                        // wrapper range end = USER_WRAPPER_FD_BASE + handle capacity
            ctx.emitter.instruction("cmp x0, x10");                             // is the backend above the wrapper range?
            ctx.emitter.instruction(&format!("b.lo {}", wrapper_label));        // stream wrapper backends through the userspace read loop
            ctx.emitter.label(&native_label);
            ctx.emitter.instruction("ldr x9, [sp, #8]");                        // the read-direction chain head
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // the opaque handle feeds either path
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            ctx.emitter.instruction(&format!("cbnz x9, {}", drain_label));      // filtered: drain through the chain, not the descriptor
            abi::emit_call_label(ctx.emitter, "__rt_fpassthru");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the drain loop after native streaming
            ctx.emitter.label(&wrapper_label);
            abi::emit_release_temporary_stack(ctx.emitter, 16);                 // x0 = the synthetic wrapper fd, its own read handle
            ctx.emitter.label(&drain_label);
            ctx.emitter.instruction("sub sp, sp, #32");                         // reserve fd, byte total, and chunk scratch storage
            ctx.emitter.instruction("str x0, [sp, #0]");                        // preserve the read handle
            ctx.emitter.instruction("str xzr, [sp, #8]");                       // initialize copied byte total to zero
            ctx.emitter.label(&loop_label);
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the read handle for EOF probing
            abi::emit_call_label(ctx.emitter, "__rt_feof");
            ctx.emitter.instruction(&format!("cbnz x0, {}", wrapper_done_label)); // stop streaming when stream_eof reports EOF
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the read handle for reading
            ctx.emitter.instruction("mov x1, #4096");                           // request a bounded read chunk
            abi::emit_call_label(ctx.emitter, "__rt_fread");
            ctx.emitter.instruction(&format!("cbz x2, {}", release_eof_label)); // stop defensively on empty wrapper reads
            ctx.emitter.instruction("str x1, [sp, #16]");                       // preserve the owned chunk pointer for release
            ctx.emitter.instruction("ldr x9, [sp, #8]");                        // load the current copied byte total
            ctx.emitter.instruction("add x9, x9, x2");                          // add this chunk's byte length
            ctx.emitter.instruction("str x9, [sp, #8]");                        // store the updated copied byte total
            ctx.emitter.instruction("bl __rt_vd_write");                        // write x1/x2 through the ob/web-aware stdout sink (register-preserving)
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // reload the owned chunk pointer
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.instruction(&format!("b {}", loop_label));              // continue draining the wrapper stream
            ctx.emitter.label(&release_eof_label);
            ctx.emitter.instruction("mov x0, x1");                              // pass the final empty chunk pointer to decref
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.label(&wrapper_done_label);
            ctx.emitter.instruction("ldr x0, [sp, #8]");                        // return the copied byte total
            ctx.emitter.instruction("add sp, sp, #32");                         // release wrapper streaming scratch storage
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            // See the AArch64 counterpart: the push slot's high half carries the read-filter
            // answer across the descriptor lookup, so the state is resolved exactly once.
            ctx.emitter.instruction("mov rdi, rax");                            // pass the opaque handle to the state lookup
            abi::emit_call_label(ctx.emitter, "__rt_stream_state");
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction(&format!("jz {}", unfiltered_label));       // no state: nothing can be attached to it
            ctx.emitter.instruction(&format!("mov r9, QWORD PTR [rax + {head}]")); // read-direction chain head
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], r9");             // remember whether the chain exists
            ctx.emitter.instruction(&format!("jmp {}", probed_label));
            ctx.emitter.label(&unfiltered_label);
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], 0");              // an unfiltered stream may use the descriptor
            ctx.emitter.label(&probed_label);
            // See the AArch64 counterpart: the chain outranks the backend, so it is asked first.
            ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 8]");             // the read-direction chain head
            ctx.emitter.instruction("test r9, r9");
            ctx.emitter.instruction(&format!("jnz {}", native_label));          // filtered: the handle is its own read handle
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // the opaque handle again
            abi::emit_call_label(ctx.emitter, "__rt_stream_fd");
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rax, r9");                             // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("jb {}", native_label));           // native descriptors use the state-aware passthru helper
            crate::codegen_support::runtime::io::emit_load_handles_cap(ctx.emitter, "r10");
            ctx.emitter.instruction("add r10, r9");                             // wrapper range end = USER_WRAPPER_FD_BASE + handle capacity
            ctx.emitter.instruction("cmp rax, r10");                            // is the backend above the wrapper range?
            ctx.emitter.instruction(&format!("jb {}", wrapper_label));          // stream wrapper backends through the userspace read loop
            ctx.emitter.label(&native_label);
            ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 8]");             // the read-direction chain head
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // the opaque handle feeds either path
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            ctx.emitter.instruction("test r9, r9");
            ctx.emitter.instruction(&format!("jnz {}", drain_label));           // filtered: drain through the chain, not the descriptor
            abi::emit_call_label(ctx.emitter, "__rt_fpassthru");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the drain loop after native streaming
            ctx.emitter.label(&wrapper_label);
            abi::emit_release_temporary_stack(ctx.emitter, 16);                 // rax = the synthetic wrapper fd, its own read handle
            ctx.emitter.label(&drain_label);
            ctx.emitter.instruction("sub rsp, 32");                             // reserve fd, byte total, and chunk scratch storage
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the read handle
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], 0");              // initialize copied byte total to zero
            ctx.emitter.label(&loop_label);
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the read handle for EOF probing
            abi::emit_call_label(ctx.emitter, "__rt_feof");
            ctx.emitter.instruction("test rax, rax");                           // test whether stream_eof reported EOF
            ctx.emitter.instruction(&format!("jnz {}", wrapper_done_label));    // stop streaming when stream_eof reports EOF
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the read handle for reading
            ctx.emitter.instruction("mov rsi, 4096");                           // request a bounded read chunk
            abi::emit_call_label(ctx.emitter, "__rt_fread");
            ctx.emitter.instruction("test rdx, rdx");                           // test whether the wrapper returned an empty chunk
            ctx.emitter.instruction(&format!("jz {}", release_eof_label));      // stop defensively on empty wrapper reads
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // preserve the owned chunk pointer for release
            ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 8]");             // load the current copied byte total
            ctx.emitter.instruction("add r8, rdx");                             // add this chunk's byte length
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], r8");             // store the updated copied byte total
            ctx.emitter.instruction("mov rsi, rax");                            // pass the chunk pointer to write()
            abi::emit_call_label(ctx.emitter, "__rt_vd_write");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // reload the owned chunk pointer
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.instruction(&format!("jmp {}", loop_label));            // continue draining the wrapper stream
            ctx.emitter.label(&release_eof_label);
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.label(&wrapper_done_label);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");            // return the copied byte total
            ctx.emitter.instruction("add rsp, 32");                             // release wrapper streaming scratch storage
            ctx.emitter.label(&done_label);
        }
    }
}

/// Lowers `feof(stream)` through the runtime EOF-flag table helper.
pub(crate) fn lower_feof(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "feof", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_open_stream_handle_to_result(ctx, stream, "feof")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the opaque stream handle to the EOF runtime helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_feof");
    store_if_result(ctx, inst)
}

/// Lowers `ftell(stream)` as `lseek(fd, 0, SEEK_CUR)`, or as PHP's own position for a wrapper.
///
/// php-src has NO tell op for userspace wrappers: `main/streams/userspace.c` calls `stream_tell`
/// only from inside `php_userstreamop_seek`, to reconcile after a seek. `ftell()` reports the
/// position PHP maintains itself, advanced by whatever each read and write moved. Asking the
/// method here — as this used to — reports whatever the wrapper chooses to say: with a
/// `stream_tell()` written to return 999, PHP answers 7 after seven bytes and elephc answered 999.
pub(crate) fn lower_ftell(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "ftell", 1)?;
    let stream = expect_operand(inst, 0)?;
    // A filtered read pulls AHEAD: the buffered wrapper reads whole chunks so the filter has
    // something to work on and parks what `fread($h, $n)` did not ask for. php advances its
    // position by the bytes RETURNED TO THE CALLER, so the descriptor probe below reports where
    // the read-ahead stopped — `26` on a 26-byte file where php answers `3`. The helper answers
    // -1 for every stream that never engaged that buffer, which keeps the probe for all of them.
    let filtered_label = ctx.next_label("ftell_filtered_position");
    load_stream_handle_to_result(ctx, stream, "ftell")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // the handle owns the filtered position
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_filtered_pos");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmn x0, #1");                              // is the filtered position tracked for this stream?
            ctx.emitter.instruction(&format!("b.ne {}", filtered_label));       // tracked: php reports the bytes handed to the caller
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, -1");                             // is the filtered position tracked for this stream?
            ctx.emitter.instruction(&format!("jne {}", filtered_label));        // tracked: php reports the bytes handed to the caller
        }
    }
    load_stream_fd_to_result(ctx, stream, "ftell")?;
    let wrapper_label = ctx.next_label("ftell_user_wrapper");
    let after_dispatch_label = ctx.next_label("ftell_after_dispatch");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("b.ge {}", wrapper_label));        // dispatch synthetic handles to stream_tell
            ctx.emitter.instruction("mov x1, #0");                              // use offset 0 for the ftell lseek probe
            ctx.emitter.instruction("mov x2, #1");                              // use SEEK_CUR for the ftell lseek probe
            ctx.emitter.syscall(199);
            ctx.emitter.instruction(&format!("b {}", after_dispatch_label));    // skip wrapper stream_tell after the native probe
            ctx.emitter.label(&wrapper_label);
            load_stream_handle_to_result(ctx, stream, "ftell")?;
            abi::emit_call_label(ctx.emitter, "__rt_stream_wrapper_pos");
            ctx.emitter.label(&after_dispatch_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rax, r9");                             // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("jge {}", wrapper_label));         // dispatch synthetic handles to stream_tell
            ctx.emitter.instruction("mov rdi, rax");                            // pass the stream fd to libc lseek()
            ctx.emitter.instruction("xor esi, esi");                            // use offset 0 for the ftell lseek probe
            ctx.emitter.instruction("mov edx, 1");                              // use SEEK_CUR for the ftell lseek probe
            ctx.emitter.instruction("call lseek");                              // query the current stream position
            ctx.emitter.instruction(&format!("jmp {}", after_dispatch_label));  // skip wrapper stream_tell after the native probe
            ctx.emitter.label(&wrapper_label);
            load_stream_handle_to_result(ctx, stream, "ftell")?;
            ctx.emitter.instruction("mov rdi, rax");                            // the handle owns the tracked position
            abi::emit_call_label(ctx.emitter, "__rt_stream_wrapper_pos");
            ctx.emitter.label(&after_dispatch_label);
        }
    }
    emit_subtract_append_skip(ctx, stream)?;
    ctx.emitter.label(&filtered_label);
    store_if_result(ctx, inst)
}

/// Turns the descriptor's offset into the position PHP reports.
///
/// They are the same for every stream but an append one, where `O_APPEND` puts each write at the
/// end of the file while PHP's position advances only by the bytes written. `__rt_fwrite`
/// accumulates the difference on the stream state; the helper answers zero for everything else,
/// including a user wrapper, so the subtraction needs no branch of its own.
fn emit_subtract_append_skip(ctx: &mut FunctionContext<'_>, stream: ValueId) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");                              // the descriptor's own offset
            load_stream_handle_to_result(ctx, stream, "ftell")?;
            abi::emit_call_label(ctx.emitter, "__rt_stream_append_skip");
            ctx.emitter.instruction("mov x1, x0");                              // what O_APPEND jumped over
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("sub x0, x0, x1");                          // PHP's position
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");                             // the descriptor's own offset
            load_stream_handle_to_result(ctx, stream, "ftell")?;
            ctx.emitter.instruction("mov rdi, rax");
            abi::emit_call_label(ctx.emitter, "__rt_stream_append_skip");
            ctx.emitter.instruction("mov rcx, rax");                            // what O_APPEND jumped over
            abi::emit_pop_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("sub rax, rcx");                            // PHP's position
        }
    }
    Ok(())
}

/// Puts PHP's position back in agreement with the descriptor after a successful seek.
///
/// PHP answers `0` right after `fseek($h, 0)` on an append stream and `1` after one more byte, so
/// the running total starts again from the seek. Without this it stays and the answer goes
/// negative.
fn emit_clear_append_skip(ctx: &mut FunctionContext<'_>, stream: ValueId, name: &str) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");                              // the seek result the caller is owed
            load_stream_handle_to_result(ctx, stream, name)?;
            abi::emit_call_label(ctx.emitter, "__rt_stream_clear_append_skip");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");                             // the seek result the caller is owed
            load_stream_handle_to_result(ctx, stream, name)?;
            ctx.emitter.instruction("mov rdi, rax");
            abi::emit_call_label(ctx.emitter, "__rt_stream_clear_append_skip");
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    Ok(())
}

/// Lowers `fseek(stream, offset, whence?)` and clears EOF state on success.
pub(crate) fn lower_fseek(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fseek", 2, 3)?;
    let stream = expect_operand(inst, 0)?;
    let offset = expect_operand(inst, 1)?;
    load_open_stream_handle_to_result(ctx, stream, "fseek")?;
    let refused_label = ctx.next_label("fseek_no_seek");
    let finished_label = ctx.next_label("fseek_no_seek_done");
    emit_seek_unsupported_branch(ctx, &refused_label);
    let success_label = ctx.next_label("fseek_success");
    let done_label = ctx.next_label("fseek_done");
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int(ctx.load_value_to_result(offset)?.codegen_repr(), "fseek offset")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    if inst.operands.len() == 3 {
        let whence = expect_operand(inst, 2)?;
        require_int(ctx.load_value_to_result(whence)?.codegen_repr(), "fseek whence")?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_fseek_aarch64(ctx, &success_label, &done_label),
        Arch::X86_64 => lower_fseek_x86_64(ctx, &success_label, &done_label),
    }
    emit_clear_append_skip(ctx, stream, "fseek")?;
    abi::emit_jump(ctx.emitter, &finished_label);
    ctx.emitter.label(&refused_label);
    emit_static_diag_warning(ctx, "Warning: fseek(): Stream does not support seeking\n");
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), -1);
    ctx.emitter.label(&finished_label);
    store_if_result(ctx, inst)
}

/// Lowers `rewind(stream)` as `lseek(fd, 0, SEEK_SET)` and clears EOF state on success.
pub(crate) fn lower_rewind(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "rewind", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_open_stream_handle_to_result(ctx, stream, "rewind")?;
    let refused_label = ctx.next_label("rewind_no_seek");
    let finished_label = ctx.next_label("rewind_no_seek_done");
    emit_seek_unsupported_branch(ctx, &refused_label);
    let success_label = ctx.next_label("rewind_success");
    let done_label = ctx.next_label("rewind_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_rewind_aarch64(ctx, &success_label, &done_label),
        Arch::X86_64 => lower_rewind_x86_64(ctx, &success_label, &done_label),
    }
    emit_clear_append_skip(ctx, stream, "rewind")?;
    abi::emit_jump(ctx.emitter, &finished_label);
    ctx.emitter.label(&refused_label);
    emit_static_diag_warning(ctx, "Warning: rewind(): Stream does not support seeking\n");
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    ctx.emitter.label(&finished_label);
    store_if_result(ctx, inst)
}

/// Branches to `refused_label` when the stream's WRAPPER has no seek op at all.
///
/// php-src refuses such a seek in `php_stream_seek`, before any descriptor is touched, and
/// `ext/zip`'s entry ops are exactly that shape. elephc serves a zip entry out of a regular
/// temp file, which seeks perfectly well, so the refusal can only come from the recorded
/// wrapper identity — see `__rt_stream_seek_unsupported` for the measured php lines.
///
/// The opaque stream handle is in the integer result register on entry and is left there, so
/// the caller's normal path is byte-for-byte what it was for every stream that does seek.
fn emit_seek_unsupported_branch(ctx: &mut FunctionContext<'_>, refused_label: &str) {
    let reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, reg);
    abi::emit_call_label(ctx.emitter, "__rt_stream_seek_unsupported");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x9, x0");                              // hold the verdict across the handle restore
            abi::emit_pop_reg(ctx.emitter, "x0");                               // the opaque stream handle, back where it was
            ctx.emitter.instruction(&format!("cbnz x9, {}", refused_label));    // → php's refusal line and its failure value
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, rax");                            // hold the verdict across the handle restore
            abi::emit_pop_reg(ctx.emitter, "rax");                              // the opaque stream handle, back where it was
            ctx.emitter.instruction("test r10, r10");                           // did the wrapper refuse to seek?
            ctx.emitter.instruction(&format!("jnz {}", refused_label));         // → php's refusal line and its failure value
        }
    }
}

/// Lowers `ftruncate(stream, size)` through the shared fd truncate runtime helper.
pub(crate) fn lower_ftruncate(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "ftruncate", 2)?;
    let stream = expect_operand(inst, 0)?;
    let size = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "ftruncate")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int(ctx.load_value_to_result(size)?.codegen_repr(), "ftruncate size")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the target file size to the ftruncate runtime helper
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the target file size to the ftruncate runtime helper
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    let wrapper_label = ctx.next_label("ftruncate_user_wrapper");
    let done_label = ctx.next_label("ftruncate_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("b.ge {}", wrapper_label));        // dispatch synthetic handles to stream_truncate
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rax, r9");                             // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("jge {}", wrapper_label));         // dispatch synthetic handles to stream_truncate
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_ftruncate");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip wrapper truncation after the native helper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip wrapper truncation after the native helper
        }
    }
    ctx.emitter.label(&wrapper_label);
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the synthetic wrapper descriptor to the truncate helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_ftruncate");
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers `fsync(stream)` through the shared fd sync runtime helper.
pub(crate) fn lower_fsync(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_unary_stream_bool_runtime(ctx, inst, "fsync", "__rt_fsync")
}

/// Lowers `fflush(stream)` through the shared fd flush runtime helper.
pub(crate) fn lower_fflush(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "fflush", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "fflush")?;
    let wrapper_label = ctx.next_label("fflush_user_wrapper");
    let done_label = ctx.next_label("fflush_done");
    // php makes `fflush()` a flush point for a `zlib.deflate` filter too: a deflate stream holds
    // its bytes until zlib's own window fills, and php pushes a `Z_SYNC_FLUSH` pass so what was
    // written so far reaches the stream. Measured on `php -n` 8.5.6 over 400 bytes to a file,
    // `filesize()` reads 0 after the write, 12 after `fflush()` and 14 after `fclose()`; elephc
    // read 0, 0, 8 — nothing appeared until the stream closed, so a long-lived stream compressed
    // everything and sent none of it. The helper returns at once for a stream carrying no filter.
    let no_zlib_label = ctx.next_label("fflush_no_zlib");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_zlib_flush_fn");
            ctx.emitter.instruction("ldr x9, [x9]");                            // the attachment publishes it, or it stays zero
            ctx.emitter.instruction(&format!("cbz x9, {}", no_zlib_label));     // no deflate filter in this program at all
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("blr x9");                                  // push the sync-flush pass for this descriptor
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_zlib_flush_fn");
            ctx.emitter.instruction("mov r9, QWORD PTR [r9]");                  // the attachment publishes it, or it stays zero
            ctx.emitter.instruction("test r9, r9");
            ctx.emitter.instruction(&format!("jz {}", no_zlib_label));          // no deflate filter in this program at all
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rdi, rax");                            // the descriptor to flush
            ctx.emitter.instruction("call r9");                                 // push the sync-flush pass for this descriptor
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    ctx.emitter.label(&no_zlib_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("b.ge {}", wrapper_label));        // dispatch synthetic handles to stream_flush
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rax, r9");                             // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("jge {}", wrapper_label));         // dispatch synthetic handles to stream_flush
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fflush");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip wrapper flushing after the native helper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip wrapper flushing after the native helper
        }
    }
    ctx.emitter.label(&wrapper_label);
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the synthetic wrapper descriptor to the flush helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fflush");
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers `fdatasync(stream)` through the shared fd data-sync runtime helper.
pub(crate) fn lower_fdatasync(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_unary_stream_bool_runtime(ctx, inst, "fdatasync", "__rt_fdatasync")
}

/// Lowers `flock(stream, operation, would_block?)` through the libc flock wrapper.
pub(crate) fn lower_flock(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "flock", 2, 3)?;
    let stream = expect_operand(inst, 0)?;
    let operation = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "flock")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int(ctx.load_value_to_result(operation)?.codegen_repr(), "flock operation")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the lock operation to the flock runtime helper
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdx, rax");                            // pass the lock operation to the flock runtime helper
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    let wrapper_label = ctx.next_label("flock_user_wrapper");
    let done_label = ctx.next_label("flock_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("b.ge {}", wrapper_label));        // dispatch synthetic handles to stream_lock
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rax, r9");                             // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("jge {}", wrapper_label));         // dispatch synthetic handles to stream_lock
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_flock");
    if inst.operands.len() == 3 {
        let would_block = expect_operand(inst, 2)?;
        let Some(slot) = source_load_local_slot(ctx, would_block)? else {
            return Err(CodegenIrError::unsupported(
                "flock would_block output for non-local arguments",
            ));
        };
        store_flock_would_block(ctx, slot)?;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip wrapper locking after the native helper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip wrapper locking after the native helper
        }
    }
    ctx.emitter.label(&wrapper_label);
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the synthetic wrapper descriptor to the lock helper
        ctx.emitter.instruction("mov rsi, rdx");                                // pass the lock operation to the wrapper method
    }
    abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_flock");
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

