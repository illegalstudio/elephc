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
    // Publishing takes the ADDRESS of an extern only the `elephc-phar` staticlib defines, so it
    // may only happen where that staticlib is coming. A LITERAL `phar://` path is its own answer
    // — the same thing that makes `file_put_contents("phar://…")` publish its write bridge — and
    // a path that only exists at run time is not: `unlink($name)` in an ordinary program asks for
    // no bridge at all, and publishing there left CI unable to LINK any mysqli program
    // (`Undefined symbols: _elephc_phar_delete_url`). For a dynamic path the program's own phar
    // requirement decides instead.
    //
    // The DISPATCH shape below still keys off `can_be_phar`: it reads a `.quad 0` slot it already
    // null-checks, so an unpublished bridge answers false rather than misbehaving.
    let publishes_phar_bridge = match path_literal.as_deref() {
        Some(literal) => literal.starts_with("phar://"),
        None => ctx.module.required_runtime_features.phar_archive,
    };
    if publishes_phar_bridge {
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
    // php throws rather than warning for an empty filename — see `emit_empty_path_value_error`.
    if let Some(path) = inst.operands.get(0).copied() {
        super::emit_empty_path_value_error(ctx, path, super::EMPTY_PATH_MESSAGE)?;
    }
    // php names the function the USER called in every one of these warnings. `__rt_copy` is
    // `__rt_file_get_contents` followed by `__rt_file_put_contents`, and left to themselves those
    // two name THEMSELVES — so a failed copy reported `file_get_contents(missing.txt)`.
    emit_open_diag_name(ctx, Some(("_diag_open_failed_copy_prefix", "Warning: copy(".len(),
                                   "_uww_name_copy", "copy".len())));
    let result = if copy_source_needs_the_lowering(ctx, inst)? {
        emit_copy_from_lowered_source(ctx, inst)
    } else if copy_source_is_a_run_time_path(ctx, inst)? {
        emit_copy_with_dynamic_filter_source(ctx, inst)
    } else {
        lower_binary_path_call_with_context(ctx, inst, "copy", "__rt_copy")
    };
    // Unconditionally: the slots are global, and a name left behind would make the next
    // `file_get_contents()` in the program call itself `copy`.
    emit_open_diag_name(ctx, None);
    result
}

