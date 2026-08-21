//! Purpose:
//! Ownership, chmod, and wrapper metadata dispatch.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Selects which ownership field a filesystem principal builtin changes.
#[derive(Clone, Copy)]
pub(super) enum PrincipalKind {
    Owner,
    Group,
}

/// Selects how `touch()` should materialize optional timestamp operands.
pub(super) enum TouchTimeShape {
    BothNow,
    MtimeAlsoAtime,
    ExplicitBoth,
}

/// Lowers the shared path/principal calling convention for `chown()` and `chgrp()`.
pub(super) fn lower_chown_or_chgrp(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    kind: PrincipalKind,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 2)?;
    let path = expect_operand(inst, 0)?;
    let principal = expect_operand(inst, 1)?;
    // php names the CALLER in the missing-hook warning, and both callers land on the same
    // `stream_metadata` hook, so the name is chosen here rather than in the shared helper.
    let (head_symbol, head_len) = match kind {
        PrincipalKind::Owner => ("_uwmh_head_chown", WRAPPER_MISSING_HOOK_HEAD_CHOWN.len()),
        PrincipalKind::Group => ("_uwmh_head_chgrp", WRAPPER_MISSING_HOOK_HEAD_CHGRP.len()),
    };
    emit_publish_missing_hook_message(
        ctx,
        head_symbol,
        head_len,
        "_uwmh_tail_metadata",
        WRAPPER_MISSING_HOOK_TAIL_METADATA.len(),
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_chown_or_chgrp_aarch64(ctx, path, principal, name, kind)?,
        Arch::X86_64 => lower_chown_or_chgrp_x86_64(ctx, path, principal, name, kind)?,
    }
    store_if_result(ctx, inst)
}

/// Materializes `chown()`/`chgrp()` operands for the ARM64 runtime ABI.
pub(super) fn lower_chown_or_chgrp_aarch64(
    ctx: &mut FunctionContext<'_>,
    path: ValueId,
    principal: ValueId,
    name: &str,
    kind: PrincipalKind,
) -> Result<()> {
    load_string_to_result(ctx, path, name)?;
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
    match ctx.load_value_to_result(principal)?.codegen_repr() {
        PhpType::Str => {
            emit_owner_group_name_wrapper_dispatch(
                ctx,
                principal_name_option(kind),
                principal_string_runtime(kind),
            );
        }
        PhpType::Int => {
            emit_owner_group_wrapper_dispatch(ctx, principal_int_option(kind));
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} principal PHP type {:?}",
                name, other
            )));
        }
    }
    Ok(())
}

/// Materializes `chown()`/`chgrp()` operands for the Linux x86_64 runtime ABI.
pub(super) fn lower_chown_or_chgrp_x86_64(
    ctx: &mut FunctionContext<'_>,
    path: ValueId,
    principal: ValueId,
    name: &str,
    kind: PrincipalKind,
) -> Result<()> {
    load_string_to_result(ctx, path, name)?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    match ctx.load_value_to_result(principal)?.codegen_repr() {
        PhpType::Str => {
            emit_owner_group_name_wrapper_dispatch(
                ctx,
                principal_name_option(kind),
                principal_string_runtime(kind),
            );
        }
        PhpType::Int => {
            emit_owner_group_wrapper_dispatch(ctx, principal_int_option(kind));
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} principal PHP type {:?}",
                name, other
            )));
        }
    }
    Ok(())
}

/// Lowers the native symlink-aware path/principal convention for `lchown()` and `lchgrp()`.
pub(super) fn lower_lchown_or_lchgrp(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    kind: PrincipalKind,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 2)?;
    let path = expect_operand(inst, 0)?;
    let principal = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_lchown_or_lchgrp_aarch64(ctx, path, principal, name, kind)?,
        Arch::X86_64 => lower_lchown_or_lchgrp_x86_64(ctx, path, principal, name, kind)?,
    }
    store_if_result(ctx, inst)
}

