//! Purpose:
//! Lowers `__elephc_curl_option_kind($option)` — classify a `curl_setopt()` option number
//! against the bridge's frozen option table.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_13`.
//!
//! Key details:
//! - The only operand is a plain integer and there is no handle to unbox, so this is the
//!   simplest curl lowering there is: load the option into the first C argument register
//!   and call. Nothing can clobber anything, so no staging is needed.
//! - The bridge pointers ARE published, unlike `__elephc_curl_setopt_unsupported_warning`'s
//!   lowering: this one really does call into `elephc_curl`.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::store_if_result;
use super::shared::{curl_arg_reg, ensure_curl_arg_count};

/// Lowers `__elephc_curl_option_kind($option)` through the option-kind helper.
pub(crate) fn lower_curl_option_kind(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_option_kind", 1)?;
    let option = super::super::super::expect_operand(inst, 0)?;
    // Load through the integer result register and move, rather than straight into the
    // argument register: `load_value_to_reg` may itself use the result register as
    // scratch, and every other curl lowering reaches the argument registers the same way.
    let scratch = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(option, scratch)?;
    abi::emit_reg_move(ctx.emitter, curl_arg_reg(ctx, 0), scratch);
    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_option_kind");
    store_if_result(ctx, inst)
}
