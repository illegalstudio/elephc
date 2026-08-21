//! Purpose:
//! Stream close, read, write, formatted IO, and CSV calls.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Lowers `fclose(stream)` after validating and unboxing the stream handle.
pub(crate) fn lower_fclose(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "fclose", 1)?;
    let stream = expect_operand(inst, 0)?;
    begin_stream_close(ctx, stream, "fclose")?;
    let success_label = ctx.next_label("fclose_ok");
    let done_label = ctx.next_label("fclose_done");
    let user_wrapper_label = ctx.next_label("fclose_user_wrapper");
    let phar_label = ctx.next_label("fclose_phar");
    let not_phar_label = ctx.next_label("fclose_not_phar");
    let after_dispatch_label = ctx.next_label("fclose_after_dispatch");
    let not_popen_label = ctx.next_label("fclose_not_popen");
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // php gives every attached filter one last `filter(..., $closing = true)` call before
            // the stream goes away, and a filter that answered PSFS_FEED_ME until then has been
            // ACCUMULATING: that dispatch is the only chance its bytes have to reach the file.
            // It runs BEFORE the teardown below, which detaches the chains it has to walk.
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // the handle the closing flush walks
            abi::emit_call_label(ctx.emitter, "__rt_stream_write_chain_close_flush");
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // resolve the opaque handle while preserving its descriptor
            abi::emit_call_label(ctx.emitter, "__rt_stream_state");
            // PHP invalidates attached filter resources at fclose(), not when the
            // last reference to the stream goes away, so the chains are closed here
            // as well as from the state destructor.
            ctx.emitter.instruction("stp x0, x1, [sp, #-16]!");                 // preserve the resolved state across the teardown call
            abi::emit_call_label(ctx.emitter, "__rt_stream_close_filter_chains");
            ctx.emitter.instruction("ldp x0, x1, [sp], #16");                   // restore the resolved state
            ctx.emitter.instruction(&format!("cbz x0, {}", not_popen_label));   // retain defensive descriptor cleanup if state vanished
            ctx.emitter.instruction(&format!(
                "ldr x9, [x0, #{}]", STREAM_BACKEND_KIND_OFFSET
            ));                                                                 // select cleanup from the authoritative StreamState backend
            ctx.emitter.instruction(&format!("cmp x9, #{}", STREAM_BACKEND_POPEN)); // is this stream owned by libc popen?
            ctx.emitter.instruction(&format!("b.ne {}", not_popen_label));      // ordinary streams retain their existing flush and close path
            ctx.emitter.instruction(&format!(
                "ldr x10, [x0, #{}]", STREAM_BACKEND_AUX_OFFSET
            ));                                                                 // load the owning FILE* independently of the reusable fd
            ctx.emitter.instruction(&format!(
                "str xzr, [x0, #{}]", STREAM_BACKEND_AUX_OFFSET
            ));                                                                 // detach process ownership before re-entrant libc cleanup
            ctx.emitter.instruction("mov x0, x10");                             // pass the owning FILE* to pclose
            abi::emit_call_label(ctx.emitter, "__rt_pclose");
            ctx.emitter.instruction("cmn x0, #1");                              // did pclose report its -1 failure sentinel?
            ctx.emitter.instruction("cset x0, ne");                             // fclose(process pipe) returns a PHP boolean
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            ctx.emitter.instruction(&format!("b {}", after_dispatch_label));    // skip descriptor-only cleanup after pclose
            ctx.emitter.label(&not_popen_label);
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            // See the AArch64 counterpart: php's closing flush must run before the teardown
            // below detaches the very chains it walks.
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // the handle the closing flush walks
            abi::emit_call_label(ctx.emitter, "__rt_stream_write_chain_close_flush");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // resolve the opaque handle while preserving its descriptor
            abi::emit_call_label(ctx.emitter, "__rt_stream_state");
            // PHP invalidates attached filter resources at fclose(), not when the
            // last reference to the stream goes away, so the chains are closed here
            // as well as from the state destructor.
            ctx.emitter.instruction("push rax");                                // preserve the resolved state across the teardown call
            ctx.emitter.instruction("mov rdi, rax");
            abi::emit_call_label(ctx.emitter, "__rt_stream_close_filter_chains");
            ctx.emitter.instruction("pop rax");                                 // restore the resolved state
            ctx.emitter.instruction("test rax, rax");                           // did the Closing StreamState resolve?
            ctx.emitter.instruction(&format!("jz {}", not_popen_label));        // retain defensive descriptor cleanup if state vanished
            ctx.emitter.instruction(&format!(
                "mov r9, QWORD PTR [rax + {}]", STREAM_BACKEND_KIND_OFFSET
            ));                                                                 // select cleanup from the authoritative StreamState backend
            ctx.emitter.instruction(&format!("cmp r9, {}", STREAM_BACKEND_POPEN)); // is this stream owned by libc popen?
            ctx.emitter.instruction(&format!("jne {}", not_popen_label));       // ordinary streams retain their existing flush and close path
            ctx.emitter.instruction(&format!(
                "mov rdi, QWORD PTR [rax + {}]", STREAM_BACKEND_AUX_OFFSET
            ));                                                                 // load the owning FILE* independently of the reusable fd
            ctx.emitter.instruction(&format!(
                "mov QWORD PTR [rax + {}], 0", STREAM_BACKEND_AUX_OFFSET
            ));                                                                 // detach process ownership before re-entrant libc cleanup
            abi::emit_call_label(ctx.emitter, "__rt_pclose");
            ctx.emitter.instruction("cmp eax, -1");                             // did pclose report its failure sentinel?
            ctx.emitter.instruction("setne al");                                // fclose(process pipe) returns a PHP boolean
            ctx.emitter.instruction("movzx eax, al");                           // widen the strict boolean close result
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            ctx.emitter.instruction(&format!("jmp {}", after_dispatch_label));  // skip descriptor-only cleanup after pclose
            ctx.emitter.label(&not_popen_label);
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov w9, #0x5000");                         // low half of the phar-write descriptor base 0x50000000
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the phar-write synthetic descriptor base
            ctx.emitter.instruction("cmp x0, x9");                              // is the descriptor below the phar-write range?
            ctx.emitter.instruction(&format!("b.lt {}", not_phar_label));       // below the PHAR range: continue with normal dispatch
            ctx.emitter.instruction("add x10, x9, #32");                        // upper bound for the 32 buffered PHAR write descriptors
            ctx.emitter.instruction("cmp x0, x10");                             // is this inside the phar-write descriptor range?
            ctx.emitter.instruction(&format!("b.lt {}", phar_label));           // finalize phar writes instead of closing a real fd
            ctx.emitter.label(&not_phar_label);
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether this is a userspace-wrapper stream
            ctx.emitter.instruction(&format!("b.ge {}", user_wrapper_label));   // dispatch synthetic handles without indexing fd tables
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9d, 0x50000000");                     // materialize the phar-write synthetic descriptor base
            ctx.emitter.instruction("cmp rax, r9");                             // is the descriptor below the phar-write range?
            ctx.emitter.instruction(&format!("jl {}", not_phar_label));         // below the PHAR range: continue with normal dispatch
            ctx.emitter.instruction("lea r10, [r9 + 32]");                      // upper bound for the 32 buffered PHAR write descriptors
            ctx.emitter.instruction("cmp rax, r10");                            // is this inside the phar-write descriptor range?
            ctx.emitter.instruction(&format!("jl {}", phar_label));             // finalize phar writes instead of closing a real fd
            ctx.emitter.label(&not_phar_label);
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rax, r9");                             // test whether this is a userspace-wrapper stream
            ctx.emitter.instruction(&format!("jge {}", user_wrapper_label));    // dispatch synthetic handles without indexing fd tables
        }
    }
    emit_zlib_flush_on_close_for_current_fd(ctx);
    emit_bz2_flush_on_close_for_current_fd(ctx);
    emit_iconv_flush_on_close_for_current_fd(ctx);
    emit_tls_session_teardown_for_handle(ctx, 0);
    let legacy_filter_cleanup_done = ctx.next_label("fclose_legacy_filter_cleanup_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #256");                            // transitional filter tables cover descriptors below 256 only
            ctx.emitter.instruction(&format!("b.hs {}", legacy_filter_cleanup_done)); // high descriptors have no table-backed filter state
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 256");                            // transitional filter tables cover descriptors below 256 only
            ctx.emitter.instruction(&format!("jae {}", legacy_filter_cleanup_done)); // high descriptors have no table-backed filter state
        }
    }
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the descriptor to the user-filter teardown helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_user_filter_release_fd");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_read_filters");
            ctx.emitter.instruction("strb wzr, [x9, x0]");                      // clear any read filter before the descriptor can be reused
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_write_filters");
            ctx.emitter.instruction("strb wzr, [x9, x0]");                      // clear any write filter before the descriptor can be reused
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_read_filters"); // read-filter table base
            ctx.emitter.instruction("mov BYTE PTR [r9 + rax], 0");              // clear any read filter before the descriptor can be reused
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_write_filters"); // write-filter table base
            ctx.emitter.instruction("mov BYTE PTR [r9 + rax], 0");              // clear any write filter before the descriptor can be reused
        }
    }
    ctx.emitter.label(&legacy_filter_cleanup_done);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.syscall(6);
            ctx.emitter.instruction("cmp x0, #0");                              // test whether close() reported success
            ctx.emitter.instruction(&format!("b.eq {}", success_label));        // branch to the true result when the stream closed cleanly
            ctx.emitter.instruction("mov x0, #0");                              // return false when the stream close failed
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the success result on the failure path
            ctx.emitter.label(&success_label);
            ctx.emitter.instruction("mov x0, #1");                              // return true when the stream closed successfully
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the stream fd to libc close()
            ctx.emitter.instruction("call close");                              // close the requested stream descriptor
            ctx.emitter.instruction("cmp rax, 0");                              // test whether close() reported success
            ctx.emitter.instruction(&format!("je {}", success_label));          // branch to the true result when the stream closed cleanly
            ctx.emitter.instruction("xor eax, eax");                            // return false when the stream close failed
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the success result on the failure path
            ctx.emitter.label(&success_label);
            ctx.emitter.instruction("mov rax, 1");                              // return true when the stream close succeeded
        }
    }
    ctx.emitter.label(&done_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", after_dispatch_label));    // skip synthetic close handlers after the native fd path
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", after_dispatch_label));  // skip synthetic close handlers after the native fd path
        }
    }
    ctx.emitter.label(&user_wrapper_label);
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the synthetic wrapper descriptor to the close helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fclose");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", after_dispatch_label));    // skip phar finalization after wrapper close dispatch
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", after_dispatch_label));  // skip phar finalization after wrapper close dispatch
        }
    }
    ctx.emitter.label(&phar_label);
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the PHAR write descriptor to the finalizer
    }
    abi::emit_call_label(ctx.emitter, "__rt_phar_write_finalize");
    ctx.emitter.label(&after_dispatch_label);
    finish_stream_close(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `fread(stream, length)` using the shared runtime file-read helper.
/// php-src's verbatim `ValueError` wording for `fread()` with a non-positive `$length`.
const FREAD_NON_POSITIVE_LENGTH_MESSAGE: &str =
    "fread(): Argument #2 ($length) must be greater than 0";

pub(crate) fn lower_fread(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "fread", 2)?;
    let stream = expect_operand(inst, 0)?;
    let length = expect_operand(inst, 1)?;
    load_open_stream_handle_to_result(ctx, stream, "fread")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int(ctx.load_value_to_result(length)?.codegen_repr(), "fread length")?;
    let length_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the requested byte count to the fread runtime helper
            abi::emit_pop_reg(ctx.emitter, "x0");
            "x1"
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the requested byte count to the fread runtime helper
            abi::emit_pop_reg(ctx.emitter, "rdi");
            "rsi"
        }
    };
    // php-src rejects zero and negatives outright, before it reads anything. elephc accepted
    // both and answered "", which reads as a legitimate empty result.
    super::super::exceptions::emit_value_error_unless(
        ctx,
        super::super::exceptions::ValueGuard::SignedAtLeast(length_reg, 1),
        FREAD_NON_POSITIVE_LENGTH_MESSAGE,
    );
    abi::emit_call_label(ctx.emitter, "__rt_fread");
    // php-src advances a userspace wrapper's position by whatever the read moved, and reports
    // THAT from ftell() — it never asks the wrapper. See __rt_stream_wrapper_pos.
    emit_advance_wrapper_position(ctx, stream, "fread")?;
    // An exhausted stream answers "" and a FAILED read answers false, so emptiness cannot
    // decide this: the helper reports which one it was in x0/rcx.
    box_stream_string_or_false_on_unconsumed_result(ctx, "fread");
    store_if_result(ctx, inst)
}

