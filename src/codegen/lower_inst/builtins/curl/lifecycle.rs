//! Purpose:
//! Lowers the four Wave D easy-handle lifecycle builtins —
//! `__elephc_curl_easy_reset($handle)`, `__elephc_curl_easy_upkeep($handle)`,
//! `__elephc_curl_easy_copy($handle)` and `__elephc_curl_easy_pause($handle, $flags)` —
//! plus the handle-free `__elephc_curl_strerror($code)`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_13`.
//!
//! Key details:
//! - THE FIRST THREE ARE THE SIMPLEST SHAPE THERE IS: one boxed handle operand, unboxed
//!   into the first C argument register, then a call. Nothing can clobber anything, so
//!   there is no staging.
//! - `__elephc_curl_easy_pause` needs its `$flags` staged across the handle unbox, which
//!   calls `__rt_mixed_unbox` and clobbers every caller-saved register — the identical
//!   hazard, and the identical fix, `easy_getinfo.rs` documents.
//! - `__elephc_curl_strerror` takes NO handle, so it is the one lowering here that never
//!   unboxes anything.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::store_if_result;
use super::shared::{curl_arg_reg, ensure_curl_arg_count, load_handle_to_first_arg};

/// Lowers `__elephc_curl_easy_reset($handle)`.
pub(crate) fn lower_curl_easy_reset(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_handle_only(
        ctx,
        inst,
        "__elephc_curl_easy_reset",
        "curl_reset",
        "__rt_curl_easy_reset",
    )
}

/// Lowers `__elephc_curl_easy_upkeep($handle)`.
pub(crate) fn lower_curl_easy_upkeep(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_handle_only(
        ctx,
        inst,
        "__elephc_curl_easy_upkeep",
        "curl_upkeep",
        "__rt_curl_easy_upkeep",
    )
}

/// Lowers `__elephc_curl_easy_copy($handle)`.
pub(crate) fn lower_curl_easy_copy(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_handle_only(
        ctx,
        inst,
        "__elephc_curl_easy_copy",
        "curl_copy_handle",
        "__rt_curl_easy_copy",
    )
}

/// Lowers `__elephc_curl_easy_pause($handle, $flags)`.
pub(crate) fn lower_curl_easy_pause(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_easy_pause", 2)?;
    let flags = super::super::super::expect_operand(inst, 1)?;

    // Stage `flags` across the handle unbox, which clobbers caller-saved registers.
    let scratch = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(flags, scratch)?;
    abi::emit_push_reg(ctx.emitter, scratch);

    load_handle_to_first_arg(ctx, inst, 0, "curl_pause")?;
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 1)); // C ABI bitmask = the CURLPAUSE_* flags

    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_easy_pause");
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_curl_strerror($code)`.
pub(crate) fn lower_curl_strerror(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_strerror", 1)?;
    let code = super::super::super::expect_operand(inst, 0)?;
    let scratch = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(code, scratch)?;
    abi::emit_reg_move(ctx.emitter, curl_arg_reg(ctx, 0), scratch);
    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_strerror");
    store_if_result(ctx, inst)
}

/// Lowers any `($handle)`-only curl builtin: unbox the handle into the first C argument
/// register, publish the bridge pointers, call.
///
/// `pub(super)` rather than private: `mime.rs`'s four handle-only mime builder lowerings
/// (`__elephc_curl_mime_new`/`_add_part`/`_post`/`_abort`) reuse this directly rather than
/// duplicating it a second time.
pub(super) fn lower_handle_only(
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
