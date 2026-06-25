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
//! - The generated eval bridge can invoke by-reference AOT parameters only when
//!   the parameter storage is already a boxed `Mixed` cell.
//! - Typed by-reference bridge dispatch stays disabled until raw typed temp
//!   writeback is implemented for each supported ABI shape.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::types::{FunctionSig, PhpType};

/// Describes the stack storage for one eval-supplied by-reference `Mixed` argument.
#[derive(Clone)]
pub(crate) struct EvalMixedRefArgSlot {
    pub(crate) param_index: usize,
    pub(crate) raw_offset: usize,
    pub(crate) original_offset: usize,
}

const EVAL_MIXED_REF_ARG_BYTES: usize = 32;

/// Returns true when an eval bridge by-reference parameter can use Mixed-cell storage.
pub(crate) fn eval_mixed_ref_param_supported(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
}

/// Returns true when every by-reference parameter in a native signature is bridgeable.
pub(crate) fn eval_signature_mixed_ref_params_supported(signature: &FunctionSig) -> bool {
    signature.params.iter().enumerate().all(|(index, (_, ty))| {
        !signature.ref_params.get(index).copied().unwrap_or(false)
            || eval_mixed_ref_param_supported(ty)
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

/// Plans the stack offsets for eval `Mixed` by-reference argument cells.
pub(crate) fn eval_mixed_ref_arg_slots(ref_params: &[bool]) -> Vec<EvalMixedRefArgSlot> {
    let total = ref_params.iter().filter(|is_ref| **is_ref).count();
    let mut seen = 0usize;
    let mut slots = Vec::with_capacity(total);
    for (param_index, is_ref) in ref_params.iter().enumerate() {
        if !*is_ref {
            continue;
        }
        let reverse_index = total - seen - 1;
        let base_offset = reverse_index * EVAL_MIXED_REF_ARG_BYTES;
        slots.push(EvalMixedRefArgSlot {
            param_index,
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

/// Writes changed ARM64 `Mixed` ref-argument cells back into the original eval cells.
pub(crate) fn emit_aarch64_write_back_mixed_ref_args(
    emitter: &mut Emitter,
    ref_slots: &[EvalMixedRefArgSlot],
    stack_offset: usize,
    label_prefix: &str,
) {
    for slot in ref_slots {
        let done_label = format!("{}_ref_{}_done", label_prefix, slot.param_index);
        abi::emit_load_temporary_stack_slot(emitter, "x9", stack_offset + slot.original_offset);
        abi::emit_load_temporary_stack_slot(emitter, "x10", stack_offset + slot.raw_offset);
        emitter.instruction("cmp x9, x10");                                     // skip writeback when the native call kept the same Mixed cell
        emitter.instruction(&format!("b.eq {}", done_label));                   // avoid self-copying and releasing the original cell payload
        emit_aarch64_replace_mixed_cell(emitter, label_prefix, slot.param_index, "x9", "x10");
        emitter.label(&done_label);
    }
}

/// Writes changed x86_64 `Mixed` ref-argument cells back into the original eval cells.
pub(crate) fn emit_x86_64_write_back_mixed_ref_args(
    emitter: &mut Emitter,
    ref_slots: &[EvalMixedRefArgSlot],
    stack_offset: usize,
    label_prefix: &str,
) {
    for slot in ref_slots {
        let done_label = format!("{}_ref_{}_done_x", label_prefix, slot.param_index);
        abi::emit_load_temporary_stack_slot(emitter, "r10", stack_offset + slot.original_offset);
        abi::emit_load_temporary_stack_slot(emitter, "r11", stack_offset + slot.raw_offset);
        emitter.instruction("cmp r10, r11");                                    // skip writeback when the native call kept the same Mixed cell
        emitter.instruction(&format!("je {}", done_label));                     // avoid self-copying and releasing the original cell payload
        emit_x86_64_replace_mixed_cell(emitter, label_prefix, slot.param_index, "r10", "r11");
        emitter.label(&done_label);
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
