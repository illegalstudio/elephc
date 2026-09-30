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

/// Emits the notice php raises for a null in a non-nullable internal parameter.
///
/// php 8.1 deprecated the coercion rather than removing it, so the notice precedes the call and
/// the call still happens. MEASURED on `php -n` 8.5.6:
/// `Deprecated: chown(): Passing null to parameter #2 ($user) of type string|int is deprecated`,
/// then the ordinary `Warning` for uid 0.
fn emit_null_principal_deprecation(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    kind: PrincipalKind,
) {
    super::fopen_core::emit_static_diag_warning(
        ctx,
        &format!(
            "Deprecated: {}(): Passing null to parameter #2 ({}) of type string|int is deprecated\n",
            name,
            principal_argument_name(kind),
        ),
    );
}

/// Which principal argument php names in its `string|int` TypeError.
fn principal_argument_name(kind: PrincipalKind) -> &'static str {
    match kind {
        PrincipalKind::Owner => "$user",
        PrincipalKind::Group => "$group",
    }
}

/// Decides php's `string|int` principal at RUN TIME, from a `mixed` operand's tag.
///
/// php declares `chown(string $filename, string|int $user)` and looks a NAME up only when the
/// argument is genuinely a string: `chown($f, "501")` warns `Unable to find uid for 501` and
/// answers false, where `chown($f, 501)` succeeds. MEASURED on `php -n` 8.5.6. A static type
/// therefore cannot pick the path for an operand that is a union — `fileowner()` is declared
/// `int|false`, which is a boxed cell — so the tag decides.
///
/// The scalar tags are php's coercive `string|int` boundary, and each was measured: an int and a
/// bool and `null` all reach the uid path (`false` and `null` become uid 0, hence
/// `Operation not permitted` for an unprivileged process), and a float truncates. Only the
/// container tags are refused, with php's own message.
///
/// ENTRY: the boxed principal is in the int-result register and the path pointer/length pair is
/// pushed. Every arm consumes that pair exactly once, so the emitted frame stays balanced on the
/// throwing paths too.
fn emit_mixed_principal_dispatch(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    kind: PrincipalKind,
    on_string: impl FnOnce(&mut FunctionContext<'_>),
    on_int: impl FnOnce(&mut FunctionContext<'_>),
) {
    let prefix = format!(
        "{}(): Argument #2 ({}) must be of type string|int, ",
        name,
        principal_argument_name(kind)
    );
    let l_string = ctx.next_label("owngrp_principal_string");
    let l_int = ctx.next_label("owngrp_principal_int");
    let l_float = ctx.next_label("owngrp_principal_float");
    let l_null = ctx.next_label("owngrp_principal_null");
    let l_array = ctx.next_label("owngrp_principal_array");
    let l_object = ctx.next_label("owngrp_principal_object");
    let l_resource = ctx.next_label("owngrp_principal_resource");
    let l_closure = ctx.next_label("owngrp_principal_closure");
    let done = ctx.next_label("owngrp_principal_done");

    // `__rt_mixed_unbox` answers the tag in the int-result register and the payload lo/hi in
    // x1/x2 (AArch64) or rdi/rdx (x86_64), peeling a nested cell on the way.
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let tag_reg = abi::int_result_reg(ctx.emitter);
    let (lo_reg, hi_reg) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rdi", "rdx"),
    };
    // Tag values: 0 int, 1 string, 2 float, 3 bool, 4 indexed array, 5 hash, 6 object,
    // 8 null, 9 resource, 10 callable.
    crate::codegen::lower_inst::enums::emit_mixed_tag_branch(ctx, tag_reg, 1, &l_string);
    crate::codegen::lower_inst::enums::emit_mixed_tag_branch(ctx, tag_reg, 2, &l_float);
    crate::codegen::lower_inst::enums::emit_mixed_tag_branch(ctx, tag_reg, 8, &l_null);
    crate::codegen::lower_inst::enums::emit_mixed_tag_branch(ctx, tag_reg, 4, &l_array);
    crate::codegen::lower_inst::enums::emit_mixed_tag_branch(ctx, tag_reg, 5, &l_array);
    crate::codegen::lower_inst::enums::emit_mixed_tag_branch(ctx, tag_reg, 6, &l_object);
    crate::codegen::lower_inst::enums::emit_mixed_tag_branch(ctx, tag_reg, 9, &l_resource);
    crate::codegen::lower_inst::enums::emit_mixed_tag_branch(ctx, tag_reg, 10, &l_closure);
    // int and bool fall through: the payload IS the uid/gid php coerces them to.
    crate::codegen::lower_inst::enums::emit_move_reg(ctx, tag_reg, lo_reg);
    abi::emit_jump(ctx.emitter, &l_int);

    ctx.emitter.label(&l_null);
    // A null that ARRIVES in a boxed cell is the same written null to php's ZPP, and draws the
    // same notice as the literal spelling does.
    emit_null_principal_deprecation(ctx, name, kind);
    abi::emit_load_int_immediate(ctx.emitter, tag_reg, 0);
    abi::emit_jump(ctx.emitter, &l_int);

    ctx.emitter.label(&l_float);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("fmov d0, {}", lo_reg));           // move the raw double bits into the float register
            abi::emit_php_float_to_int(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("movq xmm0, {}", lo_reg));         // move the raw double bits into the float register
            abi::emit_php_float_to_int(ctx.emitter, "rax");
        }
    }
    abi::emit_jump(ctx.emitter, &l_int);

    ctx.emitter.label(&l_int);
    on_int(ctx);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&l_string);
    let (string_ptr_reg, string_len_reg) = abi::string_result_regs(ctx.emitter);
    crate::codegen::lower_inst::enums::emit_move_reg(ctx, string_ptr_reg, lo_reg);
    crate::codegen::lower_inst::enums::emit_move_reg(ctx, string_len_reg, hi_reg);
    on_string(ctx);
    abi::emit_jump(ctx.emitter, &done);

    // php refuses the container shapes, and names the one it was given. The object arm reads the
    // class name from the same dense table `get_class()` uses, because php prints `stdClass
    // given`, not `object given`.
    for (label, given) in [
        (&l_array, "array"),
        (&l_resource, "resource"),
        (&l_closure, "Closure"),
    ] {
        ctx.emitter.label(label);
        emit_principal_type_error(ctx, &prefix, Some(given), lo_reg);
    }
    ctx.emitter.label(&l_object);
    emit_principal_type_error(ctx, &prefix, None, lo_reg);
    ctx.emitter.label(&done);
}

