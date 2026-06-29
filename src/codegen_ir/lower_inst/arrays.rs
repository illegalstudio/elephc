//! Purpose:
//! Lowers basic indexed-array allocation, length reads, and append operations
//! for the Phase 04 EIR backend.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Runtime append helpers may grow arrays and return a new heap pointer, so
//!   the backend writes that pointer back to the source SSA slot and local slot.

use crate::codegen::{
    abi, emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed,
    emit_release_pushed_refcounted_temp_after_array_push, runtime_value_tag,
};
use crate::codegen::platform::Arch;
use crate::codegen::sentinels::TAGGED_SCALAR_ARRAY_VALUE_TYPE;
use crate::ir::{Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};
use crate::codegen_ir::{CodegenIrError, Result};

/// Lowers indexed-array allocation through the shared runtime constructor.
pub(super) fn lower_array_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let capacity = expect_capacity(inst)?.max(4);
    let result_ty = inst.result_php_type.codegen_repr();
    let elem_ty = indexed_array_element_type(&result_ty, inst)?;
    let elem_size = array_element_size(&result_ty)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", elem_size);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", elem_size);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    let result_reg = abi::int_result_reg(ctx.emitter);
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        result_reg,
        &elem_ty,
    );
    if matches!(elem_ty, PhpType::TaggedScalar) {
        emit_tagged_scalar_array_value_type_stamp(ctx, result_reg);
    }
    store_if_result(ctx, inst)
}

/// Lowers an indexed-array length read by loading the first header word.
pub(super) fn lower_array_len(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    require_indexed_array(ctx.load_value_to_result(array)?, inst)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, result_reg, result_reg, 0);
    store_if_result(ctx, inst)
}

/// Lowers typed indexed-array widening to boxed Mixed slots.
pub(super) fn lower_array_to_mixed(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expects exactly one operand",
            inst.op.name()
        )));
    }
    let array = expect_operand(inst, 0)?;
    indexed_array_element_type(&ctx.value_php_type(array)?, inst)?;
    require_array_to_mixed_result(&inst.result_php_type.codegen_repr(), inst)?;
    emit_array_to_mixed_operands(ctx, array)?;
    abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
    store_if_result(ctx, inst)
}

/// Loads the array pointer and runtime element value tag for `__rt_array_to_mixed`.
fn emit_array_to_mixed_operands(ctx: &mut FunctionContext<'_>, array: ValueId) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.emitter.instruction("ldr x1, [x0, #-8]");                       // load the indexed-array packed header to recover the runtime slot tag
            ctx.emitter.instruction("lsr x1, x1, #8");                          // move the runtime value_type byte into the low bits
            ctx.emitter.instruction("and x1, x1, #0x7f");                       // isolate the source element value_type for Mixed boxing
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.emitter.instruction("mov rsi, QWORD PTR [rdi - 8]");            // load the indexed-array packed header to recover the runtime slot tag
            ctx.emitter.instruction("shr rsi, 8");                              // move the runtime value_type byte into the low bits
            ctx.emitter.instruction("and rsi, 0x7f");                           // isolate the source element value_type for Mixed boxing
        }
    }
    Ok(())
}

/// Lowers indexed-array promotion to associative hash storage.
pub(super) fn lower_array_to_hash(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expects exactly one operand",
            inst.op.name()
        )));
    }
    let array = expect_operand(inst, 0)?;
    require_indexed_array(ctx.value_php_type(array)?.codegen_repr(), inst)?;
    let result_value_ty = require_array_to_hash_result(&inst.result_php_type.codegen_repr(), inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let already_hash = ctx.next_label("array_to_hash_already_hash");
            let convert = ctx.next_label("array_to_hash_convert");
            let done = ctx.next_label("array_to_hash_done");
            ctx.load_value_to_reg(array, "x0")?;
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("cmp x0, #3");                              // check whether the source is already associative hash storage
            ctx.emitter.instruction(&format!("b.eq {}", already_hash));         // reuse already-promoted hashes without reinterpreting them as indexed arrays
            ctx.emitter.instruction("cmp x0, #2");                              // check whether the source is still indexed-array storage
            ctx.emitter.instruction(&format!("b.eq {}", convert));              // convert indexed arrays to hash storage
            ctx.emitter.label(&already_hash);
            abi::emit_pop_reg(ctx.emitter, "x0");
            if result_value_ty == PhpType::Mixed {
                abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            }
            ctx.emitter.instruction(&format!("b {}", done));                    // finish after reusing an existing hash payload
            ctx.emitter.label(&convert);
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_load_int_immediate(ctx.emitter, "x0", 16);
            abi::emit_load_int_immediate(ctx.emitter, "x1", runtime_value_tag(&PhpType::Mixed) as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.instruction("mov x1, x0");                              // pass the empty temporary hash as the right union operand
            abi::emit_pop_reg(ctx.emitter, "x0");
            // Keep both the source indexed array and the empty temporary hash on
            // the stack across the union so the conversion can release them after
            // the copy: array_hash_union borrows both operands and returns a fresh
            // result hash, so the source array (an owning temporary or a moved-out
            // local reference) and the temporary hash both leak unless freed here.
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_push_reg(ctx.emitter, "x1");
            abi::emit_call_label(ctx.emitter, "__rt_array_hash_union");
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // reload the empty temporary hash from the stack
            abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            ctx.emitter.instruction("ldr x0, [sp, #32]");                       // reload the temporary source indexed array from the stack
            abi::emit_call_label(ctx.emitter, "__rt_decref_array");
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_pop_reg(ctx.emitter, "x1");
            if result_value_ty == PhpType::Mixed {
                abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            }
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            let already_hash = ctx.next_label("array_to_hash_already_hash");
            let convert = ctx.next_label("array_to_hash_convert");
            let done = ctx.next_label("array_to_hash_done");
            ctx.load_value_to_reg(array, "rax")?;
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("cmp rax, 3");                              // check whether the source is already associative hash storage
            ctx.emitter.instruction(&format!("je {}", already_hash));           // reuse already-promoted hashes without reinterpreting them as indexed arrays
            ctx.emitter.instruction("cmp rax, 2");                              // check whether the source is still indexed-array storage
            ctx.emitter.instruction(&format!("je {}", convert));                // convert indexed arrays to hash storage
            ctx.emitter.label(&already_hash);
            abi::emit_pop_reg(ctx.emitter, "rax");
            if result_value_ty == PhpType::Mixed {
                ctx.emitter.instruction("mov rdi, rax");                        // pass the existing hash to the Mixed-entry conversion helper
                abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            }
            ctx.emitter.instruction(&format!("jmp {}", done));                  // finish after reusing an existing hash payload
            ctx.emitter.label(&convert);
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_push_reg(ctx.emitter, "rdi");
            abi::emit_load_int_immediate(ctx.emitter, "rdi", 16);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", runtime_value_tag(&PhpType::Mixed) as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.instruction("mov rsi, rax");                            // pass the empty temporary hash as the right union operand
            abi::emit_pop_reg(ctx.emitter, "rdi");
            // Keep both the source indexed array and the empty temporary hash on
            // the stack across the union so the conversion can release them after
            // the copy: array_hash_union borrows both operands and returns a fresh
            // result hash, so the source array (an owning temporary or a moved-out
            // local reference) and the temporary hash both leak unless freed here.
            abi::emit_push_reg(ctx.emitter, "rdi");
            abi::emit_push_reg(ctx.emitter, "rsi");
            abi::emit_call_label(ctx.emitter, "__rt_array_hash_union");
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // reload the empty temporary hash from the stack
            abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 32]");           // reload the temporary source indexed array from the stack
            abi::emit_call_label(ctx.emitter, "__rt_decref_array");
            abi::emit_pop_reg(ctx.emitter, "rax");
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_pop_reg(ctx.emitter, "rsi");
            if result_value_ty == PhpType::Mixed {
                ctx.emitter.instruction("mov rdi, rax");                        // pass the promoted hash to the Mixed-entry conversion helper
                abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            }
            ctx.emitter.label(&done);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers an indexed-array element read with PHP null-sentinel fallback on misses.
pub(super) fn lower_array_get(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let index = expect_operand(inst, 1)?;
    let elem_ty = indexed_array_element_type(&ctx.value_php_type(array)?, inst)?;
    require_array_get_result(&elem_ty, inst)?;
    let result_ty = inst.result_php_type.codegen_repr();
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_array_get_aarch64(ctx, inst, array, index, &elem_ty, &result_ty),
        Arch::X86_64 => lower_array_get_x86_64(ctx, inst, array, index, &elem_ty, &result_ty),
    }
}

