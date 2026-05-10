//! Purpose:
//! Plans and materializes outgoing call arguments into target registers and stack spill slots.
//! Copies preevaluated argument values from temporary storage into ABI-visible locations.
//!
//! Called from:
//! - `crate::codegen::expr::calls::args` and wrapper emitters
//!
//! Key details:
//! - Stack reservation and register ordering must preserve live values while matching target ABI limits.

use crate::codegen::{
    emit::Emitter,
    platform::{Arch, Target},
};
use crate::types::PhpType;

use super::super::frame::emit_adjust_sp;
use super::super::registers::{
    OutgoingArgAssignment, STACK_ARG_SENTINEL, float_arg_reg_limit, float_arg_reg_name,
    int_arg_reg_limit, int_arg_reg_name, secondary_scratch_reg, tertiary_scratch_reg,
};
use super::stack::{emit_load_temporary_stack_slot, emit_store_to_sp};

pub fn build_outgoing_arg_assignments_for_target(
    target: Target,
    arg_types: &[PhpType],
    initial_int_reg_idx: usize,
) -> Vec<OutgoingArgAssignment> {
    let mut assignments = Vec::new();
    let mut int_reg_idx = initial_int_reg_idx;
    let mut float_reg_idx = 0usize;
    let mut int_stack_only = initial_int_reg_idx >= int_arg_reg_limit(target);
    let mut float_stack_only = false;

    for ty in arg_types {
        if ty.is_float_reg() {
            if !float_stack_only && float_reg_idx < float_arg_reg_limit(target) {
                assignments.push(OutgoingArgAssignment {
                    ty: ty.clone(),
                    start_reg: float_reg_idx,
                    is_float: true,
                });
                float_reg_idx += 1;
            } else {
                assignments.push(OutgoingArgAssignment {
                    ty: ty.clone(),
                    start_reg: STACK_ARG_SENTINEL,
                    is_float: true,
                });
                float_stack_only = true;
            }
        } else {
            let reg_count = ty.register_count();
            if !int_stack_only && int_reg_idx + reg_count <= int_arg_reg_limit(target) {
                assignments.push(OutgoingArgAssignment {
                    ty: ty.clone(),
                    start_reg: int_reg_idx,
                    is_float: false,
                });
                int_reg_idx += reg_count;
            } else {
                assignments.push(OutgoingArgAssignment {
                    ty: ty.clone(),
                    start_reg: STACK_ARG_SENTINEL,
                    is_float: false,
                });
                int_stack_only = true;
            }
        }
    }

    assignments
}

fn arg_slot_size(ty: &PhpType) -> usize {
    match ty {
        PhpType::Void => 0,
        _ => 16,
    }
}

fn emit_copy_stack_arg_slot(
    emitter: &mut Emitter,
    ty: &PhpType,
    src_offset: usize,
    dst_offset: usize,
) {
    let int_reg = secondary_scratch_reg(emitter);
    let int_hi_reg = match emitter.target.arch {
        Arch::AArch64 => tertiary_scratch_reg(emitter),
        Arch::X86_64 => "r11",
    };
    let float_reg = match emitter.target.arch {
        Arch::AArch64 => "d15",
        Arch::X86_64 => "xmm15",
    };
    match ty {
        PhpType::Float => {
            emit_load_temporary_stack_slot(emitter, float_reg, src_offset);
            emit_store_to_sp(emitter, float_reg, dst_offset);
        }
        PhpType::Str => {
            emit_load_temporary_stack_slot(emitter, int_reg, src_offset);
            emit_load_temporary_stack_slot(emitter, int_hi_reg, src_offset + 8);
            emit_store_to_sp(emitter, int_reg, dst_offset);
            emit_store_to_sp(emitter, int_hi_reg, dst_offset + 8);
        }
        PhpType::Void => {}
        _ => {
            emit_load_temporary_stack_slot(emitter, int_reg, src_offset);
            emit_store_to_sp(emitter, int_reg, dst_offset);
        }
    }
}

pub fn materialize_outgoing_args(
    emitter: &mut Emitter,
    assignments: &[OutgoingArgAssignment],
) -> usize {
    let slot_sizes: Vec<usize> = assignments
        .iter()
        .map(|assignment| arg_slot_size(&assignment.ty))
        .collect();
    let total_temp_bytes: usize = slot_sizes.iter().sum();
    let mut temp_offsets = vec![0usize; assignments.len()];
    let mut running_offset = 0usize;
    for i in (0..assignments.len()).rev() {
        temp_offsets[i] = running_offset;
        running_offset += slot_sizes[i];
    }

    let overflow_indices: Vec<usize> = assignments
        .iter()
        .enumerate()
        .filter_map(|(idx, assignment)| (!assignment.in_register()).then_some(idx))
        .collect();
    let overflow_bytes: usize = overflow_indices.iter().map(|idx| slot_sizes[*idx]).sum();

    if overflow_bytes > 0 {
        emit_adjust_sp(emitter, overflow_bytes, true);
    }

    let base_shift = overflow_bytes;
    for (i, assignment) in assignments.iter().enumerate() {
        if !assignment.in_register() {
            continue;
        }
        let src_offset = base_shift + temp_offsets[i];
        match &assignment.ty {
            PhpType::Bool
            | PhpType::Int
            | PhpType::Resource(_)
            | PhpType::Iterable
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Buffer(_)
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Packed(_)
            | PhpType::Pointer(_) => {
                emit_load_temporary_stack_slot(
                    emitter,
                    int_arg_reg_name(emitter.target, assignment.start_reg),
                    src_offset,
                );
            }
            PhpType::Float => {
                emit_load_temporary_stack_slot(
                    emitter,
                    float_arg_reg_name(emitter.target, assignment.start_reg),
                    src_offset,
                );
            }
            PhpType::Str => {
                emit_load_temporary_stack_slot(
                    emitter,
                    int_arg_reg_name(emitter.target, assignment.start_reg),
                    src_offset,
                );
                emit_load_temporary_stack_slot(
                    emitter,
                    int_arg_reg_name(emitter.target, assignment.start_reg + 1),
                    src_offset + 8,
                );
            }
            PhpType::Void | PhpType::Never => {}
        }
    }

    if overflow_bytes > 0 {
        let mut dst_offset = total_temp_bytes;
        for idx in &overflow_indices {
            let src_offset = overflow_bytes + temp_offsets[*idx];
            emit_copy_stack_arg_slot(emitter, &assignments[*idx].ty, src_offset, dst_offset);
            dst_offset += slot_sizes[*idx];
        }
    }

    if total_temp_bytes > 0 {
        emit_adjust_sp(emitter, total_temp_bytes, false);
    }

    overflow_bytes
}
