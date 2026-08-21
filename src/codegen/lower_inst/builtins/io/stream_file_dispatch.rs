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
pub(super) fn emit_fpassthru_dispatch(ctx: &mut FunctionContext<'_>) {
    let wrapper_label = ctx.next_label("fpt_wrapper");
    let loop_label = ctx.next_label("fpt_loop");
    let release_eof_label = ctx.next_label("fpt_release_eof");
    let wrapper_done_label = ctx.next_label("fpt_done");
    let done_label = ctx.next_label("fpt_after");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("b.ge {}", wrapper_label));        // stream wrapper handles through the userspace read loop
            abi::emit_call_label(ctx.emitter, "__rt_fpassthru");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the wrapper read loop after native streaming
            ctx.emitter.label(&wrapper_label);
            ctx.emitter.instruction("sub sp, sp, #32");                         // reserve fd, byte total, and chunk scratch storage
            ctx.emitter.instruction("str x0, [sp, #0]");                        // preserve the synthetic wrapper fd
            ctx.emitter.instruction("str xzr, [sp, #8]");                       // initialize copied byte total to zero
            ctx.emitter.label(&loop_label);
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the wrapper fd for reading
            ctx.emitter.instruction("mov x1, #4096");                           // request a bounded wrapper read chunk
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
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rax, r9");                             // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("jge {}", wrapper_label));         // stream wrapper handles through the userspace read loop
            ctx.emitter.instruction("mov rdi, rax");                            // pass the native fd to fpassthru
            abi::emit_call_label(ctx.emitter, "__rt_fpassthru");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the wrapper read loop after native streaming
            ctx.emitter.label(&wrapper_label);
            ctx.emitter.instruction("sub rsp, 32");                             // reserve fd, byte total, and chunk scratch storage
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the synthetic wrapper fd
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], 0");              // initialize copied byte total to zero
            ctx.emitter.label(&loop_label);
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the wrapper fd for reading
            ctx.emitter.instruction("mov rsi, 4096");                           // request a bounded wrapper read chunk
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
    load_stream_fd_to_result(ctx, stream, "feof")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the stream fd to the x86_64 feof runtime helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_feof");
    store_if_result(ctx, inst)
}

/// Lowers `ftell(stream)` as `lseek(fd, 0, SEEK_CUR)`.
pub(crate) fn lower_ftell(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "ftell", 1)?;
    let stream = expect_operand(inst, 0)?;
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
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_ftell");
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
            ctx.emitter.instruction("mov rdi, rax");                            // pass the synthetic wrapper descriptor to the tell helper
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_ftell");
            ctx.emitter.label(&after_dispatch_label);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `fseek(stream, offset, whence?)` and clears EOF state on success.
pub(crate) fn lower_fseek(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fseek", 2, 3)?;
    let stream = expect_operand(inst, 0)?;
    let offset = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "fseek")?;
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
    store_if_result(ctx, inst)
}

/// Lowers `rewind(stream)` as `lseek(fd, 0, SEEK_SET)` and clears EOF state on success.
pub(crate) fn lower_rewind(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "rewind", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "rewind")?;
    let success_label = ctx.next_label("rewind_success");
    let done_label = ctx.next_label("rewind_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_rewind_aarch64(ctx, &success_label, &done_label),
        Arch::X86_64 => lower_rewind_x86_64(ctx, &success_label, &done_label),
    }
    store_if_result(ctx, inst)
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