/// Moves a userspace wrapper's PHP-side position by the bytes a read just produced.
///
/// php-src keeps `stream->position` for these streams itself and advances it on every read and
/// write; `stream_tell` is consulted only from inside the seek op. Doing the same here is what
/// lets `ftell()` answer 7 after seven bytes rather than whatever the wrapper's own
/// `stream_tell()` chooses to return. The helper is a no-op for a stream with no state, and the
/// field it moves is read only on the wrapper path, so a file stream is unaffected.
fn emit_advance_wrapper_position(
    ctx: &mut FunctionContext<'_>,
    stream: ValueId,
    name: &str,
) -> Result<()> {
    match ctx.emitter.target.arch {
        // The result travels on an explicit frame rather than in a scratch register: materializing
        // the handle clobbers the ones a caller would reach for, which showed up as a position
        // advanced by the handle instead of by the bytes read.
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #32");
            ctx.emitter.instruction("stp x1, x2, [sp, #0]");                    // the string pair the caller is owed
            ctx.emitter.instruction("str x0, [sp, #16]");                       // and the success flag beside it
            load_stream_handle_to_result(ctx, stream, name)?;
            ctx.emitter.instruction("ldr x1, [sp, #8]");                        // the bytes this read produced
            abi::emit_call_label(ctx.emitter, "__rt_stream_wrapper_pos_advance");
            ctx.emitter.instruction("ldp x1, x2, [sp, #0]");
            ctx.emitter.instruction("ldr x0, [sp, #16]");
            ctx.emitter.instruction("add sp, sp, #32");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 32");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // the string pair the caller is owed
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rcx");           // and the success flag beside it
            load_stream_handle_to_result(ctx, stream, name)?;
            ctx.emitter.instruction("mov rdi, rax");
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");            // the bytes this read produced
            abi::emit_call_label(ctx.emitter, "__rt_stream_wrapper_pos_advance");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");
            ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 16]");
            ctx.emitter.instruction("add rsp, 32");
        }
    }
    Ok(())
}