/// Materializes `lchown()`/`lchgrp()` operands for the ARM64 runtime ABI.
pub(super) fn lower_lchown_or_lchgrp_aarch64(
    ctx: &mut FunctionContext<'_>,
    path: ValueId,
    principal: ValueId,
    name: &str,
    kind: PrincipalKind,
) -> Result<()> {
    load_string_to_result(ctx, path, name)?;
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
    match ctx.load_value_to_result(principal)?.codegen_repr() {
        PhpType::Str => {
            ctx.emitter.instruction("mov x3, x1");                              // pass principal name pointer to symlink ownership helper
            ctx.emitter.instruction("mov x4, x2");                              // pass principal name length to symlink ownership helper
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            abi::emit_call_label(ctx.emitter, lprincipal_string_runtime(kind));
        }
        PhpType::Int => {
            ctx.emitter.instruction("mov x9, x0");                              // preserve uid/gid while restoring the path
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            if matches!(kind, PrincipalKind::Owner) {
                ctx.emitter.instruction("mov x3, x9");                          // pass uid and leave symlink group unchanged
                ctx.emitter.instruction("mov x4, #-1");                         // keep the symlink group unchanged
            } else {
                ctx.emitter.instruction("mov x3, #-1");                         // keep the symlink owner unchanged
                ctx.emitter.instruction("mov x4, x9");                          // pass gid and leave symlink owner unchanged
            }
            abi::emit_call_label(ctx.emitter, "__rt_lchown");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} principal PHP type {:?}",
                name, other
            )));
        }
    }
    Ok(())
}

/// Materializes `lchown()`/`lchgrp()` operands for the Linux x86_64 runtime ABI.
pub(super) fn lower_lchown_or_lchgrp_x86_64(
    ctx: &mut FunctionContext<'_>,
    path: ValueId,
    principal: ValueId,
    name: &str,
    kind: PrincipalKind,
) -> Result<()> {
    load_string_to_result(ctx, path, name)?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    match ctx.load_value_to_result(principal)?.codegen_repr() {
        PhpType::Str => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass principal name pointer to symlink ownership helper
            ctx.emitter.instruction("mov rsi, rdx");                            // pass principal name length to symlink ownership helper
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            abi::emit_call_label(ctx.emitter, lprincipal_string_runtime(kind));
        }
        PhpType::Int => {
            ctx.emitter.instruction("mov r9, rax");                             // preserve uid/gid while restoring the path
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            if matches!(kind, PrincipalKind::Owner) {
                ctx.emitter.instruction("mov rdi, r9");                         // pass uid and leave symlink group unchanged
                ctx.emitter.instruction("mov rsi, -1");                         // keep the symlink group unchanged
            } else {
                ctx.emitter.instruction("mov rdi, -1");                         // keep the symlink owner unchanged
                ctx.emitter.instruction("mov rsi, r9");                         // pass gid and leave symlink owner unchanged
            }
            abi::emit_call_label(ctx.emitter, "__rt_lchown");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} principal PHP type {:?}",
                name, other
            )));
        }
    }
    Ok(())
}

/// Returns the wrapper metadata option for string ownership changes.
pub(super) fn principal_name_option(kind: PrincipalKind) -> usize {
    match kind {
        PrincipalKind::Owner => STREAM_META_OWNER_NAME,
        PrincipalKind::Group => STREAM_META_GROUP_NAME,
    }
}

/// Returns the wrapper metadata option for integer ownership changes.
pub(super) fn principal_int_option(kind: PrincipalKind) -> usize {
    match kind {
        PrincipalKind::Owner => STREAM_META_OWNER,
        PrincipalKind::Group => STREAM_META_GROUP,
    }
}

/// Returns the string-principal runtime helper for the ownership field.
pub(super) fn principal_string_runtime(kind: PrincipalKind) -> &'static str {
    match kind {
        PrincipalKind::Owner => "__rt_chown_user",
        PrincipalKind::Group => "__rt_chgrp_group",
    }
}

/// Returns the string-principal runtime helper for symlink ownership changes.
pub(super) fn lprincipal_string_runtime(kind: PrincipalKind) -> &'static str {
    match kind {
        PrincipalKind::Owner => "__rt_lchown_user",
        PrincipalKind::Group => "__rt_lchgrp_group",
    }
}

