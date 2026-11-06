//! Purpose:
//! Lowers the internal libc-backed `__elephc_setlocale_query` and
//! `__elephc_setlocale_set` builtins used by the injected `setlocale()` prelude.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions` through the typed runtime targets.
//!
//! Key details:
//! - libc `setlocale` returns a pointer to a static buffer, so the result is persisted
//!   into an owned PHP string before another locale change can overwrite it.
//! - Locale names are copied into a NUL-terminated stack buffer because PHP strings are
//!   length-delimited and need not end in NUL.
//! - A locale name longer than 255 bytes is refused with PHP `false` instead of being
//!   truncated into a different locale.

use crate::codegen::abi;
use crate::codegen::emit_box_current_value_as_mixed;
use crate::codegen::platform::Arch;
use crate::codegen::Result;
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{ensure_arg_count, expect_operand, store_if_result};

/// Longest locale name copied into the stack buffer (excluding the NUL byte).
const MAX_LOCALE_NAME: i64 = 255;

/// Lowers `__elephc_setlocale_query(category)` through libc `setlocale(category, NULL)`.
pub(crate) fn lower_setlocale_query(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "__elephc_setlocale_query", 1)?;
    let category = expect_operand(inst, 0)?;
    super::strings::load_as_int(ctx, category, "setlocale category")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, xzr");                             // query the category without changing it
            ctx.emitter.bl_c("setlocale");                                      // read the current libc locale name
            emit_aarch64_persist_result(ctx, 16);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the locale category as the first SysV argument
            ctx.emitter.instruction("xor esi, esi");                            // query the category without changing it
            ctx.emitter.bl_c("setlocale");                                      // read the current libc locale name
            emit_x86_64_persist_result(ctx, 16);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_setlocale_set(category, locale)` through libc `setlocale`.
pub(crate) fn lower_setlocale_set(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "__elephc_setlocale_set", 2)?;
    let category = expect_operand(inst, 0)?;
    let locale = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_aarch64_set(ctx, category, locale)?,
        Arch::X86_64 => lower_x86_64_set(ctx, category, locale)?,
    }
    store_if_result(ctx, inst)
}

/// AArch64 setter: copy the PHP locale into a NUL-terminated stack buffer, then call libc.
fn lower_aarch64_set(
    ctx: &mut FunctionContext<'_>,
    category: crate::ir::ValueId,
    locale: crate::ir::ValueId,
) -> Result<()> {
    let too_long = ctx.next_label("setlocale_too_long");
    let copy_loop = ctx.next_label("setlocale_copy");
    let copy_done = ctx.next_label("setlocale_copy_done");
    ctx.emitter.instruction("sub sp, sp, #288");                                // reserve the NUL-terminated locale buffer and result slot
    ctx.load_value_to_result(locale)?;
    ctx.emitter.instruction(&format!("cmp x2, #{MAX_LOCALE_NAME}"));            // reject names that do not fit the fixed buffer
    ctx.emitter.instruction(&format!("b.hi {too_long}"));                       // answer false rather than truncating a locale name
    ctx.emitter.instruction("add x9, sp, #16");                                 // select the start of the locale-name buffer
    ctx.emitter.instruction("mov x10, #0");                                     // copy the locale name from its first byte
    ctx.emitter.label(&copy_loop);
    ctx.emitter.instruction("cmp x10, x2");                                     // check whether every locale byte has been copied
    ctx.emitter.instruction(&format!("b.hs {copy_done}"));                      // finish once the complete name is in the buffer
    ctx.emitter.instruction("ldrb w11, [x1, x10]");                             // load one locale-name byte
    ctx.emitter.instruction("strb w11, [x9, x10]");                             // store it into the NUL-terminated buffer
    ctx.emitter.instruction("add x10, x10, #1");                                // advance to the next locale byte
    ctx.emitter.instruction(&format!("b {copy_loop}"));                         // continue copying the name
    ctx.emitter.label(&copy_done);
    ctx.emitter.instruction("strb wzr, [x9, x2]");                              // append the C string terminator
    super::strings::load_as_int(ctx, category, "setlocale category")?;
    ctx.emitter.instruction("add x1, sp, #16");                                 // pass the NUL-terminated locale name
    ctx.emitter.bl_c("setlocale");                                              // apply the requested libc locale
    let after_call = ctx.next_label("setlocale_after_call");
    ctx.emitter.instruction(&format!("b {after_call}"));                        // join the shared result handling
    ctx.emitter.label(&too_long);
    ctx.emitter.instruction("mov x0, #0");                                      // an oversized locale name is refused
    ctx.emitter.label(&after_call);
    ctx.emitter.instruction("add sp, sp, #288");                                // release the locale buffer before boxing the answer
    emit_aarch64_persist_result(ctx, 16);
    Ok(())
}

/// x86_64 setter: copy the PHP locale into a NUL-terminated stack buffer, then call libc.
fn lower_x86_64_set(
    ctx: &mut FunctionContext<'_>,
    category: crate::ir::ValueId,
    locale: crate::ir::ValueId,
) -> Result<()> {
    let too_long = ctx.next_label("setlocale_too_long");
    let copy_loop = ctx.next_label("setlocale_copy");
    let copy_done = ctx.next_label("setlocale_copy_done");
    ctx.emitter.instruction("sub rsp, 288");                                    // reserve the NUL-terminated locale buffer and result slot
    ctx.load_value_to_result(locale)?;
    ctx.emitter.instruction(&format!("cmp rdx, {MAX_LOCALE_NAME}"));            // reject names that do not fit the fixed buffer
    ctx.emitter.instruction(&format!("ja {too_long}"));                         // answer false rather than truncating a locale name
    ctx.emitter.instruction("lea r8, [rsp + 16]");                              // select the start of the locale-name buffer
    ctx.emitter.instruction("xor r9d, r9d");                                    // copy the locale name from its first byte
    ctx.emitter.label(&copy_loop);
    ctx.emitter.instruction("cmp r9, rdx");                                     // check whether every locale byte has been copied
    ctx.emitter.instruction(&format!("jae {copy_done}"));                       // finish once the complete name is in the buffer
    ctx.emitter.instruction("mov r10b, BYTE PTR [rax + r9]");                   // load one locale-name byte
    ctx.emitter.instruction("mov BYTE PTR [r8 + r9], r10b");                    // store it into the NUL-terminated buffer
    ctx.emitter.instruction("inc r9");                                          // advance to the next locale byte
    ctx.emitter.instruction(&format!("jmp {copy_loop}"));                       // continue copying the name
    ctx.emitter.label(&copy_done);
    ctx.emitter.instruction("mov BYTE PTR [r8 + rdx], 0");                      // append the C string terminator
    super::strings::load_as_int(ctx, category, "setlocale category")?;
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the locale category
    ctx.emitter.instruction("lea rsi, [rsp + 16]");                             // pass the NUL-terminated locale name
    ctx.emitter.bl_c("setlocale");                                              // apply the requested libc locale
    let after_call = ctx.next_label("setlocale_after_call");
    ctx.emitter.instruction(&format!("jmp {after_call}"));                      // join the shared result handling
    ctx.emitter.label(&too_long);
    ctx.emitter.instruction("xor eax, eax");                                    // an oversized locale name is refused
    ctx.emitter.label(&after_call);
    ctx.emitter.instruction("add rsp, 288");                                    // release the locale buffer before boxing the answer
    emit_x86_64_persist_result(ctx, 16);
    Ok(())
}

/// AArch64 common tail: persist libc's returned C string, or box PHP false.
fn emit_aarch64_persist_result(ctx: &mut FunctionContext<'_>, frame: i64) {
    let false_label = ctx.next_label("setlocale_false");
    let done_label = ctx.next_label("setlocale_done");
    ctx.emitter.instruction(&format!("cbz x0, {false_label}"));                 // a NULL libc result is PHP false
    if frame != 0 {
        ctx.emitter.instruction(&format!("sub sp, sp, #{frame}"));              // reserve the saved C-string pointer slot
    }
    ctx.emitter.instruction("str x0, [sp]");                                    // preserve libc's static result pointer across strlen
    ctx.emitter.bl_c("strlen");                                                 // measure the returned locale name
    ctx.emitter.instruction("mov x2, x0");                                      // pass the locale-name length to persistence
    ctx.emitter.instruction("ldr x1, [sp]");                                    // restore the locale-name pointer
    ctx.emitter.instruction("bl __rt_str_persist");                             // copy libc's static buffer into an owned PHP string
    if frame != 0 {
        ctx.emitter.instruction(&format!("add sp, sp, #{frame}"));              // release the pointer slot
    }
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
    ctx.emitter.instruction(&format!("b {done_label}"));                        // skip the false branch after boxing the string
    ctx.emitter.label(&false_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    ctx.emitter.label(&done_label);
}

/// x86_64 common tail: persist libc's returned C string, or box PHP false.
fn emit_x86_64_persist_result(ctx: &mut FunctionContext<'_>, frame: i64) {
    let false_label = ctx.next_label("setlocale_false");
    let done_label = ctx.next_label("setlocale_done");
    ctx.emitter.instruction(&format!("test rax, rax"));                         // a NULL libc result is PHP false
    ctx.emitter.instruction(&format!("jz {false_label}"));                      // branch to the PHP false result
    if frame != 0 {
        ctx.emitter.instruction(&format!("sub rsp, {frame}"));                  // reserve the saved C-string pointer slot
    }
    ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                        // preserve libc's static result pointer across strlen
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the locale-name pointer to strlen
    ctx.emitter.bl_c("strlen");                                                 // measure the returned locale name
    ctx.emitter.instruction("mov rdx, rax");                                    // pass the locale-name length to persistence
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                        // restore the locale-name pointer
    ctx.emitter.instruction("call __rt_str_persist");                           // copy libc's static buffer into an owned PHP string
    if frame != 0 {
        ctx.emitter.instruction(&format!("add rsp, {frame}"));                  // release the pointer slot
    }
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
    ctx.emitter.instruction(&format!("jmp {done_label}"));                      // skip the false branch after boxing the string
    ctx.emitter.label(&false_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    ctx.emitter.label(&done_label);
}