/// Lowers `fwrite(stream, data, length?)` and boxes a byte count or PHP `false` on error.
///
/// php's third argument caps the write at `max(0, min($length, strlen($data)))` bytes and is
/// NOT an error when non-positive: `fwrite($h, "hello", 0)` and `fwrite($h, "hello", -1)` both
/// write nothing and answer `0`. `__rt_fwrite_filtered(stream, ptr, len)` already takes the byte
/// count in its own register, so the cap is a clamp on that register rather than a truncated
/// copy — which also keeps an attached write filter seeing exactly the bytes php gives it
/// (`fwrite($h, "abcdef", 4)` through `string.toupper` yields `ABCD`, not `ABCDEF`).
pub(crate) fn lower_fwrite(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count_between(inst, "fwrite", 2, 3)?;
    let stream = expect_operand(inst, 0)?;
    let data = expect_operand(inst, 1)?;
    // An explicit `null` is php's "no cap", the same as omitting the argument, and it carries no
    // integer to resolve — so it is settled here rather than materialised and clamped.
    let length = match inst.operands.get(2).copied() {
        Some(operand)
            if !matches!(
                ctx.value_php_type(operand)?.codegen_repr(),
                PhpType::Void | PhpType::Never
            ) =>
        {
            Some(operand)
        }
        _ => None,
    };
    load_open_stream_handle_to_result(ctx, stream, "fwrite")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            load_string_to_result(ctx, data, "fwrite data")?;
            if let Some(length) = length {
                // The string view lives in x1/x2 and `$length` is evaluated after it, in source
                // order, through a resolver that may call out and clobber both.
                ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");             // spill the string pointer and byte length
                resolve_nullable_int_operand_to_result(ctx, length, "fwrite length")?;
                ctx.emitter.instruction("ldp x1, x2, [sp], #16");               // restore the string pointer and byte length
                ctx.emitter.instruction("cmp x0, #0");                          // a negative cap writes nothing
                ctx.emitter.instruction("csel x0, xzr, x0, lt");                // clamp it up to zero
                ctx.emitter.instruction("cmp x0, x2");                          // is the cap shorter than the data?
                ctx.emitter.instruction("csel x2, x0, x2, lt");                 // write min(cap, strlen($data)) bytes
            }
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_fwrite_filtered");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            load_string_to_result(ctx, data, "fwrite data")?;
            if let Some(length) = length {
                ctx.emitter.instruction("push rax");                            // spill the string pointer
                ctx.emitter.instruction("push rdx");                            // and its byte length
                resolve_nullable_int_operand_to_result(ctx, length, "fwrite length")?;
                ctx.emitter.instruction("mov rcx, rax");                        // hold the requested cap
                ctx.emitter.instruction("pop rdx");                             // restore the byte length
                ctx.emitter.instruction("pop rax");                             // restore the string pointer
                ctx.emitter.instruction("xor r8d, r8d");                        // a negative cap writes nothing
                ctx.emitter.instruction("cmp rcx, 0");
                ctx.emitter.instruction("cmovl rcx, r8");                       // clamp it up to zero
                ctx.emitter.instruction("cmp rcx, rdx");                        // is the cap shorter than the data?
                ctx.emitter.instruction("cmovl rdx, rcx");                      // write min(cap, strlen($data)) bytes
            }
            abi::emit_pop_reg(ctx.emitter, "rdi");
            ctx.emitter.instruction("mov rsi, rax");                            // pass the string pointer to the runtime fwrite helper
            abi::emit_call_label(ctx.emitter, "__rt_fwrite_filtered");
        }
    }
    box_negative_int_or_false_result(ctx, "fwrite");
    store_if_result(ctx, inst)
}

/// Lowers `fprintf(stream, format, values...)` as `sprintf()` plus stream write.
pub(crate) fn lower_fprintf(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fprintf", 2, usize::MAX)?;
    let stream = expect_operand(inst, 0)?;
    let format = expect_operand(inst, 1)?;
    let spec_cats = super::super::strings::sprintf_spec_cats_for_format(ctx, format)?;
    load_open_stream_handle_to_result(ctx, stream, "fprintf")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    for index in (2..inst.operands.len()).rev() {
        let value = expect_operand(inst, index)?;
        let spec_cat = spec_cats.get(index - 2).copied();
        super::super::strings::pack_sprintf_like_arg(ctx, value, spec_cat, "fprintf")?;
    }
    load_string_to_result(ctx, format, "fprintf format")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(
                &format!("mov x0, #{}", inst.operands.len() - 2)
            );                                                                  // pass the number of packed fprintf operands
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", (inst.operands.len() - 2) as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_sprintf");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the formatted string pointer to fwrite
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fwrite_filtered");
    store_if_result(ctx, inst)
}