/// Lowers `chmod()` through wrapper `stream_metadata()` before libc chmod.
pub(super) fn lower_chmod_with_wrapper(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "chmod", 2)?;
    let path = expect_operand(inst, 0)?;
    let mode = expect_operand(inst, 1)?;
    emit_publish_missing_hook_message(
        ctx,
        "_uwmh_head_chmod",
        WRAPPER_MISSING_HOOK_HEAD_CHMOD.len(),
        "_uwmh_tail_metadata",
        WRAPPER_MISSING_HOOK_TAIL_METADATA.len(),
    );
    let wrapper = ctx.next_label("chmod_wrapper");
    let after = ctx.next_label("chmod_after");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, path, "chmod path")?;
            ctx.emitter.instruction("sub sp, sp, #32");                         // reserve path and mode scratch storage
            ctx.emitter.instruction("str x1, [sp, #0]");                        // preserve the path pointer
            ctx.emitter.instruction("str x2, [sp, #8]");                        // preserve the path length
            require_int(ctx.load_value_to_result(mode)?.codegen_repr(), "chmod mode")?;
            ctx.emitter.instruction("str x0, [sp, #16]");                       // preserve the requested mode
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // pass path pointer to wrapper-scheme probe
            ctx.emitter.instruction("ldr x1, [sp, #8]");                        // pass path length to wrapper-scheme probe
            abi::emit_call_label(ctx.emitter, "__rt_path_is_wrapper");
            ctx.emitter.instruction(&format!("cbnz x0, {}", wrapper));          // registered wrapper schemes use stream_metadata
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // pass path pointer to native chmod
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // pass path length to native chmod
            ctx.emitter.instruction("ldr x3, [sp, #16]");                       // pass requested mode to native chmod
            ctx.emitter.instruction("add sp, sp, #32");                         // release scratch before native chmod
            abi::emit_call_label(ctx.emitter, "__rt_chmod");
            ctx.emitter.instruction(&format!("b {}", after));                   // skip wrapper stream_metadata after native chmod
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // reload the requested mode for boxing
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
            ctx.emitter.instruction("str x0, [sp, #16]");                       // preserve the boxed mode value
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // pass wrapper path pointer
            ctx.emitter.instruction("ldr x1, [sp, #8]");                        // pass wrapper path length
            ctx.emitter.instruction(
                &format!("mov x2, #{}", STREAM_METADATA_SLOT)
            );                                                                  // select stream_metadata vtable slot
            ctx.emitter.instruction(
                &format!("mov x3, #{}", STREAM_META_ACCESS)
            );                                                                  // select STREAM_META_ACCESS
            ctx.emitter.instruction("ldr x4, [sp, #16]");                       // pass boxed mode as mixed value
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_path_op");
            ctx.emitter.instruction("str x0, [sp, #0]");                        // preserve stream_metadata result across value release
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // reload the boxed mode value
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // restore the stream_metadata boolean result
            ctx.emitter.instruction("add sp, sp, #32");                         // release scratch after wrapper chmod
            ctx.emitter.label(&after);
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, path, "chmod path")?;
            ctx.emitter.instruction("sub rsp, 32");                             // reserve path and mode scratch storage
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the path pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // preserve the path length
            require_int(ctx.load_value_to_result(mode)?.codegen_repr(), "chmod mode")?;
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // preserve the requested mode
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // pass path pointer to wrapper-scheme probe
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");            // pass path length to wrapper-scheme probe
            abi::emit_call_label(ctx.emitter, "__rt_path_is_wrapper");
            ctx.emitter.instruction("test rax, rax");                           // test whether the path scheme matched a wrapper
            ctx.emitter.instruction(&format!("jnz {}", wrapper));               // registered wrapper schemes use stream_metadata
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // pass path pointer to native chmod
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // pass path length to native chmod
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // pass requested mode to native chmod
            ctx.emitter.instruction("add rsp, 32");                             // release scratch before native chmod
            abi::emit_call_label(ctx.emitter, "__rt_chmod");
            ctx.emitter.instruction(&format!("jmp {}", after));                 // skip wrapper stream_metadata after native chmod
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // reload the requested mode for boxing
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // preserve the boxed mode value
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // pass wrapper path pointer
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");            // pass wrapper path length
            ctx.emitter.instruction(
                &format!("mov rdx, {}", STREAM_METADATA_SLOT)
            );                                                                  // select stream_metadata vtable slot
            ctx.emitter.instruction(
                &format!("mov rcx, {}", STREAM_META_ACCESS)
            );                                                                  // select STREAM_META_ACCESS
            ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 16]");            // pass boxed mode as mixed value
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_path_op");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve stream_metadata result across value release
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // reload the boxed mode value
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // restore the stream_metadata boolean result
            ctx.emitter.instruction("add rsp, 32");                             // release scratch after wrapper chmod
            ctx.emitter.label(&after);
        }
    }
    store_if_result(ctx, inst)
}