/// Lowers an indexed-array element write through target-aware runtime helpers.
pub(super) fn lower_array_set(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let index = expect_operand(inst, 1)?;
    let value = expect_operand(inst, 2)?;
    let elem_ty = indexed_array_element_type(&ctx.value_php_type(array)?, inst)?;
    let raw_value_ty = ctx.value_php_type(value)?.codegen_repr();
    let value_ty = effective_array_set_value_type(&elem_ty, &raw_value_ty, inst)?;
    require_integer_like_index(ctx.value_php_type(index)?, inst)?;
    let source_local = source_load_local_slot(ctx, array)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_array_set_aarch64(ctx, array, index, value, &raw_value_ty, &value_ty)?,
        Arch::X86_64 => lower_array_set_x86_64(ctx, array, index, value, &raw_value_ty, &value_ty)?,
    }
    ctx.store_result_value(array)?;
    if let Some(slot) = source_local {
        ctx.store_value_to_local(slot, array)?;
    }
    ctx.writeback_global_array_source(array)?;
    Ok(())
}

/// Lowers a boxed-Mixed-key write into a statically `Array(Mixed)` indexed local.
///
/// The key tag is only known at runtime (PHP `foreach` keys are always `Mixed`
/// in EIR), so the write goes through `__rt_array_set_mixed_key`, which keeps
/// integer keys on indexed storage and promotes string keys to a hash. The value
/// is consumed as a boxed `Mixed` cell exactly like `__rt_array_set_mixed`; the
/// key is read (not consumed) by the helper.
pub(super) fn lower_array_set_mixed_key(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    let value = expect_operand(inst, 2)?;
    require_indexed_array(ctx.value_php_type(array)?.codegen_repr(), inst)?;
    let key_ty = ctx.value_php_type(key)?.codegen_repr();
    if !matches!(key_ty, PhpType::Mixed | PhpType::Union(_)) {
        return Err(CodegenIrError::unsupported(format!(
            "array_set_mixed_key key PHP type {:?}",
            key_ty
        )));
    }
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            lower_array_set_mixed_key_aarch64(ctx, array, key, value, &value_ty)?
        }
        Arch::X86_64 => lower_array_set_mixed_key_x86_64(ctx, array, key, value, &value_ty)?,
    }
    // The storeback to the destination local is driven by the EIR-level
    // `store_local` of this op's result value (emitted by `store_mutated_local`
    // in `ir_lower`), so here we only materialize the call result into its SSA
    // slot. Performing the storeback via `store_result_value`/`store_value_to_local`
    // instead would leave the result SSA value unmaterialized, and the later
    // EIR `store_local <result>` would read an uninitialized slot back into the
    // destination local (clobbering it with garbage on every write).
    store_if_result(ctx, inst)
}

/// Boxes or retains a value, then stores it into a `Mixed`-keyed indexed array on AArch64.
fn lower_array_set_mixed_key_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    key: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        ctx.load_value_to_result(value)?;
        abi::emit_incref_if_refcounted(ctx.emitter, value_ty);
    } else {
        box_value_for_mixed_container(ctx, value, value_ty)?;
    }
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.load_value_to_reg(array, "x0")?;
    ctx.load_value_to_reg(key, "x1")?;
    abi::emit_pop_reg(ctx.emitter, "x2");
    abi::emit_call_label(ctx.emitter, "__rt_array_set_mixed_key");
    Ok(())
}

/// Boxes or retains a value, then stores it into a `Mixed`-keyed indexed array on x86_64.
fn lower_array_set_mixed_key_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    key: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        ctx.load_value_to_result(value)?;
        abi::emit_incref_if_refcounted(ctx.emitter, value_ty);
    } else {
        box_value_for_mixed_container(ctx, value, value_ty)?;
    }
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.load_value_to_reg(array, "rdi")?;
    ctx.load_value_to_reg(key, "rsi")?;
    abi::emit_pop_reg(ctx.emitter, "rdx");
    abi::emit_call_label(ctx.emitter, "__rt_array_set_mixed_key");
    Ok(())
}

/// Lowers an indexed-array append through the runtime helper for the value type.
pub(super) fn lower_array_push(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    let array_ty = ctx.value_php_type(array)?;
    require_indexed_array(array_ty.clone(), inst)?;
    let elem_ty = indexed_array_element_type(&array_ty, inst)?;
    let source_local = source_load_local_slot(ctx, array)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_array_push_aarch64(ctx, array, value, &elem_ty)?,
        Arch::X86_64 => lower_array_push_x86_64(ctx, array, value, &elem_ty)?,
    }
    ctx.store_result_value(array)?;
    if let Some(slot) = source_local {
        ctx.store_value_to_local(slot, array)?;
    }
    ctx.writeback_global_array_source(array)?;
    Ok(())
}

/// Lowers appends through a boxed Mixed array cell.
pub(super) fn lower_mixed_array_append(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let receiver = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    match ctx.value_php_type(receiver)?.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {}
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "mixed_array_append receiver PHP type {:?}",
                other
            )))
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_mixed_array_append_aarch64(ctx, receiver, value),
        Arch::X86_64 => lower_mixed_array_append_x86_64(ctx, receiver, value),
    }
}

