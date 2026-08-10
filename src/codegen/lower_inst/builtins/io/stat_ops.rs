//! Purpose:
//! Filesystem stat, access, link, and file-type builtins.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

use super::wrapper_dispatch::{URL_STAT_FLAGS_LINK, URL_STAT_FLAGS_NOCACHE};

/// Lowers `getcwd()` through the target-aware runtime helper.
pub(crate) fn lower_getcwd(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "getcwd", 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_getcwd");
    store_if_result(ctx, inst)
}

/// Lowers `sys_get_temp_dir()` as the project's hardcoded `/tmp` string.
pub(crate) fn lower_sys_get_temp_dir(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "sys_get_temp_dir", 0)?;
    let (label, len) = ctx.data.add_string(b"/tmp");
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    store_if_result(ctx, inst)
}

/// Lowers `tmpfile()` and boxes the anonymous stream descriptor or PHP false.
pub(crate) fn lower_tmpfile(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "tmpfile", 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_tmpfile");
    box_stream_fd_or_false_result(ctx, "tmpfile");
    store_if_result(ctx, inst)
}

/// Lowers `filesize(path)` through the target-aware runtime stat helper.
pub(crate) fn lower_filesize(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_filesize_with_wrapper(ctx, inst)
}

/// Lowers `filemtime(path)` through the target-aware runtime stat helper.
pub(crate) fn lower_filemtime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::wrapper_dispatch::lower_filemtime_with_wrapper(ctx, inst)
}

/// Lowers `linkinfo(path)` through the target-aware runtime lstat helper.
pub(crate) fn lower_linkinfo(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_int(ctx, inst, "linkinfo", "__rt_linkinfo")
}

/// Lowers `symlink(target, link)` through the target-aware libc wrapper.
pub(crate) fn lower_symlink(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_path_call(ctx, inst, "symlink", "__rt_symlink")
}

/// Lowers `link(oldpath, newpath)` through the target-aware libc wrapper.
pub(crate) fn lower_link(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_path_call(ctx, inst, "link", "__rt_link")
}

/// Lowers `readlink(path)` and boxes the owned runtime string-or-false result.
pub(crate) fn lower_readlink(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "readlink", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "readlink")?;
    abi::emit_call_label(ctx.emitter, "__rt_readlink");
    box_owned_string_or_false_result(ctx, "readlink");
    store_if_result(ctx, inst)
}

/// Lowers `fileatime(path)` and boxes the runtime integer-or-false result.
pub(crate) fn lower_fileatime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_stat_int_or_false(ctx, inst, "fileatime", "__rt_fileatime")
}

/// Lowers `filectime(path)` and boxes the runtime integer-or-false result.
pub(crate) fn lower_filectime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_stat_int_or_false(ctx, inst, "filectime", "__rt_filectime")
}

/// Lowers `fileperms(path)` and boxes the runtime integer-or-false result.
pub(crate) fn lower_fileperms(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_stat_int_or_false(ctx, inst, "fileperms", "__rt_fileperms")
}

/// Lowers `fileowner(path)` and boxes the runtime integer-or-false result.
pub(crate) fn lower_fileowner(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_stat_int_or_false(ctx, inst, "fileowner", "__rt_fileowner")
}

/// Lowers `filegroup(path)` and boxes the runtime integer-or-false result.
pub(crate) fn lower_filegroup(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_stat_int_or_false(ctx, inst, "filegroup", "__rt_filegroup")
}

/// Lowers `fileinode(path)` and boxes the runtime integer-or-false result.
pub(crate) fn lower_fileinode(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_stat_int_or_false(ctx, inst, "fileinode", "__rt_fileinode")
}

/// Lowers `filetype(path)` and boxes the runtime string-or-false result.
pub(crate) fn lower_filetype(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "filetype", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "filetype")?;
    abi::emit_call_label(ctx.emitter, "__rt_filetype");
    box_stat_string_or_false_result(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `stat(path)` and boxes the runtime stat array or PHP false result.
pub(crate) fn lower_stat(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::wrapper_dispatch::lower_path_stat_with_wrapper(
        ctx,
        inst,
        "stat",
        "__rt_stat_array",
        URL_STAT_FLAGS_NOCACHE,
    )
}

/// Lowers `lstat(path)` and boxes the runtime lstat array or PHP false result.
pub(crate) fn lower_lstat(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::wrapper_dispatch::lower_path_stat_with_wrapper(
        ctx,
        inst,
        "lstat",
        "__rt_lstat_array",
        URL_STAT_FLAGS_LINK,
    )
}

/// Lowers `fstat(stream)` and boxes the runtime stat array or PHP false result.
pub(crate) fn lower_fstat(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "fstat", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "fstat")?;
    let wrapper_label = ctx.next_label("fstat_user_wrapper");
    let done_label = ctx.next_label("fstat_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("b.ge {}", wrapper_label));        // dispatch synthetic handles to stream_stat
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rax, r9");                             // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("jge {}", wrapper_label));         // dispatch synthetic handles to stream_stat
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fstat_array");
    box_stat_array_or_false_result(ctx);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip wrapper stat after the native helper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip wrapper stat after the native helper
        }
    }
    ctx.emitter.label(&wrapper_label);
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the synthetic wrapper descriptor to the stat helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fstat");
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers `clearstatcache(...)` as an ordered no-op after EIR operand evaluation.
pub(crate) fn lower_clearstatcache(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "clearstatcache expected at most 2 args, got {}",
            inst.operands.len()
        )));
    }
    store_if_result(ctx, inst)
}

/// Lowers `is_file(path)` through the target-aware runtime stat helper.
pub(crate) fn lower_is_file(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_is_file_with_wrapper(ctx, inst)
}

/// Lowers `is_dir(path)` through the target-aware runtime stat helper.
pub(crate) fn lower_is_dir(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::wrapper_dispatch::lower_is_dir_with_wrapper(ctx, inst)
}

/// Lowers `is_readable(path)` through the target-aware runtime access helper.
pub(crate) fn lower_is_readable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::wrapper_dispatch::lower_is_readable_with_wrapper(ctx, inst)
}

/// Lowers `is_writable(path)` through the target-aware runtime access helper.
pub(crate) fn lower_is_writable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::wrapper_dispatch::lower_is_writable_with_wrapper(ctx, inst, "is_writable")
}

/// Lowers `is_writeable(path)`, PHP's alias of `is_writable(path)`.
pub(crate) fn lower_is_writeable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::wrapper_dispatch::lower_is_writable_with_wrapper(ctx, inst, "is_writeable")
}

/// Lowers `is_executable(path)` through the target-aware runtime access helper.
pub(crate) fn lower_is_executable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::wrapper_dispatch::lower_is_executable_with_wrapper(ctx, inst)
}

/// Lowers `is_link(path)` through the target-aware runtime lstat helper.
pub(crate) fn lower_is_link(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "is_link", "__rt_is_link")
}

