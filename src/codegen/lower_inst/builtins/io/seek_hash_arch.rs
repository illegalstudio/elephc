//! Purpose:
//! Target-specific seek, rewind, file write, and hash_file ABI lowering.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Emits the ARM64 `fseek()` syscall path after fd, offset, and whence are staged.
pub(super) fn lower_fseek_aarch64(
    ctx: &mut FunctionContext<'_>,
    success_label: &str,
    done_label: &str,
) {
    let wrapper_label = ctx.next_label("fseek_user_wrapper");
    let after_dispatch_label = ctx.next_label("fseek_after_dispatch");
    ctx.emitter.instruction("mov x2, x0");                                      // move whence into the third lseek syscall argument
    abi::emit_pop_reg(ctx.emitter, "x1");
    abi::emit_pop_reg(ctx.emitter, "x0");
    ctx.emitter.instruction("mov w9, #0x4000");                                 // materialize the high half of USER_WRAPPER_FD_BASE
    ctx.emitter.instruction("lsl w9, w9, #16");                                 // form the synthetic wrapper fd base 0x40000000
    ctx.emitter.instruction("cmp x0, x9");                                      // test whether this stream is a userspace-wrapper handle
    ctx.emitter.instruction(&format!("b.ge {}", wrapper_label));                // dispatch synthetic handles to stream_seek
    let allow = ctx.next_label("fseek_seek_mode_allow");
    let never = ctx.next_label("fseek_seek_mode_never");
    let only_start = ctx.next_label("fseek_seek_mode_start");
    abi::emit_branch_if_fd_table_index_out_of_range(ctx.emitter, "x0", &allow);
    abi::emit_symbol_address(ctx.emitter, "x9", "_stream_write_seek_modes");
    ctx.emitter.instruction("ldrb w10, [x9, x0]");                              // load this descriptor's write-seek mode
    ctx.emitter.instruction(&format!("cbz w10, {allow}"));                      // preserve: the seek is allowed
    ctx.emitter.instruction("cmp w10, #2");                                     // strict mode?
    ctx.emitter.instruction(&format!("b.eq {never}"));                          // strict: every write-chain seek fails
    ctx.emitter.instruction(&format!("cbnz x1, {only_start}"));                 // reset: offset 0 is required
    ctx.emitter.instruction(&format!("cbnz x2, {only_start}"));                 // reset: SEEK_SET is required
    ctx.emitter.instruction(&format!("b {allow}"));                             // reset to the start position: allowed
    ctx.emitter.label(&never);
    emit_fseek_seek_warning_arm64(ctx, "fseek()", " is never seekable", done_label, -1);
    ctx.emitter.label(&only_start);
    emit_fseek_seek_warning_arm64(ctx, "fseek()", " is seekable only to start position", done_label, -1);
    ctx.emitter.label(&allow);
    // php-src resets the filter state on every allowed seek (zlib.deflate deflateReset,
    // bzip2.compress reinitialization). Only descriptors with a registered write filter
    // carry a label pointer.
    let reset_skip = ctx.next_label("fseek_seek_reset_skip");
    let reset_zlib_skip = ctx.next_label("fseek_seek_reset_zlib_skip");
    let reset_bz2_skip = ctx.next_label("fseek_seek_reset_bz2_skip");
    abi::emit_symbol_address(ctx.emitter, "x9", "_stream_write_seek_label_ptrs");
    ctx.emitter.instruction("ldr x10, [x9, x0, lsl #3]");                       // is a write filter registered for this descriptor?
    ctx.emitter.instruction(&format!("cbz x10, {reset_skip}"));                 // no filter: keep the plain seek
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    abi::emit_store_to_sp(ctx.emitter, "x0", 0);
    abi::emit_store_to_sp(ctx.emitter, "x1", 8);
    abi::emit_store_to_sp(ctx.emitter, "x2", 16);
    abi::emit_symbol_address(ctx.emitter, "x9", "_zlib_reset_fn");
    ctx.emitter.instruction("ldr x10, [x9]");                                   // published zlib.deflate reset hook
    ctx.emitter.instruction(&format!("cbz x10, {reset_zlib_skip}"));            // no zlib filter anywhere: skip
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 0);
    ctx.emitter.instruction("blr x10");                                         // deflateReset for this descriptor
    ctx.emitter.label(&reset_zlib_skip);
    abi::emit_symbol_address(ctx.emitter, "x9", "_bz2_reset_fn");
    ctx.emitter.instruction("ldr x10, [x9]");                                   // published bzip2.compress reset hook
    ctx.emitter.instruction(&format!("cbz x10, {reset_bz2_skip}"));             // no bzip2 filter anywhere: skip
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 0);
    ctx.emitter.instruction("blr x10");                                         // reinitialize the bzip2 stream
    ctx.emitter.label(&reset_bz2_skip);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 8);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", 16);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.label(&reset_skip);
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.emitter.syscall(199);
    if ctx.emitter.platform.needs_cmp_before_error_branch() {
        ctx.emitter.instruction("cmp x0, #0");                                  // Linux reports lseek failure as a negative result
    }
    ctx.emitter.instruction(
        &ctx.emitter.platform.branch_on_syscall_success(success_label)
    );                                                                          // continue only when lseek succeeds
    abi::emit_pop_reg(ctx.emitter, "x9");
    ctx.emitter.instruction("mov x0, #-1");                                     // fseek returns -1 when lseek fails
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip EOF reset after a failed seek
    ctx.emitter.label(success_label);
    abi::emit_pop_reg(ctx.emitter, "x9");
    abi::emit_symbol_address(ctx.emitter, "x10", "_eof_flags");
    ctx.emitter.instruction("strb wzr, [x10, x9]");                             // clear EOF state for the successfully repositioned stream
    ctx.emitter.instruction("mov x0, #0");                                      // fseek returns 0 after a successful seek
    ctx.emitter.label(done_label);
    ctx.emitter.instruction(&format!("b {}", after_dispatch_label));            // skip wrapper stream_seek after the native path
    ctx.emitter.label(&wrapper_label);
    abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fseek");
    ctx.emitter.label(&after_dispatch_label);
}