/// Throws php's `string|int` TypeError, naming either a static type or the operand's class.
///
/// The pushed path pair is released first: the unwinder leaves through `__rt_throw_current`
/// rather than returning here, and an unmatched push is what the emitter's frame-balance
/// property exists to catch.
fn emit_principal_type_error(
    ctx: &mut FunctionContext<'_>,
    prefix: &str,
    given: Option<&str>,
    object_reg: &str,
) {
    let (name_ptr_reg, name_len_reg) = abi::string_result_regs(ctx.emitter);
    match given {
        Some(given) => {
            abi::emit_pop_reg_pair(ctx.emitter, name_ptr_reg, name_len_reg);
            let (label, len) = ctx.data.add_string(given.as_bytes());
            abi::emit_symbol_address(ctx.emitter, name_ptr_reg, &label);
            abi::emit_load_int_immediate(ctx.emitter, name_len_reg, len as i64);
        }
        None => {
            // Read the class name BEFORE the pop: the payload register is caller-saved and the
            // pop targets the very pair the name is built into.
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter
                        .instruction(&format!("ldr x9, [{}]", object_reg));     // load the receiver class id
                    abi::emit_symbol_address(ctx.emitter, "x10", "_class_name_entries");
                    ctx.emitter.instruction("lsl x11, x9, #4");                 // scale the class id to the 16-byte class-name row
                    ctx.emitter.instruction("add x10, x10, x11");               // address the receiver's class-name metadata
                    ctx.emitter.instruction("ldr x12, [x10]");                  // borrow the class-name pointer
                    ctx.emitter.instruction("ldr x13, [x10, #8]");              // borrow the class-name byte length
                    abi::emit_pop_reg_pair(ctx.emitter, name_ptr_reg, name_len_reg);
                    ctx.emitter
                        .instruction(&format!("mov {}, x12", name_ptr_reg));    // move the class name into the string result
                    ctx.emitter
                        .instruction(&format!("mov {}, x13", name_len_reg));    // move its length into the string result
                }
                Arch::X86_64 => {
                    ctx.emitter
                        .instruction(&format!("mov r9, QWORD PTR [{}]", object_reg)); // load the receiver class id
                    abi::emit_symbol_address(ctx.emitter, "r10", "_class_name_entries");
                    ctx.emitter.instruction("shl r9, 4");                       // scale the class id to the 16-byte class-name row
                    ctx.emitter
                        .instruction("mov r11, QWORD PTR [r10 + r9]");          // borrow the class-name pointer
                    ctx.emitter
                        .instruction("mov r10, QWORD PTR [r10 + r9 + 8]");      // borrow the class-name byte length
                    abi::emit_pop_reg_pair(ctx.emitter, name_ptr_reg, name_len_reg);
                    ctx.emitter
                        .instruction(&format!("mov {}, r11", name_ptr_reg));    // move the class name into the string result
                    ctx.emitter
                        .instruction(&format!("mov {}, r10", name_len_reg));    // move its length into the string result
                }
            }
        }
    }
    super::super::exceptions::emit_message_concat_prefix(ctx, prefix);
    super::super::exceptions::emit_message_concat_suffix(ctx, " given");
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    super::super::exceptions::emit_type_error_from_string_result(ctx);
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
        PhpType::Void => {
        // php's ZPP coerces a written `null` into the `string|int` parameter rather than
        // refusing: MEASURED on `php -n` 8.5.6, `chown($f, null)` deprecates, then reports
        // `Operation not permitted` — the answer for uid 0, not for a name. Leaving it to the
        // arm below refused a program php runs, and refused it in the BACKEND, where the
        // checker had already accepted the call.
            emit_null_principal_deprecation(ctx, name, kind);
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            emit_owner_group_wrapper_dispatch(ctx, principal_int_option(kind));
        }
        PhpType::Mixed => {
            let option = principal_int_option(kind);
            let helper = principal_string_runtime(kind);
            emit_mixed_principal_dispatch(
                ctx,
                name,
                kind,
                |ctx| emit_owner_group_name_wrapper_dispatch(ctx, principal_name_option(kind), helper),
                |ctx| emit_owner_group_wrapper_dispatch(ctx, option),
            );
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
        PhpType::Void => {
        // php's ZPP coerces a written `null` into the `string|int` parameter rather than
        // refusing: MEASURED on `php -n` 8.5.6, `chown($f, null)` deprecates, then reports
        // `Operation not permitted` — the answer for uid 0, not for a name. Leaving it to the
        // arm below refused a program php runs, and refused it in the BACKEND, where the
        // checker had already accepted the call.
            emit_null_principal_deprecation(ctx, name, kind);
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            emit_owner_group_wrapper_dispatch(ctx, principal_int_option(kind));
        }
        PhpType::Mixed => {
            let option = principal_int_option(kind);
            let helper = principal_string_runtime(kind);
            emit_mixed_principal_dispatch(
                ctx,
                name,
                kind,
                |ctx| emit_owner_group_name_wrapper_dispatch(ctx, principal_name_option(kind), helper),
                |ctx| emit_owner_group_wrapper_dispatch(ctx, option),
            );
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
        PhpType::Str => emit_lprincipal_name_dispatch_aarch64(ctx, kind),
        PhpType::Int => emit_lprincipal_id_dispatch_aarch64(ctx, kind),
        PhpType::Void => {
        // php's ZPP coerces a written `null` into the `string|int` parameter rather than
        // refusing: MEASURED on `php -n` 8.5.6, `chown($f, null)` deprecates, then reports
        // `Operation not permitted` — the answer for uid 0, not for a name. Leaving it to the
        // arm below refused a program php runs, and refused it in the BACKEND, where the
        // checker had already accepted the call.
            emit_null_principal_deprecation(ctx, name, kind);
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            emit_lprincipal_id_dispatch_aarch64(ctx, kind);
        }
        PhpType::Mixed => emit_mixed_principal_dispatch(
            ctx,
            name,
            kind,
            |ctx| emit_lprincipal_name_dispatch_aarch64(ctx, kind),
            |ctx| emit_lprincipal_id_dispatch_aarch64(ctx, kind),
        ),
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
        PhpType::Str => emit_lprincipal_name_dispatch_x86_64(ctx, kind),
        PhpType::Int => emit_lprincipal_id_dispatch_x86_64(ctx, kind),
        PhpType::Void => {
        // php's ZPP coerces a written `null` into the `string|int` parameter rather than
        // refusing: MEASURED on `php -n` 8.5.6, `chown($f, null)` deprecates, then reports
        // `Operation not permitted` — the answer for uid 0, not for a name. Leaving it to the
        // arm below refused a program php runs, and refused it in the BACKEND, where the
        // checker had already accepted the call.
            emit_null_principal_deprecation(ctx, name, kind);
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            emit_lprincipal_id_dispatch_x86_64(ctx, kind);
        }
        PhpType::Mixed => emit_mixed_principal_dispatch(
            ctx,
            name,
            kind,
            |ctx| emit_lprincipal_name_dispatch_x86_64(ctx, kind),
            |ctx| emit_lprincipal_id_dispatch_x86_64(ctx, kind),
        ),
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} principal PHP type {:?}",
                name, other
            )));
        }
    }
    Ok(())
}

