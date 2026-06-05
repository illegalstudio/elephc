//! Purpose:
//! Lowers filesystem metadata builtins for the EIR backend.
//! Reuses the shared runtime stat helpers instead of duplicating platform logic.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Path operands are already evaluated by EIR and are materialized into the
//!   string result registers expected by the legacy runtime helpers.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;
const TOUCH_ATIME_NOW: u8 = 1;
const TOUCH_MTIME_NOW: u8 = 2;
const TOUCH_BOTH_NOW: u8 = TOUCH_ATIME_NOW | TOUCH_MTIME_NOW;

/// Lowers `file_get_contents(path)` and boxes the runtime string-or-false result.
pub(super) fn lower_file_get_contents(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "file_get_contents", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "file_get_contents filename")?;
    abi::emit_call_label(ctx.emitter, "__rt_file_get_contents");
    box_owned_string_or_false_result(ctx, "fgc");
    store_if_result(ctx, inst)
}

/// Lowers `realpath(path)` and boxes the owned runtime string-or-false result.
pub(super) fn lower_realpath(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "realpath", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "realpath")?;
    abi::emit_call_label(ctx.emitter, "__rt_realpath");
    box_owned_string_or_false_result(ctx, "realpath");
    store_if_result(ctx, inst)
}

/// Lowers `file_put_contents(path, data)` through the target-aware runtime writer.
pub(super) fn lower_file_put_contents(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "file_put_contents", 2)?;
    let path = expect_operand(inst, 0)?;
    let data = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_file_put_contents_arm64(ctx, path, data)?,
        Arch::X86_64 => lower_file_put_contents_x86_64(ctx, path, data)?,
    }
    store_if_result(ctx, inst)
}

/// Lowers `file_exists(path)` through the target-aware runtime stat helper.
pub(super) fn lower_file_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "file_exists", "__rt_file_exists")
}

/// Lowers `unlink(path)` through the target-aware runtime helper.
pub(super) fn lower_unlink(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "unlink", "__rt_unlink")
}

/// Lowers `mkdir(path)` through the target-aware runtime helper.
pub(super) fn lower_mkdir(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "mkdir", "__rt_mkdir")
}

/// Lowers `rmdir(path)` through the target-aware runtime helper.
pub(super) fn lower_rmdir(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "rmdir", "__rt_rmdir")
}

/// Lowers `chdir(path)` through the target-aware runtime helper.
pub(super) fn lower_chdir(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "chdir", "__rt_chdir")
}

/// Lowers `copy(source, dest)` through the target-aware runtime helper.
pub(super) fn lower_copy(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_path_call(ctx, inst, "copy", "__rt_copy")
}

/// Lowers `rename(from, to)` through the target-aware runtime helper.
pub(super) fn lower_rename(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_path_call(ctx, inst, "rename", "__rt_rename")
}

/// Lowers `tempnam(directory, prefix)` through the target-aware runtime helper.
pub(super) fn lower_tempnam(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_path_call(ctx, inst, "tempnam", "__rt_tempnam")
}

/// Lowers `chmod(path, mode)` through the target-aware runtime helper.
pub(super) fn lower_chmod(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "chmod", 2)?;
    let path = expect_operand(inst, 0)?;
    let mode = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, path, "chmod path")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            require_int(ctx.load_value_to_result(mode)?.codegen_repr(), "chmod mode")?;
            ctx.emitter.instruction("mov x3, x0");                              // pass the requested mode to the chmod runtime helper
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, path, "chmod path")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            require_int(ctx.load_value_to_result(mode)?.codegen_repr(), "chmod mode")?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the requested mode to the chmod runtime helper
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_chmod");
    store_if_result(ctx, inst)
}

/// Lowers `chown(path, owner)` for integer UIDs and string user names.
pub(super) fn lower_chown(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_chown_or_chgrp(ctx, inst, "chown", PrincipalKind::Owner)
}

