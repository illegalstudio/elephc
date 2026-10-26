//! Purpose:
//! Resolves runtime builtin filter names before falling back to user registrations.
//!
//! Called from:
//! - Stream filter append and prepend lowering for names not known at compile time.
//!
//! Key details:
//! - Evaluates the normalized name once and shares literal filter ids and attach paths.
//! - Comparisons allocate nothing and invoke no PHP callbacks.

use super::*;

/// Dispatches a dynamic name through builtin filters and the existing user-filter resolver.
pub(super) fn lower_dynamic_filter_attach(
    ctx: &mut FunctionContext<'_>, inst: &Instruction,
) -> Result<()> {
    let stream = expect_operand(inst, 0)?;
    let filter = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "stream_filter_append")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    load_string_to_result(ctx, filter, "stream_filter_append filter")?;
    let (ptr, len) = if ctx.emitter.target.arch == Arch::AArch64 { ("x1", "x2") }
        else { ("rax", "rdx") };
    abi::emit_push_reg_pair(ctx.emitter, ptr, len);
    let done = ctx.next_label("filter_dynamic_done");
    let mut matches = Vec::new();
    for &(name, id) in BUILTIN_STREAM_FILTERS {
        let matched = ctx.next_label("filter_dynamic_builtin");
        let (label, length) = ctx.data.add_string(name.as_bytes());
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("ldp x1, x2, [sp]");                    // reload the once-evaluated filter name
                abi::emit_symbol_address(ctx.emitter, "x3", &label);
                abi::emit_load_int_immediate(ctx.emitter, "x4", length as i64);
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");            // reload the captured filter-name pointer
                ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");        // reload the captured filter-name length
                abi::emit_symbol_address(ctx.emitter, "rdx", &label);
                abi::emit_load_int_immediate(ctx.emitter, "rcx", length as i64);
            }
        }
        abi::emit_call_label(ctx.emitter, "__rt_strcmp");
        match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.emitter.instruction(&format!("cbz x0, {matched}")),// select the matching builtin filter
            Arch::X86_64 => {
                ctx.emitter.instruction("test rax, rax");                       // compare the complete filter name
                ctx.emitter.instruction(&format!("jz {matched}"));              // select the matching builtin filter
            }
        }
        matches.push((matched, id));
    }
    abi::emit_pop_reg_pair(ctx.emitter, ptr, len);
    finish_user_stream_filter_attach(ctx, inst)?;
    abi::emit_jump(ctx.emitter, &done);
    for (matched, id) in matches {
        ctx.emitter.label(&matched);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        finish_builtin_stream_filter_attach(ctx, inst, id)?;
        abi::emit_jump(ctx.emitter, &done);
    }
    ctx.emitter.label(&done);
    Ok(())
}
