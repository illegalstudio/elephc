//! Purpose:
//! Lowers typed EIR runtime operations after target selection and value placement.
//! Owns concrete helper symbols and physical calling-convention materialization.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_runtime_call()` for typed `RuntimeCall` immediates.
//!
//! Key details:
//! - PHP builtin names never participate in dispatch.
//! - Every typed call validates its EIR signature before emitting a helper call.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Instruction, RuntimeCallTarget, UnaryStringRuntime};
use crate::types::PhpType;

use super::{expect_operand, store_if_result};

/// Lowers one typed runtime operation through its target-specific helper ABI.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeCallTarget,
) -> Result<()> {
    match target {
        RuntimeCallTarget::ArrayFetchForWrite => {
            super::lower_array_fetch_for_write_runtime_call(ctx, inst)
        }
        RuntimeCallTarget::UnaryString(runtime) => lower_unary_string(ctx, inst, runtime),
        RuntimeCallTarget::Function(target) => super::runtime_functions::lower(ctx, inst, target),
    }
}

/// Lowers a typed `Str -> Str` transform using the internal string result register pair.
fn lower_unary_string(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    runtime: UnaryStringRuntime,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "typed runtime {} expected 1 operand, got {}",
            runtime.as_eir(),
            inst.operands.len(),
        )));
    }
    let value = expect_operand(inst, 0)?;
    let actual = ctx.load_value_to_result(value)?.codegen_repr();
    if actual != PhpType::Str {
        return Err(CodegenIrError::invalid_module(format!(
            "typed runtime {} expected Str, got {:?}",
            runtime.as_eir(),
            actual,
        )));
    }
    abi::emit_call_label(ctx.emitter, unary_string_symbol(runtime));
    store_if_result(ctx, inst)
}

/// Maps a backend-neutral unary string operation to its concrete runtime symbol.
fn unary_string_symbol(runtime: UnaryStringRuntime) -> &'static str {
    match runtime {
        UnaryStringRuntime::AddSlashes => "__rt_addslashes",
        UnaryStringRuntime::Base64Decode => "__rt_base64_decode",
        UnaryStringRuntime::Base64Encode => "__rt_base64_encode",
        UnaryStringRuntime::BinToHex => "__rt_bin2hex",
        UnaryStringRuntime::HexToBin => "__rt_hex2bin",
        UnaryStringRuntime::HtmlEntityDecode => "__rt_html_entity_decode",
        UnaryStringRuntime::NlToBr => "__rt_nl2br",
        UnaryStringRuntime::RawUrlDecode => "__rt_urldecode",
        UnaryStringRuntime::RawUrlEncode => "__rt_rawurlencode",
        UnaryStringRuntime::StripSlashes => "__rt_stripslashes",
        UnaryStringRuntime::StrReverse => "__rt_strrev",
        UnaryStringRuntime::StrToLower => "__rt_strtolower",
        UnaryStringRuntime::StrToUpper => "__rt_strtoupper",
        UnaryStringRuntime::UrlDecode => "__rt_urldecode",
        UnaryStringRuntime::UrlEncode => "__rt_urlencode",
    }
}
