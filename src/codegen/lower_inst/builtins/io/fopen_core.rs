//! Purpose:
//! Core fopen dispatch and php filter URL parsing.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Lowers `fopen(filename, mode)` and boxes stream resources or PHP false.
pub(crate) fn lower_fopen(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fopen", 2, 4)?;
    let filename = expect_operand(inst, 0)?;
    let mode = expect_operand(inst, 1)?;
    let filename_literal = optional_const_string_operand(ctx, filename)?;
    if let Some(path) = filename_literal.as_deref() {
        if path.starts_with("php://filter/") {
            return lower_literal_php_filter_fopen(ctx, inst, path);
        }
        if let Some(fd) = php_standard_stream_fd(path).or_else(|| php_fd_stream(path)) {
            emit_owned_descriptor(ctx, fd);
            box_stream_fd_or_false_result(ctx, "fopen");
            return store_if_result(ctx, inst);
        }
        if is_php_memory_stream(path) {
            abi::emit_call_label(ctx.emitter, "__rt_tmpfile");
            box_stream_fd_or_false_result(ctx, "fopen");
            return store_if_result(ctx, inst);
        }
        if path.starts_with("data://") {
            return lower_literal_data_fopen(ctx, inst, path);
        }
        if path.starts_with("ftp://") {
            return lower_literal_ftp_fopen(ctx, inst, path);
        }
        if path.starts_with("phar://") {
            if literal_fopen_mode_is_write(ctx, mode)? {
                return lower_literal_phar_fopen_write(ctx, inst, path);
            }
            return lower_literal_phar_fopen_read(ctx, inst, path);
        }
        if path.starts_with("http://") {
            return lower_literal_http_fopen(ctx, inst, path);
        }
        if path.starts_with("compress.zlib://") {
            return lower_literal_compress_zlib_fopen(ctx, inst, path);
        }
        if path.starts_with("compress.bzip2://") {
            return lower_literal_compress_bzip2_fopen(ctx, inst, path);
        }
    }
    if filename_literal.is_none() {
        publish_dynamic_phar_function_pointers(ctx);
        publish_dynamic_phar_write_function_pointer(ctx);
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, filename, "fopen filename")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, mode, "fopen mode")?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the mode pointer in the runtime helper's secondary string slot
            ctx.emitter.instruction("mov x4, x2");                              // pass the mode length in the runtime helper's secondary string slot
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, filename, "fopen filename")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, mode, "fopen mode")?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the mode pointer while the filename remains on the stack
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the mode length while the filename remains on the stack
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fopen_maybe_phar");
    box_stream_fd_or_false_result(ctx, "fopen");
    store_if_result(ctx, inst)
}

/// Opens an independently owned duplicate for php standard-stream and fd wrapper literals.
fn emit_owned_descriptor(ctx: &mut FunctionContext<'_>, fd: i64) {
    let argument = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_load_int_immediate(ctx.emitter, argument, fd);
    let symbol = ctx.emitter.target.extern_symbol("dup");
    abi::emit_call_label(ctx.emitter, &symbol);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sxtw x0, w0");                             // preserve a negative C int failure for resource boxing
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("movsxd rax, eax");                         // preserve a negative C int failure for resource boxing
        }
    }
}

/// Emits the boxed `fopen()` result for a compile-time literal path without storing it.
pub(super) fn emit_literal_fopen_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    path: &str,
) -> Result<()> {
    let mode = expect_operand(inst, 1)?;
    if let Some(fd) = php_standard_stream_fd(path).or_else(|| php_fd_stream(path)) {
        emit_owned_descriptor(ctx, fd);
        box_stream_fd_or_false_result(ctx, "fopen");
        return Ok(());
    }
    if is_php_memory_stream(path) {
        abi::emit_call_label(ctx.emitter, "__rt_tmpfile");
        box_stream_fd_or_false_result(ctx, "fopen");
        return Ok(());
    }
    if path.starts_with("data://") {
        return emit_literal_data_fopen_result(ctx, path);
    }
    if path.starts_with("ftp://") {
        return emit_literal_ftp_fopen_result(ctx, path);
    }
    if path.starts_with("phar://") {
        if literal_fopen_mode_is_write(ctx, mode)? {
            return emit_literal_phar_fopen_write_result(ctx, path);
        }
        return emit_literal_phar_fopen_read_result(ctx, path);
    }
    if path.starts_with("http://") {
        return emit_literal_http_fopen_result(ctx, path);
    }
    emit_runtime_fopen_literal_result(ctx, path, mode)
}

/// Emits a runtime `fopen()` call for a literal path and the caller's mode operand.
pub(super) fn emit_runtime_fopen_literal_result(
    ctx: &mut FunctionContext<'_>,
    path: &str,
    mode: ValueId,
) -> Result<()> {
    let (path_label, path_len) = ctx.data.add_string(path.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &path_label);
            ctx.emitter.instruction(&format!("mov x2, #{}", path_len));         // pass the literal fopen path byte length
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, mode, "fopen mode")?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the fopen mode pointer with the literal path
            ctx.emitter.instruction("mov x4, x2");                              // pass the fopen mode length with the literal path
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rax", &path_label);
            ctx.emitter.instruction(&format!("mov rdx, {}", path_len));         // pass the literal fopen path byte length
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, mode, "fopen mode")?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the fopen mode pointer with the literal path
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the fopen mode length with the literal path
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fopen_maybe_phar");
    box_stream_fd_or_false_result(ctx, "fopen");
    Ok(())
}

/// Lowers a literal `fopen("php://filter/...", ...)` by opening and filtering `resource=`.
pub(super) fn lower_literal_php_filter_fopen(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    path: &str,
) -> Result<()> {
    let Some((mode_bits, filter_id, resource)) = parse_php_filter_url(path) else {
        emit_fd_result(ctx, -1);
        box_stream_fd_or_false_result(ctx, "fopen_php_filter");
        return store_if_result(ctx, inst);
    };
    emit_literal_fopen_result(ctx, inst, &resource)?;
    if mode_bits != 0 {
        emit_php_filter_table_stamps(ctx, mode_bits, filter_id);
    }
    store_if_result(ctx, inst)
}

/// Parses `php://filter/[read=|write=]filter/resource=path` for literal `fopen`.
pub(super) fn parse_php_filter_url(path: &str) -> Option<(u8, u8, String)> {
    let spec = path.strip_prefix("php://filter/")?;
    let (filter_part, resource) = spec.split_once("/resource=")?;
    if resource.is_empty() || resource.starts_with("php://filter") {
        return None;
    }
    let (mode_bits, filters) = if let Some(filters) = filter_part.strip_prefix("read=") {
        (1u8, filters)
    } else if let Some(filters) = filter_part.strip_prefix("write=") {
        (2u8, filters)
    } else {
        (3u8, filter_part)
    };
    let first_filter = filters.split('|').next().unwrap_or("");
    let filter_id = stream_filter_id(first_filter).unwrap_or(0);
    let mode_bits = if filter_id == 0 { 0 } else { mode_bits };
    Some((mode_bits, filter_id, resource.to_string()))
}