/// Emits wrapper dispatch for `chown()`/`chgrp()` with a string principal.
pub(super) fn emit_owner_group_name_wrapper_dispatch(
    ctx: &mut FunctionContext<'_>,
    option: usize,
    libc_helper: &str,
) {
    let wrapper = ctx.next_label("meta_name_wrapper");
    let after = ctx.next_label("meta_name_after");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                         // reserve name scratch above the preserved path
            ctx.emitter.instruction("str x1, [sp, #0]");                        // preserve principal name pointer
            ctx.emitter.instruction("str x2, [sp, #8]");                        // preserve principal name length
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // pass path pointer to wrapper-scheme probe
            ctx.emitter.instruction("ldr x1, [sp, #24]");                       // pass path length to wrapper-scheme probe
            abi::emit_call_label(ctx.emitter, "__rt_path_is_wrapper");
            ctx.emitter.instruction(&format!("cbnz x0, {}", wrapper));          // registered wrapper schemes use stream_metadata
            ctx.emitter.instruction("ldr x1, [sp, #16]");                       // pass path pointer to libc owner/group resolver
            ctx.emitter.instruction("ldr x2, [sp, #24]");                       // pass path length to libc owner/group resolver
            ctx.emitter.instruction("ldr x3, [sp, #0]");                        // pass principal name pointer to libc resolver
            ctx.emitter.instruction("ldr x4, [sp, #8]");                        // pass principal name length to libc resolver
            ctx.emitter.instruction("add sp, sp, #32");                         // release name scratch and preserved path before libc helper
            abi::emit_call_label(ctx.emitter, libc_helper);
            ctx.emitter.instruction(&format!("b {}", after));                   // skip wrapper stream_metadata after native helper
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // reload principal name pointer for boxing
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // reload principal name length for boxing
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            ctx.emitter.instruction("str x0, [sp, #0]");                        // preserve the boxed principal value
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // pass wrapper path pointer
            ctx.emitter.instruction("ldr x1, [sp, #24]");                       // pass wrapper path length
            ctx.emitter.instruction(
                &format!("mov x2, #{}", STREAM_METADATA_SLOT)
            );                                                                  // select stream_metadata vtable slot
            ctx.emitter.instruction(&format!("mov x3, #{}", option));           // pass owner/group metadata option
            ctx.emitter.instruction("ldr x4, [sp, #0]");                        // pass boxed principal as mixed value
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_path_op");
            ctx.emitter.instruction("str x0, [sp, #8]");                        // preserve stream_metadata result across value release
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the boxed principal value
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
            ctx.emitter.instruction("ldr x0, [sp, #8]");                        // restore the stream_metadata boolean result
            ctx.emitter.instruction("add sp, sp, #32");                         // release name scratch and preserved path
            ctx.emitter.label(&after);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve name scratch above the preserved path
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve principal name pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // preserve principal name length
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // pass path pointer to wrapper-scheme probe
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");           // pass path length to wrapper-scheme probe
            abi::emit_call_label(ctx.emitter, "__rt_path_is_wrapper");
            ctx.emitter.instruction("test rax, rax");                           // test whether the path scheme matched a wrapper
            ctx.emitter.instruction(&format!("jnz {}", wrapper));               // registered wrapper schemes use stream_metadata
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // pass path pointer to libc owner/group resolver
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 24]");           // pass path length to libc owner/group resolver
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // pass principal name pointer to libc resolver
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");            // pass principal name length to libc resolver
            ctx.emitter.instruction("add rsp, 32");                             // release name scratch and preserved path before libc helper
            abi::emit_call_label(ctx.emitter, libc_helper);
            ctx.emitter.instruction(&format!("jmp {}", after));                 // skip wrapper stream_metadata after native helper
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // reload principal name pointer for boxing
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // reload principal name length for boxing
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the boxed principal value
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // pass wrapper path pointer
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");           // pass wrapper path length
            ctx.emitter.instruction(
                &format!("mov rdx, {}", STREAM_METADATA_SLOT)
            );                                                                  // select stream_metadata vtable slot
            ctx.emitter.instruction(&format!("mov rcx, {}", option));           // pass owner/group metadata option
            ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 0]");             // pass boxed principal as mixed value
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_path_op");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // preserve stream_metadata result across value release
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // reload the boxed principal value
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");            // restore the stream_metadata boolean result
            ctx.emitter.instruction("add rsp, 32");                             // release name scratch and preserved path
            ctx.emitter.label(&after);
        }
    }
}

