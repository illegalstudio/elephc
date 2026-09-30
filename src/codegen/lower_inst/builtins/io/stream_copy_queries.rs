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
    load_open_stream_handle_to_result(ctx, source, "stream_copy_to_stream")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    // php numbers this one `Argument #2 ($to)`, not `#1`.
    load_stream_fd_to_result_at(ctx, dest, "stream_copy_to_stream", 1)?;
    emit_stream_copy_frame_enter(ctx);
    // The PSFS code the copy consults at the end is a process-wide slot, and it lives in BSS —
    // it starts at ZERO, which IS PSFS_ERR_FATAL, and an earlier stream's dispatch would
    // otherwise still be showing. Reset it so only a refusal during THIS copy can be read back.
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_user_filter_last_psfs");
            ctx.emitter.instruction("mov x10, #2");                             // PSFS_PASS_ON
            ctx.emitter.instruction("str x10, [x9]");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_user_filter_last_psfs");
            ctx.emitter.instruction("mov QWORD PTR [r9], 2");                   // PSFS_PASS_ON
        }
    }
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

/// php-src's verbatim `ValueError` wording for `stream_get_line()` with a negative `$length`.
const STREAM_GET_LINE_NEGATIVE_LENGTH_MESSAGE: &str =
    "stream_get_line(): Argument #2 ($length) must be greater than or equal to 0";

/// php-src reads `PHP_SOCK_CHUNK_SIZE` bytes when the caller passes `$length` as zero.
const STREAM_GET_LINE_DEFAULT_CHUNK: i64 = 8192;

/// Lowers `stream_get_line(stream, length, ending?)`.
pub(crate) fn lower_stream_get_line(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_get_line", 2, 3)?;
    let stream = expect_operand(inst, 0)?;
    let length = expect_operand(inst, 1)?;
    load_open_stream_handle_to_result(ctx, stream, "stream_get_line")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int(
        ctx.load_value_to_result(length)?.codegen_repr(),
        "stream_get_line length",
    )?;
    // php-src rejects a negative budget outright, and reads its default chunk when the
    // caller passes zero — zero does NOT mean "read nothing".
    let length_reg = abi::int_result_reg(ctx.emitter);
    super::super::exceptions::emit_value_error_unless(
        ctx,
        super::super::exceptions::ValueGuard::SignedAtLeast(length_reg, 0),
        STREAM_GET_LINE_NEGATIVE_LENGTH_MESSAGE,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("mov x9, #{}", STREAM_GET_LINE_DEFAULT_CHUNK)); // php-src's default chunk
            ctx.emitter.instruction("cmp x0, #0");                              // did the caller ask for zero bytes?
            ctx.emitter.instruction("csel x0, x9, x0, eq");                     // zero means the default chunk, not an empty read
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov ecx, {}", STREAM_GET_LINE_DEFAULT_CHUNK)); // php-src's default chunk
            ctx.emitter.instruction("test rax, rax");                           // did the caller ask for zero bytes?
            ctx.emitter.instruction("cmove rax, rcx");                          // zero means the default chunk, not an empty read
        }
    }
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
    box_stream_string_or_false_on_unconsumed_result(ctx, "stream_get_line");
    store_if_result(ctx, inst)
}

/// Lowers `stream_get_meta_data(stream)` through the metadata runtime helper.
pub(crate) fn lower_stream_get_meta_data(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_get_meta_data", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_open_stream_handle_to_result(ctx, stream, "stream_get_meta_data")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the opaque stream handle to the metadata helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_get_meta_data");
    store_if_result(ctx, inst)
}

/// Lowers `stream_get_wrappers()` to the static built-in wrapper list, then
/// appends user-registered scheme names via the `__rt_stream_get_wrappers`
/// runtime helper.
pub(crate) fn lower_stream_get_wrappers(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_get_wrappers", 0)?;
    emit_static_string_array(ctx, crate::types::stream_constants::STREAM_WRAPPERS);
    // -- append user-registered scheme names from the runtime registration table --
    abi::emit_call_label(ctx.emitter, "__rt_stream_get_wrappers");
    store_if_result(ctx, inst)
}

/// Lowers `stream_get_transports()` to the static transport list.
pub(crate) fn lower_stream_get_transports(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_get_transports", 0)?;
    emit_static_string_array(ctx, crate::types::stream_constants::STREAM_TRANSPORTS);
    store_if_result(ctx, inst)
}

/// Lowers `stream_get_filters()` to the static built-in filter list, then
/// appends user-registered filter names via the `__rt_stream_get_filters`
/// runtime helper.
pub(crate) fn lower_stream_get_filters(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_get_filters", 0)?;
    emit_static_string_array(ctx, crate::types::stream_constants::STREAM_FILTERS);
    // -- append user-registered filter names from the runtime registry --
    abi::emit_call_label(ctx.emitter, "__rt_stream_get_filters");
    store_if_result(ctx, inst)
}

/// Lowers php 8.4's `http_get_last_response_headers()` to its runtime wrapper.
///
/// The result is `?array`, so it comes back boxed: the helper answers a Mixed
/// `null` while no response has been buffered and a Mixed indexed array once one
/// has. That is a real distinction in php — `NULL` before the first request, not
/// an empty array.
pub(crate) fn lower_http_get_last_response_headers(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "http_get_last_response_headers", 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_http_get_last_response_headers");
    store_if_result(ctx, inst)
}

/// Lowers php 8.4's `http_clear_last_response_headers()` to its runtime wrapper.
///
/// php's return type is `void`; the helper writes nothing back, so there is no
/// result to store beyond whatever the caller's void handling wants.
pub(crate) fn lower_http_clear_last_response_headers(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "http_clear_last_response_headers", 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_http_clear_last_response_headers");
    store_if_result(ctx, inst)
}
