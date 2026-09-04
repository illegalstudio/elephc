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
    // A read the wrapper REFUSED is not a short read: php answers -1, not the byte count so far.
    let refused_label = ctx.next_label("fpt_refused");
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
            // The OPAQUE HANDLE, not the fd. `__rt_fread`/`__rt_feof` take a handle and resolve
            // the descriptor themselves; passing the fd worked only because that resolution maps a
            // synthetic fd to itself, and it hid the stream STATE — which is where php keeps the
            // bytes a previous read left behind.
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // the opaque handle drives the loop
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            ctx.emitter.label(&drain_label);
            ctx.emitter.instruction("sub sp, sp, #32");                         // reserve fd, byte total, and chunk scratch storage
            ctx.emitter.instruction("str x0, [sp, #0]");                        // preserve the read handle
            ctx.emitter.instruction("str xzr, [sp, #8]");                       // initialize copied byte total to zero
            ctx.emitter.label(&loop_label);
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the read handle for EOF probing
            // fpassthru's OWN probe, not one the program wrote: it must not warn about a missing
            // stream_eof. php reads first and lets the read refuse, naming fpassthru().
            crate::codegen_support::runtime::io::emit_feof_call(ctx.emitter, true);
            ctx.emitter.instruction(&format!("cbnz x0, {}", wrapper_done_label)); // stop streaming when stream_eof reports EOF
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the read handle for reading
            ctx.emitter.instruction("mov x1, #4096");                           // request a bounded read chunk
            abi::emit_call_label(ctx.emitter, "__rt_fread");
            ctx.emitter.instruction(&format!("cbz x0, {}", refused_label));     // the read refused: php answers -1
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
            ctx.emitter.instruction(&format!("b {}", wrapper_done_label));
            ctx.emitter.label(&refused_label);
            ctx.emitter.instruction("mov x9, #-1");                             // php's fpassthru failure answer
            ctx.emitter.instruction("str x9, [sp, #8]");                        // replaces the total, however much was passed
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
            // See the AArch64 counterpart: the loop is driven by the opaque handle, not the fd.
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // the opaque handle drives the loop
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            ctx.emitter.label(&drain_label);
            ctx.emitter.instruction("sub rsp, 32");                             // reserve fd, byte total, and chunk scratch storage
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the read handle
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], 0");              // initialize copied byte total to zero
            ctx.emitter.label(&loop_label);
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the read handle for EOF probing
            // See the AArch64 arm: fpassthru's own probe stays silent.
            crate::codegen_support::runtime::io::emit_feof_call(ctx.emitter, true);
            ctx.emitter.instruction("test rax, rax");                           // test whether stream_eof reported EOF
            ctx.emitter.instruction(&format!("jnz {}", wrapper_done_label));    // stop streaming when stream_eof reports EOF
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the read handle for reading
            ctx.emitter.instruction("mov rsi, 4096");                           // request a bounded read chunk
            abi::emit_call_label(ctx.emitter, "__rt_fread");
            ctx.emitter.instruction("test rcx, rcx");                           // the read's verdict travels beside the pair
            ctx.emitter.instruction(&format!("jz {}", refused_label));          // the read refused: php answers -1
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
            ctx.emitter.instruction(&format!("jmp {}", wrapper_done_label));
            ctx.emitter.label(&refused_label);
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], -1");             // php's fpassthru failure answer
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
    crate::codegen_support::runtime::io::emit_feof_call(ctx.emitter, false); // the PROGRAM asked
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
    let seekable_label = ctx.next_label("ftell_seekable");
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
            let plat = ctx.emitter.platform;
            if plat.needs_cmp_before_error_branch() {
                ctx.emitter.instruction("cmp x0, #0");                          // Linux: a negative result means lseek failed
            }
            ctx.emitter
                .instruction(&plat.branch_on_syscall_success(&seekable_label));
            // NOT SEEKABLE. The probe's answer is an ERRNO, and on macOS it arrives in the same
            // register a real offset would, so `ftell()` on a socket answered 29 — ESPIPE read as
            // a byte count. php keeps a logical position for such a stream and reports that: the
            // bytes that have crossed the handle in either direction. Measured on `php -n` 8.5.6,
            // a socket pair reads 0 fresh, 5 after a five-byte write, 11 after six more, and stays
            // 11 across a failed `fseek`; the reading end counts what it reads, 3 then 5 then 11.
            //
            // That count already exists: `emit_advance_wrapper_position` runs on every `fread`,
            // `fgets`, `fgetc`, `fwrite` and `stream_get_contents` for EVERY stream with state,
            // not just a wrapper — its own doc notes the field is simply never read for the rest.
            // This is the reader that gives it a second use.
            load_stream_handle_to_result(ctx, stream, "ftell")?;
            abi::emit_call_label(ctx.emitter, "__rt_stream_wrapper_pos");
            ctx.emitter.instruction(&format!("b {}", after_dispatch_label));
            ctx.emitter.label(&seekable_label);
            // Only the DESCRIPTOR runs ahead of php's position. A wrapper's own tracked position
            // already reports what was handed to the caller, so subtracting there would count
            // the read-ahead twice.
            emit_subtract_pending_held(ctx, stream)?;
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
            // libc reports the failure as -1 rather than leaking the errno, but the answer is the
            // same one the AArch64 counterpart explains: a stream `lseek` refuses is one php keeps
            // a logical position for.
            ctx.emitter.instruction("cmp rax, -1");
            ctx.emitter
                .instruction(&format!("jne {}", seekable_label));
            load_stream_handle_to_result(ctx, stream, "ftell")?;
            ctx.emitter.instruction("mov rdi, rax");                            // the handle owns the tracked position
            abi::emit_call_label(ctx.emitter, "__rt_stream_wrapper_pos");
            ctx.emitter.instruction(&format!("jmp {}", after_dispatch_label));
            ctx.emitter.label(&seekable_label);
            // See the AArch64 counterpart: only the descriptor runs ahead of php's position.
            emit_subtract_pending_held(ctx, stream)?;
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

