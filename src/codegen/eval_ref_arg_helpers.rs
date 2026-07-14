//! Purpose:
//! Shares the by-reference argument bridge helpers used by eval-dispatched AOT
//! methods and constructors.
//!
//! Called from:
//! - `crate::codegen::eval_method_helpers`
//! - `crate::codegen::eval_constructor_helpers`
//! - `crate::codegen::lower_inst::builtins::eval`
//!
//! Key details:
//! - The generated eval bridge writes back through the original eval `Mixed`
//!   cells after native AOT methods mutate by-reference argument storage.
//! - Boxed `Mixed`/union references use a pointer slot; supported typed scalar,
//!   string, array, iterable, and object references use raw ABI storage that is
//!   boxed again during writeback.

use crate::codegen::emit::Emitter;
use crate::codegen::{abi, emit_box_current_value_as_mixed, runtime_value_tag};
use crate::types::{FunctionSig, PhpType};

/// Describes the stack storage for one eval-supplied by-reference argument.
#[derive(Clone)]
pub(crate) struct EvalRefArgSlot {
    pub(crate) param_index: usize,
    pub(crate) param_ty: PhpType,
    pub(crate) raw_offset: usize,
    pub(crate) original_offset: usize,
    pub(crate) raw_refcounted_owned: bool,
}

const EVAL_REF_ARG_BYTES: usize = 32;

/// Returns true when an eval bridge by-reference parameter can be staged safely.
pub(crate) fn eval_ref_param_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Mixed
            | PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Iterable
            | PhpType::Object(_)
            | PhpType::TaggedScalar
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
    raw_refcounted_owned: bool,
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
            raw_refcounted_owned,
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

/// Boxes one ARM64 typed raw ref slot and replaces the original eval Mixed cell.
fn emit_aarch64_write_back_typed_ref_arg(
    emitter: &mut Emitter,
    slot: &EvalRefArgSlot,
    stack_offset: usize,
    label_prefix: &str,
) {
    if typed_ref_arg_is_refcounted_heap(&slot.param_ty) {
        emit_aarch64_write_back_refcounted_typed_ref_arg(
            emitter,
            slot,
            stack_offset,
            label_prefix,
        );
        return;
    }
    let done_label = format!("{}_ref_{}_typed_done", label_prefix, slot.param_index);
    emit_aarch64_skip_unchanged_typed_ref_arg(emitter, slot, stack_offset, &done_label);
    emit_aarch64_load_typed_ref_slot(emitter, &slot.param_ty, stack_offset + slot.raw_offset);
    emit_box_current_value_as_mixed(emitter, &slot.param_ty);
    emitter.instruction("mov x10, x0");                                         // keep the newly boxed ref value available for cell replacement
    abi::emit_load_temporary_stack_slot(emitter, "x9", stack_offset + slot.original_offset);
    emit_aarch64_replace_mixed_cell(emitter, label_prefix, slot.param_index, "x9", "x10");
    emit_aarch64_release_typed_ref_slot(emitter, slot, stack_offset, label_prefix);
    emitter.label(&done_label);
}

/// Boxes one x86_64 typed raw ref slot and replaces the original eval Mixed cell.
fn emit_x86_64_write_back_typed_ref_arg(
    emitter: &mut Emitter,
    slot: &EvalRefArgSlot,
    stack_offset: usize,
    label_prefix: &str,
) {
    if typed_ref_arg_is_refcounted_heap(&slot.param_ty) {
        emit_x86_64_write_back_refcounted_typed_ref_arg(
            emitter,
            slot,
            stack_offset,
            label_prefix,
        );
        return;
    }
    let done_label = format!("{}_ref_{}_typed_done_x", label_prefix, slot.param_index);
    emit_x86_64_skip_unchanged_typed_ref_arg(emitter, slot, stack_offset, &done_label);
    emit_x86_64_load_typed_ref_slot(emitter, &slot.param_ty, stack_offset + slot.raw_offset);
    emit_box_current_value_as_mixed(emitter, &slot.param_ty);
    emitter.instruction("mov r11, rax");                                        // keep the newly boxed ref value available for cell replacement
    abi::emit_load_temporary_stack_slot(emitter, "r10", stack_offset + slot.original_offset);
    emit_x86_64_replace_mixed_cell(emitter, label_prefix, slot.param_index, "r10", "r11");
    emit_x86_64_release_typed_ref_slot(emitter, slot, stack_offset, label_prefix);
    emitter.label(&done_label);
}