/// Emits the Linux x86_64 `fseek()` libc path after fd, offset, and whence are staged.
pub(super) fn lower_fseek_x86_64(
    ctx: &mut FunctionContext<'_>,
    success_label: &str,
    done_label: &str,
) {
    let wrapper_label = ctx.next_label("fseek_user_wrapper");
    let after_dispatch_label = ctx.next_label("fseek_after_dispatch");
    ctx.emitter.instruction("mov rdx, rax");                                    // move whence into the third lseek argument
    abi::emit_pop_reg(ctx.emitter, "rsi");
    abi::emit_pop_reg(ctx.emitter, "rdi");
    ctx.emitter.instruction("mov r9d, 0x40000000");                             // materialize USER_WRAPPER_FD_BASE for synthetic handles
    ctx.emitter.instruction("cmp rdi, r9");                                     // test whether this stream is a userspace-wrapper handle
    ctx.emitter.instruction(&format!("jge {}", wrapper_label));                 // dispatch synthetic handles to stream_seek
    let allow = ctx.next_label("fseek_seek_mode_allow");
    let never = ctx.next_label("fseek_seek_mode_never");
    let only_start = ctx.next_label("fseek_seek_mode_start");
    abi::emit_branch_if_fd_table_index_out_of_range(ctx.emitter, "rdi", &allow);
    abi::emit_symbol_address(ctx.emitter, "r9", "_stream_write_seek_modes");
    ctx.emitter.instruction("movzx r10d, BYTE PTR [r9 + rdi]");                 // load this descriptor's write-seek mode
    ctx.emitter.instruction(&format!("test r10d, r10d"));                       // preserve mode?
    ctx.emitter.instruction(&format!("jz {allow}"));                            // preserve: the seek is allowed
    ctx.emitter.instruction("cmp r10d, 2");                                     // strict mode?
    ctx.emitter.instruction(&format!("je {never}"));                            // strict: every write-chain seek fails
    ctx.emitter.instruction(&format!("test rsi, rsi"));                         // reset: offset 0 is required
    ctx.emitter.instruction(&format!("jnz {only_start}"));                      // reset: non-start offset fails
    ctx.emitter.instruction(&format!("test rdx, rdx"));                         // reset: SEEK_SET is required
    ctx.emitter.instruction(&format!("jnz {only_start}"));                      // reset: non-SEEK_SET fails
    ctx.emitter.instruction(&format!("jmp {allow}"));                           // reset to the start position: allowed
    ctx.emitter.label(&never);
    emit_fseek_seek_warning_x86_64(ctx, "fseek()", " is never seekable", done_label, -1);
    ctx.emitter.label(&only_start);
    emit_fseek_seek_warning_x86_64(ctx, "fseek()", " is seekable only to start position", done_label, -1);
    ctx.emitter.label(&allow);
    let reset_skip = ctx.next_label("fseek_seek_reset_skip");
    let reset_zlib_skip = ctx.next_label("fseek_seek_reset_zlib_skip");
    let reset_bz2_skip = ctx.next_label("fseek_seek_reset_bz2_skip");
    abi::emit_symbol_address(ctx.emitter, "r9", "_stream_write_seek_label_ptrs");
    ctx.emitter.instruction("mov r10, QWORD PTR [r9 + rdi*8]");                 // is a write filter registered for this descriptor?
    ctx.emitter.instruction(&format!("test r10, r10"));
    ctx.emitter.instruction(&format!("jz {reset_skip}"));                       // no filter: keep the plain seek
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    abi::emit_store_to_sp(ctx.emitter, "rdi", 0);
    abi::emit_store_to_sp(ctx.emitter, "rsi", 8);
    abi::emit_store_to_sp(ctx.emitter, "rdx", 16);
    abi::emit_symbol_address(ctx.emitter, "r9", "_zlib_reset_fn");
    ctx.emitter.instruction("mov r10, QWORD PTR [r9]");                         // published zlib.deflate reset hook
    ctx.emitter.instruction(&format!("test r10, r10"));
    ctx.emitter.instruction(&format!("jz {reset_zlib_skip}"));                  // no zlib filter anywhere: skip
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
    ctx.emitter.instruction("call r10");                                        // deflateReset for this descriptor
    ctx.emitter.label(&reset_zlib_skip);
    abi::emit_symbol_address(ctx.emitter, "r9", "_bz2_reset_fn");
    ctx.emitter.instruction("mov r10, QWORD PTR [r9]");                         // published bzip2.compress reset hook
    ctx.emitter.instruction(&format!("test r10, r10"));
    ctx.emitter.instruction(&format!("jz {reset_bz2_skip}"));                   // no bzip2 filter anywhere: skip
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
    ctx.emitter.instruction("call r10");                                        // reinitialize the bzip2 stream
    ctx.emitter.label(&reset_bz2_skip);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 8);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", 16);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.label(&reset_skip);
    abi::emit_push_reg(ctx.emitter, "rdi");
    ctx.emitter.instruction("call lseek");                                      // reposition the stream through libc lseek()
    ctx.emitter.instruction("cmp rax, 0");                                      // test whether lseek returned a non-negative offset
    ctx.emitter.instruction(&format!("jge {}", success_label));                 // continue only when lseek succeeds
    abi::emit_pop_reg(ctx.emitter, "r10");
    ctx.emitter.instruction("mov rax, -1");                                     // fseek returns -1 when lseek fails
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip EOF reset after a failed seek
    ctx.emitter.label(success_label);
    abi::emit_pop_reg(ctx.emitter, "r10");
    ctx.emitter.instruction("lea r11, [rip + _eof_flags]");                     // materialize the EOF-flag table base
    ctx.emitter.instruction("mov BYTE PTR [r11 + r10], 0");                     // clear EOF state for the successfully repositioned stream
    ctx.emitter.instruction("xor eax, eax");                                    // fseek returns 0 after a successful seek
    ctx.emitter.label(done_label);
    ctx.emitter.instruction(&format!("jmp {}", after_dispatch_label));          // skip wrapper stream_seek after the native path
    ctx.emitter.label(&wrapper_label);
    abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fseek");
    ctx.emitter.label(&after_dispatch_label);
}