/// Subtracts the bytes a read pulled AHEAD of what the caller was handed.
///
/// php reads a whole chunk and serves the request out of it, so the descriptor sits past the
/// position php reports. `ftell()` must report what the program has consumed, not where the
/// descriptor stopped: on a 192-byte file, `fread($h, 5)` leaves the descriptor at 192 and php
/// answers 5. The helper answers 0 for every stream holding nothing, which is every stream that
/// has not read ahead — the same shape as the append-skip correction above.
fn emit_subtract_pending_held(ctx: &mut FunctionContext<'_>, stream: ValueId) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");                              // the position computed so far
            load_stream_handle_to_result(ctx, stream, "ftell")?;
            abi::emit_call_label(ctx.emitter, "__rt_stream_pending_held");
            ctx.emitter.instruction("mov x1, x0");                              // what the read pulled ahead
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("sub x0, x0, x1");                          // PHP's position
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");                             // the position computed so far
            load_stream_handle_to_result(ctx, stream, "ftell")?;
            ctx.emitter.instruction("mov rdi, rax");
            abi::emit_call_label(ctx.emitter, "__rt_stream_pending_held");
            ctx.emitter.instruction("mov rcx, rax");                            // what the read pulled ahead
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
            // A seek also invalidates the stream's READ BUFFER: what it holds came from wherever
            // the last read stopped, and the caller has just asked to continue somewhere else.
            // Without this, a chunk read ahead of `fseek($h, 3)` was served after it — valid
            // bytes, for a position the program had left.
            load_stream_handle_to_result(ctx, stream, name)?;
            abi::emit_call_label(ctx.emitter, "__rt_stream_pending_clear");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");                             // the seek result the caller is owed
            load_stream_handle_to_result(ctx, stream, name)?;
            ctx.emitter.instruction("mov rdi, rax");
            abi::emit_call_label(ctx.emitter, "__rt_stream_clear_append_skip");
            // See the AArch64 counterpart: the read buffer is stale after a seek.
            load_stream_handle_to_result(ctx, stream, name)?;
            ctx.emitter.instruction("mov rdi, rax");
            abi::emit_call_label(ctx.emitter, "__rt_stream_pending_clear");
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    Ok(())
}

/// Lowers `fseek(stream, offset, whence?)` and clears EOF state on success.
/// Runs php's write-filter-chain flush, the one that is NOT the close.
///
/// MEASURED on `php -n` 8.5.6 against a filter that echoes its own calls: `fflush()`, `rewind()`
/// and `fseek()` each add one `filter(closing = false)` with an EMPTY brigade, and `ftell()` and
/// `feof()` add none. elephc made the call on none of the three, so a filter that accumulates
/// until it is asked kept its bytes through every flush point php offers it.
///
/// Loads the handle itself and leaves the result register holding it again, so a caller can drop
/// this line in wherever the handle is already what it wants.
fn emit_write_chain_flush(
    ctx: &mut FunctionContext<'_>,
    stream: crate::ir::ValueId,
    caller: &str,
) -> Result<()> {
    load_stream_handle_to_result(ctx, stream, caller)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // the handle the chain walk resolves
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_write_chain_flush");
    Ok(())
}