/// Writes one ARM64 refcounted raw ref slot back with borrowed/owned slot semantics.
fn emit_aarch64_write_back_refcounted_typed_ref_arg(
    emitter: &mut Emitter,
    slot: &EvalRefArgSlot,
    stack_offset: usize,
    label_prefix: &str,
) {
    let changed_label = format!(
        "{}_ref_{}_typed_changed",
        label_prefix, slot.param_index
    );
    let unchanged_label = format!(
        "{}_ref_{}_typed_unchanged",
        label_prefix, slot.param_index
    );
    let done_label = format!("{}_ref_{}_typed_done", label_prefix, slot.param_index);
    emit_aarch64_branch_on_refcounted_raw_slot_change(
        emitter,
        slot,
        stack_offset,
        &changed_label,
        &unchanged_label,
    );
    emitter.label(&changed_label);
    emit_aarch64_load_typed_ref_slot(emitter, &slot.param_ty, stack_offset + slot.raw_offset);
    emit_box_current_value_as_mixed(emitter, &slot.param_ty);
    emitter.instruction("mov x10, x0");                                         // keep the boxed replacement while updating the eval ref cell
    abi::emit_load_temporary_stack_slot(emitter, "x9", stack_offset + slot.original_offset);
    if slot.raw_refcounted_owned {
        emit_aarch64_replace_mixed_cell(emitter, label_prefix, slot.param_index, "x9", "x10");
    } else {
        emit_aarch64_replace_mixed_cell_without_releasing_old(emitter, "x9", "x10");
    }
    emit_aarch64_release_refcounted_raw_slot_value(emitter, slot, stack_offset);
    emitter.instruction(&format!("b {}", done_label));                          // finish after transferring the changed raw ref payload
    emitter.label(&unchanged_label);
    if slot.raw_refcounted_owned {
        emit_aarch64_release_refcounted_raw_slot_value(emitter, slot, stack_offset);
    }
    emitter.label(&done_label);
}

/// Writes one x86_64 refcounted raw ref slot back with borrowed/owned slot semantics.
fn emit_x86_64_write_back_refcounted_typed_ref_arg(
    emitter: &mut Emitter,
    slot: &EvalRefArgSlot,
    stack_offset: usize,
    label_prefix: &str,
) {
    let changed_label = format!(
        "{}_ref_{}_typed_changed_x",
        label_prefix, slot.param_index
    );
    let unchanged_label = format!(
        "{}_ref_{}_typed_unchanged_x",
        label_prefix, slot.param_index
    );
    let done_label = format!("{}_ref_{}_typed_done_x", label_prefix, slot.param_index);
    emit_x86_64_branch_on_refcounted_raw_slot_change(
        emitter,
        slot,
        stack_offset,
        &changed_label,
        &unchanged_label,
    );
    emitter.label(&changed_label);
    emit_x86_64_load_typed_ref_slot(emitter, &slot.param_ty, stack_offset + slot.raw_offset);
    emit_box_current_value_as_mixed(emitter, &slot.param_ty);
    emitter.instruction("mov r11, rax");                                        // keep the boxed replacement while updating the eval ref cell
    abi::emit_load_temporary_stack_slot(emitter, "r10", stack_offset + slot.original_offset);
    if slot.raw_refcounted_owned {
        emit_x86_64_replace_mixed_cell(emitter, label_prefix, slot.param_index, "r10", "r11");
    } else {
        emit_x86_64_replace_mixed_cell_without_releasing_old(emitter, "r10", "r11");
    }
    emit_x86_64_release_refcounted_raw_slot_value(emitter, slot, stack_offset);
    emitter.instruction(&format!("jmp {}", done_label));                        // finish after transferring the changed raw ref payload
    emitter.label(&unchanged_label);
    if slot.raw_refcounted_owned {
        emit_x86_64_release_refcounted_raw_slot_value(emitter, slot, stack_offset);
    }
    emitter.label(&done_label);
}

/// Branches on whether an ARM64 refcounted raw ref slot differs from the eval cell.
fn emit_aarch64_branch_on_refcounted_raw_slot_change(
    emitter: &mut Emitter,
    slot: &EvalRefArgSlot,
    stack_offset: usize,
    changed_label: &str,
    unchanged_label: &str,
) {
    abi::emit_load_temporary_stack_slot(emitter, "x9", stack_offset + slot.original_offset);
    abi::emit_load_temporary_stack_slot(emitter, "x10", stack_offset + slot.raw_offset);
    emitter.instruction("ldr x11, [x9, #8]");                                   // load the eval cell's current refcounted payload pointer
    emitter.instruction("cmp x10, x11");                                        // did the native by-ref call replace the payload pointer?
    emitter.instruction(&format!("b.ne {}", changed_label));                    // changed raw slots need boxing and eval-cell replacement
    emitter.instruction(&format!("b {}", unchanged_label));                     // unchanged raw slots keep the existing eval cell payload
}

