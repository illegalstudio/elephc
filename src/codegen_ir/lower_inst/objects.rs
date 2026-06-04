//! Purpose:
//! Lowers the first object metadata opcodes for the Phase 04 EIR backend.
//! Supports simple object allocation and named `instanceof` checks.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Object payload layout must match the legacy backend and runtime helpers:
//!   heap kind word before payload, class id at payload offset 0.
//! - This slice intentionally rejects constructors and property initialization
//!   until method dispatch and property lowering are available.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_data, expect_operand, store_if_result};
use crate::codegen_ir::{CodegenIrError, Result};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Lowers fixed-class object allocation for classes with no property initialization.
pub(super) fn lower_object_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if !inst.operands.is_empty() {
        return Err(CodegenIrError::unsupported(
            "object construction with constructor arguments",
        ));
    }
    let class_name = class_name_immediate(ctx, inst)?;
    let class_info = ctx
        .module
        .class_infos
        .get(class_name)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", class_name)))?;
    if !class_info.properties.is_empty()
        || class_info.allow_dynamic_properties
        || class_info.defaults.iter().any(Option::is_some)
        || !class_info.vtable_methods.is_empty()
        || !class_info.static_vtable_methods.is_empty()
    {
        return Err(CodegenIrError::unsupported(format!(
            "object allocation requiring property or method metadata for {}",
            class_name
        )));
    }
    emit_empty_object_allocation(ctx, class_info.class_id)?;
    store_if_result(ctx, inst)
}

/// Lowers named `instanceof` using runtime class/interface metadata.
pub(super) fn lower_instanceof(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let value_ty = ctx.value_php_type(value)?;
    if !matches!(value_ty, PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_)) {
        emit_false(ctx);
        return store_if_result(ctx, inst);
    }
    let class_name = class_name_immediate(ctx, inst)?;
    let Some((target_id, target_kind)) = classify_named_target(ctx, class_name) else {
        emit_false(ctx);
        return store_if_result(ctx, inst);
    };
    reject_method_metadata_target(ctx, class_name)?;
    match value_ty {
        PhpType::Object(_) => {
            ctx.load_value_to_reg(value, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
            emit_match_call(ctx, target_id, target_kind, "__rt_exception_matches");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_reg(value, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
            emit_match_call(ctx, target_id, target_kind, "__rt_mixed_instanceof");
        }
        _ => emit_false(ctx),
    }
    store_if_result(ctx, inst)
}

/// Emits allocation and class-id stamping for an object with an empty payload body.
fn emit_empty_object_allocation(ctx: &mut FunctionContext<'_>, class_id: u64) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #8");                              // request an object payload containing only the class id
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #4");                              // heap kind 4 marks object instances for ownership helpers
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the heap header before the object payload
            ctx.emitter.instruction(&format!("mov x10, #{}", class_id));        // materialize the compile-time class id
            ctx.emitter.instruction("str x10, [x0]");                           // store the class id at object payload offset zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, 8");                              // request an object payload containing only the class id
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 4)); // materialize the x86_64 object heap kind word
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the heap header before the object payload
            ctx.emitter.instruction(&format!("mov r10, {}", class_id));         // materialize the compile-time class id
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store the class id at object payload offset zero
        }
    }
    Ok(())
}

/// Emits the metadata matcher call with object-or-mixed input already in argument 0.
fn emit_match_call(
    ctx: &mut FunctionContext<'_>,
    target_id: u64,
    target_kind: i64,
    helper: &str,
) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        target_id as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        target_kind,
    );
    abi::emit_call_label(ctx.emitter, helper);
}

/// Classifies a named target as a class `(kind 0)` or interface `(kind 1)`.
fn classify_named_target(
    ctx: &FunctionContext<'_>,
    class_name: &str,
) -> Option<(u64, i64)> {
    let normalized = class_name.trim_start_matches('\\');
    if let Some(class_info) = ctx.module.class_infos.get(normalized) {
        return Some((class_info.class_id, 0));
    }
    ctx.module
        .interface_infos
        .get(normalized)
        .map(|interface_info| (interface_info.interface_id, 1))
}

/// Rejects targets whose runtime metadata would reference method symbols not emitted yet.
fn reject_method_metadata_target(ctx: &FunctionContext<'_>, class_name: &str) -> Result<()> {
    let normalized = class_name.trim_start_matches('\\');
    if let Some(class_info) = ctx.module.class_infos.get(normalized) {
        if !class_info.vtable_methods.is_empty() || !class_info.static_vtable_methods.is_empty() {
            return Err(CodegenIrError::unsupported(format!(
                "instanceof target with method metadata {}",
                normalized
            )));
        }
    }
    if let Some(interface_info) = ctx.module.interface_infos.get(normalized) {
        if !interface_info.method_order.is_empty() {
            return Err(CodegenIrError::unsupported(format!(
                "instanceof interface target with method metadata {}",
                normalized
            )));
        }
    }
    Ok(())
}

/// Emits a boolean false result for non-object values or unresolved targets.
fn emit_false(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
}

/// Resolves an instruction class-name immediate into the module data pool.
fn class_name_immediate<'a>(
    ctx: &'a FunctionContext<'_>,
    inst: &Instruction,
) -> Result<&'a str> {
    let data = expect_data(inst)?;
    ctx.module
        .data
        .class_names
        .get(data.as_raw() as usize)
        .map(String::as_str)
        .ok_or_else(|| CodegenIrError::missing_entry("class data", data.as_raw()))
}