pub(crate) fn lower_fseek(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fseek", 2, 3)?;
    let stream = expect_operand(inst, 0)?;
    let offset = expect_operand(inst, 1)?;
    emit_write_chain_flush(ctx, stream, "fseek")?;
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
    emit_write_chain_flush(ctx, stream, "rewind")?;
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
    // NO buffer clear here, deliberately: php KEEPS its read buffer across a truncation. MEASURED
    // on `php -n` 8.5.6 — after `fread($h, 4)` then `ftruncate($h, 5)`, the next `fread($h, 4)`
    // hands back `"efgh"`, bytes the truncation had already removed, and `feof()` stays false.
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
    // php's `fflush()` CLEARS the flush debt, so the close that follows does not flush again:
    // MEASURED on `php -n` 8.5.6, `write; fflush; close` calls `stream_flush()` ONCE, while
    // `write; fflush; write; close` calls it twice. The debt lives on the StreamState, which only
    // the HANDLE reaches — the descriptor loaded below cannot.
    emit_write_chain_flush(ctx, stream, "fflush")?;
    load_stream_handle_to_result(ctx, stream, "fflush")?;
    let debt_cleared = ctx.next_label("fflush_debt_cleared");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_call_label(ctx.emitter, "__rt_stream_state");
            ctx.emitter.instruction(&format!("cbz x0, {}", debt_cleared));
            ctx.emitter.instruction(&format!(
                "str xzr, [x0, #{}]",
                crate::codegen_support::runtime::resources::layout::STREAM_WRITTEN_SINCE_FLUSH_OFFSET
            ));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");
            abi::emit_call_label(ctx.emitter, "__rt_stream_state");
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction(&format!("jz {}", debt_cleared));
            ctx.emitter.instruction(&format!(
                "mov QWORD PTR [rax + {}], 0",
                crate::codegen_support::runtime::resources::layout::STREAM_WRITTEN_SINCE_FLUSH_OFFSET
            ));
        }
    }
    ctx.emitter.label(&debt_cleared);
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
///
/// php reaches `flock()` through `php_stream_lock`, which only a stream whose ops carry a locking
/// one answers — so `php://memory`, `php://temp`, `php://output`, `php://input` and `data:` are
/// all `false`, MEASURED on `php -n` 8.5.6. elephc backs the first two with `tmpfile()`, a REAL
/// descriptor that locks, so `flock()` answered true for every one of them. The wrapper is asked
/// first, before the descriptor is even resolved — the same order `ftell()` uses above.
///
/// The question asked is `stream_supports_lock()`'s own, not the narrower wrapper-only one this
/// used to ask: php decides both from the same place — the stream's ops — so the two cannot
/// disagree, and the narrow one did not know about SOCKETS. A socket carries
/// `php_stream_socket_ops`, which has no `set_option` at all, and no wrapper id can tell one from
/// a file because `stream_socket_pair()` and `fopen()` are recorded identically. On macOS
/// `flock(2)` refuses a socket by itself and the gap was invisible; on Linux it SUCCEEDS, so
/// `@flock($pair[0], LOCK_EX)` answered true where php answers false — caught by CI's
/// linux-aarch64 shard, which is the only place the two platforms differ here.
pub(crate) fn lower_flock(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "flock", 2, 3)?;
    let stream = expect_operand(inst, 0)?;
    let operation = expect_operand(inst, 1)?;
    let unlockable_label = ctx.next_label("flock_wrapper_cannot_lock");
    load_stream_handle_to_result(ctx, stream, "flock")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // the handle owns the wrapper identity
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_supports_lock");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cbz x0, {}", unlockable_label));         // php has no lock op to reach here
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // php has no lock op to reach here
            ctx.emitter
                .instruction(&format!("jz {}", unlockable_label));
        }
    }
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
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("b {}", done_label)), // skip the wrapper refusal below
        Arch::X86_64 => ctx.emitter.instruction(&format!("jmp {}", done_label)),
    }
    ctx.emitter.label(&unlockable_label);
    // php still writes the out-parameter while refusing: `flock($memory, LOCK_EX, $b)` answers
    // false and leaves `$b` as int 0, not null. MEASURED on `php -n` 8.5.6.
    if inst.operands.len() == 3 {
        let would_block = expect_operand(inst, 2)?;
        let Some(slot) = source_load_local_slot(ctx, would_block)? else {
            return Err(CodegenIrError::unsupported(
                "flock would_block output for non-local arguments",
            ));
        };
        let (verdict_reg, block_reg) = match ctx.emitter.target.arch {
            Arch::AArch64 => ("x0", "x1"),
            Arch::X86_64 => ("rax", "rdx"),
        };
        abi::emit_load_int_immediate(ctx.emitter, verdict_reg, 0);              // the verdict this path reports
        abi::emit_load_int_immediate(ctx.emitter, block_reg, 0);                // nothing blocked: no lock was attempted
        store_flock_would_block(ctx, slot)?;
    }
    emit_bool_result(ctx, false);                                               // php answers false, and warns about nothing
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