/// Emits the AArch64 symlink ownership call for a NAME principal in the string-result registers.
fn emit_lprincipal_name_dispatch_aarch64(ctx: &mut FunctionContext<'_>, kind: PrincipalKind) {
    ctx.emitter.instruction("mov x3, x1");                                      // pass principal name pointer to symlink ownership helper
    ctx.emitter.instruction("mov x4, x2");                                      // pass principal name length to symlink ownership helper
    abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
    abi::emit_call_label(ctx.emitter, lprincipal_string_runtime(kind));
}

/// Emits the AArch64 symlink ownership call for a uid/gid principal in the int-result register.
fn emit_lprincipal_id_dispatch_aarch64(ctx: &mut FunctionContext<'_>, kind: PrincipalKind) {
    ctx.emitter.instruction("mov x9, x0");                                      // preserve uid/gid while restoring the path
    abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
    if matches!(kind, PrincipalKind::Owner) {
        ctx.emitter.instruction("mov x3, x9");                                  // pass uid and leave symlink group unchanged
        ctx.emitter.instruction("mov x4, #-1");                                 // keep the symlink group unchanged
    } else {
        ctx.emitter.instruction("mov x3, #-1");                                 // keep the symlink owner unchanged
        ctx.emitter.instruction("mov x4, x9");                                  // pass gid and leave symlink owner unchanged
    }
    abi::emit_call_label(ctx.emitter, lprincipal_int_runtime(kind));
}

