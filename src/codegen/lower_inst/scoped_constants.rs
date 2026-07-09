//! Purpose:
//! Lowers scoped constant reads that remain dynamic at EIR codegen time.
//! Currently covers enum case singleton loads for Phase 04 parity.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Enum cases are stored in global singleton slots initialized before main
//!   user code runs. The load result is an object pointer.

use crate::codegen::abi;
use crate::ir::Instruction;
use crate::names::enum_case_symbol;

use super::super::context::FunctionContext;
use super::{expect_data, store_if_result};
use crate::codegen::{CodegenIrError, Result};

/// Lowers a scoped enum-case read into the current object pointer result register.
pub(super) fn lower_scoped_constant_get(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let (enum_name, case_name) = scoped_constant_label(ctx, inst)?;
    let enum_info = ctx
        .module
        .enum_infos
        .get(enum_name)
        .ok_or_else(|| CodegenIrError::unsupported(format!("scoped constant {}::{}", enum_name, case_name)))?;
    if !enum_info.cases.iter().any(|case| case.name == case_name) {
        return Err(CodegenIrError::unsupported(format!(
            "scoped enum constant {}::{}",
            enum_name,
            case_name
        )));
    }
    let symbol = enum_case_symbol(enum_name, case_name);
    abi::emit_load_symbol_to_reg(ctx.emitter, abi::int_result_reg(ctx.emitter), &symbol, 0);
    store_if_result(ctx, inst)
}

/// Resolves the string immediate `Enum::Case` attached to a scoped constant read.
fn scoped_constant_label<'a>(
    ctx: &'a FunctionContext<'_>,
    inst: &Instruction,
) -> Result<(&'a str, &'a str)> {
    let data = expect_data(inst)?;
    let label = ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?;
    let (enum_name, case_name) = label.rsplit_once("::").ok_or_else(|| {
        CodegenIrError::invalid_module(format!("invalid scoped constant label '{}'", label))
    })?;
    Ok((enum_name.trim_start_matches('\\'), case_name))
}