/// Lowers `chgrp(path, group)` for integer GIDs and string group names.
pub(super) fn lower_chgrp(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_chown_or_chgrp(ctx, inst, "chgrp", PrincipalKind::Group)
}

/// Lowers `umask(mask?)` through the target-aware runtime helper.
pub(super) fn lower_umask(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
pub(super) fn lower_touch(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "touch", 1, 3)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "touch path")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_touch_args_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_touch_args_x86_64(ctx, inst)?,
    }
    abi::emit_call_label(ctx.emitter, "__rt_touch");
    store_if_result(ctx, inst)
}

/// Lowers `basename(path, suffix?)` through the target-aware runtime helper.
pub(super) fn lower_basename(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
pub(super) fn lower_dirname(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
pub(super) fn lower_fnmatch(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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

/// Selects which ownership field a filesystem principal builtin changes.
#[derive(Clone, Copy)]
enum PrincipalKind {
    Owner,
    Group,
}

/// Selects how `touch()` should materialize optional timestamp operands.
enum TouchTimeShape {
    BothNow,
    MtimeAlsoAtime,
    ExplicitBoth,
}

/// Lowers the shared path/principal calling convention for `chown()` and `chgrp()`.
fn lower_chown_or_chgrp(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    kind: PrincipalKind,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 2)?;
    let path = expect_operand(inst, 0)?;
    let principal = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_chown_or_chgrp_aarch64(ctx, path, principal, name, kind)?,
        Arch::X86_64 => lower_chown_or_chgrp_x86_64(ctx, path, principal, name, kind)?,
    }
    store_if_result(ctx, inst)
}

/// Materializes `chown()`/`chgrp()` operands for the ARM64 runtime ABI.
fn lower_chown_or_chgrp_aarch64(
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
            ctx.emitter.instruction("mov x3, x1");                              // pass the principal-name pointer to the resolver helper
            ctx.emitter.instruction("mov x4, x2");                              // pass the principal-name length to the resolver helper
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            abi::emit_call_label(ctx.emitter, principal_string_runtime(kind));
        }
        PhpType::Int => {
            match kind {
                PrincipalKind::Owner => {
                    ctx.emitter.instruction("mov x3, x0");                      // pass the target uid to chown(path, uid, -1)
                    ctx.emitter.instruction("mov x4, #-1");                     // keep the file group unchanged
                }
                PrincipalKind::Group => {
                    ctx.emitter.instruction("mov x4, x0");                      // pass the target gid to chown(path, -1, gid)
                    ctx.emitter.instruction("mov x3, #-1");                     // keep the file owner unchanged
                }
            }
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            abi::emit_call_label(ctx.emitter, "__rt_chown");
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
fn lower_chown_or_chgrp_x86_64(
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
            ctx.emitter.instruction("mov rdi, rax");                            // pass the principal-name pointer to the resolver helper
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the principal-name length to the resolver helper
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            abi::emit_call_label(ctx.emitter, principal_string_runtime(kind));
        }
        PhpType::Int => {
            match kind {
                PrincipalKind::Owner => {
                    ctx.emitter.instruction("mov rdi, rax");                    // pass the target uid to chown(path, uid, -1)
                    ctx.emitter.instruction("mov rsi, -1");                     // keep the file group unchanged
                }
                PrincipalKind::Group => {
                    ctx.emitter.instruction("mov rsi, rax");                    // pass the target gid to chown(path, -1, gid)
                    ctx.emitter.instruction("mov rdi, -1");                     // keep the file owner unchanged
                }
            }
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            abi::emit_call_label(ctx.emitter, "__rt_chown");
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

/// Returns the string-principal runtime helper for the ownership field.
fn principal_string_runtime(kind: PrincipalKind) -> &'static str {
    match kind {
        PrincipalKind::Owner => "__rt_chown_user",
        PrincipalKind::Group => "__rt_chgrp_group",
    }
}

/// Materializes timestamp arguments for the `touch()` call on ARM64.
fn lower_touch_args_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    match touch_time_shape(ctx, inst)? {
        TouchTimeShape::BothNow => {
            ctx.emitter.instruction("mov x3, #0");                              // ignored mtime seconds when runtime uses current time
            ctx.emitter.instruction("mov x4, #0");                              // ignored atime seconds when runtime uses current time
            ctx.emitter.instruction(&format!("mov x5, #{}", TOUCH_BOTH_NOW));   // mark mtime and atime as current-time fields
        }
        TouchTimeShape::MtimeAlsoAtime => {
            let mtime = expect_operand(inst, 1)?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            require_int(ctx.load_value_to_result(mtime)?.codegen_repr(), "touch mtime")?;
            ctx.emitter.instruction("mov x3, x0");                              // pass explicit mtime seconds
            ctx.emitter.instruction("mov x4, x0");                              // default atime to the explicit mtime seconds
            ctx.emitter.instruction("mov x5, #0");                              // mark both timestamp fields as explicit
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        TouchTimeShape::ExplicitBoth => {
            let mtime = expect_operand(inst, 1)?;
            let atime = expect_operand(inst, 2)?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            require_int(ctx.load_value_to_result(mtime)?.codegen_repr(), "touch mtime")?;
            ctx.emitter.instruction("str x0, [sp, #-16]!");                     // save explicit mtime while atime is evaluated
            require_int(ctx.load_value_to_result(atime)?.codegen_repr(), "touch atime")?;
            ctx.emitter.instruction("mov x4, x0");                              // pass explicit atime seconds
            ctx.emitter.instruction("ldr x3, [sp], #16");                       // restore explicit mtime seconds
            ctx.emitter.instruction("mov x5, #0");                              // mark both timestamp fields as explicit
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
    }
    Ok(())
}

/// Materializes timestamp arguments for the `touch()` call on x86_64.
fn lower_touch_args_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    match touch_time_shape(ctx, inst)? {
        TouchTimeShape::BothNow => {
            ctx.emitter.instruction("mov rdi, 0");                              // ignored mtime seconds when runtime uses current time
            ctx.emitter.instruction("mov rsi, 0");                              // ignored atime seconds when runtime uses current time
            ctx.emitter.instruction(&format!("mov rcx, {}", TOUCH_BOTH_NOW));   // mark mtime and atime as current-time fields
        }
        TouchTimeShape::MtimeAlsoAtime => {
            let mtime = expect_operand(inst, 1)?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            require_int(ctx.load_value_to_result(mtime)?.codegen_repr(), "touch mtime")?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass explicit mtime seconds
            ctx.emitter.instruction("mov rsi, rax");                            // default atime to the explicit mtime seconds
            ctx.emitter.instruction("mov rcx, 0");                              // mark both timestamp fields as explicit
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
        TouchTimeShape::ExplicitBoth => {
            let mtime = expect_operand(inst, 1)?;
            let atime = expect_operand(inst, 2)?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            require_int(ctx.load_value_to_result(mtime)?.codegen_repr(), "touch mtime")?;
            ctx.emitter.instruction("sub rsp, 16");                             // reserve aligned temporary storage for mtime
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // save explicit mtime while atime is evaluated
            require_int(ctx.load_value_to_result(atime)?.codegen_repr(), "touch atime")?;
            ctx.emitter.instruction("mov rsi, rax");                            // pass explicit atime seconds
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // restore explicit mtime seconds
            ctx.emitter.instruction("add rsp, 16");                             // release the aligned mtime temporary
            ctx.emitter.instruction("mov rcx, 0");                              // mark both timestamp fields as explicit
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    Ok(())
}

/// Classifies optional `touch()` timestamp operands after EIR type checking.
fn touch_time_shape(ctx: &FunctionContext<'_>, inst: &Instruction) -> Result<TouchTimeShape> {
    match inst.operands.len() {
        1 => Ok(TouchTimeShape::BothNow),
        2 if is_nullish_value(ctx, expect_operand(inst, 1)?)? => Ok(TouchTimeShape::BothNow),
        2 => Ok(TouchTimeShape::MtimeAlsoAtime),
        _ if is_nullish_value(ctx, expect_operand(inst, 1)?)?
            && is_nullish_value(ctx, expect_operand(inst, 2)?)? =>
        {
            Ok(TouchTimeShape::BothNow)
        }
        _ if is_nullish_value(ctx, expect_operand(inst, 2)?)? => {
            Ok(TouchTimeShape::MtimeAlsoAtime)
        }
        _ => Ok(TouchTimeShape::ExplicitBoth),
    }
}

/// Returns true when an EIR value represents PHP `null`.
fn is_nullish_value(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    Ok(matches!(
        ctx.value_php_type(value)?.codegen_repr(),
        PhpType::Void
    ))
}

/// Lowers `getcwd()` through the target-aware runtime helper.
pub(super) fn lower_getcwd(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "getcwd", 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_getcwd");
    store_if_result(ctx, inst)
}

/// Lowers `sys_get_temp_dir()` as the project's hardcoded `/tmp` string.
pub(super) fn lower_sys_get_temp_dir(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "sys_get_temp_dir", 0)?;
    let (label, len) = ctx.data.add_string(b"/tmp");
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    store_if_result(ctx, inst)
}

/// Lowers `filesize(path)` through the target-aware runtime stat helper.
pub(super) fn lower_filesize(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_int(ctx, inst, "filesize", "__rt_filesize")
}

/// Lowers `filemtime(path)` through the target-aware runtime stat helper.
pub(super) fn lower_filemtime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_int(ctx, inst, "filemtime", "__rt_filemtime")
}

/// Lowers `linkinfo(path)` through the target-aware runtime lstat helper.
pub(super) fn lower_linkinfo(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_int(ctx, inst, "linkinfo", "__rt_linkinfo")
}

/// Lowers `symlink(target, link)` through the target-aware libc wrapper.
pub(super) fn lower_symlink(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_path_call(ctx, inst, "symlink", "__rt_symlink")
}

/// Lowers `link(oldpath, newpath)` through the target-aware libc wrapper.
pub(super) fn lower_link(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_path_call(ctx, inst, "link", "__rt_link")
}

/// Lowers `readlink(path)` and boxes the owned runtime string-or-false result.
pub(super) fn lower_readlink(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "readlink", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "readlink")?;
    abi::emit_call_label(ctx.emitter, "__rt_readlink");
    box_owned_string_or_false_result(ctx, "readlink");
    store_if_result(ctx, inst)
}

/// Lowers `fileatime(path)` and boxes the runtime integer-or-false result.
pub(super) fn lower_fileatime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_stat_int_or_false(ctx, inst, "fileatime", "__rt_fileatime")
}

/// Lowers `filectime(path)` and boxes the runtime integer-or-false result.
pub(super) fn lower_filectime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_stat_int_or_false(ctx, inst, "filectime", "__rt_filectime")
}