/// Lowers `vfprintf(stream, format, values)` through `__rt_vsprintf` then fwrite.
pub(crate) fn lower_vfprintf(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "vfprintf", 3)?;
    let stream = expect_operand(inst, 0)?;
    let format = expect_operand(inst, 1)?;
    let values = expect_operand(inst, 2)?;
    load_open_stream_handle_to_result(ctx, stream, "vfprintf")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #32");                         // reserve fd and format scratch storage
            ctx.emitter.instruction("str x0, [sp, #0]");                        // save the descriptor across formatting
            load_string_to_result(ctx, format, "vfprintf format")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #8]");                    // save the format pointer and length
            ctx.load_value_to_result(values)?;
            ctx.emitter.instruction("ldp x1, x2, [sp, #8]");                    // restore the format pointer and length
            abi::emit_call_label(ctx.emitter, "__rt_vsprintf");
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the destination descriptor
            abi::emit_call_label(ctx.emitter, "__rt_fwrite_filtered");
            ctx.emitter.instruction("add sp, sp, #32");                         // release vfprintf scratch storage
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 32");                             // reserve fd and format scratch storage
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // save the descriptor across formatting
            load_string_to_result(ctx, format, "vfprintf format")?;
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // save the format pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rdx");           // save the format byte length
            ctx.load_value_to_result(values)?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the values array to vsprintf
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");            // restore the format pointer
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");           // restore the format byte length
            abi::emit_call_label(ctx.emitter, "__rt_vsprintf");
            ctx.emitter.instruction("mov rsi, rax");                            // pass the formatted string pointer to fwrite
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // reload the destination descriptor
            abi::emit_call_label(ctx.emitter, "__rt_fwrite_filtered");
            ctx.emitter.instruction("add rsp, 32");                             // release vfprintf scratch storage
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `fgets(stream)` through the shared line-read runtime helper.
/// php-src's verbatim `ValueError` wording for `fgets()` with a non-positive `$length`.
const FGETS_NON_POSITIVE_LENGTH_MESSAGE: &str =
    "fgets(): Argument #2 ($length) must be greater than 0";

pub(crate) fn lower_fgets(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count_between(inst, "fgets", 1, 2)?;
    let stream = expect_operand(inst, 0)?;
    // PHP's optional `$length` bounds the line at `$length - 1` bytes. Zero means unbounded here,
    // which is what an omitted argument resolves to, so the helper needs no separate flag.
    match inst.operands.get(1).copied() {
        None => {
            load_open_stream_handle_to_result(ctx, stream, "fgets")?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => ctx.emitter.instruction("mov x1, #0"),          // no bound
                Arch::X86_64 => {
                    ctx.emitter.instruction("mov rdi, rax");                     // the opaque stream handle
                    ctx.emitter.instruction("xor esi, esi");                     // no bound
                }
            }
        }
        Some(length) => {
            resolve_nullable_int_operand_to_result(ctx, length, "fgets length")?;
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            load_open_stream_handle_to_result(ctx, stream, "fgets")?;
            let bound_reg = match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_pop_reg(ctx.emitter, "x1");                        // the requested bound
                    "x1"
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("mov rdi, rax");                     // the opaque stream handle
                    abi::emit_pop_reg(ctx.emitter, "rsi");                       // the requested bound
                    "rsi"
                }
            };
            // A null `$length` is php's "no bound", which is the helper's zero — and must reach
            // it WITHOUT passing through the guard below, whose zero is the rejected case. The
            // sentinel only appears when the argument arrived as a boxed null.
            let unbounded = ctx.next_label("fgets_unbounded");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_load_int_immediate(ctx.emitter, "x9", NULL_SENTINEL);
                    ctx.emitter.instruction(&format!("cmp {}, x9", bound_reg));  // was the length null?
                    ctx.emitter.instruction(&format!("b.ne {}", unbounded));     // a real length still faces the guard
                    ctx.emitter.instruction(&format!("mov {}, #0", bound_reg));  // null → the helper's unbounded read
                }
                Arch::X86_64 => {
                    abi::emit_load_int_immediate(ctx.emitter, "r10", NULL_SENTINEL);
                    ctx.emitter.instruction(&format!("cmp {}, r10", bound_reg)); // was the length null?
                    ctx.emitter.instruction(&format!("jne {}", unbounded));      // a real length still faces the guard
                    ctx.emitter.instruction(&format!("xor {}, {}", bound_reg, bound_reg)); // null → unbounded
                }
            }
            let bounded = ctx.next_label("fgets_bounded");
            match ctx.emitter.target.arch {
                Arch::AArch64 => ctx.emitter.instruction(&format!("b {}", bounded)),
                Arch::X86_64 => ctx.emitter.instruction(&format!("jmp {}", bounded)),
            }
            ctx.emitter.label(&unbounded);
            // Zero is what an omitted argument means to the helper, so a caller-supplied zero
            // must never reach it. php-src rejects zero and negatives outright.
            super::super::exceptions::emit_value_error_unless(
                ctx,
                super::super::exceptions::ValueGuard::SignedAtLeast(bound_reg, 1),
                FGETS_NON_POSITIVE_LENGTH_MESSAGE,
            );
            ctx.emitter.label(&bounded);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fgets");
    box_stream_string_or_false_on_empty_result(ctx, "fgets");
    store_if_result(ctx, inst)
}

/// Lowers `fgetc(stream)` and boxes the one-byte string or PHP false result.
pub(crate) fn lower_fgetc(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "fgetc", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_open_stream_handle_to_result(ctx, stream, "fgetc")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the opaque stream handle to the x86_64 fgetc helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_fgetc");
    box_stream_string_or_false_on_empty_result(ctx, "fgetc");
    store_if_result(ctx, inst)
}

/// One CSV control argument, as the three CSV builtins each spell it.
///
/// php-src validates every one of them BEFORE it reads a byte: a separator or enclosure has to
/// be exactly one character, an escape has to be empty or one character, and anything else is a
/// catchable `ValueError` naming that function's own argument position. elephc used to take the
/// first byte and drop the rest in silence, so `fgetcsv($h, 0, "::")` quietly parsed on `:`.
struct CsvControl {
    /// Operand index of this control in the lowered instruction.
    operand: usize,
    /// Byte handed to the runtime when the argument is ABSENT.
    ///
    /// The runtime reads a zero separator/enclosure as "use my default" and a zero escape as
    /// RFC 4180 doubling mode, which is what php reaches through an EMPTY `$escape` — never
    /// through an omitted one. The default therefore has to be spelled out here.
    default: u8,
    /// Context string for the string-load diagnostic.
    context: &'static str,
    /// php-src's `Argument #N` position for this function.
    position: usize,
    /// The php parameter name, without its `$`.
    parameter: &'static str,
    /// Whether an EMPTY string is accepted (true only for `$escape`).
    empty_allowed: bool,
}

