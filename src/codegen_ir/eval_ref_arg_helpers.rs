//! Purpose:
//! Shares the by-reference argument bridge helpers used by eval-dispatched AOT
//! methods and constructors.
//!
//! Called from:
//! - `crate::codegen_ir::eval_method_helpers`
//! - `crate::codegen_ir::eval_constructor_helpers`
//! - `crate::codegen_ir::lower_inst::builtins::eval`
//!
//! Key details:
//! - The generated eval bridge writes back through the original eval `Mixed`
//!   cells after native AOT methods mutate by-reference argument storage.
//! - Boxed `Mixed`/union references use a pointer slot; supported typed scalar
//!   references use raw ABI storage that is boxed again during writeback.

use crate::codegen::emit::Emitter;
use crate::codegen::{abi, emit_box_current_value_as_mixed};
use crate::types::{FunctionSig, PhpType};

/// Describes the stack storage for one eval-supplied by-reference argument.
#[derive(Clone)]
pub(crate) struct EvalRefArgSlot {
    pub(crate) param_index: usize,
    pub(crate) param_ty: PhpType,
    pub(crate) raw_offset: usize,
    pub(crate) original_offset: usize,
}

const EVAL_REF_ARG_BYTES: usize = 32;

/// Returns true when an eval bridge by-reference parameter can be staged safely.
pub(crate) fn eval_ref_param_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Mixed | PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::TaggedScalar
    )
}

/// Returns true when every by-reference parameter in a native signature is bridgeable.
pub(crate) fn eval_signature_ref_params_supported(signature: &FunctionSig) -> bool {
    signature.params.iter().enumerate().all(|(index, (_, ty))| {
        !signature.ref_params.get(index).copied().unwrap_or(false)
            || eval_ref_param_supported(ty)
    })
}

/// Normalizes a sparse ref-parameter vector to the visible parameter count.
pub(crate) fn eval_normalized_ref_params(param_count: usize, ref_params: &[bool]) -> Vec<bool> {
    (0..param_count)
        .map(|index| ref_params.get(index).copied().unwrap_or(false))
        .collect()
}

/// Converts visible parameter types to the ABI shape used by eval bridge calls.
pub(crate) fn eval_abi_param_types_for_refs(
    param_types: &[PhpType],
    ref_params: &[bool],
) -> Vec<PhpType> {
    param_types
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            if ref_params.get(index).copied().unwrap_or(false) {
                PhpType::Int
            } else {
                ty.codegen_repr()
            }
        })
        .collect()
}

/// Plans the stack offsets for eval by-reference argument cells.
pub(crate) fn eval_ref_arg_slots(
    param_types: &[PhpType],
    ref_params: &[bool],
) -> Vec<EvalRefArgSlot> {
    let total = ref_params.iter().filter(|is_ref| **is_ref).count();
    let mut seen = 0usize;
    let mut slots = Vec::with_capacity(total);
    for (param_index, is_ref) in ref_params.iter().enumerate() {
        if !*is_ref {
            continue;
        }
        let reverse_index = total - seen - 1;
        let base_offset = reverse_index * EVAL_REF_ARG_BYTES;
        slots.push(EvalRefArgSlot {
            param_index,
            param_ty: param_types[param_index].codegen_repr(),
            raw_offset: base_offset,
            original_offset: base_offset + 16,
        });
        seen += 1;
    }
    slots
}

/// Returns the temporary outgoing-argument stack slot size for one ABI argument.
pub(crate) fn eval_arg_temp_slot_size(ty: &PhpType) -> usize {
    if matches!(ty.codegen_repr(), PhpType::Void | PhpType::Never) {
        0
    } else {
        16
    }
}