/// Lowers `fileperms(path)` and boxes the runtime integer-or-false result.
pub(super) fn lower_fileperms(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_stat_int_or_false(ctx, inst, "fileperms", "__rt_fileperms")
}

/// Lowers `fileowner(path)` and boxes the runtime integer-or-false result.
pub(super) fn lower_fileowner(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_stat_int_or_false(ctx, inst, "fileowner", "__rt_fileowner")
}

/// Lowers `filegroup(path)` and boxes the runtime integer-or-false result.
pub(super) fn lower_filegroup(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_stat_int_or_false(ctx, inst, "filegroup", "__rt_filegroup")
}

/// Lowers `fileinode(path)` and boxes the runtime integer-or-false result.
pub(super) fn lower_fileinode(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_stat_int_or_false(ctx, inst, "fileinode", "__rt_fileinode")
}

/// Lowers `filetype(path)` and boxes the runtime string-or-false result.
pub(super) fn lower_filetype(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "filetype", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "filetype")?;
    abi::emit_call_label(ctx.emitter, "__rt_filetype");
    box_stat_string_or_false_result(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `clearstatcache(...)` as an ordered no-op after EIR operand evaluation.
pub(super) fn lower_clearstatcache(
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
pub(super) fn lower_is_file(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "is_file", "__rt_is_file")
}

/// Lowers `is_dir(path)` through the target-aware runtime stat helper.
pub(super) fn lower_is_dir(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "is_dir", "__rt_is_dir")
}