/// Branches on whether an x86_64 refcounted raw ref slot differs from the eval cell.
fn emit_x86_64_branch_on_refcounted_raw_slot_change(
    emitter: &mut Emitter,
    slot: &EvalRefArgSlot,
    stack_offset: usize,
    changed_label: &str,
    unchanged_label: &str,
) {
    abi::emit_load_temporary_stack_slot(emitter, "r10", stack_offset + slot.original_offset);
    abi::emit_load_temporary_stack_slot(emitter, "r11", stack_offset + slot.raw_offset);
    emitter.instruction("mov r9, QWORD PTR [r10 + 8]");                         // load the eval cell's current refcounted payload pointer
    emitter.instruction("cmp r11, r9");                                         // did the native by-ref call replace the payload pointer?
    emitter.instruction(&format!("jne {}", changed_label));                     // changed raw slots need boxing and eval-cell replacement
    emitter.instruction(&format!("jmp {}", unchanged_label));                   // unchanged raw slots keep the existing eval cell payload
}

/// Releases the current ARM64 refcounted raw slot value.
fn emit_aarch64_release_refcounted_raw_slot_value(
    emitter: &mut Emitter,
    slot: &EvalRefArgSlot,
    stack_offset: usize,
) {
    abi::emit_load_temporary_stack_slot(emitter, "x0", stack_offset + slot.raw_offset);
    abi::emit_decref_if_refcounted(emitter, &slot.param_ty.codegen_repr());
}

/// Releases the current x86_64 refcounted raw slot value.
fn emit_x86_64_release_refcounted_raw_slot_value(
    emitter: &mut Emitter,
    slot: &EvalRefArgSlot,
    stack_offset: usize,
) {
    abi::emit_load_temporary_stack_slot(emitter, "rax", stack_offset + slot.raw_offset);
    abi::emit_decref_if_refcounted(emitter, &slot.param_ty.codegen_repr());
}

/// Skips ARM64 typed writeback when the raw slot still matches the original Mixed payload.
fn emit_aarch64_skip_unchanged_typed_ref_arg(
    emitter: &mut Emitter,
    slot: &EvalRefArgSlot,
    stack_offset: usize,
    done_label: &str,
) {
    let Some(expected_tag) = typed_ref_arg_unchanged_runtime_tag(&slot.param_ty) else {
        return;
    };
    let changed_label = format!("{}_changed", done_label);
    abi::emit_load_temporary_stack_slot(emitter, "x9", stack_offset + slot.original_offset);
    abi::emit_load_temporary_stack_slot(emitter, "x10", stack_offset + slot.raw_offset);
    emitter.instruction("ldr x11, [x9]");                                       // load the original eval cell runtime tag before skipping writeback
    emitter.instruction(&format!("cmp x11, #{}", expected_tag));                // only skip when the eval cell already has the target scalar tag
    emitter.instruction(&format!("b.ne {}", changed_label));                    // coerce or rewrite cells whose original tag differs
    emitter.instruction("ldr x11, [x9, #8]");                                   // load the original eval cell scalar payload word
    emitter.instruction("cmp x10, x11");                                        // did the native call leave the raw ref slot unchanged?
    emitter.instruction(&format!("b.eq {}", done_label));                       // keep the existing Mixed cell when no replacement is needed
    emitter.label(&changed_label);
}

/// Skips x86_64 typed writeback when the raw slot still matches the original Mixed payload.
fn emit_x86_64_skip_unchanged_typed_ref_arg(
    emitter: &mut Emitter,
    slot: &EvalRefArgSlot,
    stack_offset: usize,
    done_label: &str,
) {
    let Some(expected_tag) = typed_ref_arg_unchanged_runtime_tag(&slot.param_ty) else {
        return;
    };
    let changed_label = format!("{}_changed", done_label);
    abi::emit_load_temporary_stack_slot(emitter, "r10", stack_offset + slot.original_offset);
    abi::emit_load_temporary_stack_slot(emitter, "r11", stack_offset + slot.raw_offset);
    emitter.instruction("mov r9, QWORD PTR [r10]");                             // load the original eval cell runtime tag before skipping writeback
    emitter.instruction(&format!("cmp r9, {}", expected_tag));                  // only skip when the eval cell already has the target scalar tag
    emitter.instruction(&format!("jne {}", changed_label));                     // coerce or rewrite cells whose original tag differs
    emitter.instruction("mov r9, QWORD PTR [r10 + 8]");                         // load the original eval cell scalar payload word
    emitter.instruction("cmp r11, r9");                                         // did the native call leave the raw ref slot unchanged?
    emitter.instruction(&format!("je {}", done_label));                         // keep the existing Mixed cell when no replacement is needed
    emitter.label(&changed_label);
}