/// Whether `copy()`'s source names a wrapper only the LOWERING resolves.
///
/// The runtime helper `__rt_copy` reaches every REGISTERED wrapper on its own; these built-in
/// schemes are opened while lowering `file_get_contents()` and are invisible to it. A run-time
/// path is left alone: this route needs the literal to recognise the scheme, and a dynamic one
/// already reaches whatever the runtime knows.
fn copy_source_needs_the_lowering(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<bool> {
    let Some(source) = inst.operands.first().copied() else {
        return Ok(false);
    };
    let Some(literal) = optional_const_string_operand(ctx, source)? else {
        return Ok(false);
    };
    Ok(literal.starts_with("data:")
        || literal.starts_with("php://filter/")
        || literal.starts_with("compress.zlib://")
        || literal.starts_with("compress.bzip2://")
        || literal.starts_with("phar://")
        || literal.starts_with("zip://"))
}

/// Whether `copy()`'s source is a path only the RUN TIME can spell.
///
/// A literal is resolved by `copy_source_needs_the_lowering` above; anything else may still turn
/// out to be a `php://filter/...` URL once the program runs, and the runtime cannot open one.
fn copy_source_is_a_run_time_path(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<bool> {
    let Some(source) = inst.operands.first().copied() else {
        return Ok(false);
    };
    Ok(optional_const_string_operand(ctx, source)?.is_none())
}

/// Emits `copy()` for a source spelled at RUN TIME, giving a `php://filter/...` URL the read
/// `fopen()` performs and leaving every other path to `__rt_copy`.
///
/// ```text
/// $src = 'php://filter/read=string.rot13/resource=' . $raw;
/// copy($src, $dst);
/// ```
///
/// `__rt_copy` reads through `__rt_file_get_contents`, whose runtime half is a `stat` and an
/// `open(2)`: it reaches every REGISTERED wrapper, and knows nothing of php's own filter scheme,
/// which only the lowering resolves. So the very URL that `fopen()` and `file_get_contents()`
/// both open answered `Failed to open stream` here — measured, with the URL written as a literal
/// working and the same string assembled at run time failing.
///
/// Only the FILTER route is emitted, not the whole dynamic reader `file_get_contents()` uses:
/// its compress arm pulls zlib into the link of every program that calls `copy()`, and its
/// remaining arms answered a copy with a crash. A URL that names no filter falls through to
/// `__rt_copy` with the path untouched, which is what it always did.
fn emit_copy_with_dynamic_filter_source(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let source = expect_operand(inst, 0)?;
    let destination = expect_operand(inst, 1)?;
    let done_all = ctx.next_label("copy_dynamic_done");
    load_string_to_result(ctx, source, "copy")?;
    let filtered = super::emit_dynamic_php_filter_read_route(
        ctx,
        "_diag_open_failed_copy_prefix",
        "Warning: copy(",
        "copy",
    )?;
    // The fall-through is every other path: the ordinary copy, unchanged.
    lower_binary_path_call_with_context(ctx, inst, "copy", "__rt_copy")?;
    abi::emit_jump(ctx.emitter, &done_all);
    let failed = format!("{}_failed", filtered);
    ctx.emitter.label(&filtered);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x1, {}", failed));            // a failed open writes nothing
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");                   // the bytes, across the path load
            load_string_to_result(ctx, destination, "copy")?;
            abi::emit_pop_reg_pair(ctx.emitter, "x3", "x4");                    // the data pair the writer reads
            ctx.emitter.instruction("mov x5, xzr");                             // no FILE_APPEND for this caller
            abi::emit_call_label(ctx.emitter, "__rt_file_put_contents");
            ctx.emitter.instruction("cmp x0, #0");
            ctx.emitter.instruction("cset x0, ge");                             // a zero-byte write copied an empty file
            abi::emit_jump(ctx.emitter, &done_all);
            ctx.emitter.label(&failed);
            ctx.emitter.instruction("mov x0, #0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // a failed open writes nothing
            ctx.emitter.instruction(&format!("jz {}", failed));
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");                 // the bytes, across the path load
            load_string_to_result(ctx, destination, "copy")?;
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");                  // the data pair the writer reads
            ctx.emitter.instruction("xor ecx, ecx");                            // no FILE_APPEND for this caller
            abi::emit_call_label(ctx.emitter, "__rt_file_put_contents");
            ctx.emitter.instruction("cmp rax, 0");
            ctx.emitter.instruction("setge al");                                // a zero-byte write copied an empty file
            ctx.emitter.instruction("movzx rax, al");
            abi::emit_jump(ctx.emitter, &done_all);
            ctx.emitter.label(&failed);
            ctx.emitter.instruction("xor eax, eax");
        }
    }
    ctx.emitter.label(&done_all);
    store_if_result(ctx, inst)
}

/// Emits `copy()` as the wrapper-aware read followed by the write `__rt_copy` already performs.
///
/// Mirrors `__rt_copy` exactly once the bytes are in hand, including the two rules its own body
/// carries: a NULL pointer is a failed source open and must leave the destination untouched, and a
/// zero-byte write is a SUCCESS because php copies an empty file.
fn emit_copy_from_lowered_source(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let destination = expect_operand(inst, 1)?;
    let failed = ctx.next_label("copy_source_failed");
    let done = ctx.next_label("copy_done");
    let context_scope = super::phar_read::emit_file_get_contents_bytes(ctx, inst, true, "copy")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x1, {}", failed));            // a failed open writes nothing
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");                   // the bytes, across the path load
            load_string_to_result(ctx, destination, "copy")?;
            abi::emit_pop_reg_pair(ctx.emitter, "x3", "x4");                    // the data pair the writer reads
            ctx.emitter.instruction("mov x5, xzr");                             // the writer reads $flags for FILE_APPEND: this caller has none
            abi::emit_call_label(ctx.emitter, "__rt_file_put_contents");
            ctx.emitter.instruction("cmp x0, #0");
            ctx.emitter.instruction("cset x0, ge");                             // a zero-byte write copied an empty file
            ctx.emitter.instruction(&format!("b {}", done));
            ctx.emitter.label(&failed);
            ctx.emitter.instruction("mov x0, #0");
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // a failed open writes nothing
            ctx.emitter.instruction(&format!("jz {}", failed));
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");                 // the bytes, across the path load
            load_string_to_result(ctx, destination, "copy")?;
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");                  // the data pair the writer reads
            ctx.emitter.instruction("xor ecx, ecx");                            // the writer reads $flags for FILE_APPEND: this caller has none
            abi::emit_call_label(ctx.emitter, "__rt_file_put_contents");
            ctx.emitter.instruction("cmp rax, 0");
            ctx.emitter.instruction("setge al");                                // a zero-byte write copied an empty file
            ctx.emitter.instruction("movzx rax, al");
            ctx.emitter.instruction(&format!("jmp {}", done));
            ctx.emitter.label(&failed);
            ctx.emitter.instruction("xor eax, eax");
            ctx.emitter.label(&done);
        }
    }
    if context_scope {
        finish_fopen_context_scope(ctx);
    }
    store_if_result(ctx, inst)
}