/// Writes changed ARM64 ref-argument cells back into the original eval cells.
pub(crate) fn emit_aarch64_write_back_ref_args(
    emitter: &mut Emitter,
    ref_slots: &[EvalRefArgSlot],
    stack_offset: usize,
    label_prefix: &str,
) {
    for slot in ref_slots {
        if matches!(slot.param_ty.codegen_repr(), PhpType::Mixed) {
            emit_aarch64_write_back_mixed_ref_arg(emitter, slot, stack_offset, label_prefix);
        } else {
            emit_aarch64_write_back_typed_ref_arg(emitter, slot, stack_offset, label_prefix);
        }
    }
}

/// Writes changed x86_64 ref-argument cells back into the original eval cells.
pub(crate) fn emit_x86_64_write_back_ref_args(
    emitter: &mut Emitter,
    ref_slots: &[EvalRefArgSlot],
    stack_offset: usize,
    label_prefix: &str,
) {
    for slot in ref_slots {
        if matches!(slot.param_ty.codegen_repr(), PhpType::Mixed) {
            emit_x86_64_write_back_mixed_ref_arg(emitter, slot, stack_offset, label_prefix);
        } else {
            emit_x86_64_write_back_typed_ref_arg(emitter, slot, stack_offset, label_prefix);
        }
    }
}

/// Writes one ARM64 boxed `Mixed` ref slot back when native code replaced the cell pointer.
fn emit_aarch64_write_back_mixed_ref_arg(
    emitter: &mut Emitter,
    slot: &EvalRefArgSlot,
    stack_offset: usize,
    label_prefix: &str,
) {
    let done_label = format!("{}_ref_{}_done", label_prefix, slot.param_index);
    abi::emit_load_temporary_stack_slot(emitter, "x9", stack_offset + slot.original_offset);
    abi::emit_load_temporary_stack_slot(emitter, "x10", stack_offset + slot.raw_offset);
    emitter.instruction("cmp x9, x10");                                         // skip writeback when the native call kept the same Mixed cell
    emitter.instruction(&format!("b.eq {}", done_label));                       // avoid self-copying and releasing the original cell payload
    emit_aarch64_replace_mixed_cell(emitter, label_prefix, slot.param_index, "x9", "x10");
    emitter.label(&done_label);
}

/// Writes one x86_64 boxed `Mixed` ref slot back when native code replaced the cell pointer.
fn emit_x86_64_write_back_mixed_ref_arg(
    emitter: &mut Emitter,
    slot: &EvalRefArgSlot,
    stack_offset: usize,
    label_prefix: &str,
) {
    let done_label = format!("{}_ref_{}_done_x", label_prefix, slot.param_index);
    abi::emit_load_temporary_stack_slot(emitter, "r10", stack_offset + slot.original_offset);
    abi::emit_load_temporary_stack_slot(emitter, "r11", stack_offset + slot.raw_offset);
    emitter.instruction("cmp r10, r11");                                        // skip writeback when the native call kept the same Mixed cell
    emitter.instruction(&format!("je {}", done_label));                         // avoid self-copying and releasing the original cell payload
    emit_x86_64_replace_mixed_cell(emitter, label_prefix, slot.param_index, "r10", "r11");
    emitter.label(&done_label);
}

/// Boxes one ARM64 typed scalar ref slot and replaces the original eval Mixed cell.
fn emit_aarch64_write_back_typed_ref_arg(
    emitter: &mut Emitter,
    slot: &EvalRefArgSlot,
    stack_offset: usize,
    label_prefix: &str,
) {
    emit_aarch64_load_typed_ref_slot(emitter, &slot.param_ty, stack_offset + slot.raw_offset);
    emit_box_current_value_as_mixed(emitter, &slot.param_ty);
    emitter.instruction("mov x10, x0");                                         // keep the newly boxed ref value available for cell replacement
    abi::emit_load_temporary_stack_slot(emitter, "x9", stack_offset + slot.original_offset);
    emit_aarch64_replace_mixed_cell(emitter, label_prefix, slot.param_index, "x9", "x10");
}

