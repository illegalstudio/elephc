//! Purpose:
//! Stream copy, line reads, metadata, and registry queries.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Lowers `stream_copy_to_stream(from, to, length?, offset?)` through wrapper-aware read/write loops.
pub(crate) fn lower_stream_copy_to_stream(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_copy_to_stream", 2, 4)?;
    let source = expect_operand(inst, 0)?;
    let dest = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, source, "stream_copy_to_stream")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    load_stream_fd_to_result(ctx, dest, "stream_copy_to_stream")?;
    emit_stream_copy_frame_enter(ctx);
    materialize_stream_copy_length(ctx, inst)?;
    if inst.operands.len() >= 4 {
        let offset = expect_operand(inst, 3)?;
        require_int(
            ctx.load_value_to_result(offset)?.codegen_repr(),
            "stream_copy_to_stream offset",
        )?;
        let skip_seek = ctx.next_label("scs_skip_seek");
        let wrap_seek = ctx.next_label("scs_wrap_seek");
        let seek_failed = ctx.next_label("scs_seek_failed");
        let boxed_done = ctx.next_label("scs_boxed_done");
        lower_stream_copy_seek(ctx, &skip_seek, &wrap_seek, &seek_failed);
        lower_stream_copy_loop_and_box(ctx, &seek_failed, &boxed_done);
    } else {
        let seek_failed = ctx.next_label("scs_seek_unreachable");
        let boxed_done = ctx.next_label("scs_boxed_done");
        lower_stream_copy_loop_and_box(ctx, &seek_failed, &boxed_done);
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_get_line(stream, length, ending?)`.
pub(crate) fn lower_stream_get_line(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_get_line", 2, 3)?;
    let stream = expect_operand(inst, 0)?;
    let length = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "stream_get_line")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int(
        ctx.load_value_to_result(length)?.codegen_repr(),
        "stream_get_line length",
    )?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    if inst.operands.len() == 3 {
        let ending = expect_operand(inst, 2)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                load_string_to_result(ctx, ending, "stream_get_line ending")?;
                ctx.emitter.instruction("mov x3, x2");                          // pass the ending-delimiter byte length to the runtime helper
                ctx.emitter.instruction("mov x2, x1");                          // pass the ending-delimiter pointer to the runtime helper
                abi::emit_pop_reg(ctx.emitter, "x1");
                abi::emit_pop_reg(ctx.emitter, "x0");
            }
            Arch::X86_64 => {
                load_string_to_result(ctx, ending, "stream_get_line ending")?;
                ctx.emitter.instruction("mov rcx, rdx");                        // pass the ending-delimiter byte length to the runtime helper
                ctx.emitter.instruction("mov rdx, rax");                        // pass the ending-delimiter pointer to the runtime helper
                abi::emit_pop_reg(ctx.emitter, "rsi");
                abi::emit_pop_reg(ctx.emitter, "rdi");
            }
        }
    } else {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x2, #0");                          // signal that no ending delimiter was supplied
                ctx.emitter.instruction("mov x3, #0");                          // signal a zero-length ending delimiter
                abi::emit_pop_reg(ctx.emitter, "x1");
                abi::emit_pop_reg(ctx.emitter, "x0");
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("xor edx, edx");                        // signal that no ending delimiter was supplied
                ctx.emitter.instruction("xor ecx, ecx");                        // signal a zero-length ending delimiter
                abi::emit_pop_reg(ctx.emitter, "rsi");
                abi::emit_pop_reg(ctx.emitter, "rdi");
            }
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_get_line");
    store_if_result(ctx, inst)
}

/// Lowers `stream_get_meta_data(stream)` through the metadata runtime helper.
pub(crate) fn lower_stream_get_meta_data(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_get_meta_data", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "stream_get_meta_data")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the descriptor to the stream metadata helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_get_meta_data");
    store_if_result(ctx, inst)
}

/// Returns builtin protocols plus the currently registered userspace protocols.
pub(crate) fn lower_stream_get_wrappers(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_get_wrappers", 0)?;
    emit_static_string_array(
        ctx,
        &[
            "file", "php", "data", "ftp", "http", "https", "ftps",
            "compress.zlib", "compress.bzip2", "phar", "glob",
        ],
    );
    append_registered_wrappers(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `stream_get_transports()` to the static transport list.
pub(crate) fn lower_stream_get_transports(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_get_transports", 0)?;
    emit_static_string_array(
        ctx,
        &[
            "tcp", "udp", "unix", "udg", "tls", "ssl", "sslv2", "sslv3",
            "tlsv1.0", "tlsv1.1", "tlsv1.2", "tlsv1.3",
        ],
    );
    store_if_result(ctx, inst)
}

/// Lowers `stream_get_filters()` to the static built-in filter list.
pub(crate) fn lower_stream_get_filters(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_get_filters", 0)?;
    emit_static_string_array(
        ctx,
        &[
            "string.toupper",
            "string.tolower",
            "string.rot13",
            "string.strip_tags",
            "convert.base64-encode",
            "convert.base64-decode",
            "convert.quoted-printable-encode",
            "convert.quoted-printable-decode",
            "convert.iconv.*",
            "dechunk",
            "zlib.deflate",
            "zlib.inflate",
            "bzip2.compress",
            "bzip2.decompress",
        ],
    );
    store_if_result(ctx, inst)
}

/// Copies live registration names into the fresh array, preserving it across growth and calls.
fn append_registered_wrappers(ctx: &mut FunctionContext<'_>) {
    let scan = ctx.next_label("stream_wrappers_scan");
    let next = ctx.next_label("stream_wrappers_next");
    let done = ctx.next_label("stream_wrappers_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #32");                         // reserve the owned result and registry cursor
            ctx.emitter.instruction("str x0, [sp]");                            // retain the array pointer across string appends
            ctx.emitter.instruction("str xzr, [sp, #8]");                       // begin with the first registration slot
            ctx.emitter.label(&scan);
            ctx.emitter.instruction("ldr x9, [sp, #8]");                        // reload the cursor after any allocating append
            ctx.emitter.instruction("cmp x9, #64");                             // bound the scan by the registration table capacity
            ctx.emitter.instruction(&format!("b.hs {done}"));                   // finish after inspecting every slot
            abi::emit_symbol_address(ctx.emitter, "x10", "_user_wrappers");
            ctx.emitter.instruction("add x10, x10, x9, lsl #5");                // select one four-word protocol/class registration
            ctx.emitter.instruction("ldr x1, [x10]");                           // borrow the registered protocol bytes
            ctx.emitter.instruction(&format!("cbz x1, {next}"));                // empty slots contribute no protocol
            ctx.emitter.instruction("ldr x2, [x10, #8]");                       // load the protocol byte length
            ctx.emitter.instruction("ldr x0, [sp]");                            // pass the current growable result array
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
            ctx.emitter.instruction("str x0, [sp]");                            // preserve the possibly relocated result
            ctx.emitter.label(&next);
            ctx.emitter.instruction("ldr x9, [sp, #8]");                        // reload the cursor clobbered by the runtime call
            ctx.emitter.instruction("add x9, x9, #1");                          // advance to the next registration slot
            ctx.emitter.instruction("str x9, [sp, #8]");                        // keep the cursor across the next append
            ctx.emitter.instruction(&format!("b {scan}"));                      // inspect the next registration
            ctx.emitter.label(&done);
            ctx.emitter.instruction("ldr x0, [sp]");                            // return the independently owned protocol array
            ctx.emitter.instruction("add sp, sp, #32");                         // retire the aligned temporary frame
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 32");                             // reserve aligned result and cursor storage
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // retain the fresh result across string appends
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], 0");              // begin with the first registration slot
            ctx.emitter.label(&scan);
            ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 8]");             // reload the cursor after an allocating append
            ctx.emitter.instruction("cmp r9, 64");                              // bound the scan by the registration table capacity
            ctx.emitter.instruction(&format!("jae {done}"));                    // finish after inspecting every slot
            abi::emit_symbol_address(ctx.emitter, "r10", "_user_wrappers");
            ctx.emitter.instruction("shl r9, 5");                               // scale the index by the four-word registration size
            ctx.emitter.instruction("add r10, r9");                             // address the selected registration
            ctx.emitter.instruction("mov rsi, QWORD PTR [r10]");                // borrow the registered protocol bytes
            ctx.emitter.instruction("test rsi, rsi");                           // test whether this registration slot is occupied
            ctx.emitter.instruction(&format!("jz {next}"));                     // empty slots contribute no protocol
            ctx.emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");            // pass the protocol byte length
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // pass the current growable result array
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // preserve the possibly relocated result
            ctx.emitter.label(&next);
            ctx.emitter.instruction("add QWORD PTR [rsp + 8], 1");              // advance the durable registration cursor
            ctx.emitter.instruction(&format!("jmp {scan}"));                    // inspect the next registration
            ctx.emitter.label(&done);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                // return the independently owned protocol array
            ctx.emitter.instruction("add rsp, 32");                             // retire the aligned temporary frame
        }
    }
}