/// Materializes the three CSV control bytes on the stack, in `separator, enclosure, escape`
/// order, rejecting any that is not the single character php-src requires.
///
/// Each byte is pushed as it is produced; the packing step that follows pops them in reverse.
fn emit_csv_control_bytes(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    function: &str,
    controls: &[CsvControl; 3],
) -> Result<()> {
    let arch = ctx.emitter.target.arch;
    for control in controls {
        if inst.operands.len() > control.operand {
            let value = expect_operand(inst, control.operand)?;
            load_string_to_result(ctx, value, control.context)?;
            // The length is still in the string ABI's second register, which is exactly what
            // php-src measures: it counts CHARACTERS, so a two-byte separator and an empty one
            // are the same rejection. `$escape` alone accepts the empty string, because that is
            // how a caller asks for RFC 4180 doubling.
            let (minimum, message) = if control.empty_allowed {
                (
                    0,
                    format!(
                        "{function}(): Argument #{} (${}) must be empty or a single character",
                        control.position, control.parameter
                    ),
                )
            } else {
                (
                    1,
                    format!(
                        "{function}(): Argument #{} (${}) must be a single character",
                        control.position, control.parameter
                    ),
                )
            };
            let length_reg = abi::string_result_regs(ctx.emitter).1;
            super::super::exceptions::emit_value_error_unless(
                ctx,
                super::super::exceptions::ValueGuard::SignedInRange(length_reg, minimum, 1),
                &message,
            );
            let empty_label = ctx.next_label("csv_empty");
            let done_label = ctx.next_label("csv_done");
            match arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction(&format!("cbz x2, {}", empty_label)); // only `$escape` can still be empty here
                    ctx.emitter.instruction("ldrb w0, [x1]");                   // load first byte of the CSV delimiter string
                    ctx.emitter.instruction(&format!("b {}", done_label));      // skip the empty-string fallback
                    ctx.emitter.label(&empty_label);
                    ctx.emitter.instruction("mov w0, #0");                      // an empty $escape is the runtime's doubling mode
                    ctx.emitter.label(&done_label);
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("test rdx, rdx");                   // only `$escape` can still be empty here
                    ctx.emitter.instruction(&format!("jz {}", empty_label));    // branch if string is empty
                    ctx.emitter.instruction("movzx eax, BYTE PTR [rax]");       // load first byte of the CSV delimiter string
                    ctx.emitter.instruction(&format!("jmp {}", done_label));    // skip the empty-string fallback
                    ctx.emitter.label(&empty_label);
                    ctx.emitter.instruction("mov eax, 0");                      // an empty $escape is the runtime's doubling mode
                    ctx.emitter.label(&done_label);
                }
            }
        } else {
            match arch {
                Arch::AArch64 => {
                    ctx.emitter
                        .instruction(&format!("mov w0, #{}", control.default)); // use default CSV delimiter byte
                }
                Arch::X86_64 => {
                    ctx.emitter
                        .instruction(&format!("mov eax, {}", control.default)); // use default CSV delimiter byte
                }
            }
        }
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));        // save extracted delimiter byte
    }
    Ok(())
}

/// The `fgetcsv()` and `fputcsv()` control arguments — both name them `#3`, `#4` and `#5`.
const STREAM_CSV_CONTROLS: [CsvControl; 3] = [
    CsvControl { operand: 2, default: b',', context: "csv separator", position: 3, parameter: "separator", empty_allowed: false },
    CsvControl { operand: 3, default: b'"', context: "csv enclosure", position: 4, parameter: "enclosure", empty_allowed: false },
    CsvControl { operand: 4, default: b'\\', context: "csv escape", position: 5, parameter: "escape", empty_allowed: true },
];

/// The `str_getcsv()` control arguments, one position earlier: it has no `$length`.
const STRING_CSV_CONTROLS: [CsvControl; 3] = [
    CsvControl { operand: 1, default: b',', context: "str_getcsv separator", position: 2, parameter: "separator", empty_allowed: false },
    CsvControl { operand: 2, default: b'"', context: "str_getcsv enclosure", position: 3, parameter: "enclosure", empty_allowed: false },
    CsvControl { operand: 3, default: b'\\', context: "str_getcsv escape", position: 4, parameter: "escape", empty_allowed: true },
];

/// Lowers `fgetcsv(stream, length?, separator?, enclosure?, escape?)` through the CSV row
/// runtime helper, passing separator/enclosure/escape as a packed `csv_opts` word.
pub(crate) fn lower_fgetcsv(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fgetcsv", 1, 5)?;
    let stream = expect_operand(inst, 0)?;
    let arch = ctx.emitter.target.arch;
    load_open_stream_handle_to_result(ctx, stream, "fgetcsv")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));            // save the opaque stream handle on stack

    // -- extract first byte of separator / enclosure / escape (or default) --
    emit_csv_control_bytes(ctx, inst, "fgetcsv", &STREAM_CSV_CONTROLS)?;
    emit_csv_escape_deprecation(ctx, inst, "fgetcsv", 4);

    // -- pack csv_opts = (esc << 16) | (enc << 8) | sep --
    match arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x1");                                // pop escape byte
            ctx.emitter.instruction("lsl x1, x1, #16");                         // shift escape to bits 16..23
            abi::emit_pop_reg(ctx.emitter, "x0");                                // pop enclosure byte
            ctx.emitter.instruction("orr x1, x1, x0, lsl #8");                  // include enclosure in csv_opts
            abi::emit_pop_reg(ctx.emitter, "x0");                                // pop separator byte
            ctx.emitter.instruction("orr x1, x1, x0");                          // complete csv_opts in x1
            abi::emit_pop_reg(ctx.emitter, "x0");                                // restore the opaque stream handle into x0
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rax");                               // pop escape byte
            ctx.emitter.instruction("shl rax, 16");                             // shift escape to bits 16..23
            ctx.emitter.instruction("mov rsi, rax");                            // start accumulating csv_opts
            abi::emit_pop_reg(ctx.emitter, "rax");                               // pop enclosure byte
            ctx.emitter.instruction("shl rax, 8");                              // shift enclosure to bits 8..15
            ctx.emitter.instruction("or rsi, rax");                             // include enclosure in csv_opts
            abi::emit_pop_reg(ctx.emitter, "rax");                               // pop separator byte
            ctx.emitter.instruction("or rsi, rax");                             // complete csv_opts in rsi
            abi::emit_pop_reg(ctx.emitter, "rdi");                               // restore the opaque stream handle into rdi
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fgetcsv");                           // call the CSV row parser runtime
    if arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                 // 0 (EOF), 1 (blank record), or the parsed row
    }
    // php-src splits "no line at all" from "a line with no fields": the first is `false`, the
    // second is `php_bc_fgetcsv_empty_line()` — `[null]`. This helper keeps the EOF null pointer
    // for the boxer below and turns the blank sentinel into that `[null]`, widening every other
    // row to boxed Mixed cells so the null is representable at all.
    abi::emit_call_label(ctx.emitter, "__rt_fgetcsv_row_to_mixed");
    // The OWNED boxer, as scandir/glob/file use: every non-null answer here is a FRESHLY created
    // array — the widened row, or the substituted `[null]` — so the box must become its sole
    // owner. The plain boxer only retains, leaving the creation reference alive; that leaked the
    // whole row per call, and boxing the cells made each leaked row one block per field heavier.
    box_listing_or_false_result(ctx, "fgetcsv");                                 // EOF is the null pointer, and PHP calls that false
    store_if_result(ctx, inst)
}