/// Lowers `is_readable(path)` through the target-aware runtime access helper.
pub(super) fn lower_is_readable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "is_readable", "__rt_is_readable")
}

/// Lowers `is_writable(path)` through the target-aware runtime access helper.
pub(super) fn lower_is_writable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "is_writable", "__rt_is_writable")
}

/// Lowers `is_writeable(path)`, PHP's alias of `is_writable(path)`.
pub(super) fn lower_is_writeable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "is_writeable", "__rt_is_writable")
}

/// Lowers `is_executable(path)` through the target-aware runtime access helper.
pub(super) fn lower_is_executable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "is_executable", "__rt_is_executable")
}

/// Lowers `is_link(path)` through the target-aware runtime lstat helper.
pub(super) fn lower_is_link(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "is_link", "__rt_is_link")
}

/// Loads a path string into runtime argument/result registers and stores the boolean result.
fn lower_unary_path_predicate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    require_string(ctx.load_value_to_result(path)?.codegen_repr(), name)?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Loads a path string into runtime argument/result registers and stores the integer result.
fn lower_unary_path_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, name)?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Loads two path strings into the runtime ABI, calls a helper, and stores its result.
fn lower_binary_path_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 2)?;
    let first = expect_operand(inst, 0)?;
    let second = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, first, name)?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, second, name)?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the second path pointer in the runtime helper's secondary string slot
            ctx.emitter.instruction("mov x4, x2");                              // pass the second path length in the runtime helper's secondary string slot
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, first, name)?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, second, name)?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the second path pointer while the first path remains on the stack
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the second path length while the first path remains on the stack
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Verifies that a builtin call has a lowered operand count within an inclusive range.
fn ensure_arg_count_between(inst: &Instruction, name: &str, min: usize, max: usize) -> Result<()> {
    let actual = inst.operands.len();
    if (min..=max).contains(&actual) {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {}..={} args, got {}",
        name, min, max, actual
    )))
}

