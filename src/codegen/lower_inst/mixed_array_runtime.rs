//! Purpose:
//! Lowers boxed Mixed array reads, writes, and fetch-for-write operations.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Lowers binary runtime fallbacks that Phase 04 can identify by operand type.
pub(super) fn lower_binary_runtime_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let receiver = expect_operand(inst, 0)?;
    let receiver_ty = ctx.value_php_type(receiver)?.codegen_repr();
    let result_ty = inst.result_php_type.codegen_repr();
    match (receiver_ty, &result_ty) {
        (PhpType::Mixed | PhpType::Union(_), PhpType::Void) => {
            lower_mixed_cell_runtime_assign(ctx, inst)
        }
        (PhpType::Mixed | PhpType::Union(_), _) => {
            lower_mixed_array_runtime_get(ctx, inst, false)
        }
        (PhpType::AssocArray { .. }, PhpType::Void) => hashes::lower_hash_append(ctx, inst),
        (other, _) => Err(CodegenIrError::unsupported(format!(
            "runtime_call with receiver PHP type {:?} returning PHP type {:?}",
            other, inst.result_php_type
        ))),
    }
}

/// Lowers `$mixed[$key]` through the shared boxed Mixed array/hash/stdClass reader.
pub(super) fn lower_mixed_array_runtime_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    for_write: bool,
) -> Result<()> {
    let receiver = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    let warn_on_missing = expect_operand(inst, 2)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            hashes::materialize_hash_key_aarch64(ctx, key)?;
            ctx.load_value_to_reg(warn_on_missing, "x3")?;
            ctx.load_value_to_reg(receiver, "x0")?;
        }
        Arch::X86_64 => {
            hashes::materialize_hash_key_x86_64(ctx, key)?;
            ctx.load_value_to_reg(warn_on_missing, "rcx")?;
            ctx.load_value_to_reg(receiver, "rdi")?;
        }
    }
    abi::emit_call_label(
        ctx.emitter,
        if for_write {
            "__rt_mixed_array_get_for_write"
        } else {
            "__rt_mixed_array_get"
        },
    );
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)
}

/// Lowers typed fetch-for-write parent reads of nested array writes (issue #555).
///
/// Two receiver shapes share the `ArrayFetchForWrite` runtime target:
/// - boxed `Mixed` receiver → `__rt_mixed_array_get_for_write(cell, key)`
///   autovivifies missing/null elements inside the receiver cell and returns
///   an owned boxed cell (the STORED one whenever storage is boxed);
/// - concrete `Array`/`AssocArray` receiver → `__rt_array_ensure_elem_for_write
///   (container, tag, key)` autovivifies the element and returns the possibly
///   promoted/reallocated container pointer for the local storeback.
pub(super) fn lower_array_fetch_for_write_runtime_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let receiver = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    let receiver_ty = ctx.value_php_type(receiver)?.codegen_repr();
    match receiver_ty {
        PhpType::Mixed | PhpType::Union(_) => {
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    hashes::materialize_hash_key_aarch64(ctx, key)?;
                    ctx.load_value_to_reg(receiver, "x0")?;
                }
                Arch::X86_64 => {
                    hashes::materialize_hash_key_x86_64(ctx, key)?;
                    ctx.load_value_to_reg(receiver, "rdi")?;
                }
            }
            abi::emit_call_label(ctx.emitter, "__rt_mixed_array_get_for_write");
            cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
            store_if_result(ctx, inst)
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            let tag: i64 = if matches!(receiver_ty, PhpType::Array(_)) {
                4
            } else {
                5
            };
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    hashes::materialize_hash_key_aarch64(ctx, key)?;
                    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
                    ctx.load_value_to_reg(receiver, "x0")?;
                    abi::emit_load_int_immediate(ctx.emitter, "x1", tag);
                    abi::emit_pop_reg_pair(ctx.emitter, "x2", "x3");
                }
                Arch::X86_64 => {
                    hashes::materialize_hash_key_x86_64(ctx, key)?;
                    abi::emit_push_reg_pair(ctx.emitter, "rsi", "rdx");
                    ctx.load_value_to_reg(receiver, "rdi")?;
                    abi::emit_load_int_immediate(ctx.emitter, "rsi", tag);
                    abi::emit_pop_reg_pair(ctx.emitter, "rdx", "rcx");
                }
            }
            abi::emit_call_label(ctx.emitter, "__rt_array_ensure_elem_for_write");
            store_if_result(ctx, inst)
        }
        PhpType::Object(_) => {
            objects::lower_dynamic_property_fetch_for_write(ctx, inst, receiver, key)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "fetch-for-write runtime_call with receiver PHP type {:?}",
            other
        ))),
    }
}

/// Lowers `$mixed[$key] = $value` through the shared boxed Mixed array/hash/stdClass writer.
pub(super) fn lower_mixed_array_runtime_set(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let receiver = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    let value = expect_operand(inst, 2)?;
    match ctx.value_php_type(receiver)?.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {}
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "runtime_call array set with receiver PHP type {:?}",
                other
            )))
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_mixed_array_runtime_set_aarch64(ctx, receiver, key, value)?,
        Arch::X86_64 => lower_mixed_array_runtime_set_x86_64(ctx, receiver, key, value)?,
    }
    Ok(())
}

/// Materializes AArch64 operands for the boxed Mixed array/hash writer.
pub(super) fn lower_mixed_array_runtime_set_aarch64(
    ctx: &mut FunctionContext<'_>,
    receiver: ValueId,
    key: ValueId,
    value: ValueId,
) -> Result<()> {
    let value_ty = ctx.load_value_to_result(value)?.codegen_repr();
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty);
    }
    abi::emit_push_reg(ctx.emitter, "x0");
    hashes::materialize_hash_key_aarch64(ctx, key)?;
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
    ctx.load_value_to_reg(receiver, "x0")?;
    abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
    abi::emit_pop_reg(ctx.emitter, "x3");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_array_set");
    Ok(())
}

/// Materializes x86_64 operands for the boxed Mixed array/hash writer.
pub(super) fn lower_mixed_array_runtime_set_x86_64(
    ctx: &mut FunctionContext<'_>,
    receiver: ValueId,
    key: ValueId,
    value: ValueId,
) -> Result<()> {
    let value_ty = ctx.load_value_to_result(value)?.codegen_repr();
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty);
    }
    abi::emit_push_reg(ctx.emitter, "rax");
    hashes::materialize_hash_key_x86_64(ctx, key)?;
    abi::emit_push_reg_pair(ctx.emitter, "rsi", "rdx");
    ctx.load_value_to_reg(receiver, "rdi")?;
    abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
    abi::emit_pop_reg(ctx.emitter, "rcx");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_array_set");
    Ok(())
}
