//! Purpose:
//! Lowers `preg_last_error()` / `preg_last_error_msg()` and the error-state bookkeeping
//! every `preg_*` call performs around its runtime helper.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions` for the two error builtins.
//! - `crate::codegen::lower_inst::builtins::regex` around each regex helper call.
//!
//! Key details:
//! - Each `preg_*` call clears `_preg_last_error` first. The runtime's
//!   `__rt_preg_exec` wrapper records match failures (backtrack limit, malformed UTF-8);
//!   the lowering records `PREG_INTERNAL_ERROR` when the helper reports an invalid
//!   pattern or the removed `/e` modifier.
//! - Like PHP, a call whose match failed returns `false` (`preg_match`, `preg_match_all`,
//!   `preg_split`) or `null` (`preg_replace`, `preg_replace_callback`) instead of the
//!   partial result the helper produced; that result is released.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::super::context::FunctionContext;

const ERROR_SYMBOL: &str = "_preg_last_error";
const PREG_INTERNAL_ERROR: i64 = 1;
const NULL_TAG: i64 = 8;

/// Lowers `preg_last_error()` to a load of the runtime error slot.
pub(crate) fn lower_preg_last_error(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "preg_last_error", 0)?;
    abi::emit_load_symbol_to_reg(ctx.emitter, abi::int_result_reg(ctx.emitter), ERROR_SYMBOL, 0);
    super::store_if_result(ctx, inst)
}

/// Lowers `preg_last_error_msg()` through the runtime message table lookup.
pub(crate) fn lower_preg_last_error_msg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "preg_last_error_msg", 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_preg_last_error_msg");
    super::store_if_result(ctx, inst)
}

/// Clears the error slot before a `preg_*` helper runs.
pub(super) fn emit_clear_error(ctx: &mut FunctionContext<'_>) {
    abi::emit_store_zero_to_symbol(ctx.emitter, ERROR_SYMBOL, 0);
}

/// Branches to `label` when the helper recorded a match error, preserving the result registers.
pub(super) fn emit_branch_if_error(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "x10", ERROR_SYMBOL, 0);
            ctx.emitter.instruction(&format!("cbnz x10, {label}"));             // a recorded match error makes the call fail
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", ERROR_SYMBOL, 0);
            ctx.emitter.instruction("test r10, r10");                           // did the helper record a match error?
            ctx.emitter.instruction(&format!("jnz {label}"));                   // a recorded match error makes the call fail
        }
    }
}

/// Records `PREG_INTERNAL_ERROR`, preserving the result registers.
pub(super) fn emit_record_internal_error(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov x10, #{}", PREG_INTERNAL_ERROR)); // an invalid pattern is PHP's internal error
            abi::emit_store_reg_to_symbol(ctx.emitter, "x10", ERROR_SYMBOL, 0);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov r10, {}", PREG_INTERNAL_ERROR)); // an invalid pattern is PHP's internal error
            abi::emit_store_reg_to_symbol(ctx.emitter, "r10", ERROR_SYMBOL, 0);
        }
    }
}

/// Finishes a boxed `string|null` replacement result against the recorded error state.
///
/// A match error releases the helper's partial result and returns `null`; a `null`
/// result without one is an invalid pattern, recorded as `PREG_INTERNAL_ERROR`.
pub(super) fn finish_replacement_result(ctx: &mut FunctionContext<'_>) {
    let failed = ctx.next_label("preg_replace_match_error");
    let done = ctx.next_label("preg_replace_error_done");
    let keep = ctx.next_label("preg_replace_keep_result");
    let result = abi::int_result_reg(ctx.emitter);
    emit_branch_if_error(ctx, &failed);
    // -- a null result without a match error is an invalid pattern --
    abi::emit_push_reg(ctx.emitter, result);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp x0, #{}", NULL_TAG));         // did the helper return PHP null?
            ctx.emitter.instruction(&format!("b.ne {keep}"));                   // a string result needs no error record
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp rax, {}", NULL_TAG));         // did the helper return PHP null?
            ctx.emitter.instruction(&format!("jne {keep}"));                    // a string result needs no error record
        }
    }
    emit_record_internal_error(ctx);
    ctx.emitter.label(&keep);
    abi::emit_pop_reg(ctx.emitter, result);
    abi::emit_jump(ctx.emitter, &done);
    // -- a match error replaces the partial result with null --
    ctx.emitter.label(&failed);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov x0, #{}", NULL_TAG));         // runtime tag 8 = null
            ctx.emitter.instruction("mov x1, #0");                              // null has no low payload word
            ctx.emitter.instruction("mov x2, #0");                              // null has no high payload word
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov rax, {}", NULL_TAG));         // runtime tag 8 = null
            ctx.emitter.instruction("xor edi, edi");                            // null has no low payload word
            ctx.emitter.instruction("xor esi, esi");                            // null has no high payload word
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.label(&done);
}
