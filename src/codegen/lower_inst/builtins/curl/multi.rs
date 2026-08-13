//! Purpose:
//! Lowers the internal builtins behind PHP's curl MULTI interface —
//! `__elephc_curl_multi_init/add/remove/exec/select/info_read/setopt/errno/strerror` —
//! plus `__elephc_curl_easy_id`, the handle-identity read the `CurlMultiHandle` object's
//! PHP-side map is keyed on.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_13`.
//!
//! Key details:
//! - EVERY OPERAND IS STAGED THROUGH THE STACK before the handle is unboxed, the same
//!   hazard (and the same fix) `easy_setopt.rs` documents at length: unboxing a Mixed
//!   handle calls `__rt_mixed_unbox`, which clobbers every caller-saved register,
//!   including any C argument register already loaded.
//! - `__elephc_curl_multi_add`/`_remove` TAKE TWO BOXED HANDLES, which is the one shape
//!   the easy lane never needed. The EASY handle is unboxed first and pushed; the MULTI
//!   handle is unboxed second, straight into the first C argument register; the easy id is
//!   then popped into the second. Unboxing in the other order would leave the first id in
//!   a register the second unbox destroys.
//! - `__elephc_curl_easy_id` CALLS NOTHING. The bridge id is already the payload of the
//!   handle's Mixed cell, so the whole lowering is the unbox itself: it lands in the
//!   integer result register, which is exactly where an `int`-returning builtin's answer
//!   belongs. It still carries the TypeError guard every other handle read has, because
//!   the operand comes from a `mixed` property that a caller could have overwritten.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::store_if_result;
use super::shared::{
    curl_arg_reg, ensure_curl_arg_count, load_handle_to_first_arg, load_handle_to_result,
};

/// Lowers `__elephc_curl_easy_id($handle)` — the handle's own bridge id, unboxed.
pub(crate) fn lower_curl_easy_id(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_easy_id", 1)?;
    load_handle_to_result(ctx, inst, 0, "curl_multi_add_handle")?;
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_curl_multi_init()` — allocate a multi handle and hand back the boxed
/// Mixed cell (resource kind 7) that owns it.
pub(crate) fn lower_curl_multi_init(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_multi_init", 0)?;
    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_multi_init");
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_curl_multi_add($multi, $easy)`.
pub(crate) fn lower_curl_multi_add(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_two_handles(
        ctx,
        inst,
        "__elephc_curl_multi_add",
        "curl_multi_add_handle",
        "__rt_curl_multi_add",
    )
}

/// Lowers `__elephc_curl_multi_remove($multi, $easy)`.
pub(crate) fn lower_curl_multi_remove(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_two_handles(
        ctx,
        inst,
        "__elephc_curl_multi_remove",
        "curl_multi_remove_handle",
        "__rt_curl_multi_remove",
    )
}

/// Lowers `__elephc_curl_multi_exec($multi)`.
pub(crate) fn lower_curl_multi_exec(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_multi_only(
        ctx,
        inst,
        "__elephc_curl_multi_exec",
        "curl_multi_exec",
        "__rt_curl_multi_exec",
    )
}

/// Lowers `__elephc_curl_multi_errno($multi)`.
pub(crate) fn lower_curl_multi_errno(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_multi_only(
        ctx,
        inst,
        "__elephc_curl_multi_errno",
        "curl_multi_errno",
        "__rt_curl_multi_errno",
    )
}

/// Lowers `__elephc_curl_multi_select($multi, $timeout_ms)`.
pub(crate) fn lower_curl_multi_select(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_multi_and_int(
        ctx,
        inst,
        "__elephc_curl_multi_select",
        "curl_multi_select",
        "__rt_curl_multi_select",
    )
}

/// Lowers `__elephc_curl_multi_info_read($multi, $field)`.
pub(crate) fn lower_curl_multi_info_read(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_multi_and_int(
        ctx,
        inst,
        "__elephc_curl_multi_info_read",
        "curl_multi_info_read",
        "__rt_curl_multi_info_read",
    )
}

/// Lowers `__elephc_curl_multi_setopt($multi, $option, $value)`.
pub(crate) fn lower_curl_multi_setopt(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_multi_setopt", 3)?;
    let option = super::super::super::expect_operand(inst, 1)?;
    let value = super::super::super::expect_operand(inst, 2)?;

    // Stage value then option, so the pops below restore them in argument order.
    let scratch = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(value, scratch)?;
    abi::emit_push_reg(ctx.emitter, scratch);
    ctx.load_value_to_reg(option, scratch)?;
    abi::emit_push_reg(ctx.emitter, scratch);

    load_handle_to_first_arg(ctx, inst, 0, "curl_multi_setopt")?;
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 1)); // C ABI opt = the CURLMOPT_* number
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 2)); // C ABI value = the integer value

    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_multi_setopt");
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_curl_multi_strerror($code)`. Handle-free, exactly like
/// `__elephc_curl_strerror`, but a DIFFERENT numbering space (`CURLMcode`).
pub(crate) fn lower_curl_multi_strerror(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_multi_strerror", 1)?;
    let code = super::super::super::expect_operand(inst, 0)?;
    let scratch = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(code, scratch)?;
    abi::emit_reg_move(ctx.emitter, curl_arg_reg(ctx, 0), scratch);
    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_multi_strerror");
    store_if_result(ctx, inst)
}

/// Lowers a `($multi)`-only multi builtin.
fn lower_multi_only(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    builtin_name: &str,
    php_name: &str,
    runtime_label: &str,
) -> Result<()> {
    ensure_curl_arg_count(inst, builtin_name, 1)?;
    load_handle_to_first_arg(ctx, inst, 0, php_name)?;
    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Lowers a `($multi, int)` multi builtin, staging the integer across the handle unbox.
fn lower_multi_and_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    builtin_name: &str,
    php_name: &str,
    runtime_label: &str,
) -> Result<()> {
    ensure_curl_arg_count(inst, builtin_name, 2)?;
    let number = super::super::super::expect_operand(inst, 1)?;

    let scratch = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(number, scratch)?;
    abi::emit_push_reg(ctx.emitter, scratch);

    load_handle_to_first_arg(ctx, inst, 0, php_name)?;
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 1)); // C ABI arg 1 = the integer operand

    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Lowers a `($multi, $easy)` multi builtin: TWO boxed handles, unboxed in the order this
/// module's header explains (easy first and staged, multi second and straight into the
/// first C argument register).
fn lower_two_handles(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    builtin_name: &str,
    php_name: &str,
    runtime_label: &str,
) -> Result<()> {
    ensure_curl_arg_count(inst, builtin_name, 2)?;

    load_handle_to_result(ctx, inst, 1, php_name)?;
    let scratch = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, scratch);

    load_handle_to_first_arg(ctx, inst, 0, php_name)?;
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 1)); // C ABI arg 1 = the easy handle id

    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}