/// Lowers PHP indexed-array union through the shared runtime helper.
pub(super) fn lower_array_union(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let left = expect_operand(inst, 0)?;
    let right = expect_operand(inst, 1)?;
    require_indexed_array(ctx.value_php_type(left)?, inst)?;
    require_indexed_array(ctx.value_php_type(right)?, inst)?;
    require_indexed_array(inst.result_php_type.codegen_repr(), inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(left, "x0")?;
            ctx.load_value_to_reg(right, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(left, "rdi")?;
            ctx.load_value_to_reg(right, "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_union");
    store_if_result(ctx, inst)
}

/// Lowers indexed+associative array union through the shared hash runtime helper.
pub(super) fn lower_array_hash_union(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let left = expect_operand(inst, 0)?;
    let right = expect_operand(inst, 1)?;
    require_indexed_array(ctx.value_php_type(left)?, inst)?;
    require_assoc_union_hash_operand(ctx.value_php_type(right)?, inst)?;
    let result_value_ty = require_array_to_hash_result(&inst.result_php_type.codegen_repr(), inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(left, "x0")?;
            ctx.load_value_to_reg(right, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(left, "rdi")?;
            ctx.load_value_to_reg(right, "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_hash_union");
    convert_hash_union_result_to_mixed_if_needed(ctx, &result_value_ty);
    store_if_result(ctx, inst)
}

/// Lowers an indexed-array element read for AArch64 targets.
fn lower_array_get_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    index: ValueId,
    elem_ty: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    let len_reg = abi::secondary_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(index, result_reg)?;
    ctx.load_value_to_reg(array, array_reg)?;
    let null_label = ctx.next_label("array_get_null");
    let done_label = ctx.next_label("array_get_done");

    ctx.emitter.instruction(&format!("cmp {}, #0", result_reg));                // check whether the indexed-array offset is negative
    ctx.emitter.instruction(&format!("b.lt {}", null_label));                   // negative indexed-array offsets read as null
    abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 0);
    ctx.emitter.instruction(&format!("cmp {}, {}", result_reg, len_reg));       // compare the requested offset against the indexed-array length
    ctx.emitter.instruction(&format!("b.ge {}", null_label));                   // out-of-range indexed-array offsets read as null
    emit_array_get_in_bounds_aarch64(ctx, array_reg, result_reg, elem_ty, result_ty)?;
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the null fallback after a successful indexed-array read
    ctx.emitter.label(&null_label);
    emit_undefined_array_key_warning(ctx);
    emit_array_get_null_fallback(ctx, result_ty);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers an indexed-array element write for AArch64 targets.
fn lower_array_set_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
    value: ValueId,
    raw_value_ty: &PhpType,
    value_ty: &PhpType,
) -> Result<()> {
    if matches!(value_ty, PhpType::Mixed) {
        return lower_mixed_array_set_aarch64(ctx, array, index, value, raw_value_ty);
    }
    ctx.load_value_to_reg(array, "x0")?;
    ctx.load_value_to_reg(index, "x1")?;
    match value_ty {
        PhpType::Int | PhpType::Bool | PhpType::Float => {
            ctx.load_value_to_reg(value, "x2")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_int");
        }
        PhpType::Callable => {
            ctx.load_value_to_reg(value, "x0")?;
            abi::emit_incref_if_refcounted(ctx.emitter, value_ty);
            ctx.emitter.instruction("mov x2, x0");                              // pass an array-owned callable descriptor to the indexed-array setter
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(index, "x1")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_int");
        }
        PhpType::Str => {
            ctx.load_string_value_to_regs(value, "x2", "x3")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_str");
        }
        other if other.is_refcounted() => {
            ctx.load_value_to_reg(value, "x2")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_refcounted");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_set value PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Lowers an indexed-array element read for x86_64 targets.
fn lower_array_get_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    index: ValueId,
    elem_ty: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    let len_reg = abi::secondary_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(array, array_reg)?;
    ctx.load_value_to_reg(index, result_reg)?;
    let null_label = ctx.next_label("array_get_null");
    let done_label = ctx.next_label("array_get_done");

    ctx.emitter.instruction(&format!("cmp {}, 0", result_reg));                 // check whether the indexed-array offset is negative
    ctx.emitter.instruction(&format!("jl {}", null_label));                     // negative indexed-array offsets read as null
    abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 0);
    ctx.emitter.instruction(&format!("cmp {}, {}", result_reg, len_reg));       // compare the requested offset against the indexed-array length
    ctx.emitter.instruction(&format!("jge {}", null_label));                    // out-of-range indexed-array offsets read as null
    emit_array_get_in_bounds_x86_64(ctx, array_reg, result_reg, elem_ty, result_ty)?;
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the null fallback after a successful indexed-array read
    ctx.emitter.label(&null_label);
    emit_undefined_array_key_warning(ctx);
    emit_array_get_null_fallback(ctx, result_ty);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers an indexed-array element write for x86_64 targets.
fn lower_array_set_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
    value: ValueId,
    raw_value_ty: &PhpType,
    value_ty: &PhpType,
) -> Result<()> {
    if matches!(value_ty, PhpType::Mixed) {
        return lower_mixed_array_set_x86_64(ctx, array, index, value, raw_value_ty);
    }
    ctx.load_value_to_reg(array, "rdi")?;
    ctx.load_value_to_reg(index, "rsi")?;
    match value_ty {
        PhpType::Int | PhpType::Bool | PhpType::Float => {
            ctx.load_value_to_reg(value, "rdx")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_int");
        }
        PhpType::Callable => {
            ctx.load_value_to_reg(value, "rax")?;
            abi::emit_incref_if_refcounted(ctx.emitter, value_ty);
            ctx.emitter.instruction("mov rdx, rax");                            // pass an array-owned callable descriptor to the indexed-array setter
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(index, "rsi")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_int");
        }
        PhpType::Str => {
            ctx.load_string_value_to_regs(value, "rdx", "rcx")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_str");
        }
        other if other.is_refcounted() => {
            ctx.load_value_to_reg(value, "rdx")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_refcounted");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_set value PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Emits the in-bounds indexed-array payload load for AArch64.
fn emit_array_get_in_bounds_aarch64(
    ctx: &mut FunctionContext<'_>,
    array_reg: &str,
    index_reg: &str,
    elem_ty: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    match elem_ty {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, index_reg, 0x7fff_ffff_ffff_fffe);
        }
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip the indexed-array header to reach element payloads
            ctx.emitter.instruction(&format!("ldr {}, [{}, {}, lsl #3]", index_reg, array_reg, index_reg)); // load the selected pointer-sized indexed-array element
            if matches!(elem_ty, PhpType::Callable) {
                abi::emit_incref_if_refcounted(ctx.emitter, elem_ty);
            }
            if matches!(result_ty, PhpType::TaggedScalar) {
                crate::codegen::sentinels::emit_tagged_scalar_from_int_result(ctx.emitter);
            }
        }
        PhpType::Float => {
            ctx.emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip the indexed-array header to reach float payloads
            ctx.emitter.instruction(&format!("ldr d0, [{}, {}, lsl #3]", array_reg, index_reg)); // load the selected indexed-array float element
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.emitter.instruction(&format!("lsl {}, {}, #4", index_reg, index_reg)); // scale the string-array offset by the pointer-plus-length slot size
            ctx.emitter.instruction(&format!("add {}, {}, {}", array_reg, array_reg, index_reg)); // move to the selected string slot within the indexed array
            ctx.emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip the indexed-array header before loading the string slot
            abi::emit_load_from_address(ctx.emitter, ptr_reg, array_reg, 0);
            abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 8);
        }
        PhpType::TaggedScalar => {
            let tag_reg = crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter);
            ctx.emitter.instruction(&format!("lsl {}, {}, #4", index_reg, index_reg)); // scale the tagged-scalar offset by the payload-plus-tag slot size
            ctx.emitter.instruction(&format!("add {}, {}, {}", array_reg, array_reg, index_reg)); // move to the selected tagged-scalar slot within the indexed array
            ctx.emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip the indexed-array header before loading the tagged-scalar slot
            abi::emit_load_from_address(ctx.emitter, index_reg, array_reg, 0);
            abi::emit_load_from_address(ctx.emitter, tag_reg, array_reg, 8);
        }
        PhpType::Mixed => {
            ctx.emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip the indexed-array header to reach Mixed cell payloads
            ctx.emitter.instruction(&format!("ldr {}, [{}, {}, lsl #3]", index_reg, array_reg, index_reg)); // load the selected boxed Mixed cell
            abi::emit_incref_if_refcounted(ctx.emitter, elem_ty);
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip the indexed-array header to reach pointer payloads
            ctx.emitter.instruction(&format!("ldr {}, [{}, {}, lsl #3]", index_reg, array_reg, index_reg)); // load the selected refcounted indexed-array element
            abi::emit_incref_if_refcounted(ctx.emitter, other);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_get element PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Emits the in-bounds indexed-array payload load for x86_64.
fn emit_array_get_in_bounds_x86_64(
    ctx: &mut FunctionContext<'_>,
    array_reg: &str,
    index_reg: &str,
    elem_ty: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    match elem_ty {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, index_reg, 0x7fff_ffff_ffff_fffe);
        }
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.emitter.instruction(&format!("lea {}, [{} + 24]", array_reg, array_reg)); // skip the indexed-array header to reach element payloads
            ctx.emitter.instruction(&format!("mov {}, QWORD PTR [{} + {} * 8]", index_reg, array_reg, index_reg)); // load the selected pointer-sized indexed-array element
            if matches!(elem_ty, PhpType::Callable) {
                abi::emit_incref_if_refcounted(ctx.emitter, elem_ty);
            }
            if matches!(result_ty, PhpType::TaggedScalar) {
                crate::codegen::sentinels::emit_tagged_scalar_from_int_result(ctx.emitter);
            }
        }
        PhpType::Float => {
            ctx.emitter.instruction(&format!("lea {}, [{} + 24]", array_reg, array_reg)); // skip the indexed-array header to reach float payloads
            ctx.emitter.instruction(&format!("movsd xmm0, QWORD PTR [{} + {} * 8]", array_reg, index_reg)); // load the selected indexed-array float element
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.emitter.instruction(&format!("shl {}, 4", index_reg));          // scale the string-array offset by the pointer-plus-length slot size
            ctx.emitter.instruction(&format!("add {}, {}", array_reg, index_reg)); // move to the selected string slot within the indexed array
            ctx.emitter.instruction(&format!("add {}, 24", array_reg));         // skip the indexed-array header before loading the string slot
            abi::emit_load_from_address(ctx.emitter, ptr_reg, array_reg, 0);
            abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 8);
        }
        PhpType::TaggedScalar => {
            let tag_reg = crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter);
            ctx.emitter.instruction(&format!("shl {}, 4", index_reg));          // scale the tagged-scalar offset by the payload-plus-tag slot size
            ctx.emitter.instruction(&format!("add {}, {}", array_reg, index_reg)); // move to the selected tagged-scalar slot within the indexed array
            ctx.emitter.instruction(&format!("add {}, 24", array_reg));         // skip the indexed-array header before loading the tagged-scalar slot
            abi::emit_load_from_address(ctx.emitter, index_reg, array_reg, 0);
            abi::emit_load_from_address(ctx.emitter, tag_reg, array_reg, 8);
        }
        PhpType::Mixed => {
            ctx.emitter.instruction(&format!("lea {}, [{} + 24]", array_reg, array_reg)); // skip the indexed-array header to reach Mixed cell payloads
            ctx.emitter.instruction(&format!("mov {}, QWORD PTR [{} + {} * 8]", index_reg, array_reg, index_reg)); // load the selected boxed Mixed cell
            abi::emit_incref_if_refcounted(ctx.emitter, elem_ty);
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction(&format!("lea {}, [{} + 24]", array_reg, array_reg)); // skip the indexed-array header to reach pointer payloads
            ctx.emitter.instruction(&format!("mov {}, QWORD PTR [{} + {} * 8]", index_reg, array_reg, index_reg)); // load the selected refcounted indexed-array element
            abi::emit_incref_if_refcounted(ctx.emitter, other);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_get element PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Emits PHP's undefined integer array-key warning for the key in the result register.
fn emit_undefined_array_key_warning(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_warn_undefined_array_key_int");
}

/// Emits the null/miss fallback in the result shape expected by the array element type.
fn emit_array_get_null_fallback(ctx: &mut FunctionContext<'_>, elem_ty: &PhpType) {
    match elem_ty {
        PhpType::TaggedScalar => {
            crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
        }
        PhpType::Float => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("fmov d0, xzr");                        // materialize a stable zero float for an out-of-bounds array read
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("xorpd xmm0, xmm0");                    // materialize a stable zero float for an out-of-bounds array read
            }
        },
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
            abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
        }
        PhpType::Mixed => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_load_int_immediate(ctx.emitter, "x0", 8);
                abi::emit_load_int_immediate(ctx.emitter, "x1", 0);
                abi::emit_load_int_immediate(ctx.emitter, "x2", 0);
                abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            }
            Arch::X86_64 => {
                abi::emit_load_int_immediate(ctx.emitter, "rax", 8);
                abi::emit_load_int_immediate(ctx.emitter, "rdi", 0);
                abi::emit_load_int_immediate(ctx.emitter, "rsi", 0);
                abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            }
        },
        _ => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
        }
    }
}