/// Emits the ARM64 `rewind()` syscall path and boolean result.
pub(super) fn lower_rewind_aarch64(
    ctx: &mut FunctionContext<'_>,
    success_label: &str,
    done_label: &str,
) {
    let wrapper_label = ctx.next_label("rewind_user_wrapper");
    let after_dispatch_label = ctx.next_label("rewind_after_dispatch");
    ctx.emitter.instruction("mov w9, #0x4000");                                 // materialize the high half of USER_WRAPPER_FD_BASE
    ctx.emitter.instruction("lsl w9, w9, #16");                                 // form the synthetic wrapper fd base 0x40000000
    ctx.emitter.instruction("cmp x0, x9");                                      // test whether this stream is a userspace-wrapper handle
    ctx.emitter.instruction(&format!("b.ge {}", wrapper_label));                // dispatch synthetic handles to stream_seek
    let allow = ctx.next_label("rewind_seek_mode_allow");
    abi::emit_branch_if_fd_table_index_out_of_range(ctx.emitter, "x0", &allow);
    abi::emit_symbol_address(ctx.emitter, "x9", "_stream_write_seek_modes");
    ctx.emitter.instruction("ldrb w10, [x9, x0]");                              // load this descriptor's write-seek mode
    ctx.emitter.instruction("cmp w10, #2");                                     // strict mode?
    ctx.emitter.instruction(&format!("b.ne {allow}"));                          // preserve/reset: rewind to the start is allowed
    emit_fseek_seek_warning_arm64(ctx, "rewind()", " is never seekable", done_label, 0);
    ctx.emitter.label(&allow);
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.emitter.instruction("mov x1, #0");                                      // use offset 0 for rewind
    ctx.emitter.instruction("mov x2, #0");                                      // use SEEK_SET for rewind
    ctx.emitter.syscall(199);
    if ctx.emitter.platform.needs_cmp_before_error_branch() {
        ctx.emitter.instruction("cmp x0, #0");                                  // Linux reports lseek failure as a negative result
    }
    ctx.emitter.instruction(
        &ctx.emitter.platform.branch_on_syscall_success(success_label)
    );                                                                          // continue only when rewind succeeds
    abi::emit_pop_reg(ctx.emitter, "x9");
    ctx.emitter.instruction("mov x0, #0");                                      // rewind returns false when lseek fails
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip EOF reset after a failed rewind
    ctx.emitter.label(success_label);
    abi::emit_pop_reg(ctx.emitter, "x9");
    abi::emit_symbol_address(ctx.emitter, "x10", "_eof_flags");
    ctx.emitter.instruction("strb wzr, [x10, x9]");                             // clear EOF state after rewinding the stream
    ctx.emitter.instruction("mov x0, #1");                                      // rewind returns true after a successful seek
    ctx.emitter.label(done_label);
    ctx.emitter.instruction(&format!("b {}", after_dispatch_label));            // skip wrapper stream_seek after the native path
    ctx.emitter.label(&wrapper_label);
    ctx.emitter.instruction("mov x1, #0");                                      // pass offset 0 to wrapper stream_seek
    ctx.emitter.instruction("mov x2, #0");                                      // pass SEEK_SET to wrapper stream_seek
    abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fseek");
    ctx.emitter.instruction("cmp x0, #0");                                      // wrapper fseek returns zero on success
    ctx.emitter.instruction("cset x0, eq");                                     // rewind returns true only when wrapper seek succeeded
    ctx.emitter.label(&after_dispatch_label);
}

