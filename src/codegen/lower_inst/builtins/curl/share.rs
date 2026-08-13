//! Purpose:
//! Lowers the internal builtins behind PHP's curl SHARE interface —
//! `__elephc_curl_share_init`/`_setopt`/`_errno`/`_strerror`/`_init_persistent` plus
//! `__elephc_curl_easy_set_share` (the `CURLOPT_SHARE` attach point on the EASY lane).
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_13`.
//!
//! Key details:
//! - Every shape here is one `multi.rs`/`easy_setopt.rs`/`str_op.rs` already established:
//!   a no-operand handle producer (`lower_curl_share_init`, exactly `lower_curl_multi_init`
//!   parameterized on a different runtime label), a `(handle, int, int)` setter staged
//!   through the stack (`lower_curl_share_setopt`, exactly `lower_curl_multi_setopt`'s
//!   shape), a `(handle)`-only forwarder (`lower_curl_share_errno`), a handle-free
//!   `(code)` message lookup (`lower_curl_share_strerror`, exactly
//!   `lower_curl_multi_strerror`'s shape), a `(handle, handle)` two-handle attach
//!   (`lower_curl_easy_set_share`, exactly `multi.rs`'s `lower_two_handles` shape), and a
//!   `(string)`-only handle producer (`lower_curl_share_init_persistent`, the same
//!   ptr/len staging `easy_setopt.rs`/`str_op.rs` use for their string operands, feeding a
//!   handle-producing runtime helper that otherwise needs no operand of its own). No new
//!   asm shape is introduced by this file.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::store_if_result;
use super::super::strings::load_string_arg_to_regs;
use super::shared::{
    curl_arg_reg, ensure_curl_arg_count, load_handle_to_first_arg, load_handle_to_result,
};

/// Lowers `__elephc_curl_share_init()` — allocate a share handle and hand back the boxed
/// Mixed cell (resource kind 8) that owns it.
pub(crate) fn lower_curl_share_init(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_share_init", 0)?;
    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_share_init");
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_curl_share_setopt($share, $option, $value)`. Staging order mirrors
/// `multi.rs`'s `lower_curl_multi_setopt` exactly: value then option pushed, so the pops
/// after the handle unbox restore them in argument order.
pub(crate) fn lower_curl_share_setopt(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_share_setopt", 3)?;
    let option = super::super::super::expect_operand(inst, 1)?;
    let value = super::super::super::expect_operand(inst, 2)?;

    let scratch = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(value, scratch)?;
    abi::emit_push_reg(ctx.emitter, scratch);
    ctx.load_value_to_reg(option, scratch)?;
    abi::emit_push_reg(ctx.emitter, scratch);

    load_handle_to_first_arg(ctx, inst, 0, "curl_share_setopt")?;
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 1)); // C ABI opt = the CURLSHOPT_* number
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 2)); // C ABI value = the integer value

    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_share_setopt");
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_curl_share_errno($share)`.
pub(crate) fn lower_curl_share_errno(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_share_errno", 1)?;
    load_handle_to_first_arg(ctx, inst, 0, "curl_share_errno")?;
    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_share_errno");
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_curl_share_strerror($code)`. Handle-free, exactly like
/// `__elephc_curl_multi_strerror`, but a DIFFERENT numbering space (`CURLSHcode`).
pub(crate) fn lower_curl_share_strerror(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_share_strerror", 1)?;
    let code = super::super::super::expect_operand(inst, 0)?;
    let scratch = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(code, scratch)?;
    abi::emit_reg_move(ctx.emitter, curl_arg_reg(ctx, 0), scratch);
    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_share_strerror");
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_curl_easy_set_share($handle, $share)` — TWO boxed handles, unboxed in
/// the order `multi.rs`'s `lower_two_handles` documents: the SHARE handle first and
/// staged, the EASY handle second and straight into the first C argument register (the
/// bridge's `elephc_curl_easy_set_share(easy_id, share_id)` takes them in that order, so
/// the staged value pops into the SECOND C argument).
pub(crate) fn lower_curl_easy_set_share(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_easy_set_share", 2)?;

    load_handle_to_result(ctx, inst, 1, "curl_setopt")?;
    let scratch = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, scratch);

    load_handle_to_first_arg(ctx, inst, 0, "curl_setopt")?;
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 1)); // C ABI arg 1 = the share handle id

    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_easy_set_share");
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_curl_share_init_persistent($lock_data_csv)` — a single STRING operand
/// (no handle), materialized straight into the two C argument registers exactly the way
/// `easy_setopt.rs`/`str_op.rs` stage a string operand (`int_result_reg`/
/// `secondary_scratch_reg` first, since `load_string_arg_to_regs` may itself call a
/// runtime helper), then moved into the target's actual argument registers. No stack
/// staging is needed: with no handle to unbox and no second operand, there is no
/// interleaved call that could clobber the freshly loaded pointer/length.
pub(crate) fn lower_curl_share_init_persistent(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_share_init_persistent", 1)?;
    let ptr_scratch = abi::int_result_reg(ctx.emitter);
    let len_scratch = abi::secondary_scratch_reg(ctx.emitter);
    load_string_arg_to_regs(
        ctx,
        inst,
        0,
        "__elephc_curl_share_init_persistent",
        ptr_scratch,
        len_scratch,
    )?;
    abi::emit_reg_move(ctx.emitter, curl_arg_reg(ctx, 0), ptr_scratch);
    abi::emit_reg_move(ctx.emitter, curl_arg_reg(ctx, 1), len_scratch);

    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_share_init_persistent");
    store_if_result(ctx, inst)
}