/// Boxes one x86_64 typed scalar ref slot and replaces the original eval Mixed cell.
fn emit_x86_64_write_back_typed_ref_arg(
    emitter: &mut Emitter,
    slot: &EvalRefArgSlot,
    stack_offset: usize,
    label_prefix: &str,
) {
    emit_x86_64_load_typed_ref_slot(emitter, &slot.param_ty, stack_offset + slot.raw_offset);
    emit_box_current_value_as_mixed(emitter, &slot.param_ty);
    emitter.instruction("mov r11, rax");                                        // keep the newly boxed ref value available for cell replacement
    abi::emit_load_temporary_stack_slot(emitter, "r10", stack_offset + slot.original_offset);
    emit_x86_64_replace_mixed_cell(emitter, label_prefix, slot.param_index, "r10", "r11");
}

/// Loads one ARM64 typed scalar ref slot into the canonical result registers.
fn emit_aarch64_load_typed_ref_slot(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_load_temporary_stack_slot(emitter, "d0", offset);
        }
        PhpType::TaggedScalar => {
            abi::emit_load_temporary_stack_slot(emitter, "x0", offset);
            abi::emit_load_temporary_stack_slot(emitter, "x1", offset + 8);
        }
        PhpType::Int | PhpType::Bool => {
            abi::emit_load_temporary_stack_slot(emitter, "x0", offset);
        }
        _ => {}
    }
}

/// Loads one x86_64 typed scalar ref slot into the canonical result registers.
fn emit_x86_64_load_typed_ref_slot(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_load_temporary_stack_slot(emitter, "xmm0", offset);
        }
        PhpType::TaggedScalar => {
            abi::emit_load_temporary_stack_slot(emitter, "rax", offset);
            abi::emit_load_temporary_stack_slot(emitter, "rdx", offset + 8);
        }
        PhpType::Int | PhpType::Bool => {
            abi::emit_load_temporary_stack_slot(emitter, "rax", offset);
        }
        _ => {}
    }
}

/// Copies one replacement ARM64 Mixed cell payload into an existing target cell.
fn emit_aarch64_replace_mixed_cell(
    emitter: &mut Emitter,
    label_prefix: &str,
    param_index: usize,
    target_reg: &str,
    replacement_reg: &str,
) {
    let release_string = format!("{}_ref_{}_release_string", label_prefix, param_index);
    let copy_new = format!("{}_ref_{}_copy_new", label_prefix, param_index);
    let done = format!("{}_ref_{}_assign_done", label_prefix, param_index);

    abi::emit_push_reg_pair(emitter, target_reg, replacement_reg);
    emitter.instruction("ldr x9, [sp]");                                        // reload the original Mixed cell pointer for old-payload inspection
    emitter.instruction("ldr x11, [x9]");                                       // inspect the old payload tag before overwriting the cell
    emitter.instruction("cmp x11, #1");                                         // strings own a persisted heap payload that needs safe free
    emitter.instruction(&format!("b.eq {}", release_string));                   // release string payloads through the string-safe free path
    emitter.instruction("cmp x11, #4");                                         // tags below array/hash/object/mixed are scalar payloads
    emitter.instruction(&format!("b.lo {}", copy_new));                         // scalar payloads can be overwritten directly
    emitter.instruction("cmp x11, #7");                                         // tags above the refcounted payload range are not released here
    emitter.instruction(&format!("b.hi {}", copy_new));                         // unknown/null payload tags can be overwritten directly
    emitter.instruction("ldr x0, [x9, #8]");                                    // pass the old refcounted child payload to the generic release helper
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.instruction(&format!("b {}", copy_new));                            // continue with replacement after releasing the old child
    emitter.label(&release_string);
    emitter.instruction("ldr x9, [sp]");                                        // reload the original Mixed cell before reading its string payload
    emitter.instruction("ldr x0, [x9, #8]");                                    // pass the old string payload pointer to the safe free helper
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    emitter.label(&copy_new);
    emitter.instruction("ldr x9, [sp]");                                        // reload the original Mixed cell pointer for replacement
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the replacement Mixed cell pointer
    emitter.instruction("ldr x11, [x10]");                                      // copy the replacement runtime tag
    emitter.instruction("str x11, [x9]");                                       // overwrite the target cell tag
    emitter.instruction("ldr x11, [x10, #8]");                                  // copy the replacement low payload word
    emitter.instruction("str x11, [x9, #8]");                                   // overwrite the target cell low payload word
    emitter.instruction("ldr x11, [x10, #16]");                                 // copy the replacement high payload word
    emitter.instruction("str x11, [x9, #16]");                                  // overwrite the target cell high payload word
    emitter.instruction("mov x0, x10");                                         // pass the now-empty replacement cell storage to heap_free
    abi::emit_call_label(emitter, "__rt_heap_free");
    emitter.label(&done);
    abi::emit_release_temporary_stack(emitter, 16);
}