/// Emits the Linux x86_64 `rewind()` libc path and boolean result.
pub(super) fn lower_rewind_x86_64(
    ctx: &mut FunctionContext<'_>,
    success_label: &str,
    done_label: &str,
) {
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the stream fd to libc lseek()
    let wrapper_label = ctx.next_label("rewind_user_wrapper");
    let after_dispatch_label = ctx.next_label("rewind_after_dispatch");
    ctx.emitter.instruction("mov r9d, 0x40000000");                             // materialize USER_WRAPPER_FD_BASE for synthetic handles
    ctx.emitter.instruction("cmp rdi, r9");                                     // test whether this stream is a userspace-wrapper handle
    ctx.emitter.instruction(&format!("jge {}", wrapper_label));                 // dispatch synthetic handles to stream_seek
    let allow = ctx.next_label("rewind_seek_mode_allow");
    abi::emit_branch_if_fd_table_index_out_of_range(ctx.emitter, "rdi", &allow);
    abi::emit_symbol_address(ctx.emitter, "r9", "_stream_write_seek_modes");
    ctx.emitter.instruction("movzx r10d, BYTE PTR [r9 + rdi]");                 // load this descriptor's write-seek mode
    ctx.emitter.instruction("cmp r10d, 2");                                     // strict mode?
    ctx.emitter.instruction(&format!("jne {allow}"));                           // preserve/reset: rewind to the start is allowed
    emit_fseek_seek_warning_x86_64(ctx, "rewind()", " is never seekable", done_label, 0);
    ctx.emitter.label(&allow);
    abi::emit_push_reg(ctx.emitter, "rdi");
    ctx.emitter.instruction("xor esi, esi");                                    // use offset 0 for rewind
    ctx.emitter.instruction("xor edx, edx");                                    // use SEEK_SET for rewind
    ctx.emitter.instruction("call lseek");                                      // rewind the stream through libc lseek()
    ctx.emitter.instruction("cmp rax, 0");                                      // test whether lseek returned a non-negative offset
    ctx.emitter.instruction(&format!("jge {}", success_label));                 // continue only when rewind succeeds
    abi::emit_pop_reg(ctx.emitter, "r10");
    ctx.emitter.instruction("xor eax, eax");                                    // rewind returns false when lseek fails
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip EOF reset after a failed rewind
    ctx.emitter.label(success_label);
    abi::emit_pop_reg(ctx.emitter, "r10");
    ctx.emitter.instruction("lea r11, [rip + _eof_flags]");                     // materialize the EOF-flag table base
    ctx.emitter.instruction("mov BYTE PTR [r11 + r10], 0");                     // clear EOF state after rewinding the stream
    ctx.emitter.instruction("mov rax, 1");                                      // rewind returns true after a successful seek
    ctx.emitter.label(done_label);
    ctx.emitter.instruction(&format!("jmp {}", after_dispatch_label));          // skip wrapper stream_seek after the native path
    ctx.emitter.label(&wrapper_label);
    ctx.emitter.instruction("xor esi, esi");                                    // pass offset 0 to wrapper stream_seek
    ctx.emitter.instruction("xor edx, edx");                                    // pass SEEK_SET to wrapper stream_seek
    abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fseek");
    ctx.emitter.instruction("cmp rax, 0");                                      // wrapper fseek returns zero on success
    ctx.emitter.instruction("sete al");                                         // mark wrapper seek success as true
    ctx.emitter.instruction("movzx eax, al");                                   // widen rewind bool result
    ctx.emitter.label(&after_dispatch_label);
}