/// Publishes the name php should print in open-failure warnings, or clears it.
///
/// `Some((prefix symbol, prefix length, bare name symbol, bare name length))` while a delegating
/// builtin owns the diagnostics; `None` hands them back to the helpers that raise them.
pub(super) fn emit_open_diag_name(
    ctx: &mut FunctionContext<'_>,
    published: Option<(&str, usize, &str, usize)>,
) {
    let scratch = abi::secondary_scratch_reg(ctx.emitter);
    let value = abi::tertiary_scratch_reg(ctx.emitter);
    let slots: [(&str, Option<(&str, usize)>); 2] = match published {
        Some((prefix, prefix_len, name, name_len)) => [
            ("_rt_open_diag_prefix", Some((prefix, prefix_len))),
            ("_rt_open_diag_name", Some((name, name_len))),
        ],
        None => [("_rt_open_diag_prefix", None), ("_rt_open_diag_name", None)],
    };
    for (slot, published) in slots {
        let len_slot = format!("{slot}_len");
        match published {
            Some((symbol, len)) => {
                abi::emit_symbol_address(ctx.emitter, value, symbol);
                abi::emit_store_reg_to_symbol(ctx.emitter, value, slot, 0);
                abi::emit_load_int_immediate(ctx.emitter, value, len as i64);
                abi::emit_store_reg_to_symbol(ctx.emitter, value, &len_slot, 0);
            }
            None => {
                abi::emit_load_int_immediate(ctx.emitter, value, 0);
                abi::emit_store_reg_to_symbol(ctx.emitter, value, slot, 0);
                abi::emit_store_reg_to_symbol(ctx.emitter, value, &len_slot, 0);
            }
        }
    }
    let _ = scratch;
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
    // php throws for an empty directory name, in wording of its own — see the constant.
    super::emit_empty_path_value_error(ctx, path, super::SCANDIR_EMPTY_PATH_MESSAGE)?;
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
    ensure_arg_count_between(inst, "glob", 1, 2)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "glob")?;
    // `$flags` rides beside the pattern pair, the way `fnmatch`'s does. php's OWN numbering
    // travels here unchanged; `__rt_glob` validates it and translates it to the platform's libc
    // bits, because php 8.5 ships its own glob and its constants are the bits of no libc.
    match inst.operands.get(1).copied() {
        None => match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.emitter.instruction("mov x3, #0"),             // php's default is no flags
            Arch::X86_64 => ctx.emitter.instruction("xor ecx, ecx"),
        },
        Some(flags) => {
            match ctx.emitter.target.arch {
                Arch::AArch64 => abi::emit_push_reg_pair(ctx.emitter, "x1", "x2"),
                Arch::X86_64 => abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx"),
            }
            require_int(ctx.load_value_to_result(flags)?.codegen_repr(), "glob flags")?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("mov x9, x0");                      // hold the flags while the pattern pair returns
                    abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
                    ctx.emitter.instruction("mov x3, x9");
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("mov rcx, rax");
                    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
                }
            }
        }
    }
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