/// Loads a path string, calls a stat helper, boxes int success or PHP false, and stores it.
fn lower_unary_path_stat_int_or_false(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, name)?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    box_stat_int_or_false_result(ctx);
    store_if_result(ctx, inst)
}

/// Materializes `file_put_contents` arguments for the ARM64 runtime ABI.
fn lower_file_put_contents_arm64(
    ctx: &mut FunctionContext<'_>,
    path: ValueId,
    data: ValueId,
) -> Result<()> {
    load_string_to_result(ctx, path, "file_put_contents filename")?;
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
    load_string_to_result(ctx, data, "file_put_contents data")?;
    ctx.emitter.instruction("mov x3, x1");                                      // pass the data pointer in the runtime helper's second string slot
    ctx.emitter.instruction("mov x4, x2");                                      // pass the data length in the runtime helper's second string slot
    abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
    abi::emit_call_label(ctx.emitter, "__rt_file_put_contents");
    Ok(())
}

/// Materializes `file_put_contents` arguments for the Linux x86_64 runtime ABI.
fn lower_file_put_contents_x86_64(
    ctx: &mut FunctionContext<'_>,
    path: ValueId,
    data: ValueId,
) -> Result<()> {
    load_string_to_result(ctx, path, "file_put_contents filename")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_string_to_result(ctx, data, "file_put_contents data")?;
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the data pointer while the filename remains on the temporary stack
    ctx.emitter.instruction("mov rsi, rdx");                                    // pass the data length while the filename remains on the temporary stack
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    abi::emit_call_label(ctx.emitter, "__rt_file_put_contents");
    Ok(())
}