/// Lowers an indexed-array append for AArch64 targets.
fn lower_array_push_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?;
    if array_push_value_needs_mixed_unbox(elem_ty, &value_ty) {
        return lower_array_push_unboxed_mixed_aarch64(ctx, array, value, elem_ty);
    }
    if array_push_value_needs_mixed_box(elem_ty, &value_ty) {
        return lower_mixed_array_push_aarch64(ctx, array, value, &value_ty);
    }
    match value_ty {
        PhpType::TaggedScalar if elem_ty.codegen_repr() == PhpType::TaggedScalar => {
            lower_array_push_tagged_scalar_aarch64(ctx, array, value)?;
        }
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_reg(value, "x1")?;
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::TaggedScalar if elem_ty.codegen_repr() == PhpType::Int => {
            ctx.load_value_to_result(value)?;
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
            ctx.emitter.instruction("mov x1, x0");                              // pass the nullable integer payload after PHP null-to-zero coercion
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Callable => {
            ctx.load_value_to_reg(value, "x0")?;
            abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
            ctx.emitter.instruction("mov x1, x0");                              // pass an array-owned callable descriptor to the append helper
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Float => {
            ctx.load_value_to_reg(value, "x1")?;
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Str => {
            ctx.load_string_value_to_regs(value, "x1", "x2")?;
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the string append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        }
        other if other.is_refcounted() => {
            ctx.load_value_to_reg(value, "x1")?;
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the refcounted append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_push for PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Lowers an indexed-array append for x86_64 targets.
fn lower_array_push_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?;
    if array_push_value_needs_mixed_unbox(elem_ty, &value_ty) {
        return lower_array_push_unboxed_mixed_x86_64(ctx, array, value, elem_ty);
    }
    if array_push_value_needs_mixed_box(elem_ty, &value_ty) {
        return lower_mixed_array_push_x86_64(ctx, array, value, &value_ty);
    }
    match value_ty {
        PhpType::TaggedScalar if elem_ty.codegen_repr() == PhpType::TaggedScalar => {
            lower_array_push_tagged_scalar_x86_64(ctx, array, value)?;
        }
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_reg(array, "r11")?;
            ctx.load_value_to_reg(value, "rsi")?;
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::TaggedScalar if elem_ty.codegen_repr() == PhpType::Int => {
            ctx.load_value_to_result(value)?;
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
            ctx.emitter.instruction("mov rsi, rax");                            // pass the nullable integer payload after PHP null-to-zero coercion
            ctx.load_value_to_reg(array, "r11")?;
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Callable => {
            ctx.load_value_to_reg(value, "rax")?;
            abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
            ctx.emitter.instruction("mov rsi, rax");                            // pass an array-owned callable descriptor to the append helper
            ctx.load_value_to_reg(array, "r11")?;
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Float => {
            ctx.load_value_to_reg(array, "r11")?;
            ctx.load_value_to_reg(value, "rsi")?;
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Str => {
            ctx.load_value_to_reg(array, "r11")?;
            ctx.load_string_value_to_regs(value, "rsi", "rdx")?;
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the string append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        }
        other if other.is_refcounted() => {
            ctx.load_value_to_reg(array, "r11")?;
            ctx.load_value_to_reg(value, "rsi")?;
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the refcounted append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_push for PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Returns true when an append into a Mixed array must box a concrete value first.
fn array_push_value_needs_mixed_box(elem_ty: &PhpType, value_ty: &PhpType) -> bool {
    matches!(elem_ty.codegen_repr(), PhpType::Mixed)
        && !matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
}

/// Returns true when a boxed Mixed value should be unboxed before a typed append.
fn array_push_value_needs_mixed_unbox(elem_ty: &PhpType, value_ty: &PhpType) -> bool {
    !matches!(elem_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        && matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
}

/// Appends an inline tagged scalar into a 16-byte tagged-scalar indexed array on AArch64.
fn lower_array_push_tagged_scalar_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
) -> Result<()> {
    let check_label = ctx.next_label("array_push_tagged_check");
    let grow_label = ctx.next_label("array_push_tagged_grow");
    let done_label = ctx.next_label("array_push_tagged_done");
    ctx.load_value_to_result(value)?;
    ctx.emitter.instruction("sub sp, sp, #32");                                 // reserve spill slots for the tagged payload and mutable array pointer
    ctx.emitter.instruction("str x0, [sp, #0]");                                // save the tagged-scalar payload across uniqueness and growth calls
    ctx.emitter.instruction("str x1, [sp, #8]");                                // save the tagged-scalar runtime tag across uniqueness and growth calls
    ctx.load_value_to_reg(array, "x0")?;
    abi::emit_call_label(ctx.emitter, "__rt_array_ensure_unique");
    ctx.emitter.instruction("str x0, [sp, #16]");                               // preserve the unique indexed-array pointer across the capacity check
    ctx.emitter.instruction("ldr x9, [x0]");                                    // load length before first-write tagged-scalar shape specialization
    ctx.emitter.instruction(&format!("cbnz x9, {}", check_label));              // existing arrays already have their tagged-scalar shape fixed
    ctx.emitter.instruction("mov x10, #16");                                    // tagged-scalar slots store payload and runtime tag words
    ctx.emitter.instruction("str x10, [x0, #16]");                              // elem_size = 16 before growth can copy tagged-scalar slots
    emit_tagged_scalar_array_value_type_stamp(ctx, "x0");
    ctx.emitter.label(&check_label);
    ctx.emitter.instruction("ldr x0, [sp, #16]");                               // reload the current indexed-array pointer before checking capacity
    ctx.emitter.instruction("ldr x9, [x0]");                                    // load the current logical length
    ctx.emitter.instruction("ldr x10, [x0, #8]");                               // load the current capacity
    ctx.emitter.instruction("cmp x9, x10");                                     // is the tagged-scalar array already full?
    ctx.emitter.instruction(&format!("b.ge {}", grow_label));                   // grow before writing when the append would exceed capacity
    ctx.emitter.instruction("lsl x10, x9, #4");                                 // convert length to a byte offset for 16-byte tagged-scalar slots
    ctx.emitter.instruction("add x10, x0, x10");                                // move to the selected append slot base
    ctx.emitter.instruction("add x10, x10, #24");                               // skip the indexed-array header before storing the slot
    ctx.emitter.instruction("ldr x11, [sp, #0]");                               // reload the tagged-scalar payload for the appended slot
    ctx.emitter.instruction("ldr x12, [sp, #8]");                               // reload the tagged-scalar runtime tag for the appended slot
    ctx.emitter.instruction("str x11, [x10]");                                  // store the tagged-scalar payload word in the append slot
    ctx.emitter.instruction("str x12, [x10, #8]");                              // store the tagged-scalar runtime tag word in the append slot
    ctx.emitter.instruction("add x9, x9, #1");                                  // advance the indexed-array logical length
    ctx.emitter.instruction("str x9, [x0]");                                    // publish the updated logical length
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the growth path after storing the tagged-scalar slot
    ctx.emitter.label(&grow_label);
    ctx.emitter.instruction("ldr x0, [sp, #16]");                               // reload the unique indexed-array pointer for growth
    abi::emit_call_label(ctx.emitter, "__rt_array_grow");
    ctx.emitter.instruction("str x0, [sp, #16]");                               // preserve the grown indexed-array pointer before retrying the append
    ctx.emitter.instruction(&format!("b {}", check_label));                     // retry the capacity check against the grown storage
    ctx.emitter.label(&done_label);
    ctx.emitter.instruction("add sp, sp, #32");                                 // release tagged-scalar append spill slots
    Ok(())
}

/// Appends an inline tagged scalar into a 16-byte tagged-scalar indexed array on x86_64.
fn lower_array_push_tagged_scalar_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
) -> Result<()> {
    let check_label = ctx.next_label("array_push_tagged_check");
    let grow_label = ctx.next_label("array_push_tagged_grow");
    let done_label = ctx.next_label("array_push_tagged_done");
    ctx.load_value_to_result(value)?;
    ctx.emitter.instruction("sub rsp, 32");                                     // reserve spill slots for the tagged payload and mutable array pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                        // save the tagged-scalar payload across uniqueness and growth calls
    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                    // save the tagged-scalar runtime tag across uniqueness and growth calls
    ctx.load_value_to_reg(array, "rdi")?;
    abi::emit_call_label(ctx.emitter, "__rt_array_ensure_unique");
    ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");                   // preserve the unique indexed-array pointer across the capacity check
    ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                        // load length before first-write tagged-scalar shape specialization
    ctx.emitter.instruction("test r10, r10");                                   // is this the first append into a tagged-scalar array?
    ctx.emitter.instruction(&format!("jnz {}", check_label));                   // existing arrays already have their tagged-scalar shape fixed
    ctx.emitter.instruction("mov QWORD PTR [rax + 16], 16");                    // elem_size = 16 before growth can copy tagged-scalar slots
    emit_tagged_scalar_array_value_type_stamp(ctx, "rax");
    ctx.emitter.label(&check_label);
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                   // reload the current indexed-array pointer before checking capacity
    ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                        // load the current logical length
    ctx.emitter.instruction("mov r11, QWORD PTR [rax + 8]");                    // load the current capacity
    ctx.emitter.instruction("cmp r10, r11");                                    // is the tagged-scalar array already full?
    ctx.emitter.instruction(&format!("jae {}", grow_label));                    // grow before writing when the append would exceed capacity
    ctx.emitter.instruction("mov rcx, r10");                                    // copy the logical length before scaling it into a byte offset
    ctx.emitter.instruction("shl rcx, 4");                                      // convert length to a byte offset for 16-byte tagged-scalar slots
    ctx.emitter.instruction("lea rcx, [rax + rcx + 24]");                       // compute the address of the next tagged-scalar append slot
    ctx.emitter.instruction("mov r8, QWORD PTR [rsp]");                         // reload the tagged-scalar payload for the appended slot
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 8]");                     // reload the tagged-scalar runtime tag for the appended slot
    ctx.emitter.instruction("mov QWORD PTR [rcx], r8");                         // store the tagged-scalar payload word in the append slot
    ctx.emitter.instruction("mov QWORD PTR [rcx + 8], r9");                     // store the tagged-scalar runtime tag word in the append slot
    ctx.emitter.instruction("add r10, 1");                                      // advance the indexed-array logical length
    ctx.emitter.instruction("mov QWORD PTR [rax], r10");                        // publish the updated logical length
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the growth path after storing the tagged-scalar slot
    ctx.emitter.label(&grow_label);
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the unique indexed-array pointer to the growth helper
    abi::emit_call_label(ctx.emitter, "__rt_array_grow");
    ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");                   // preserve the grown indexed-array pointer before retrying the append
    ctx.emitter.instruction(&format!("jmp {}", check_label));                   // retry the capacity check against the grown storage
    ctx.emitter.label(&done_label);
    ctx.emitter.instruction("add rsp, 32");                                     // release tagged-scalar append spill slots
    Ok(())
}

/// Appends an unboxed Mixed payload into a typed indexed array on AArch64.
fn lower_array_push_unboxed_mixed_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    ctx.load_value_to_reg(value, "x0")?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match elem_ty.codegen_repr() {
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Float => {
            ctx.emitter.instruction("mov x11, x1");                             // keep the unboxed scalar payload while loading the array receiver
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x1, x11");                             // pass the unboxed scalar payload to the append helper
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Str => {
            ctx.emitter.instruction("mov x11, x1");                             // keep the unboxed string pointer while loading the array receiver
            ctx.emitter.instruction("mov x12, x2");                             // keep the unboxed string length while loading the array receiver
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x1, x11");                             // pass the unboxed string pointer to the string append helper
            ctx.emitter.instruction("mov x2, x12");                             // pass the unboxed string length to the string append helper
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the string append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction("mov x11, x1");                             // keep the unboxed heap payload while loading the array receiver
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x1, x11");                             // pass the unboxed heap payload to the append helper
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the refcounted append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_push unboxed Mixed into PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Appends an unboxed Mixed payload into a typed indexed array on x86_64 targets.
fn lower_array_push_unboxed_mixed_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    ctx.load_value_to_reg(value, "rax")?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match elem_ty.codegen_repr() {
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Float => {
            ctx.emitter.instruction("mov r10, rdi");                            // keep the unboxed scalar payload while loading the array receiver
            ctx.load_value_to_reg(array, "r11")?;
            ctx.emitter.instruction("mov rsi, r10");                            // pass the unboxed scalar payload to the append helper
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Str => {
            ctx.emitter.instruction("mov r10, rdi");                            // keep the unboxed string pointer while loading the array receiver
            ctx.emitter.instruction("mov r9, rdx");                             // keep the unboxed string length while loading the array receiver
            ctx.load_value_to_reg(array, "r11")?;
            ctx.emitter.instruction("mov rsi, r10");                            // pass the unboxed string pointer to the string append helper
            ctx.emitter.instruction("mov rdx, r9");                             // pass the unboxed string length to the string append helper
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the string append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction("mov r10, rdi");                            // keep the unboxed heap payload while loading the array receiver
            ctx.load_value_to_reg(array, "r11")?;
            ctx.emitter.instruction("mov rsi, r10");                            // pass the unboxed heap payload to the append helper
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the refcounted append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_push unboxed Mixed into PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Appends to an indexed array stored inside a boxed Mixed cell on AArch64.
fn lower_mixed_array_append_aarch64(
    ctx: &mut FunctionContext<'_>,
    receiver: ValueId,
    value: ValueId,
) -> Result<()> {
    let drop_label = ctx.next_label("mixed_array_append_drop");
    let done_label = ctx.next_label("mixed_array_append_done");
    prepare_boxed_mixed_value_for_container(ctx, value)?;
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.load_value_to_reg(receiver, "x0")?;
    abi::emit_push_reg(ctx.emitter, "x0");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp x0, #4");                                      // require an indexed-array payload before deriving the append key
    ctx.emitter.instruction(&format!("b.ne {}", drop_label));                   // drop the boxed value when the Mixed cell is not an indexed array
    ctx.emitter.instruction(&format!("cbz x1, {}", drop_label));                // drop the boxed value when the indexed-array payload is null
    ctx.emitter.instruction("mov x0, x1");                                      // pass the unboxed indexed-array payload to the Mixed conversion helper
    ctx.emitter.instruction("ldr x1, [x0, #-8]");                               // load indexed-array metadata before Mixed-slot conversion
    ctx.emitter.instruction("lsr x1, x1, #8");                                  // move the runtime value_type tag into the low bits
    ctx.emitter.instruction("and x1, x1, #0x7f");                               // isolate the indexed-array value_type tag
    abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
    abi::emit_pop_reg(ctx.emitter, "x10");
    ctx.emitter.instruction("str x0, [x10, #8]");                               // publish the converted indexed array back into the Mixed cell
    ctx.emitter.instruction("ldr x1, [x0]");                                    // use the current logical length as the append index
    ctx.emitter.instruction("mov x0, x10");                                     // pass the target Mixed cell to the runtime setter
    abi::emit_pop_reg(ctx.emitter, "x3");
    ctx.emitter.instruction("mov x2, #-1");                                     // key_hi = -1 marks an integer array key
    abi::emit_call_label(ctx.emitter, "__rt_mixed_array_set");
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the failure cleanup after the setter consumes the value
    ctx.emitter.label(&drop_label);
    abi::emit_pop_reg(ctx.emitter, "x9");
    abi::emit_pop_reg(ctx.emitter, "x0");
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Appends to an indexed array stored inside a boxed Mixed cell on x86_64.
fn lower_mixed_array_append_x86_64(
    ctx: &mut FunctionContext<'_>,
    receiver: ValueId,
    value: ValueId,
) -> Result<()> {
    let drop_label = ctx.next_label("mixed_array_append_drop");
    let done_label = ctx.next_label("mixed_array_append_done");
    prepare_boxed_mixed_value_for_container(ctx, value)?;
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.load_value_to_reg(receiver, "rax")?;
    abi::emit_push_reg(ctx.emitter, "rax");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp rax, 4");                                      // require an indexed-array payload before deriving the append key
    ctx.emitter.instruction(&format!("jne {}", drop_label));                    // drop the boxed value when the Mixed cell is not an indexed array
    ctx.emitter.instruction("test rdi, rdi");                                   // verify the unboxed indexed-array payload is present
    ctx.emitter.instruction(&format!("je {}", drop_label));                     // drop the boxed value when the indexed-array payload is null
    ctx.emitter.instruction("mov rsi, QWORD PTR [rdi - 8]");                    // load indexed-array metadata before Mixed-slot conversion
    ctx.emitter.instruction("shr rsi, 8");                                      // move the runtime value_type tag into the low bits
    ctx.emitter.instruction("and rsi, 0x7f");                                   // isolate the indexed-array value_type tag
    abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
    abi::emit_pop_reg(ctx.emitter, "r10");
    ctx.emitter.instruction("mov QWORD PTR [r10 + 8], rax");                    // publish the converted indexed array back into the Mixed cell
    ctx.emitter.instruction("mov rsi, QWORD PTR [rax]");                        // use the current logical length as the append index
    ctx.emitter.instruction("mov rdi, r10");                                    // pass the target Mixed cell to the runtime setter
    abi::emit_pop_reg(ctx.emitter, "rcx");
    ctx.emitter.instruction("mov rdx, -1");                                     // key_hi = -1 marks an integer array key
    abi::emit_call_label(ctx.emitter, "__rt_mixed_array_set");
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the failure cleanup after the setter consumes the value
    ctx.emitter.label(&drop_label);
    abi::emit_pop_reg(ctx.emitter, "r11");
    abi::emit_pop_reg(ctx.emitter, "rax");
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Materializes the appended value as an owned boxed Mixed cell.
fn prepare_boxed_mixed_value_for_container(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        ctx.load_value_to_result(value)?;
        abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
    } else {
        box_value_for_mixed_container(ctx, value, &value_ty)?;
    }
    Ok(())
}

/// Boxes a concrete AArch64 value and appends the owned Mixed cell to a Mixed array.
fn lower_mixed_array_push_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    box_value_for_mixed_container(ctx, value, value_ty)?;
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.load_value_to_reg(array, "x9")?;
    ctx.emitter.instruction("mov x1, x0");                                      // pass the boxed Mixed payload to the refcounted append helper
    ctx.emitter.instruction("mov x0, x9");                                      // pass the indexed-array receiver to the refcounted append helper
    abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
    emit_release_pushed_refcounted_temp_after_array_push(ctx.emitter, &PhpType::Mixed);
    Ok(())
}

/// Boxes a concrete x86_64 value and appends the owned Mixed cell to a Mixed array.
fn lower_mixed_array_push_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    box_value_for_mixed_container(ctx, value, value_ty)?;
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.load_value_to_reg(array, "r11")?;
    ctx.emitter.instruction("mov rsi, rax");                                    // pass the boxed Mixed payload to the refcounted append helper
    ctx.emitter.instruction("mov rdi, r11");                                    // pass the indexed-array receiver to the refcounted append helper
    abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
    emit_release_pushed_refcounted_temp_after_array_push(ctx.emitter, &PhpType::Mixed);
    Ok(())
}

/// Boxes or retains a value, then stores it into a Mixed indexed array on AArch64.
fn lower_mixed_array_set_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    if matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        ctx.load_value_to_result(value)?;
        abi::emit_incref_if_refcounted(ctx.emitter, value_ty);
    } else {
        box_value_for_mixed_container(ctx, value, value_ty)?;
    }
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.load_value_to_reg(array, "x0")?;
    ctx.load_value_to_reg(index, "x1")?;
    abi::emit_pop_reg(ctx.emitter, "x2");
    abi::emit_call_label(ctx.emitter, "__rt_array_set_mixed");
    Ok(())
}

/// Boxes or retains a value, then stores it into a Mixed indexed array on x86_64.
fn lower_mixed_array_set_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    if matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        ctx.load_value_to_result(value)?;
        abi::emit_incref_if_refcounted(ctx.emitter, value_ty);
    } else {
        box_value_for_mixed_container(ctx, value, value_ty)?;
    }
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.load_value_to_reg(array, "rdi")?;
    ctx.load_value_to_reg(index, "rsi")?;
    abi::emit_pop_reg(ctx.emitter, "rdx");
    abi::emit_call_label(ctx.emitter, "__rt_array_set_mixed");
    Ok(())
}

/// Boxes a value for a Mixed array, consuming owned producers when possible.
fn box_value_for_mixed_container(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    ctx.load_value_to_result(value)?;
    if ctx.value_can_own_mixed_box_source(value)? {
        emit_box_current_owned_value_as_mixed(ctx.emitter, &value_ty.codegen_repr());
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty.codegen_repr());
    }
    Ok(())
}

/// Returns the PHP element type for an indexed-array operand.
fn indexed_array_element_type(array_ty: &PhpType, inst: &Instruction) -> Result<PhpType> {
    match array_ty {
        PhpType::Array(elem_ty) => Ok(elem_ty.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            inst.op.name(),
            other
        ))),
    }
}

/// Resolves the runtime value type that an indexed array write can store.
fn effective_array_set_value_type(
    elem_ty: &PhpType,
    value_ty: &PhpType,
    inst: &Instruction,
) -> Result<PhpType> {
    let elem_ty = elem_ty.codegen_repr();
    let value_ty = value_ty.codegen_repr();
    if matches!(elem_ty, PhpType::Mixed) {
        return Ok(PhpType::Mixed);
    }
    if matches!(elem_ty, PhpType::Never | PhpType::Void) {
        return require_supported_array_set_value(value_ty, inst);
    }
    if elem_ty == value_ty {
        return require_supported_array_set_value(value_ty, inst);
    }
    Err(CodegenIrError::unsupported(format!(
        "array_set element PHP type {:?} with value PHP type {:?}",
        elem_ty, value_ty
    )))
}

/// Rejects indexed-array write payload types that do not have Phase 04 storage lowering yet.
fn require_supported_array_set_value(value_ty: PhpType, inst: &Instruction) -> Result<PhpType> {
    if matches!(
        value_ty,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Callable
            | PhpType::Float
            | PhpType::Str
            | PhpType::TaggedScalar
    ) {
        return Ok(value_ty);
    }
    if value_ty.is_refcounted() {
        return Ok(value_ty);
    }
    Err(CodegenIrError::unsupported(format!(
        "{} value PHP type {:?}",
        inst.op.name(),
        value_ty
    )))
}

/// Verifies that an indexed-array write uses an integer-like offset value.
fn require_integer_like_index(index_ty: PhpType, inst: &Instruction) -> Result<()> {
    if matches!(index_ty.codegen_repr(), PhpType::Int | PhpType::Bool | PhpType::Callable) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} index PHP type {:?}",
        inst.op.name(),
        index_ty
    )))
}

/// Rejects array-get result shapes that do not match the lowered array element type.
fn require_array_get_result(elem_ty: &PhpType, inst: &Instruction) -> Result<()> {
    let result_ty = inst.result_php_type.codegen_repr();
    if crate::codegen::sentinels::null_repr_is_tagged()
        && matches!(elem_ty, PhpType::Int)
        && result_ty == PhpType::TaggedScalar
    {
        return Ok(());
    }
    if matches!(elem_ty, PhpType::TaggedScalar) && result_ty == PhpType::TaggedScalar {
        return Ok(());
    }
    if matches!(elem_ty, PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Float | PhpType::Str)
        && result_ty == *elem_ty
    {
        return Ok(());
    }
    if elem_ty.is_refcounted() && result_ty == *elem_ty {
        return Ok(());
    }
    if matches!(elem_ty, PhpType::Void | PhpType::Never)
        && matches!(result_ty, PhpType::Void | PhpType::Never)
    {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_get element PHP type {:?} with result PHP type {:?}",
        elem_ty, inst.result_php_type
    )))
}

