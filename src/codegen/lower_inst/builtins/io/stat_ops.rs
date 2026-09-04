//! Purpose:
//! Filesystem stat, access, link, and file-type builtins.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;
use crate::codegen_support::platform::Platform;

/// Byte length of the path `tmpfile()` resolves, fixed by the `/tmp/elephc-XXXXXX` template
/// the runtime helper hands to `mkstemp`, which substitutes the six Xs in place.
const TMPFILE_PATH_LEN: i64 = 18;

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
    // php answers TMPDIR when it is set and non-empty, with exactly ONE trailing slash
    // removed — `/tmp///` becomes `/tmp//`, not `/tmp` — and otherwise P_tmpdir, which keeps
    // its trailing slash. A hardcoded `/tmp` is not just a different string: on macOS php
    // hands out a private per-user directory, so every program sharing /tmp instead is a
    // behaviour change as well as a compliance one.
    let (name_label, name_len) = ctx.data.add_string(b"TMPDIR");
    let fallback: &[u8] = match ctx.emitter.target.platform {
        Platform::MacOS => b"/var/tmp/",
        _ => b"/tmp",
    };
    let (fallback_label, fallback_len) = ctx.data.add_string(fallback);
    let use_fallback = ctx.next_label("tmpdir_fallback");
    let done = ctx.next_label("tmpdir_done");
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &name_label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, name_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_getenv");                           // answers "" when unset
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x2, {}", use_fallback));      // unset or empty: P_tmpdir
            ctx.emitter.instruction("sub x9, x2, #1");                          // index of the last byte
            ctx.emitter.instruction("ldrb w10, [x1, x9]");
            ctx.emitter.instruction("cmp w10, #47");                            // '/'
            ctx.emitter.instruction(&format!("b.ne {}", done));                 // no trailing slash to drop
            ctx.emitter.instruction("mov x2, x9");                              // drop exactly one
            ctx.emitter.instruction(&format!("b {}", done));
            ctx.emitter.label(&use_fallback);
            abi::emit_symbol_address(ctx.emitter, "x1", &fallback_label);
            ctx.emitter.instruction(&format!("mov x2, #{}", fallback_len));
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rdx, rdx");
            ctx.emitter.instruction(&format!("jz {}", use_fallback));           // unset or empty: P_tmpdir
            ctx.emitter.instruction("mov r9, rdx");
            ctx.emitter.instruction("sub r9, 1");                               // index of the last byte
            ctx.emitter.instruction("movzx r10d, BYTE PTR [rax + r9]");
            ctx.emitter.instruction("cmp r10d, 47");                            // '/'
            ctx.emitter.instruction(&format!("jne {}", done));                  // no trailing slash to drop
            ctx.emitter.instruction("mov rdx, r9");                             // drop exactly one
            ctx.emitter.instruction(&format!("jmp {}", done));
            ctx.emitter.label(&use_fallback);
            abi::emit_symbol_address(ctx.emitter, "rax", &fallback_label);
            ctx.emitter.instruction(&format!("mov rdx, {}", fallback_len));
            ctx.emitter.label(&done);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `tmpfile()` and boxes the anonymous stream descriptor or PHP false.
pub(crate) fn lower_tmpfile(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "tmpfile", 0)?;
    // The NAMED helper: php keeps the file it created linked for as long as the handle lives, so
    // the URI it reports below names something `file_exists()`, `filesize()` and a second
    // `fopen()` can all reach. The anonymous `__rt_tmpfile` — unlinked on the spot — stays what
    // `data:` and the https reader use for scratch storage.
    abi::emit_call_label(ctx.emitter, "__rt_tmpfile_named");
    box_stream_fd_or_false_result(ctx, "tmpfile");
    // PHP reports the file `tmpfile()` created as the stream URI. The name only exists in the
    // buffer the helper published.
    emit_record_stream_meta_after_boxed_symbol(ctx, 0, "_tmpfile_last_path", TMPFILE_PATH_LEN);
    emit_record_stream_mode_literal_after_boxed(ctx, "r+b");
    emit_stream_owns_its_file_after_boxed(ctx);
    store_if_result(ctx, inst)
}