/// Returns the runtime scalar tag that can skip writeback on exact tag and payload match.
fn typed_ref_arg_unchanged_runtime_tag(ty: &PhpType) -> Option<u8> {
    match ty.codegen_repr() {
        ty @ (PhpType::Int | PhpType::Bool | PhpType::Float) => Some(runtime_value_tag(&ty)),
        _ => None,
    }
}

/// Returns true when a typed raw ref slot stores a refcounted heap payload pointer.
fn typed_ref_arg_is_refcounted_heap(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable | PhpType::Object(_)
    )
}

/// Loads one ARM64 typed raw ref slot into the canonical result registers.
fn emit_aarch64_load_typed_ref_slot(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty.codegen_repr() {
        PhpType::Str => {
            abi::emit_load_temporary_stack_slot(emitter, "x1", offset);
            abi::emit_load_temporary_stack_slot(emitter, "x2", offset + 8);
        }
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
        PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Iterable
        | PhpType::Object(_) => {
            abi::emit_load_temporary_stack_slot(emitter, "x0", offset);
        }
        _ => {}
    }
}

/// Loads one x86_64 typed raw ref slot into the canonical result registers.
fn emit_x86_64_load_typed_ref_slot(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty.codegen_repr() {
        PhpType::Str => {
            abi::emit_load_temporary_stack_slot(emitter, "rax", offset);
            abi::emit_load_temporary_stack_slot(emitter, "rdx", offset + 8);
        }
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
        PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Iterable
        | PhpType::Object(_) => {
            abi::emit_load_temporary_stack_slot(emitter, "rax", offset);
        }
        _ => {}
    }
}

/// Releases any owned ARM64 payload left in one typed raw ref slot after writeback.
fn emit_aarch64_release_typed_ref_slot(
    emitter: &mut Emitter,
    slot: &EvalRefArgSlot,
    stack_offset: usize,
    label_prefix: &str,
) {
    let raw_offset = stack_offset + slot.raw_offset;
    match slot.param_ty.codegen_repr() {
        PhpType::Str => {
            abi::emit_load_temporary_stack_slot(emitter, "x0", raw_offset);
            abi::emit_call_label(emitter, "__rt_heap_free_safe");
        }
        ty @ (PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Iterable
            | PhpType::Object(_)) => {
            emit_aarch64_release_refcounted_raw_slot(emitter, slot, stack_offset, label_prefix, &ty);
        }
        _ => {}
    }
}

/// Releases any owned x86_64 payload left in one typed raw ref slot after writeback.
fn emit_x86_64_release_typed_ref_slot(
    emitter: &mut Emitter,
    slot: &EvalRefArgSlot,
    stack_offset: usize,
    label_prefix: &str,
) {
    let raw_offset = stack_offset + slot.raw_offset;
    match slot.param_ty.codegen_repr() {
        PhpType::Str => {
            abi::emit_load_temporary_stack_slot(emitter, "rax", raw_offset);
            abi::emit_call_label(emitter, "__rt_heap_free_safe");
        }
        ty @ (PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Iterable
            | PhpType::Object(_)) => {
            emit_x86_64_release_refcounted_raw_slot(emitter, slot, stack_offset, label_prefix, &ty);
        }
        _ => {}
    }
}

/// Releases an ARM64 raw refcounted slot when it owns a value not retained by the eval cell.
fn emit_aarch64_release_refcounted_raw_slot(
    emitter: &mut Emitter,
    slot: &EvalRefArgSlot,
    stack_offset: usize,
    label_prefix: &str,
    ty: &PhpType,
) {
    let release_label = format!("{}_ref_{}_release_raw", label_prefix, slot.param_index);
    let done_label = format!("{}_ref_{}_release_raw_done", label_prefix, slot.param_index);
    if !slot.raw_refcounted_owned {
        abi::emit_load_temporary_stack_slot(emitter, "x9", stack_offset + slot.original_offset);
        abi::emit_load_temporary_stack_slot(emitter, "x10", stack_offset + slot.raw_offset);
        emitter.instruction("ldr x11, [x9, #8]");                               // load the original eval cell payload for borrowed-slot comparison
        emitter.instruction("cmp x10, x11");                                    // changed raw pointers are owned by the native assignment path
        emitter.instruction(&format!("b.ne {}", release_label));                // release only raw heap values introduced by the native call
        emitter.instruction(&format!("b {}", done_label));                      // keep borrowed original payloads owned by the eval cell
    }
    emitter.label(&release_label);
    abi::emit_load_temporary_stack_slot(emitter, "x0", stack_offset + slot.raw_offset);
    abi::emit_decref_if_refcounted(emitter, ty);
    emitter.label(&done_label);
}