/// Copies one replacement x86_64 Mixed cell payload into an existing target cell.
fn emit_x86_64_replace_mixed_cell(
    emitter: &mut Emitter,
    label_prefix: &str,
    param_index: usize,
    target_reg: &str,
    replacement_reg: &str,
) {
    let release_string = format!("{}_ref_{}_release_string_x", label_prefix, param_index);
    let copy_new = format!("{}_ref_{}_copy_new_x", label_prefix, param_index);
    let done = format!("{}_ref_{}_assign_done_x", label_prefix, param_index);

    abi::emit_push_reg_pair(emitter, target_reg, replacement_reg);
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // reload the original Mixed cell pointer for old-payload inspection
    emitter.instruction("mov r9, QWORD PTR [r10]");                             // inspect the old payload tag before overwriting the cell
    emitter.instruction("cmp r9, 1");                                           // strings own a persisted heap payload that needs safe free
    emitter.instruction(&format!("je {}", release_string));                     // release string payloads through the string-safe free path
    emitter.instruction("cmp r9, 4");                                           // tags below array/hash/object/mixed are scalar payloads
    emitter.instruction(&format!("jl {}", copy_new));                           // scalar payloads can be overwritten directly
    emitter.instruction("cmp r9, 7");                                           // tags above the refcounted payload range are not released here
    emitter.instruction(&format!("jg {}", copy_new));                           // unknown/null payload tags can be overwritten directly
    emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                        // pass the old refcounted child payload to the generic release helper
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.instruction(&format!("jmp {}", copy_new));                          // continue with replacement after releasing the old child
    emitter.label(&release_string);
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // reload the original Mixed cell before reading its string payload
    emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                        // pass the old string payload pointer to the safe free helper
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    emitter.label(&copy_new);
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // reload the original Mixed cell pointer for replacement
    emitter.instruction("mov r11, QWORD PTR [rsp + 8]");                        // reload the replacement Mixed cell pointer
    emitter.instruction("mov r9, QWORD PTR [r11]");                             // copy the replacement runtime tag
    emitter.instruction("mov QWORD PTR [r10], r9");                             // overwrite the target cell tag
    emitter.instruction("mov r9, QWORD PTR [r11 + 8]");                         // copy the replacement low payload word
    emitter.instruction("mov QWORD PTR [r10 + 8], r9");                         // overwrite the target cell low payload word
    emitter.instruction("mov r9, QWORD PTR [r11 + 16]");                        // copy the replacement high payload word
    emitter.instruction("mov QWORD PTR [r10 + 16], r9");                        // overwrite the target cell high payload word
    emitter.instruction("mov rax, r11");                                        // pass the now-empty replacement cell storage to heap_free
    abi::emit_call_label(emitter, "__rt_heap_free");
    emitter.label(&done);
    abi::emit_release_temporary_stack(emitter, 16);
}