/// Marks the freshly boxed stream as owning the file its URI names.
///
/// The close path removes it then, which is when php removes it. Without the mark the file would
/// outlive the process; with it, and with the deterministic shutdown that already closes
/// request-owned resources at exit, a program that never calls `fclose()` still leaves nothing.
fn emit_stream_owns_its_file_after_boxed(ctx: &mut FunctionContext<'_>) {
    let done = ctx.next_label("tmpfile_owns_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // inspect the boxed result tag
            ctx.emitter.instruction("cmp x9, #9");                              // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("b.ne {}", done));                 // a failed tmpfile owns nothing
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("ldr x0, [x0, #8]");                        // the opaque stream handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_own_its_file");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp QWORD PTR [rax], 9");                  // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("jne {}", done));                  // a failed tmpfile owns nothing
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rax + 8]");            // the opaque stream handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_own_its_file");
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    ctx.emitter.label(&done);
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
    lower_filemtime_with_wrapper(ctx, inst)
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
    lower_fileatime_with_wrapper(ctx, inst)
}

/// Lowers `filectime(path)` and boxes the runtime integer-or-false result.
pub(crate) fn lower_filectime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_filectime_with_wrapper(ctx, inst)
}

/// Lowers `fileperms(path)` and boxes the runtime integer-or-false result.
pub(crate) fn lower_fileperms(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_fileperms_with_wrapper(ctx, inst)
}

/// Lowers `fileowner(path)` and boxes the runtime integer-or-false result.
pub(crate) fn lower_fileowner(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_fileowner_with_wrapper(ctx, inst)
}

/// Lowers `filegroup(path)` and boxes the runtime integer-or-false result.
pub(crate) fn lower_filegroup(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_filegroup_with_wrapper(ctx, inst)
}

/// Lowers `fileinode(path)` and boxes the runtime integer-or-false result.
pub(crate) fn lower_fileinode(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_fileinode_with_wrapper(ctx, inst)
}

/// Lowers `filetype(path)` and boxes the runtime string-or-false result.
pub(crate) fn lower_filetype(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_filetype_with_wrapper(ctx, inst)
}

/// Lowers `stat(path)` and boxes the runtime stat array or PHP false result.
pub(crate) fn lower_stat(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_stat_with_wrapper(ctx, inst)
}

/// Lowers `lstat(path)` and boxes the runtime lstat array or PHP false result.
pub(crate) fn lower_lstat(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_lstat_with_wrapper(ctx, inst)
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
    // php names the CALLER when the class has no `stream_stat`, so the head travels in.
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", "_uwmh_head_fstat");
            ctx.emitter.instruction(&format!(
                "mov x2, #{}",
                crate::codegen_support::runtime::data::WRAPPER_MISSING_HOOK_HEAD_FSTAT.len()
            ));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the synthetic wrapper descriptor to the stat helper
            abi::emit_symbol_address(ctx.emitter, "rsi", "_uwmh_head_fstat");
            ctx.emitter.instruction(&format!(
                "mov rdx, {}",
                crate::codegen_support::runtime::data::WRAPPER_MISSING_HOOK_HEAD_FSTAT.len()
            ));
        }
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
    // php's stat cache is what makes `filesize()` then `is_dir()` call a wrapper's `url_stat()`
    // ONCE, and this is the only thing that empties it. Its arguments are accepted and ignored:
    // MEASURED, `clearstatcache(true, "other://path")` clears an unrelated cached path too.
    abi::emit_call_label(ctx.emitter, "__rt_clear_stat_cache");
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
    lower_is_dir_with_wrapper(ctx, inst)
}

/// Lowers `is_readable(path)` through the target-aware runtime access helper.
pub(crate) fn lower_is_readable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_is_readable_with_wrapper(ctx, inst)
}

/// Lowers `is_writable(path)` through the target-aware runtime access helper.
pub(crate) fn lower_is_writable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_is_writable_with_wrapper(ctx, inst)
}

/// Lowers `is_writeable(path)`, PHP's alias of `is_writable(path)`.
pub(crate) fn lower_is_writeable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_is_writeable_with_wrapper(ctx, inst)
}

/// Lowers `is_executable(path)` through the target-aware runtime access helper.
pub(crate) fn lower_is_executable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_is_executable_with_wrapper(ctx, inst)
}

/// Lowers `is_link(path)` through the target-aware runtime lstat helper.
pub(crate) fn lower_is_link(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_is_link_with_wrapper(ctx, inst)
}

