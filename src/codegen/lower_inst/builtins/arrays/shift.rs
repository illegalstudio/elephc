//! Purpose:
//! Lowers PHP `array_shift()` calls for indexed arrays in the Phase 04 EIR backend.
//! Handles target-aware slot compaction and Mixed boxing for the removed value.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays::lower_array_shift()`.
//!
//! Key details:
//! - Mutates the caller-visible array after copy-on-write splitting, publishing the split
//!   pointer through `ReceiverPlace` so a by-reference parameter observes it as well.
//! - Returns PHP `mixed`, including boxed null for empty arrays.
//! - Supports pointer-sized, float, string, Mixed, and refcounted indexed payloads.
//! - Slot compaction transfers the removed owner into the returned Mixed cell.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::context::FunctionContext;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::receiver_place::ReceiverPlace;
use super::super::super::{expect_operand, store_if_result};

/// Lowers `array_shift()` for indexed arrays by compacting slots and boxing `T|null` as Mixed.
pub(super) fn lower_array_shift(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_shift", 1)?;
    let array = expect_operand(inst, 0)?;
    if ctx.value_php_type(array)?.codegen_repr() == PhpType::Mixed {
        return super::boxed_mutation::lower_boxed_array_take(ctx, inst, array, true);
    }
    // A concrete associative source: `array_shift` renumbers the surviving integer keys, so it
    // needs the hash-rebuilding path rather than the indexed slot compactor. The rebuilt table
    // keeps the source's value representation while the removed value is boxed to `mixed`.
    if let PhpType::AssocArray { .. } = ctx.value_php_type(array)?.codegen_repr() {
        return super::boxed_mutation::lower_assoc_array_shift(ctx, inst, array);
    }
    let elem_ty = array_shift_element_type(ctx.value_php_type(array)?)?;
    require_array_shift_result_type(&inst.result_php_type.codegen_repr())?;
    let receiver = ReceiverPlace::resolve(ctx, array)?;
    ensure_unique_array_shift_source(ctx, array)?;
    receiver.store_back_after_consuming_split(ctx, array)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_array_shift_aarch64(ctx, array, &elem_ty)?,
        Arch::X86_64 => lower_array_shift_x86_64(ctx, array, &elem_ty)?,
    }
    store_if_result(ctx, inst)
}

/// Returns the supported element payload type for an indexed-array `array_shift()`.
fn array_shift_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            if matches!(
                elem,
                PhpType::Int
                    | PhpType::Bool
                    | PhpType::Float
                    | PhpType::Str
                    | PhpType::Callable
                    | PhpType::Mixed
                    | PhpType::Void
                    | PhpType::Never
                    | PhpType::Pointer(_)
                    | PhpType::Buffer(_)
            ) || elem.is_refcounted()
            {
                return Ok(elem);
            }
            Err(CodegenIrError::unsupported(format!(
                "array_shift indexed-array element PHP type {:?}",
                elem
            )))
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_shift for PHP type {:?}",
            other
        ))),
    }
}

/// Verifies the lowered `array_shift()` result uses PHP's `mixed` shape.
pub(super) fn require_array_shift_result_type(result_ty: &PhpType) -> Result<()> {
    if result_ty == &PhpType::Mixed {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_shift result PHP type {:?}",
        result_ty
    )))
}

/// Splits a shared indexed array before `array_shift()` mutates its slots.
fn ensure_unique_array_shift_source(ctx: &mut FunctionContext<'_>, array: ValueId) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_ensure_unique");
    ctx.store_result_value(array)
}