/// Emits the x86_64 symlink ownership call for a NAME principal in the string-result registers.
fn emit_lprincipal_name_dispatch_x86_64(ctx: &mut FunctionContext<'_>, kind: PrincipalKind) {
    ctx.emitter.instruction("mov rdi, rax");                                    // pass principal name pointer to symlink ownership helper
    ctx.emitter.instruction("mov rsi, rdx");                                    // pass principal name length to symlink ownership helper
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    abi::emit_call_label(ctx.emitter, lprincipal_string_runtime(kind));
}

/// Emits the x86_64 symlink ownership call for a uid/gid principal in the int-result register.
fn emit_lprincipal_id_dispatch_x86_64(ctx: &mut FunctionContext<'_>, kind: PrincipalKind) {
    ctx.emitter.instruction("mov r9, rax");                                     // preserve uid/gid while restoring the path
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    if matches!(kind, PrincipalKind::Owner) {
        ctx.emitter.instruction("mov rdi, r9");                                 // pass uid and leave symlink group unchanged
        ctx.emitter.instruction("mov rsi, -1");                                 // keep the symlink group unchanged
    } else {
        ctx.emitter.instruction("mov rdi, -1");                                 // keep the symlink owner unchanged
        ctx.emitter.instruction("mov rsi, r9");                                 // pass gid and leave symlink owner unchanged
    }
    abi::emit_call_label(ctx.emitter, lprincipal_int_runtime(kind));
}

/// Returns the ownership syscall entry point that names THIS caller in its warning.
///
/// `chown()` and `chgrp()` are one syscall with the other principal set to `-1`, but php names
/// the caller in the diagnostic — `Warning: chgrp(): No such file or directory` — so each has its
/// own entry point rather than a shared one taking the name as an argument.
pub(super) fn principal_int_runtime(kind: PrincipalKind) -> &'static str {
    match kind {
        PrincipalKind::Owner => "__rt_chown",
        PrincipalKind::Group => "__rt_chgrp",
    }
}

/// The symlink-aware sibling of [`principal_int_runtime`].
pub(super) fn lprincipal_int_runtime(kind: PrincipalKind) -> &'static str {
    match kind {
        PrincipalKind::Owner => "__rt_lchown",
        PrincipalKind::Group => "__rt_lchgrp",
    }
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
            // php coerces `$permissions` from any scalar — `"0644"` reads as DECIMAL 644 — so the
            // same helper every other int-taking builtin uses does the work here too.
            crate::codegen::lower_inst::builtins::strings::load_as_int(ctx, mode, "chmod mode")?;
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
            // php coerces `$permissions` from any scalar — `"0644"` reads as DECIMAL 644 — so the
            // same helper every other int-taking builtin uses does the work here too.
            crate::codegen::lower_inst::builtins::strings::load_as_int(ctx, mode, "chmod mode")?;
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
    let runtime = principal_int_runtime(if is_owner {
        PrincipalKind::Owner
    } else {
        PrincipalKind::Group
    });
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
            abi::emit_call_label(ctx.emitter, runtime);
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
            abi::emit_call_label(ctx.emitter, runtime);
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