/// Boxes an owned runtime string result into PHP `string|false` Mixed form.
fn box_owned_string_or_false_result(ctx: &mut FunctionContext<'_>, label_prefix: &str) {
    let false_label = ctx.next_label(&format!("{}_false", label_prefix));
    let done_label = ctx.next_label(&format!("{}_done", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x1, {}", false_label));       // branch when the runtime returned a null string pointer for failure
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            ctx.emitter.instruction("mov x0, #24");                             // request a mixed cell payload with tag and two value words
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #5");                              // select heap kind 5 for a boxed Mixed cell
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the allocation header as a Mixed cell
            ctx.emitter.instruction("mov x9, #1");                              // select runtime tag 1 for a string Mixed payload
            ctx.emitter.instruction("str x9, [x0]");                            // store the string tag in the Mixed cell
            abi::emit_pop_reg_pair(ctx.emitter, "x10", "x11");
            ctx.emitter.instruction("stp x10, x11, [x0, #8]");                  // store the owned string pointer and length in the Mixed cell
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the string Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether the runtime returned a null string pointer for failure
            ctx.emitter.instruction(&format!("jz {}", false_label));            // box false when the runtime string helper failed
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            ctx.emitter.instruction("mov rax, 24");                             // request a mixed cell payload with tag and two value words
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 5)); // materialize the x86_64 Mixed heap kind word
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the allocation header as a Mixed cell
            ctx.emitter.instruction("mov r10, 1");                              // select runtime tag 1 for a string Mixed payload
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store the string tag in the Mixed cell
            abi::emit_pop_reg_pair(ctx.emitter, "r10", "r11");
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");            // store the owned string pointer in the Mixed cell
            ctx.emitter.instruction("mov QWORD PTR [rax + 16], r11");           // store the owned string length in the Mixed cell
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the string Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes the raw stat integer payload into PHP `int|false` Mixed form.
fn box_stat_int_or_false_result(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("stat_int_false");
    let done_label = ctx.next_label("stat_int_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x1, {}", false_label));       // box PHP false when the runtime success flag is unset
            ctx.emitter.instruction("mov x2, xzr");                             // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x1, x0");                              // pass the stat integer as the Mixed low payload word
            ctx.emitter.instruction("mov x0, #0");                              // select runtime tag 0 for an integer Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the integer Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rdx, rdx");                           // test whether the runtime success flag is set
            ctx.emitter.instruction(&format!("jz {}", false_label));            // box PHP false when the stat helper failed
            ctx.emitter.instruction("mov rdi, rax");                            // pass the stat integer as the Mixed low payload word
            ctx.emitter.instruction("xor esi, esi");                            // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("xor eax, eax");                            // select runtime tag 0 for an integer Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the integer Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes the raw stat string slice into PHP `string|false` Mixed form.
fn box_stat_string_or_false_result(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("stat_string_false");
    let done_label = ctx.next_label("stat_string_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x1, {}", false_label));       // box PHP false when the runtime returned a null string pointer
            ctx.emitter.instruction("mov x0, #1");                              // select runtime tag 1 for a string Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the string Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether the runtime returned a null string pointer
            ctx.emitter.instruction(&format!("jz {}", false_label));            // box PHP false when filetype failed
            ctx.emitter.instruction("mov rdi, rax");                            // pass the filetype string pointer as the Mixed low payload word
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the filetype string length as the Mixed high payload word
            ctx.emitter.instruction("mov eax, 1");                              // select runtime tag 1 for a string Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the string Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Loads a string SSA value into the target string result registers.
fn load_string_to_result(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    context: &str,
) -> Result<()> {
    require_string(ctx.load_value_to_result(value)?.codegen_repr(), context)
}

/// Verifies that a filesystem path argument has the supported string representation.
fn require_string(ty: PhpType, name: &str) -> Result<()> {
    if ty == PhpType::Str {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        name,
        ty
    )))
}

/// Verifies that a path builtin scalar argument has the supported integer representation.
fn require_int(ty: PhpType, name: &str) -> Result<()> {
    if ty == PhpType::Int {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        name,
        ty
    )))
}
