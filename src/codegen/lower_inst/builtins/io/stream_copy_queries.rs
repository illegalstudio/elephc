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
    // php answers `false` at EOF with nothing read, and `""` for an empty line — two results
    // the raw (pointer, length) pair separates by its POINTER, exactly as `getenv` does for an
    // absent variable. A length test would call an empty line EOF.
    box_string_or_false_on_null_pointer_result(ctx, "stream_get_line");
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

/// Lowers `stream_get_wrappers()` to the static built-in wrapper list.
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
