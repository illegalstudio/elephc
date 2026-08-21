//! Purpose:
//! Filesystem mutations and path string builtins.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Lowers `file_exists(path)` through the target-aware runtime stat helper.
pub(crate) fn lower_file_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_file_exists_with_wrapper(ctx, inst)
}

/// Lowers `unlink(path, context)` through the target-aware runtime helper.
///
/// `$context` is accepted and IGNORED: php threads a stream context into the wrapper's `unlink()`
/// through `$this->context`, and elephc has no context plumbing on the path-op route yet. Refusing
/// the argument outright was worse — it made `unlink($p, $ctx)` a compile error on a signature php
/// documents.
pub(crate) fn lower_unlink(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "unlink", 1, 2)?;
    let path = expect_operand(inst, 0)?;
    let path_literal = optional_const_string_operand(ctx, path)?;
    let can_be_phar = path_literal
        .as_deref()
        .map(|path| path.starts_with("phar://"))
        .unwrap_or(true);
    if can_be_phar {
        publish_phar_delete_function_pointer(ctx);
    }
    emit_publish_missing_hook_message(
        ctx,
        "_uwmh_head_unlink",
        WRAPPER_MISSING_HOOK_HEAD_UNLINK.len(),
        "_uwmh_tail_unlink",
        WRAPPER_MISSING_HOOK_TAIL_UNLINK.len(),
    );
    load_string_to_result(ctx, path, "unlink")?;
    if can_be_phar {
        emit_unlink_maybe_phar_dispatch(ctx);
    } else {
        emit_single_path_wrapper_dispatch(ctx, "__rt_unlink", STREAM_WRAPPER_UNLINK_SLOT);
    }
    store_if_result(ctx, inst)
}

/// Lowers `mkdir(path, permissions, recursive, context)` through the target-aware runtime helper.
///
/// `$permissions` and `$recursive` reach BOTH routes: the POSIX `mkdir` gets the mode and creates
/// missing parents when asked, and a userspace wrapper's `mkdir()` receives what php passes it —
/// measured on 8.5.6 as `($path, 511, 8)` by default, `($path, 493, 8)` for an explicit `0755`, and
/// `($path, 448, 9)` for `0700` recursive, i.e. `STREAM_REPORT_ERRORS | STREAM_MKDIR_RECURSIVE`.
/// `$context` is accepted and ignored (see [`lower_unlink`]).
pub(crate) fn lower_mkdir(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "mkdir", 1, 4)?;
    let path = expect_operand(inst, 0)?;
    let permissions = inst.operands.get(1).copied();
    let recursive = inst.operands.get(2).copied();
    emit_publish_missing_hook_message(
        ctx,
        "_uwmh_head_mkdir",
        WRAPPER_MISSING_HOOK_HEAD_MKDIR.len(),
        "_uwmh_tail_mkdir",
        WRAPPER_MISSING_HOOK_TAIL_MKDIR.len(),
    );
    load_string_to_result(ctx, path, "mkdir")?;
    emit_mkdir_wrapper_dispatch(ctx, permissions, recursive)?;
    store_if_result(ctx, inst)
}

/// Lowers `rmdir(path, context)` through the target-aware runtime helper.
///
/// php hands a wrapper's `rmdir()` an `$options` of `STREAM_REPORT_ERRORS` (8), measured on 8.5.6;
/// `$context` is accepted and ignored (see [`lower_unlink`]).
pub(crate) fn lower_rmdir(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "rmdir", 1, 2)?;
    let path = expect_operand(inst, 0)?;
    emit_publish_missing_hook_message(
        ctx,
        "_uwmh_head_rmdir",
        WRAPPER_MISSING_HOOK_HEAD_RMDIR.len(),
        "_uwmh_tail_rmdir",
        WRAPPER_MISSING_HOOK_TAIL_RMDIR.len(),
    );
    load_string_to_result(ctx, path, "rmdir")?;
    emit_single_path_wrapper_dispatch_with_options(
        ctx,
        "__rt_rmdir",
        STREAM_WRAPPER_RMDIR_SLOT,
        STREAM_REPORT_ERRORS,
        0,
    );
    store_if_result(ctx, inst)
}

/// Lowers `chdir(path)` through the target-aware runtime helper.
pub(crate) fn lower_chdir(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "chdir", "__rt_chdir")
}

/// Lowers `copy(source, dest)` through the target-aware runtime helper.
pub(crate) fn lower_copy(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_path_call_with_context(ctx, inst, "copy", "__rt_copy")
}

/// Lowers `rename(from, to)` through the target-aware runtime helper.
pub(crate) fn lower_rename(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_rename_with_wrapper(ctx, inst)
}

/// Lowers `tempnam(directory, prefix)` through the target-aware runtime helper.
pub(crate) fn lower_tempnam(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_path_call(ctx, inst, "tempnam", "__rt_tempnam")
}

