//! Purpose:
//! Lowers `__elephc_curl_setopt_unsupported_warning($option)` — raise PHP's warning for a
//! `CURLOPT_*` option this build cannot apply safely.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_13`.
//!
//! Key details:
//! - The only operand is a plain integer, loaded into the integer result register the
//!   runtime helper reads it from. No bridge pointers are published: this helper reaches
//!   the diagnostic channel only, never libcurl.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::store_if_result;
use super::shared::ensure_curl_arg_count;

/// Lowers `__elephc_curl_setopt_unsupported_warning($option)` through the warning helper.
pub(crate) fn lower_curl_setopt_unsupported_warning(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_warning(
        ctx,
        inst,
        "__elephc_curl_setopt_unsupported_warning",
        "__rt_curl_warn_unsupported_option",
    )
}

/// Lowers `__elephc_curl_multi_setopt_unsupported_warning($option)`. Identical shape; only
/// the helper (and therefore the function name in the message) differs.
pub(crate) fn lower_curl_multi_setopt_unsupported_warning(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_warning(
        ctx,
        inst,
        "__elephc_curl_multi_setopt_unsupported_warning",
        "__rt_curl_multi_warn_unsupported_option",
    )
}

/// Loads the option number into the integer result register and calls the warning helper.
fn lower_warning(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    builtin_name: &str,
    runtime_label: &str,
) -> Result<()> {
    ensure_curl_arg_count(inst, builtin_name, 1)?;
    let option = super::super::super::expect_operand(inst, 0)?;
    ctx.load_value_to_reg(option, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}