/// Lowers `str_getcsv(string, separator?, enclosure?, escape?)` through the shared CSV
/// state machine, packing separator/enclosure/escape into the same `csv_opts` word
/// `fgetcsv()` uses.
///
/// The parser unescapes IN PLACE, so the runtime helper copies the subject first — the
/// argument here may be a literal in read-only memory.
pub(crate) fn lower_str_getcsv(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "str_getcsv", 1, 4)?;
    let subject = expect_operand(inst, 0)?;
    let arch = ctx.emitter.target.arch;

    // -- pack csv_opts: (esc << 16) | (enc << 8) | sep --
    //
    // The DEFAULT escape is spelled out rather than left as the zero the runtime reads as RFC
    // 4180 doubling: php's `str_getcsv($s)` uses `"\\"`, the same byte `fgetcsv()` defaults to,
    // and only an explicitly EMPTY `$escape` asks for doubling.
    emit_csv_control_bytes(ctx, inst, "str_getcsv", &STRING_CSV_CONTROLS)?;
    emit_csv_escape_deprecation(ctx, inst, "str_getcsv", 3);
    match arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x0");                                // escape byte
            ctx.emitter.instruction("lsl x0, x0, #16");
            ctx.emitter.instruction("mov x9, x0");
            abi::emit_pop_reg(ctx.emitter, "x0");                                // enclosure byte
            ctx.emitter.instruction("lsl x0, x0, #8");
            ctx.emitter.instruction("orr x9, x9, x0");
            abi::emit_pop_reg(ctx.emitter, "x0");                                // separator byte
            ctx.emitter.instruction("orr x9, x9, x0");
            abi::emit_push_reg(ctx.emitter, "x9");                               // hold csv_opts across the subject load
            load_string_to_result(ctx, subject, "str_getcsv string")?;
            abi::emit_pop_reg(ctx.emitter, "x0");                                // csv_opts, with the subject in x1/x2
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rax");                               // escape byte
            ctx.emitter.instruction("shl rax, 16");
            ctx.emitter.instruction("mov r9, rax");
            abi::emit_pop_reg(ctx.emitter, "rax");                               // enclosure byte
            ctx.emitter.instruction("shl rax, 8");
            ctx.emitter.instruction("or r9, rax");
            abi::emit_pop_reg(ctx.emitter, "rax");                               // separator byte
            ctx.emitter.instruction("or r9, rax");
            abi::emit_push_reg(ctx.emitter, "r9");                               // hold csv_opts across the subject load
            load_string_to_result(ctx, subject, "str_getcsv string")?;
            ctx.emitter.instruction("mov rsi, rax");                             // subject pointer
            ctx.emitter.instruction("mov rdx, rdx");                             // subject length already in rdx
            abi::emit_pop_reg(ctx.emitter, "rdi");                               // csv_opts
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_getcsv");
    if arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                 // the parsed row, or the null pointer for no record
    }
    // php-src substitutes `php_bc_fgetcsv_empty_line()` — a one-element array holding null —
    // when the parser reports no record, and the row widens to boxed Mixed cells so that null
    // survives the read back. An `array<string>` slot cannot: the sentinel reaches the slot but
    // nothing reads it as null.
    abi::emit_call_label(ctx.emitter, "__rt_csv_row_to_mixed");
    store_if_result(ctx, inst)
}

/// Lowers `fputcsv(stream, fields, separator?, enclosure?, escape?, eol?)` for string arrays,
/// The `$escape` argument index for each CSV function that takes one.
///
/// PHP 8.4 deprecates omitting it, because 9.0 changes the default from `"\\"` to `""` —
/// a silent behaviour change for anyone relying on today's value. The notice fires on the
/// ARGUMENT being absent, not on its value, so passing the default explicitly is quiet.
///
/// It is also VERSION-GATED, which the rest of the diagnostic surface already is and this was
/// not: PHP 8.2 and 8.3 print nothing here, so `--php-version 8.3` printing the notice made
/// elephc noisier than the interpreter it is asked to imitate.
///
/// And it comes LAST, after the control characters are validated. php checks the separator, the
/// enclosure and the escape for being a single character before it reaches the notice, so a call
/// that throws `ValueError` never prints one: `fgetcsv($h, 0, ";;")` is one line on `php -n`
/// 8.5.6 and was two here. Every caller must emit it after `emit_csv_control_bytes()`.
fn emit_csv_escape_deprecation(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    function: &str,
    escape_index: usize,
) {
    if inst.operands.len() > escape_index {
        return;
    }
    if crate::codegen::compile_php_version() < crate::php_version::PhpVersion::Php84 {
        return;
    }
    let symbol = format!("_diag_csv_escape_deprecated_{function}_msg");
    let length = format!(
        "Deprecated: {function}(): the $escape parameter must be provided as its default value will change\n"
    )
    .len();
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.adrp("x1", &symbol);
            ctx.emitter.add_lo12("x1", "x1", &symbol);
            ctx.emitter.instruction(&format!("mov x2, #{length}"));              // the notice byte length
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("lea rdi, [rip + {symbol}]"));             // the notice pointer
            ctx.emitter.instruction(&format!("mov esi, {length}"));              // the notice byte length
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");                      // stderr, and `@` suppresses it
}

/// php-src's wording when `$fields` is not an array — the only other thing an
/// `array<string>|false` value can be at run time.
const FPUTCSV_FIELDS_NOT_ARRAY_MESSAGE: &str =
    "fputcsv(): Argument #2 ($fields) must be of type array, false given";

/// Reports whether a declared type is a boxed union whose only non-`false` member is a
/// string array — the shape `scandir()`, `glob()` and `file()` return.
///
/// `fgetcsv()` no longer qualifies: its rows are `array<mixed>|false`, because php answers
/// `[null]` for a blank line. Those take the branch below that reads the element lane from the
/// array HEADER at run time, which is what an `array<mixed>` needs anyway.
fn boxed_string_array_union(ty: &PhpType) -> bool {
    let PhpType::Union(members) = ty else {
        return false;
    };
    let mut saw_string_array = false;
    for member in members {
        match member {
            PhpType::False => {}
            PhpType::Array(element) if element.codegen_repr() == PhpType::Str => {
                saw_string_array = true;
            }
            _ => return false,
        }
    }
    saw_string_array
}