/// Lowers `scandir(path)` through the target-aware runtime directory listing helper.
///
/// php's signature is `array|false`, so the raw pointer is boxed rather than stored bare:
/// the runtime answers NULL for a directory it cannot open, and the boxing turns that into
/// PHP false — which is what lets `scandir($d) === false`, the manual's own failure test,
/// finally fire. The success side boxes the indexed array as a tag-4 Mixed payload.
pub(crate) fn lower_scandir(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    // `$context` is the third parameter, accepted and ignored (see `lower_unlink`).
    super::super::ensure_arg_count_between(inst, "scandir", 1, 3)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "scandir")?;
    // $sorting_order rides beside the path pair, defaulting to SCANDIR_SORT_ASCENDING —
    // php sorts the listing unless SCANDIR_SORT_NONE asks it not to.
    match inst.operands.get(1).copied() {
        None => match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.emitter.instruction("mov x0, #0"),
            Arch::X86_64 => ctx.emitter.instruction("xor edi, edi"),
        },
        Some(order) => {
            match ctx.emitter.target.arch {
                Arch::AArch64 => abi::emit_push_reg_pair(ctx.emitter, "x1", "x2"),
                Arch::X86_64 => abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx"),
            }
            ctx.load_value_to_result(order)?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("mov x9, x0");                      // hold the order while the pair returns
                    abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
                    ctx.emitter.instruction("mov x0, x9");
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("mov rdi, rax");
                    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
                }
            }
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_scandir");
    // The shared helper makes the box the listing's sole owner; see its doc for the
    // copy-on-write consequence of leaving the creation reference alive.
    box_listing_or_false_result(ctx, "scandir");
    store_if_result(ctx, inst)
}

/// Lowers `glob(pattern)` through the target-aware runtime glob expansion helper.
pub(crate) fn lower_glob(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    // php's signature is array|false, so the listing is boxed like scandir's. The runtime
    // never produces the false today — a pattern with no matches answers the empty array,
    // exactly as php does — but the union is what the checker now declares.
    super::super::ensure_arg_count(inst, "glob", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "glob")?;
    abi::emit_call_label(ctx.emitter, "__rt_glob");
    box_listing_or_false_result(ctx, "glob");
    store_if_result(ctx, inst)
}

/// Lowers `chmod(path, mode)` through the target-aware runtime helper.
pub(crate) fn lower_chmod(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_chmod_with_wrapper(ctx, inst)
}

/// Lowers `chown(path, owner)` for integer UIDs and string user names.
pub(crate) fn lower_chown(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_chown_or_chgrp(ctx, inst, "chown", PrincipalKind::Owner)
}

/// Lowers `chgrp(path, group)` for integer GIDs and string group names.
pub(crate) fn lower_chgrp(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_chown_or_chgrp(ctx, inst, "chgrp", PrincipalKind::Group)
}

/// Lowers `lchown(path, owner)` for integer UIDs and string user names without following symlinks.
pub(crate) fn lower_lchown(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_lchown_or_lchgrp(ctx, inst, "lchown", PrincipalKind::Owner)
}

/// Lowers `lchgrp(path, group)` for integer GIDs and string group names without following symlinks.
pub(crate) fn lower_lchgrp(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_lchown_or_lchgrp(ctx, inst, "lchgrp", PrincipalKind::Group)
}

/// Lowers `umask(mask?)` through the target-aware runtime helper.
pub(crate) fn lower_umask(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "umask", 0, 1)?;
    if inst.operands.is_empty() {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, #0");                          // probe the current umask with a temporary zero mask
                abi::emit_call_label(ctx.emitter, "__rt_umask");
                ctx.emitter.instruction("stp x0, xzr, [sp, #-16]!");            // save the probed previous mask while restoring it
                ctx.emitter.instruction("ldr x0, [sp]");                        // pass the previous mask back to restore process state
                abi::emit_call_label(ctx.emitter, "__rt_umask");
                ctx.emitter.instruction("ldp x0, xzr, [sp], #16");              // return the originally probed mask to PHP
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("xor eax, eax");                        // probe the current umask with a temporary zero mask
                abi::emit_call_label(ctx.emitter, "__rt_umask");
                ctx.emitter.instruction("push rax");                            // save the probed previous mask while restoring it
                ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");            // pass the previous mask back to restore process state
                abi::emit_call_label(ctx.emitter, "__rt_umask");
                ctx.emitter.instruction("pop rax");                             // return the originally probed mask to PHP
            }
        }
        return store_if_result(ctx, inst);
    }
    let mask = expect_operand(inst, 0)?;
    require_int(ctx.load_value_to_result(mask)?.codegen_repr(), "umask mask")?;
    abi::emit_call_label(ctx.emitter, "__rt_umask");
    store_if_result(ctx, inst)
}

