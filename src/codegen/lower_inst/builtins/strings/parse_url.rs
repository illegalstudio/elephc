//! Purpose:
//! Lowers the PHP `parse_url()` builtin into its target-aware runtime call.
//!
//! Called from:
//! - The string builtin lowering facade through typed `RuntimeFnId::ParseUrl` dispatch.
//!
//! Key details:
//! - Source string evaluation stays in the shared coercion path while the optional selector
//!   is materialized in the runtime scanner's architecture-specific argument register.

use super::*;

/// Lowers `parse_url(url, component?)` into the Mixed-returning runtime scanner.
pub(crate) fn lower_parse_url(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count_between(inst, "parse_url", 1, 2)?;
    let ptr_reg = string_ptr_reg(ctx);
    let len_reg = string_len_reg(ctx);
    load_string_arg_to_regs(ctx, inst, 0, "parse_url url", ptr_reg, len_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if let Some(component) = inst.operands.get(1).copied() {
                abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
                load_value_to_first_int_arg(ctx, component)?;
                ctx.emitter.instruction("mov x3, x0");                          // pass the selected PHP_URL_* component to the runtime scanner
                abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            } else {
                ctx.emitter.instruction("mov x3, #-1");                         // use PHP's full-array default component selector
            }
        }
        Arch::X86_64 => {
            if let Some(component) = inst.operands.get(1).copied() {
                abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
                load_value_to_first_int_arg(ctx, component)?;
                ctx.emitter.instruction("mov rdi, rax");                        // pass the selected PHP_URL_* component to the runtime scanner
                abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            } else {
                ctx.emitter.instruction("mov rdi, -1");                         // use PHP's full-array default component selector
            }
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("mov x5, #0"),                  // full array result mode
        Arch::X86_64 => ctx.emitter.instruction("xor r11d, r11d"),               // full array result mode
    }
    abi::emit_call_label(ctx.emitter, "__rt_parse_url");
    store_if_result(ctx, inst)
}

/// Lowers the allocation-free `count(parse_url(...))` helper.
pub(crate) fn lower_parse_url_count(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count_between(inst, "parse_url", 1, 1)?;
    let ptr_reg = string_ptr_reg(ctx);
    let len_reg = string_len_reg(ctx);
    load_string_arg_to_regs(ctx, inst, 0, "parse_url url", ptr_reg, len_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x3, #-1");                             // the count form only covers the full-array default
            ctx.emitter.instruction("mov x5, #1");                              // count-only result mode
            abi::emit_call_label(ctx.emitter, "__rt_parse_url");
            let ok = ctx.next_label("parse_url_count_ok");
            ctx.emitter.instruction(&format!("cbnz x1, {ok}"));                  // status 1 means the parser built an array
            crate::codegen::lower_inst::builtins::count_empty::emit_count_type_error(ctx, "false");
            ctx.emitter.label(&ok);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, -1");                             // the count form only covers the full-array default
            ctx.emitter.instruction("mov r11d, 1");                             // count-only result mode
            abi::emit_call_label(ctx.emitter, "__rt_parse_url");
            let ok = ctx.next_label("parse_url_count_ok");
            ctx.emitter.instruction(&format!("test rdx, rdx"));                  // did the parser build an array?
            ctx.emitter.instruction(&format!("jnz {ok}"));                       // status 1 means the count is valid
            crate::codegen::lower_inst::builtins::count_empty::emit_count_type_error(ctx, "false");
            ctx.emitter.label(&ok);
        }
    }
    store_if_result(ctx, inst)
}