/// Materializes `file_put_contents` arguments for the ARM64 runtime ABI.
pub(super) fn lower_file_put_contents_arm64(
    ctx: &mut FunctionContext<'_>,
    path: ValueId,
    data: ValueId,
    flags: Option<ValueId>,
    helper: &str,
) -> Result<()> {
    // `$flags` is resolved FIRST: an unboxing cast clobbers the argument registers, so it
    // cannot run once the two strings are staged.
    if let Some(flags) = flags {
        resolve_int_operand_to_result(ctx, flags, "file_put_contents flags")?;
        abi::emit_push_reg(ctx.emitter, "x0");
    }
    load_string_to_result(ctx, path, "file_put_contents filename")?;
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
    load_string_to_result(ctx, data, "file_put_contents data")?;
    ctx.emitter.instruction("mov x3, x1");                                      // pass the data pointer in the runtime helper's second string slot
    ctx.emitter.instruction("mov x4, x2");                                      // pass the data length in the runtime helper's second string slot
    abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
    if flags.is_some() {
        abi::emit_pop_reg(ctx.emitter, "x5");                                   // reload the resolved PHP flags as the helper's fifth argument
    }
    abi::emit_call_label(ctx.emitter, helper);
    Ok(())
}

/// Materializes `file_put_contents` arguments for the Linux x86_64 runtime ABI.
pub(super) fn lower_file_put_contents_x86_64(
    ctx: &mut FunctionContext<'_>,
    path: ValueId,
    data: ValueId,
    flags: Option<ValueId>,
    helper: &str,
) -> Result<()> {
    // `$flags` is resolved FIRST: an unboxing cast clobbers the argument registers, so it
    // cannot run once the two strings are staged.
    if let Some(flags) = flags {
        resolve_int_operand_to_result(ctx, flags, "file_put_contents flags")?;
        abi::emit_push_reg(ctx.emitter, "rax");
    }
    load_string_to_result(ctx, path, "file_put_contents filename")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_string_to_result(ctx, data, "file_put_contents data")?;
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the data pointer while the filename remains on the temporary stack
    ctx.emitter.instruction("mov rsi, rdx");                                    // pass the data length while the filename remains on the temporary stack
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    if flags.is_some() {
        abi::emit_pop_reg(ctx.emitter, "r8");                                   // reload the resolved PHP flags as the helper's flags argument
    }
    abi::emit_call_label(ctx.emitter, helper);
    Ok(())
}