/// Lowers `touch(path, mtime?, atime?)` through the target-aware runtime helper.
pub(crate) fn lower_touch(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "touch", 1, 3)?;
    let path = expect_operand(inst, 0)?;
    emit_publish_missing_hook_message(
        ctx,
        "_uwmh_head_touch",
        WRAPPER_MISSING_HOOK_HEAD_TOUCH.len(),
        "_uwmh_tail_metadata",
        WRAPPER_MISSING_HOOK_TAIL_METADATA.len(),
    );
    load_string_to_result(ctx, path, "touch path")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_touch_args_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_touch_args_x86_64(ctx, inst)?,
    }
    emit_touch_wrapper_dispatch(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `basename(path, suffix?)` through the target-aware runtime helper.
pub(crate) fn lower_basename(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "basename", 1, 2)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "basename path")?;
    if inst.operands.len() == 2 {
        let suffix = expect_operand(inst, 1)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
                load_string_to_result(ctx, suffix, "basename suffix")?;
                ctx.emitter.instruction("mov x3, x1");                          // pass the suffix pointer in the runtime helper's secondary string slot
                ctx.emitter.instruction("mov x4, x2");                          // pass the suffix length in the runtime helper's secondary string slot
                abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            }
            Arch::X86_64 => {
                abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
                load_string_to_result(ctx, suffix, "basename suffix")?;
                ctx.emitter.instruction("mov rdi, rax");                        // pass the suffix pointer while the path remains on the stack
                ctx.emitter.instruction("mov rsi, rdx");                        // pass the suffix length while the path remains on the stack
                abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            }
        }
    } else {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x3, #0");                          // signal that no suffix pointer was supplied
                ctx.emitter.instruction("mov x4, #0");                          // signal that no suffix length was supplied
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("xor edi, edi");                        // signal that no suffix pointer was supplied
                ctx.emitter.instruction("xor esi, esi");                        // signal that no suffix length was supplied
            }
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_basename");
    store_if_result(ctx, inst)
}

/// Lowers `dirname(path, levels?)` through the target-aware runtime helper.
pub(crate) fn lower_dirname(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "dirname", 1, 2)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "dirname path")?;
    if inst.operands.len() == 1 {
        abi::emit_call_label(ctx.emitter, "__rt_dirname");
        return store_if_result(ctx, inst);
    }
    let levels = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            require_int(ctx.load_value_to_result(levels)?.codegen_repr(), "dirname levels")?;
            ctx.emitter.instruction("mov x3, x0");                              // pass the requested parent depth to the levels-aware runtime helper
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            require_int(ctx.load_value_to_result(levels)?.codegen_repr(), "dirname levels")?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the requested parent depth to the levels-aware runtime helper
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_dirname_levels");
    store_if_result(ctx, inst)
}

/// Lowers `fnmatch(pattern, filename, flags?)` through the target-aware runtime helper.
pub(crate) fn lower_fnmatch(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fnmatch", 2, 3)?;
    let pattern = expect_operand(inst, 0)?;
    let filename = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, pattern, "fnmatch pattern")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, filename, "fnmatch filename")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            if inst.operands.len() == 3 {
                let flags = expect_operand(inst, 2)?;
                require_int(ctx.load_value_to_result(flags)?.codegen_repr(), "fnmatch flags")?;
                ctx.emitter.instruction("mov x5, x0");                          // pass the caller-supplied fnmatch flags to the runtime helper
            } else {
                ctx.emitter.instruction("mov x5, #0");                          // use the PHP default flags value
            }
            abi::emit_pop_reg_pair(ctx.emitter, "x3", "x4");
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, pattern, "fnmatch pattern")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, filename, "fnmatch filename")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            if inst.operands.len() == 3 {
                let flags = expect_operand(inst, 2)?;
                require_int(ctx.load_value_to_result(flags)?.codegen_repr(), "fnmatch flags")?;
                ctx.emitter.instruction("mov rcx, rax");                        // pass the caller-supplied fnmatch flags to the runtime helper
            } else {
                ctx.emitter.instruction("xor ecx, ecx");                        // use the PHP default flags value
            }
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fnmatch");
    store_if_result(ctx, inst)
}

/// Lowers `pathinfo(path, flags?)` through string, array, or boxed dynamic helpers.
pub(crate) fn lower_pathinfo(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "pathinfo", 1, 2)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "pathinfo path")?;
    let result_ty = inst.result_php_type.codegen_repr();
    if inst.operands.len() == 1 {
        abi::emit_call_label(ctx.emitter, "__rt_pathinfo_array");
        if result_ty == PhpType::Mixed {
            box_owned_pathinfo_array_as_mixed(ctx);
        }
        return store_if_result(ctx, inst);
    }
    let flag = expect_operand(inst, 1)?;
    match result_ty {
        PhpType::AssocArray { .. } => {
            abi::emit_call_label(ctx.emitter, "__rt_pathinfo_array");
        }
        PhpType::Str => {
            lower_pathinfo_string(ctx, flag)?;
        }
        PhpType::Mixed => {
            lower_pathinfo_mixed(ctx, flag)?;
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "pathinfo result PHP type {:?}",
                other
            )));
        }
    }
    store_if_result(ctx, inst)
}