/// Verifies that `array_to_mixed` produces an indexed array with boxed Mixed slots.
fn require_array_to_mixed_result(result_ty: &PhpType, inst: &Instruction) -> Result<()> {
    match result_ty {
        PhpType::Array(elem_ty) if elem_ty.codegen_repr() == PhpType::Mixed => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} result PHP type {:?}",
            inst.op.name(),
            other
        ))),
    }
}

/// Verifies that `array_to_hash` produces associative-array storage.
fn require_array_to_hash_result(result_ty: &PhpType, inst: &Instruction) -> Result<PhpType> {
    match result_ty {
        PhpType::AssocArray { value, .. } => Ok(value.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} result PHP type {:?}",
            inst.op.name(),
            other
        ))),
    }
}

/// Verifies that a cross-array union operand uses associative hash storage.
fn require_assoc_union_hash_operand(ty: PhpType, inst: &Instruction) -> Result<()> {
    if matches!(ty.codegen_repr(), PhpType::AssocArray { .. }) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} hash operand PHP type {:?}",
        inst.op.name(),
        ty
    )))
}

/// Converts a just-returned hash union result to boxed Mixed entries when required.
fn convert_hash_union_result_to_mixed_if_needed(
    ctx: &mut FunctionContext<'_>,
    result_value_ty: &PhpType,
) {
    if result_value_ty != &PhpType::Mixed {
        return;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the hash result to the Mixed-entry conversion helper
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
        }
    }
}

