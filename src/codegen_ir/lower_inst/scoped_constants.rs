//! Purpose:
//! Lowers scoped constant reads that remain dynamic at EIR codegen time.
//! Currently covers enum case singleton loads for Phase 04 parity.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Enum cases are stored in global singleton slots initialized before main
//!   user code runs. The load result is an object pointer.

use crate::codegen::abi;
use crate::ir::Instruction;
use crate::names::enum_case_symbol;

use super::super::context::FunctionContext;
use super::{builtins, expect_data, store_if_result};
use crate::codegen_ir::{CodegenIrError, Result};

/// Lowers a scoped enum-case read into the current object pointer result register.
pub(super) fn lower_scoped_constant_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let (enum_name, case_name) = scoped_constant_label(ctx, inst)?;
    let class_name = enum_name.to_string();
    let constant_name = case_name.to_string();
    if let Some(enum_info) = ctx.module.enum_infos.get(class_name.as_str()) {
        if enum_info
            .cases
            .iter()
            .any(|case| case.name == constant_name.as_str())
        {
            let symbol = enum_case_symbol(&class_name, &constant_name);
            abi::emit_load_symbol_to_reg(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &symbol,
                0,
            );
            return store_if_result(ctx, inst);
        }
    }
    if builtins::has_eval_context(ctx) {
        return builtins::lower_eval_class_constant_fetch(ctx, inst, &class_name, &constant_name);
    }
    Err(CodegenIrError::unsupported(format!(
        "scoped constant {}::{}",
        class_name, constant_name
    )))
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