/// Releases an x86_64 raw refcounted slot when it owns a value not retained by the eval cell.
fn emit_x86_64_release_refcounted_raw_slot(
    emitter: &mut Emitter,
    slot: &EvalRefArgSlot,
    stack_offset: usize,
    label_prefix: &str,
    ty: &PhpType,
) {
    let release_label = format!("{}_ref_{}_release_raw_x", label_prefix, slot.param_index);
    let done_label = format!("{}_ref_{}_release_raw_done_x", label_prefix, slot.param_index);
    if !slot.raw_refcounted_owned {
        abi::emit_load_temporary_stack_slot(emitter, "r10", stack_offset + slot.original_offset);
        abi::emit_load_temporary_stack_slot(emitter, "r11", stack_offset + slot.raw_offset);
        emitter.instruction("mov r9, QWORD PTR [r10 + 8]");                     // load the original eval cell payload for borrowed-slot comparison
        emitter.instruction("cmp r11, r9");                                     // changed raw pointers are owned by the native assignment path
        emitter.instruction(&format!("jne {}", release_label));                 // release only raw heap values introduced by the native call
        emitter.instruction(&format!("jmp {}", done_label));                    // keep borrowed original payloads owned by the eval cell
    }
    emitter.label(&release_label);
    abi::emit_load_temporary_stack_slot(emitter, "rax", stack_offset + slot.raw_offset);
    abi::emit_decref_if_refcounted(emitter, ty);
    emitter.label(&done_label);
}

/// Copies a replacement ARM64 Mixed cell into an existing target cell without releasing old payload.
fn emit_aarch64_replace_mixed_cell_without_releasing_old(
    emitter: &mut Emitter,
    target_reg: &str,
    replacement_reg: &str,
) {
    abi::emit_push_reg_pair(emitter, target_reg, replacement_reg);
    emitter.instruction("ldr x9, [sp]");                                        // reload the original Mixed cell pointer for direct replacement
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the replacement Mixed cell pointer
    emitter.instruction("ldr x11, [x10]");                                      // copy the replacement runtime tag
    emitter.instruction("str x11, [x9]");                                       // overwrite the target cell tag
    emitter.instruction("ldr x11, [x10, #8]");                                  // copy the replacement low payload word
    emitter.instruction("str x11, [x9, #8]");                                   // overwrite the target cell low payload word
    emitter.instruction("ldr x11, [x10, #16]");                                 // copy the replacement high payload word
    emitter.instruction("str x11, [x9, #16]");                                  // overwrite the target cell high payload word
    emitter.instruction("mov x0, x10");                                         // pass the now-empty replacement cell storage to heap_free
    abi::emit_call_label(emitter, "__rt_heap_free");
    abi::emit_release_temporary_stack(emitter, 16);
}

/// Copies a replacement x86_64 Mixed cell into an existing target cell without releasing old payload.
fn emit_x86_64_replace_mixed_cell_without_releasing_old(
    emitter: &mut Emitter,
    target_reg: &str,
    replacement_reg: &str,
) {
    abi::emit_push_reg_pair(emitter, target_reg, replacement_reg);
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // reload the original Mixed cell pointer for direct replacement
    emitter.instruction("mov r11, QWORD PTR [rsp + 8]");                        // reload the replacement Mixed cell pointer
    emitter.instruction("mov r9, QWORD PTR [r11]");                             // copy the replacement runtime tag
    emitter.instruction("mov QWORD PTR [r10], r9");                             // overwrite the target cell tag
    emitter.instruction("mov r9, QWORD PTR [r11 + 8]");                         // copy the replacement low payload word
    emitter.instruction("mov QWORD PTR [r10 + 8], r9");                         // overwrite the target cell low payload word
    emitter.instruction("mov r9, QWORD PTR [r11 + 16]");                        // copy the replacement high payload word
    emitter.instruction("mov QWORD PTR [r10 + 16], r9");                        // overwrite the target cell high payload word
    emitter.instruction("mov rax, r11");                                        // pass the now-empty replacement cell storage to heap_free
    abi::emit_call_label(emitter, "__rt_heap_free");
    abi::emit_release_temporary_stack(emitter, 16);
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