/// Returns the local/ref-cell slot loaded by an array operand when it can be written back after growth.
fn source_load_local_slot(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<LocalSlotId>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    if matches!(inst_ref.op, Op::LoadLocal | Op::LoadRefCell) {
        if let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate {
            return Ok(Some(slot));
        }
    }
    Ok(None)
}

/// Returns the runtime element-slot width for an indexed-array PHP type.
fn array_element_size(ty: &PhpType) -> Result<i64> {
    match ty {
        PhpType::Array(elem) => {
            if matches!(
                elem.codegen_repr(),
                PhpType::Str | PhpType::TaggedScalar | PhpType::Never
            ) {
                Ok(16)
            } else {
                Ok(8)
            }
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_new result PHP type {:?}",
            other
        ))),
    }
}

/// Stamps an indexed array as carrying inline tagged-scalar `{payload, tag}` slots.
fn emit_tagged_scalar_array_value_type_stamp(ctx: &mut FunctionContext<'_>, array_reg: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr x10, [{}, #-8]", array_reg)); // load the packed indexed-array metadata before replacing value_type bits
            ctx.emitter.instruction("mov x11, #0x80ff");                        // preserve heap kind and persistent COW metadata only
            ctx.emitter.instruction("and x10, x10, x11");                       // clear stale indexed-array value_type bits
            ctx.emitter.instruction(&format!("mov x11, #{}", TAGGED_SCALAR_ARRAY_VALUE_TYPE)); // value_type 11 = inline tagged-scalar slots
            ctx.emitter.instruction("lsl x11, x11, #8");                        // move the tagged-scalar value_type into the packed kind word
            ctx.emitter.instruction("orr x10, x10, x11");                       // combine stable metadata with the tagged-scalar value_type tag
            ctx.emitter.instruction(&format!("str x10, [{}, #-8]", array_reg)); // publish tagged-scalar indexed-array metadata
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov r10, QWORD PTR [{} - 8]", array_reg)); // load the packed indexed-array metadata before replacing value_type bits
            ctx.emitter.instruction("mov r11, 0xffffffff000080ff");             // preserve heap marker, indexed-array kind, and persistent COW metadata
            ctx.emitter.instruction("and r10, r11");                            // clear stale indexed-array value_type bits
            ctx.emitter.instruction(&format!("or r10, 0x{:x}", TAGGED_SCALAR_ARRAY_VALUE_TYPE << 8)); // add value_type 11 for inline tagged-scalar slots
            ctx.emitter.instruction(&format!("mov QWORD PTR [{} - 8], r10", array_reg)); // publish tagged-scalar indexed-array metadata
        }
    }
}

/// Verifies that an array opcode receives an indexed array.
fn require_indexed_array(ty: PhpType, inst: &Instruction) -> Result<()> {
    if matches!(ty, PhpType::Array(_)) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        inst.op.name(),
        ty
    )))
}

/// Returns the capacity immediate attached to an array allocation.
fn expect_capacity(inst: &Instruction) -> Result<u32> {
    match inst.immediate {
        Some(Immediate::Capacity(capacity)) => Ok(capacity),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing capacity immediate",
            inst.op.name()
        ))),
    }
}