/// Replaces a boxed array in the result register with the array pointer it carries.
///
/// A value that is not an array at run time has no row to write, which PHP reports as a
/// `TypeError` rather than writing an empty row. Reaching this with the box still on would be
/// worse than a wrong row: the cell's tag word reads as a length, so a two-field row renders as
/// four fields of raw header bytes.
fn emit_unwrap_boxed_string_array(ctx: &mut FunctionContext<'_>, label_prefix: &str) {
    let ok = ctx.next_label(&format!("{}_fields_array", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // the boxed value's runtime tag
            ctx.emitter.instruction("cmp x9, #4");                              // tag 4 = indexed array
            ctx.emitter.instruction(&format!("b.eq {}", ok));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                // the boxed value's runtime tag
            ctx.emitter.instruction("cmp r10, 4");                              // tag 4 = indexed array
            ctx.emitter.instruction(&format!("je {}", ok));
        }
    }
    super::super::super::exceptions::emit_type_error(ctx, FPUTCSV_FIELDS_NOT_ARRAY_MESSAGE);
    ctx.emitter.label(&ok);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("ldr x0, [x0, #8]"),           // the array the box carries
        Arch::X86_64 => ctx.emitter.instruction("mov rax, QWORD PTR [rax + 8]"), // the array the box carries
    }
}

/// The `value_type` an element layout of PHP strings carries — the only layout that stores
/// 16-byte `(ptr, len)` slots rather than 8-byte payloads.
const CSV_FIELD_TAG_STRING: u8 = 1;

/// Returns the runtime `value_type` tag `__rt_fputcsv` needs to read a field array's elements.
///
/// PHP casts every field to string (`php_fputcsv` calls `zval_get_tmp_string` per field), so the
/// question a CSV writer must answer is not "is this a string array?" but "how are its elements
/// stored?". The tags below are the subset of the indexed-array `value_type` numbering that can
/// appear in a CSV row, and they MUST keep agreeing with `emit_array_value_type_stamp`, which is
/// what writes them into the array header — the runtime reuses each tag directly as the Mixed
/// cell tag it builds to format the field.
///
/// Objects, nested arrays and resources are deliberately absent: PHP warns and renders those
/// ("Array to string conversion"), which is a separate behaviour this writer does not yet carry,
/// so they stay a lowering-time refusal rather than becoming a silently wrong field.
/// `None` means the element layout is not knowable at compile time and the array header — the
/// same authority `__rt_implode` consults — must be read at run time instead.
fn csv_field_value_type_tag(ty: &PhpType, name: &str) -> Result<Option<u8>> {
    let element = match ty {
        PhpType::Array(element) => element.codegen_repr(),
        // A gradually-typed value carries no element type here, but the array it points at still
        // carries its stamped `value_type`. Reading the header keeps those rows writable rather
        // than guessing a layout that a wrong guess would misread as raw memory.
        PhpType::Mixed | PhpType::Union(_) => return Ok(None),
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} for PHP type {:?}",
                name, other
            )));
        }
    };
    match element {
        PhpType::Str => Ok(Some(CSV_FIELD_TAG_STRING)),
        PhpType::Int => Ok(Some(0)),
        PhpType::Float => Ok(Some(2)),
        PhpType::Bool => Ok(Some(3)),
        PhpType::Mixed | PhpType::Union(_) => Ok(Some(7)),
        // An empty array literal carries an uninhabited element type. No element is ever
        // dereferenced, so the string layout is the safe choice and keeps `fputcsv($f, [])`
        // from being rejected at lowering time.
        PhpType::Void | PhpType::Never => Ok(Some(CSV_FIELD_TAG_STRING)),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP element type {:?}",
            name, other
        ))),
    }
}

/// Reads the loaded array's stamped `value_type` and leaves it in the CSV element-tag position.
///
/// Emitted only when the lowering could not name the element layout. The array pointer is in the
/// result register; the tag lands in the scratch register the packing step ORs into `csv_opts`.
fn emit_csv_field_tag_from_header(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0, #-8]");                       // load the packed array kind word
            ctx.emitter.instruction("lsr x9, x9, #8");                          // move the value_type tag into the low bits
            ctx.emitter.instruction("and x9, x9, #0xf");                        // isolate the 4-bit element value_type tag
            ctx.emitter.instruction("lsl x9, x9, #24");                         // park it in the csv_opts element lane
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rax - 8]");             // load the packed array kind word
            ctx.emitter.instruction("shr r9, 8");                               // move the value_type tag into the low bits
            ctx.emitter.instruction("and r9, 0xf");                             // isolate the 4-bit element value_type tag
            ctx.emitter.instruction("shl r9, 24");                              // park it in the csv_opts element lane
        }
    }
}