/// Emits wrapper dispatch for `chown()`/`chgrp()` with an integer principal.
pub(super) fn emit_owner_group_wrapper_dispatch(ctx: &mut FunctionContext<'_>, option: usize) {
    let wrapper = ctx.next_label("meta_owngrp_wrapper");
    let after = ctx.next_label("meta_owngrp_after");
    let is_owner = option == STREAM_META_OWNER;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x9, x0");                              // preserve the uid/gid value across path restoration
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the preserved path pointer and length
            ctx.emitter.instruction("sub sp, sp, #32");                         // reserve path and principal scratch storage
            ctx.emitter.instruction("str x1, [sp, #0]");                        // preserve the path pointer
            ctx.emitter.instruction("str x2, [sp, #8]");                        // preserve the path length
            ctx.emitter.instruction("str x9, [sp, #16]");                       // preserve the uid/gid value
            ctx.emitter.instruction("mov x0, x1");                              // pass path pointer to wrapper-scheme probe
            ctx.emitter.instruction("mov x1, x2");                              // pass path length to wrapper-scheme probe
            abi::emit_call_label(ctx.emitter, "__rt_path_is_wrapper");
            ctx.emitter.instruction(&format!("cbnz x0, {}", wrapper));          // registered wrapper schemes use stream_metadata
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // pass path pointer to native chown
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // pass path length to native chown
            if is_owner {
                ctx.emitter.instruction("ldr x3, [sp, #16]");                   // pass uid and leave gid unchanged
                ctx.emitter.instruction("mov x4, #-1");                         // keep the file group unchanged
            } else {
                ctx.emitter.instruction("mov x3, #-1");                         // keep the file owner unchanged
                ctx.emitter.instruction("ldr x4, [sp, #16]");                   // pass gid and leave uid unchanged
            }
            ctx.emitter.instruction("add sp, sp, #32");                         // release scratch before native chown
            abi::emit_call_label(ctx.emitter, "__rt_chown");
            ctx.emitter.instruction(&format!("b {}", after));                   // skip wrapper stream_metadata after native helper
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // reload uid/gid for boxing
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
            ctx.emitter.instruction("str x0, [sp, #16]");                       // preserve the boxed principal value
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // pass wrapper path pointer
            ctx.emitter.instruction("ldr x1, [sp, #8]");                        // pass wrapper path length
            ctx.emitter.instruction(
                &format!("mov x2, #{}", STREAM_METADATA_SLOT)
            );                                                                  // select stream_metadata vtable slot
            ctx.emitter.instruction(&format!("mov x3, #{}", option));           // pass owner/group metadata option
            ctx.emitter.instruction("ldr x4, [sp, #16]");                       // pass boxed principal as mixed value
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_path_op");
            ctx.emitter.instruction("str x0, [sp, #0]");                        // preserve stream_metadata result across value release
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // reload the boxed principal value
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // restore the stream_metadata boolean result
            ctx.emitter.instruction("add sp, sp, #32");                         // release wrapper metadata scratch storage
            ctx.emitter.label(&after);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, rax");                             // preserve the uid/gid value across path restoration
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            ctx.emitter.instruction("sub rsp, 32");                             // reserve path and principal scratch storage
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the path pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // preserve the path length
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], r9");            // preserve the uid/gid value
            ctx.emitter.instruction("mov rdi, rax");                            // pass path pointer to wrapper-scheme probe
            ctx.emitter.instruction("mov rsi, rdx");                            // pass path length to wrapper-scheme probe
            abi::emit_call_label(ctx.emitter, "__rt_path_is_wrapper");
            ctx.emitter.instruction("test rax, rax");                           // test whether the path scheme matched a wrapper
            ctx.emitter.instruction(&format!("jnz {}", wrapper));               // registered wrapper schemes use stream_metadata
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // pass path pointer to native chown
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // pass path length to native chown
            if is_owner {
                ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");       // pass uid and leave gid unchanged
                ctx.emitter.instruction("mov rsi, -1");                         // keep the file group unchanged
            } else {
                ctx.emitter.instruction("mov rdi, -1");                         // keep the file owner unchanged
                ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 16]");       // pass gid and leave uid unchanged
            }
            ctx.emitter.instruction("add rsp, 32");                             // release scratch before native chown
            abi::emit_call_label(ctx.emitter, "__rt_chown");
            ctx.emitter.instruction(&format!("jmp {}", after));                 // skip wrapper stream_metadata after native helper
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // reload uid/gid for boxing
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // preserve the boxed principal value
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // pass wrapper path pointer
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");            // pass wrapper path length
            ctx.emitter.instruction(
                &format!("mov rdx, {}", STREAM_METADATA_SLOT)
            );                                                                  // select stream_metadata vtable slot
            ctx.emitter.instruction(&format!("mov rcx, {}", option));           // pass owner/group metadata option
            ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 16]");            // pass boxed principal as mixed value
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_path_op");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve stream_metadata result across value release
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // reload the boxed principal value
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // restore the stream_metadata boolean result
            ctx.emitter.instruction("add rsp, 32");                             // release wrapper metadata scratch storage
            ctx.emitter.label(&after);
        }
    }
}

