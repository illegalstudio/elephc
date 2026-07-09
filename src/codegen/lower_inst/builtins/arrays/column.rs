//! Purpose:
//! Lowers PHP `array_column()` builtin calls for the EIR backend.
//! Materializes an indexed array of associative rows plus a string column key
//! into the existing target-aware runtime extraction helpers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays::lower_array_column()`.
//!
//! Key details:
//! - Result array metadata is normalized after runtime extraction so empty
//!   results still carry the correct indexed-array value type.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::context::FunctionContext;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::{expect_operand, store_if_result};

/// Lowers `array_column()` by dispatching to the helper matching row value ownership.
pub(super) fn lower_array_column(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_column", 2)?;
    let array = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    let value_ty = array_column_source_value_type(ctx.value_php_type(array)?)?;
    require_array_column_key_type(ctx.value_php_type(key)?)?;
    let result_elem_ty = array_column_result_element_type(inst, &value_ty)?;
    lower_array_column_call(ctx, array, key, &value_ty)?;
    super::normalize_indexed_array_result(ctx, "array_column", &value_ty, &result_elem_ty)?;
    super::box_array_result_for_mixed_builtin(ctx, inst, &result_elem_ty);
    store_if_result(ctx, inst)
}

/// Returns the row value type extracted from an indexed array of associative rows.
fn array_column_source_value_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(inner) => match inner.codegen_repr() {
            PhpType::AssocArray { value, .. } => Ok(value.codegen_repr()),
            other => Err(CodegenIrError::unsupported(format!(
                "array_column row PHP type {:?}",
                other
            ))),
        },
        other => Err(CodegenIrError::unsupported(format!(
            "array_column for PHP type {:?}",
            other
        ))),
    }
}

/// Verifies that the column key can be passed through the runtime string-key ABI.
fn require_array_column_key_type(ty: PhpType) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Str => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_column key PHP type {:?}",
            other
        ))),
    }
}

/// Returns the element type required by the lowered EIR result slot.
fn array_column_result_element_type(inst: &Instruction, value_ty: &PhpType) -> Result<PhpType> {
    match inst.result_php_type.codegen_repr() {
        PhpType::Array(elem) => {
            let result_elem_ty = elem.codegen_repr();
            if &result_elem_ty == value_ty || result_elem_ty == PhpType::Mixed {
                Ok(result_elem_ty)
            } else {
                Err(CodegenIrError::unsupported(format!(
                    "array_column result element PHP type {:?} for source value PHP type {:?}",
                    result_elem_ty,
                    value_ty
                )))
            }
        }
        PhpType::Mixed | PhpType::Union(_) => Ok(value_ty.clone()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_column result PHP type {:?}",
            other
        ))),
    }
}

/// Materializes `array_column()` arguments and calls the selected runtime helper.
fn lower_array_column_call(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    key: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_string_value_to_regs(key, "x1", "x2")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_string_value_to_regs(key, "rsi", "rdx")?;
        }
    }
    abi::emit_call_label(ctx.emitter, array_column_runtime_helper(value_ty));
    Ok(())
}

/// Returns the runtime helper that matches the extracted row value representation.
fn array_column_runtime_helper(value_ty: &PhpType) -> &'static str {
    if value_ty == &PhpType::Str {
        "__rt_array_column_str"
    } else if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        "__rt_array_column_mixed"
    } else if value_ty.is_refcounted() {
        "__rt_array_column_ref"
    } else {
        "__rt_array_column"
    }
}