/// Emits the AArch64 `array_shift()` sequence for indexed arrays.
fn lower_array_shift_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    let empty_label = ctx.next_label("array_shift_empty");
    let done_label = ctx.next_label("array_shift_done");
    ctx.load_value_to_reg(array, "x0")?;
    elephc_memoir::values::packed_slots::emit_load_length(ctx.emitter, "x9", "x0"); // load the indexed-array length before deciding whether shift is empty
    ctx.emitter.instruction(&format!("cbz x9, {}", empty_label));               // return boxed null when array_shift() runs on an empty array
    emit_array_shift_save_first_aarch64(ctx, elem_ty)?;
    let loop_label = ctx.next_label("array_shift_loop");
    let compact_done = ctx.next_label("array_shift_compact_done");
    elephc_memoir::values::array_shift::emit_array_shift_compact_aarch64(
        ctx.emitter, matches!(elem_ty, PhpType::Str), &loop_label, &compact_done,
    );
    ctx.emitter.instruction("sub x9, x9, #1");                                  // decrement the indexed-array length after removing the first element
    elephc_memoir::values::packed_slots::emit_publish_length(ctx.emitter, "x0", "x9"); // persist the shortened indexed-array length in the header
    emit_array_shift_restore_first_aarch64(ctx, elem_ty)?;
    crate::codegen::emit_box_current_owned_value_as_mixed(ctx.emitter, elem_ty);
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the empty-array boxed-null path after loading the removed value
    ctx.emitter.label(&empty_label);
    emit_array_shift_null(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the x86_64 `array_shift()` sequence for indexed arrays.
fn lower_array_shift_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    let empty_label = ctx.next_label("array_shift_empty");
    let done_label = ctx.next_label("array_shift_done");
    ctx.load_value_to_reg(array, "rax")?;
    elephc_memoir::values::packed_slots::emit_load_length(ctx.emitter, "r10", "rax"); // load the indexed-array length before deciding whether shift is empty
    ctx.emitter.instruction("test r10, r10");                                   // check whether the indexed array has any live elements
    ctx.emitter.instruction(&format!("jz {}", empty_label));                    // return boxed null when array_shift() runs on an empty array
    emit_array_shift_save_first_x86_64(ctx, elem_ty)?;
    let loop_label = ctx.next_label("array_shift_loop");
    let compact_done = ctx.next_label("array_shift_compact_done");
    elephc_memoir::values::array_shift::emit_array_shift_compact_x86_64(
        ctx.emitter, matches!(elem_ty, PhpType::Str), &loop_label, &compact_done,
    );
    ctx.emitter.instruction("sub r10, 1");                                      // decrement the indexed-array length after removing the first element
    elephc_memoir::values::packed_slots::emit_publish_length(ctx.emitter, "rax", "r10"); // persist the shortened indexed-array length in the header
    emit_array_shift_restore_first_x86_64(ctx, elem_ty)?;
    crate::codegen::emit_box_current_owned_value_as_mixed(ctx.emitter, elem_ty);
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the empty-array boxed-null path after loading the removed value
    ctx.emitter.label(&empty_label);
    emit_array_shift_null(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Preserves the first AArch64 indexed-array payload before the compaction loop.
fn emit_array_shift_save_first_aarch64(
    ctx: &mut FunctionContext<'_>,
    elem_ty: &PhpType,
) -> Result<()> {
    match elem_ty {
        PhpType::Int
        | PhpType::Bool
        | PhpType::Callable
        | PhpType::Mixed
        | PhpType::Pointer(_)
        | PhpType::Buffer(_) => {
            elephc_memoir::values::packed_slots::emit_packed_payload_aarch64(ctx.emitter, "x10", "x0");
            ctx.emitter.instruction("ldr x11, [x10]");                          // preserve the removed first pointer-sized payload across compaction
        }
        PhpType::Float => {
            elephc_memoir::values::packed_slots::emit_packed_payload_aarch64(ctx.emitter, "x10", "x0");
            ctx.emitter.instruction("ldr d1, [x10]");                           // preserve the removed first float payload across compaction
        }
        PhpType::Str => {
            elephc_memoir::values::packed_slots::emit_packed_payload_aarch64(ctx.emitter, "x10", "x0");
            ctx.emitter.instruction("ldr x11, [x10]");                          // preserve the removed first string pointer across compaction
            ctx.emitter.instruction("ldr x12, [x10, #8]");                      // preserve the removed first string length across compaction
        }
        PhpType::Void | PhpType::Never => {
            elephc_memoir::values::packed_slots::emit_packed_payload_aarch64(ctx.emitter, "x10", "x0");
            ctx.emitter.instruction("mov x11, #0");                             // preserve a null payload for impossible void-array live elements
        }
        other if other.is_refcounted() => {
            elephc_memoir::values::packed_slots::emit_packed_payload_aarch64(ctx.emitter, "x10", "x0");
            ctx.emitter.instruction("ldr x11, [x10]");                          // preserve the removed first heap pointer across compaction
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_shift element PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Preserves the first x86_64 indexed-array payload before the compaction loop.
fn emit_array_shift_save_first_x86_64(
    ctx: &mut FunctionContext<'_>,
    elem_ty: &PhpType,
) -> Result<()> {
    match elem_ty {
        PhpType::Int
        | PhpType::Bool
        | PhpType::Callable
        | PhpType::Mixed
        | PhpType::Pointer(_)
        | PhpType::Buffer(_) => {
            elephc_memoir::values::packed_slots::emit_packed_payload_x86_64(ctx.emitter, "r11", "rax");
            ctx.emitter.instruction("mov r8, QWORD PTR [r11]");                 // preserve the removed first pointer-sized payload across compaction
        }
        PhpType::Float => {
            elephc_memoir::values::packed_slots::emit_packed_payload_x86_64(ctx.emitter, "r11", "rax");
            ctx.emitter.instruction("movsd xmm1, QWORD PTR [r11]");             // preserve the removed first float payload across compaction
        }
        PhpType::Str => {
            elephc_memoir::values::packed_slots::emit_packed_payload_x86_64(ctx.emitter, "r11", "rax");
            ctx.emitter.instruction("mov r8, QWORD PTR [r11]");                 // preserve the removed first string pointer across compaction
            ctx.emitter.instruction("mov r9, QWORD PTR [r11 + 8]");             // preserve the removed first string length across compaction
        }
        PhpType::Void | PhpType::Never => {
            elephc_memoir::values::packed_slots::emit_packed_payload_x86_64(ctx.emitter, "r11", "rax");
            ctx.emitter.instruction("xor r8d, r8d");                            // preserve a null payload for impossible void-array live elements
        }
        other if other.is_refcounted() => {
            elephc_memoir::values::packed_slots::emit_packed_payload_x86_64(ctx.emitter, "r11", "rax");
            ctx.emitter.instruction("mov r8, QWORD PTR [r11]");                 // preserve the removed first heap pointer across compaction
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_shift element PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Restores the preserved AArch64 first payload into the canonical result registers.
fn emit_array_shift_restore_first_aarch64(
    ctx: &mut FunctionContext<'_>,
    elem_ty: &PhpType,
) -> Result<()> {
    match elem_ty {
        PhpType::Int
        | PhpType::Bool
        | PhpType::Callable
        | PhpType::Mixed
        | PhpType::Pointer(_)
        | PhpType::Buffer(_) => {
            ctx.emitter.instruction("mov x0, x11");                             // restore the removed pointer-sized payload into the result register
        }
        PhpType::Float => {
            ctx.emitter.instruction("fmov d0, d1");                             // restore the removed float payload into the result register
        }
        PhpType::Str => {
            ctx.emitter.instruction("mov x1, x11");                             // restore the removed string pointer into the string result register
            ctx.emitter.instruction("mov x2, x12");                             // restore the removed string length into the string result register
        }
        PhpType::Void | PhpType::Never => {
            ctx.emitter.instruction("mov x0, #0");                              // materialize a null payload for impossible void-array live elements
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction("mov x0, x11");                             // restore the removed heap pointer into the result register
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_shift element PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Restores the preserved x86_64 first payload into the canonical result registers.
fn emit_array_shift_restore_first_x86_64(
    ctx: &mut FunctionContext<'_>,
    elem_ty: &PhpType,
) -> Result<()> {
    match elem_ty {
        PhpType::Int
        | PhpType::Bool
        | PhpType::Callable
        | PhpType::Mixed
        | PhpType::Pointer(_)
        | PhpType::Buffer(_) => {
            ctx.emitter.instruction("mov rax, r8");                             // restore the removed pointer-sized payload into the result register
        }
        PhpType::Float => {
            ctx.emitter.instruction("movsd xmm0, xmm1");                        // restore the removed float payload into the result register
        }
        PhpType::Str => {
            ctx.emitter.instruction("mov rax, r8");                             // restore the removed string pointer into the string result register
            ctx.emitter.instruction("mov rdx, r9");                             // restore the removed string length into the string result register
        }
        PhpType::Void | PhpType::Never => {
            ctx.emitter.instruction("xor eax, eax");                            // materialize a null payload for impossible void-array live elements
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction("mov rax, r8");                             // restore the removed heap pointer into the result register
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_shift element PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Boxes PHP null for an empty `array_shift()` result.
fn emit_array_shift_null(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
}