/// passing separator/enclosure/escape as a packed `csv_opts` word and eol as (ptr, len).
pub(crate) fn lower_fputcsv(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fputcsv", 2, 6)?;
    let stream = expect_operand(inst, 0)?;
    let fields = expect_operand(inst, 1)?;
    let arch = ctx.emitter.target.arch;

    load_stream_fd_to_result(ctx, stream, "fputcsv")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));            // save stream fd on stack
    // `scandir()`, `glob()` and `file()` answer `array<string>|false`, which is stored boxed, so
    // the boxed form is unwrapped here rather than rejected: the union guarantees the payload IS
    // a string array and the existing writer works on it unchanged once the box is off.
    // `fgetcsv()` reaches the `else` arm instead — its rows are `array<mixed>|false` so that a
    // blank line can come back as php's `[null]` — and the element lane is then read from the
    // array header at run time. `while (($row = fgetcsv($in)) !== false) fputcsv($out, $row);`
    // is the whole point of the pair and still round-trips, blank lines included.
    let field_tag = if boxed_string_array_union(&ctx.raw_value_php_type(fields)?) {
        ctx.load_value_to_result(fields)?;
        emit_unwrap_boxed_string_array(ctx, "fputcsv");
        Some(CSV_FIELD_TAG_STRING)
    } else {
        let declared = ctx.value_php_type(fields)?;
        ctx.load_value_to_result(fields)?;
        let tag = csv_field_value_type_tag(&declared, "fputcsv fields")?;
        if tag.is_none() {
            // A gradually-typed row arrives BOXED — `foreach ([[1, 2]] as $row)` hands over a
            // Mixed cell, not the inner array. The header read below must see the array itself.
            emit_unwrap_boxed_string_array(ctx, "fputcsv");
        }
        tag
    };
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));            // save fields array pointer
    if field_tag.is_none() {
        // The element lane is pushed ON TOP of the array pointer so the packing step, which pops
        // the three delimiter bytes it pushed after this, finds the tag waiting directly beneath.
        emit_csv_field_tag_from_header(ctx);
        let tag_reg = match arch {
            Arch::AArch64 => "x9",
            Arch::X86_64 => "r9",
        };
        abi::emit_push_reg(ctx.emitter, tag_reg);                                 // save the element value_type read from the header
    }

    // -- extract first byte of separator / enclosure / escape (or default) --
    //
    // The DEFAULT escape is `"\\"`, not the zero byte that means RFC 4180 doubling. Defaulting
    // to doubling made `fputcsv($h, ['a\\"b'])` write `"a\""b"` where php writes `"a\"b"`: the
    // escape already neutralizes the quote, so php never doubles it.
    emit_csv_control_bytes(ctx, inst, "fputcsv", &STREAM_CSV_CONTROLS)?;
    emit_csv_escape_deprecation(ctx, inst, "fputcsv", 4);

    // -- pack csv_opts = (esc << 16) | (enc << 8) | sep --
    match arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x2");                                // pop escape byte
            ctx.emitter.instruction("lsl x2, x2, #16");                         // shift escape to bits 16..23
            abi::emit_pop_reg(ctx.emitter, "x0");                                // pop enclosure byte
            ctx.emitter.instruction("orr x2, x2, x0, lsl #8");                  // include enclosure in csv_opts
            abi::emit_pop_reg(ctx.emitter, "x0");                                // pop separator byte
            ctx.emitter.instruction("orr x2, x2, x0");                          // complete csv_opts in x2
            match field_tag {
                // Tag 0 (int) is already the cleared lane, and AArch64 cannot encode `orr #0`.
                Some(0) => {}
                Some(tag) => ctx
                    .emitter
                    .instruction(&format!("orr x2, x2, #{}", (tag as i64) << 24)), // stamp the element value_type into bits 24..27
                None => {
                    abi::emit_pop_reg(ctx.emitter, "x0");                        // pop the element value_type read from the header
                    ctx.emitter.instruction("orr x2, x2, x0");                   // it arrives already shifted into bits 24..27
                }
            }
            abi::emit_push_reg(ctx.emitter, "x2");                              // save packed csv_opts
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rax");                               // pop escape byte
            ctx.emitter.instruction("shl rax, 16");                             // shift escape to bits 16..23
            ctx.emitter.instruction("mov rdx, rax");                            // start accumulating csv_opts
            abi::emit_pop_reg(ctx.emitter, "rax");                               // pop enclosure byte
            ctx.emitter.instruction("shl rax, 8");                              // shift enclosure to bits 8..15
            ctx.emitter.instruction("or rdx, rax");                             // include enclosure in csv_opts
            abi::emit_pop_reg(ctx.emitter, "rax");                               // pop separator byte
            ctx.emitter.instruction("or rdx, rax");                             // complete csv_opts in rdx
            match field_tag {
                // Tag 0 (int) is already the cleared lane; mirror the AArch64 skip so both
                // architectures emit the same packing for an int-array row.
                Some(0) => {}
                Some(tag) => ctx
                    .emitter
                    .instruction(&format!("or rdx, {}", (tag as i64) << 24)),   // stamp the element value_type into bits 24..27
                None => {
                    abi::emit_pop_reg(ctx.emitter, "rax");                       // pop the element value_type read from the header
                    ctx.emitter.instruction("or rdx, rax");                      // it arrives already shifted into bits 24..27
                }
            }
            abi::emit_push_reg(ctx.emitter, "rdx");                             // save packed csv_opts
        }
    }

    // -- push eol (ptr, len) or (0, 0) for default --
    if inst.operands.len() > 5 {
        let eol = expect_operand(inst, 5)?;
        load_string_to_result(ctx, eol, "fputcsv eol")?;
        match arch {
            Arch::AArch64 => {
                abi::emit_push_reg(ctx.emitter, "x1");                          // save eol string pointer
                abi::emit_push_reg(ctx.emitter, "x2");                          // save eol string length
            }
            Arch::X86_64 => {
                abi::emit_push_reg(ctx.emitter, "rax");                         // save eol string pointer
                abi::emit_push_reg(ctx.emitter, "rdx");                         // save eol string length
            }
        }
    } else {
        // An ABSENT `$eol` is php's `"\n"`, but an EMPTY one writes no terminator at all —
        // `fputcsv($h, ["a", "b"], ",", '"', "\\", "")` answers 3, not 4. A zero LENGTH cannot
        // tell the two apart, and neither can the pointer: a materialized empty string leaves
        // it undefined. The absent case is therefore marked by a NEGATIVE length, which no
        // real string can have, and the helper reads the sign rather than the pointer.
        match arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, #0");                          // no eol string to point at
                abi::emit_push_reg(ctx.emitter, "x0");                          // push eol ptr
                ctx.emitter.instruction("mov x0, #-1");                         // a negative length means the ARGUMENT was absent
                abi::emit_push_reg(ctx.emitter, "x0");                          // push eol len
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rax, 0");                          // no eol string to point at
                abi::emit_push_reg(ctx.emitter, "rax");                         // push eol ptr
                ctx.emitter.instruction("mov rax, -1");                         // a negative length means the ARGUMENT was absent
                abi::emit_push_reg(ctx.emitter, "rax");                         // push eol len
            }
        }
    }

    // -- pop all into ABI registers: fd, arr, csv_opts, eol_ptr, eol_len --
    match arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x4");                                // eol length -> arg5
            abi::emit_pop_reg(ctx.emitter, "x3");                                // eol pointer -> arg4
            abi::emit_pop_reg(ctx.emitter, "x2");                                // csv_opts -> arg3
            abi::emit_pop_reg(ctx.emitter, "x1");                                // fields array -> arg2
            abi::emit_pop_reg(ctx.emitter, "x0");                                // stream fd -> arg1
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "r8");                                // eol length -> arg5
            abi::emit_pop_reg(ctx.emitter, "rcx");                               // eol pointer -> arg4
            abi::emit_pop_reg(ctx.emitter, "rdx");                               // csv_opts -> arg3
            abi::emit_pop_reg(ctx.emitter, "rsi");                               // fields array -> arg2
            abi::emit_pop_reg(ctx.emitter, "rdi");                               // stream fd -> arg1
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fputcsv");                           // call the CSV row writer runtime
    store_if_result(ctx, inst)
}

/// Lowers `fpassthru(stream)` through the remaining-bytes stream runtime helper.
pub(crate) fn lower_fpassthru(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "fpassthru", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_open_stream_handle_to_result(ctx, stream, "fpassthru")?;
    emit_fpassthru_dispatch(ctx);
    store_if_result(ctx, inst)
}
