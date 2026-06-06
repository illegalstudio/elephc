//! Purpose:
//! Lowers enum-specific static helper methods for the EIR backend.
//! Handles runtime arrays of enum singleton objects without relying on legacy
//! AST method emitters.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_static_method_call()`.
//!
//! Key details:
//! - Enum cases are pre-initialized global singleton object slots.
//! - `Enum::cases()` returns a new indexed array that owns retained singleton references.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::Instruction;
use crate::names::{enum_case_symbol, php_symbol_key};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::store_if_result;
use crate::codegen_ir::{CodegenIrError, Result};

/// Attempts to lower a static method call when the receiver is an enum.
pub(super) fn try_lower_enum_static_method(
    ctx: &mut FunctionContext<'_>,
    enum_name: &str,
    method_name: &str,
    inst: &Instruction,
) -> Result<Option<()>> {
    let method_key = php_symbol_key(method_name);
    if !ctx.module.enum_infos.contains_key(enum_name) {
        return Ok(None);
    }
    match method_key.as_str() {
        "cases" => {
            lower_enum_cases(ctx, enum_name, inst)?;
            Ok(Some(()))
        }
        _ => Ok(None),
    }
}

/// Lowers `EnumName::cases()` into a fresh indexed array of retained singleton objects.
fn lower_enum_cases(
    ctx: &mut FunctionContext<'_>,
    enum_name: &str,
    inst: &Instruction,
) -> Result<()> {
    if !inst.operands.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "{}::cases with EIR arguments",
            enum_name
        )));
    }
    let case_names = ctx
        .module
        .enum_infos
        .get(enum_name)
        .ok_or_else(|| CodegenIrError::unsupported(format!("enum cases for {}", enum_name)))?
        .cases
        .iter()
        .map(|case| case.name.clone())
        .collect::<Vec<_>>();
    emit_enum_cases_array(ctx, enum_name, &case_names)?;
    store_if_result(ctx, inst)
}

/// Emits allocation and element stores for an enum cases result array.
fn emit_enum_cases_array(
    ctx: &mut FunctionContext<'_>,
    enum_name: &str,
    case_names: &[String],
) -> Result<()> {
    let capacity = case_names.len().max(4);
    let result_reg = abi::int_result_reg(ctx.emitter);
    let array_ptr_reg = abi::symbol_scratch_reg(ctx.emitter);
    let len_reg = abi::temp_int_reg(ctx.emitter.target);
    emit_array_new_call(ctx, capacity);
    abi::emit_push_reg(ctx.emitter, result_reg);
    let elem_ty = PhpType::Object(enum_name.to_string());
    for (index, case_name) in case_names.iter().enumerate() {
        emit_enum_case_store(ctx, enum_name, case_name, index, &elem_ty, array_ptr_reg, len_reg);
    }
    abi::emit_pop_reg(ctx.emitter, result_reg);
    Ok(())
}

/// Emits the target-specific `__rt_array_new` call for an enum cases array.
fn emit_array_new_call(ctx: &mut FunctionContext<'_>, capacity: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 8);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 8);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
}

/// Stores one retained enum case singleton into the in-progress cases array.
fn emit_enum_case_store(
    ctx: &mut FunctionContext<'_>,
    enum_name: &str,
    case_name: &str,
    index: usize,
    elem_ty: &PhpType,
    array_ptr_reg: &str,
    len_reg: &str,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let case_label = enum_case_symbol(enum_name, case_name);
    abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, &case_label, 0);
    abi::emit_incref_if_refcounted(ctx.emitter, elem_ty);
    abi::emit_load_temporary_stack_slot(ctx.emitter, array_ptr_reg, 0);
    if index == 0 {
        crate::codegen::emit_array_value_type_stamp(ctx.emitter, array_ptr_reg, elem_ty);
    }
    abi::emit_store_to_address(ctx.emitter, result_reg, array_ptr_reg, 24 + index * 8);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, (index + 1) as i64);
    abi::emit_store_to_address(ctx.emitter, len_reg, array_ptr_reg, 0);
}