/// Materializes and hashes `hash_file()` arguments on AArch64.
pub(super) fn lower_hash_file_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    fail: &str,
    done: &str,
) -> Result<()> {
    super::super::strings::load_string_arg_to_regs(ctx, inst, 0, "hash_file", "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the algorithm string while materializing the filename
    super::super::strings::load_string_arg_to_regs(ctx, inst, 1, "hash_file", "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the filename string while materializing the binary flag
    super::super::strings::materialize_truthy_flag(ctx, inst, 2, "hash_file")?;
    ctx.emitter.instruction("str x0, [sp, #-16]!");                             // preserve the raw-output flag after all PHP arguments are materialized
    ctx.emitter.instruction("ldp x1, x2, [sp, #16]");                           // reload the filename string for the file reader helper
    abi::emit_call_label(ctx.emitter, "__rt_file_get_contents_maybe_url");
    ctx.emitter.instruction(&format!("cbz x1, {}", fail));                      // null file bytes mean the file could not be read
    ctx.emitter.instruction("mov x3, x1");                                      // pass file bytes as the hash data pointer
    ctx.emitter.instruction("mov x4, x2");                                      // pass file byte count as the hash data length
    ctx.emitter.instruction("ldr x5, [sp]");                                    // restore the raw-output flag into the hash ABI register
    ctx.emitter.instruction("ldp x1, x2, [sp, #32]");                           // restore the hash algorithm string
    ctx.emitter.instruction("add sp, sp, #48");                                 // discard saved algorithm, filename, and flag slots
    crate::codegen::hash_crypto::publish_elephc_crypto_function_pointers(
        ctx.emitter,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash");
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    ctx.emitter.instruction(&format!("b {}", done));                            // proceed to box the digest string
    ctx.emitter.label(fail);
    ctx.emitter.instruction("add sp, sp, #48");                                 // discard saved hash_file arguments on the failure path
    ctx.emitter.instruction("mov x1, #0");                                      // null pointer asks the common boxer to return PHP false
    ctx.emitter.instruction("mov x2, #0");                                      // clear the unused string length for the failure sentinel
    ctx.emitter.label(done);
    Ok(())
}

/// Materializes and hashes `hash_file()` arguments on Linux x86_64.
pub(super) fn lower_hash_file_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    fail: &str,
    done: &str,
) -> Result<()> {
    super::super::strings::load_string_arg_to_regs(ctx, inst, 0, "hash_file", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    super::super::strings::load_string_arg_to_regs(ctx, inst, 1, "hash_file", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    super::super::strings::materialize_truthy_flag(ctx, inst, 2, "hash_file")?;
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                   // reload the filename pointer for the file reader helper
    ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 24]");                   // reload the filename length for the file reader helper
    abi::emit_call_label(ctx.emitter, "__rt_file_get_contents_maybe_url");
    ctx.emitter.instruction("test rax, rax");                                   // null file bytes mean the file could not be read
    ctx.emitter.instruction(&format!("jz {}", fail));                           // return PHP false for unreadable files
    ctx.emitter.instruction("mov rdi, rax");                                    // pass file bytes as the hash data pointer
    ctx.emitter.instruction("mov rsi, rdx");                                    // pass file byte count as the hash data length
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                        // restore the raw-output flag into the hash ABI register
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                   // restore the algorithm string pointer
    ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 40]");                   // restore the algorithm string length
    ctx.emitter.instruction("add rsp, 48");                                     // discard saved algorithm, filename, and flag slots
    crate::codegen::hash_crypto::publish_elephc_crypto_function_pointers(
        ctx.emitter,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash");
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    ctx.emitter.instruction(&format!("jmp {}", done));                          // proceed to box the digest string
    ctx.emitter.label(fail);
    ctx.emitter.instruction("add rsp, 48");                                     // discard saved hash_file arguments on the failure path
    ctx.emitter.instruction("xor eax, eax");                                    // null pointer asks the common boxer to return PHP false
    ctx.emitter.instruction("xor edx, edx");                                    // clear the unused string length for the failure sentinel
    ctx.emitter.label(done);
    Ok(())
}

/// Emits PHP 8.6's `fseek()` write-seek-mode warning on AArch64 and fails the seek.
///
/// `x0` holds the descriptor; the stored label pointer/length name the php-src filter and
/// the message is built through the shared diagnostic fragment channel.
fn emit_fseek_seek_warning_arm64(
    ctx: &mut FunctionContext<'_>,
    function: &str,
    suffix: &str,
    done_label: &str,
    failure: i64,
) {
    let prefix = format!("{function}: Stream filter ");
    let (fseek_filter, fseek_filter_len) = ctx.data.add_string(prefix.as_bytes());
    let (suffix_symbol, suffix_len) = ctx.data.add_string(suffix.as_bytes());
    abi::emit_symbol_address(ctx.emitter, "x9", "_stream_write_seek_label_ptrs");
    ctx.emitter.instruction("ldr x11, [x9, x0, lsl #3]");                       // label pointer for this descriptor
    abi::emit_symbol_address(ctx.emitter, "x9", "_stream_write_seek_label_lens");
    ctx.emitter.instruction("ldr x12, [x9, x0, lsl #3]");                       // label length for this descriptor
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    abi::emit_store_to_sp(ctx.emitter, "x11", 0);
    abi::emit_store_to_sp(ctx.emitter, "x12", 8);
    abi::emit_symbol_address(ctx.emitter, "x1", "_rt_warning_prefix");
    abi::emit_load_int_immediate(ctx.emitter, "x2", 9);
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
    abi::emit_symbol_address(ctx.emitter, "x1", &fseek_filter);
    abi::emit_load_int_immediate(ctx.emitter, "x2", fseek_filter_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", 8);
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
    abi::emit_symbol_address(ctx.emitter, "x1", &suffix_symbol);
    abi::emit_load_int_immediate(ctx.emitter, "x2", suffix_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", 8);
    abi::emit_load_int_immediate(ctx.emitter, "x9", (9 + fseek_filter_len + suffix_len) as i64);
    ctx.emitter.instruction("add x10, x10, x9");                                // full buffer length without the newline
    abi::emit_store_reg_to_symbol(ctx.emitter, "x10", "_rt_diag_warning_message_len", 0);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    // The completing entry appends the final fragment: pass the newline here.
    abi::emit_symbol_address(ctx.emitter, "x1", "_rt_diag_newline");
    abi::emit_load_int_immediate(ctx.emitter, "x2", 1);
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
    ctx.emitter.instruction(&format!("mov x0, #{}", failure));                  // fail the seek with the caller's result
    ctx.emitter.instruction(&format!("b {}", done_label));
}

/// Emits PHP 8.6's `fseek()` write-seek-mode warning on x86_64 and fails the seek.
///
/// `rdi` holds the descriptor; the stored label pointer/length name the php-src filter and
/// the message is built through the shared diagnostic fragment channel.
fn emit_fseek_seek_warning_x86_64(
    ctx: &mut FunctionContext<'_>,
    function: &str,
    suffix: &str,
    done_label: &str,
    failure: i64,
) {
    let prefix = format!("{function}: Stream filter ");
    let (fseek_filter, fseek_filter_len) = ctx.data.add_string(prefix.as_bytes());
    let (suffix_symbol, suffix_len) = ctx.data.add_string(suffix.as_bytes());
    abi::emit_symbol_address(ctx.emitter, "r9", "_stream_write_seek_label_ptrs");
    ctx.emitter.instruction("mov r11, QWORD PTR [r9 + rdi*8]");                  // label pointer for this descriptor
    abi::emit_symbol_address(ctx.emitter, "r9", "_stream_write_seek_label_lens");
    ctx.emitter.instruction("mov r12, QWORD PTR [r9 + rdi*8]");                  // label length for this descriptor
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    abi::emit_store_to_sp(ctx.emitter, "r11", 0);
    abi::emit_store_to_sp(ctx.emitter, "r12", 8);
    abi::emit_symbol_address(ctx.emitter, "rdi", "_rt_warning_prefix");
    abi::emit_load_int_immediate(ctx.emitter, "rsi", 9);
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
    abi::emit_symbol_address(ctx.emitter, "rdi", &fseek_filter);
    abi::emit_load_int_immediate(ctx.emitter, "rsi", fseek_filter_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 8);
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
    abi::emit_symbol_address(ctx.emitter, "rdi", &suffix_symbol);
    abi::emit_load_int_immediate(ctx.emitter, "rsi", suffix_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
    abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 8);
    abi::emit_load_int_immediate(ctx.emitter, "r9", (9 + fseek_filter_len + suffix_len) as i64);
    ctx.emitter.instruction("add r10, r9");                                     // full buffer length without the newline
    abi::emit_store_reg_to_symbol(ctx.emitter, "r10", "_rt_diag_warning_message_len", 0);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    // The completing entry appends the final fragment: pass the newline here.
    abi::emit_symbol_address(ctx.emitter, "rdi", "_rt_diag_newline");
    abi::emit_load_int_immediate(ctx.emitter, "rsi", 1);
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
    ctx.emitter.instruction(&format!("mov rax, {}", failure));                  // fail the seek with the caller's result
    ctx.emitter.instruction(&format!("jmp {}", done_label));
}
