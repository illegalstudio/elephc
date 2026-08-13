//! Purpose:
//! Lowers `__elephc_curl_easy_getinfo_long($handle, $info)` — read a `long`-typed
//! `CURLINFO_*` field from an easy handle's most recent transfer.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_13`.
//!
//! Key details:
//! - Marshalling follows `easy_setopt_long`'s exact shape with one fewer operand: `$info`
//!   is staged on the stack BEFORE the handle is unboxed, because unboxing calls
//!   `__rt_mixed_unbox`, which clobbers every caller-saved register including whatever
//!   register `$info` would otherwise still be sitting in.
//! - The runtime helper computes the THIRD C argument itself (the address of its own
//!   stack out-parameter for the fetched `long`), so this lowering only ever marshals two
//!   operands into two argument registers, unlike `easy_setopt_long`'s three.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::store_if_result;
use super::shared::{curl_arg_reg, ensure_curl_arg_count, load_handle_to_first_arg};

/// Lowers `__elephc_curl_easy_getinfo_long($handle, $info)` through the getinfo helper.
pub(crate) fn lower_curl_easy_getinfo_long(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_easy_getinfo_long", 2)?;
    let info = super::super::super::expect_operand(inst, 1)?;

    // Stage `info` across the handle unbox (which clobbers caller-saved registers via
    // `__rt_mixed_unbox`), mirroring `easy_setopt_long`'s identical hazard.
    let scratch = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(info, scratch)?;
    abi::emit_push_reg(ctx.emitter, scratch);

    load_handle_to_first_arg(ctx, inst, 0, "curl_getinfo")?;
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 1)); // C ABI info = the CURLINFO_* option number

    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_easy_getinfo_long");
    store_if_result(ctx, inst)
}
