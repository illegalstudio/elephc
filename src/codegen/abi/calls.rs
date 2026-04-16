use crate::codegen::{emit::Emitter, platform::{Arch, Target}};
use crate::types::PhpType;

use super::frame::{emit_adjust_sp, emit_sp_address, load_from_caller_stack, store_at_offset};
use super::registers::{
    float_arg_reg_limit, float_arg_reg_name, float_result_reg, int_arg_reg_limit,
    int_arg_reg_name, int_result_reg, secondary_scratch_reg, string_result_regs,
    tertiary_scratch_reg, IncomingArgCursor, OutgoingArgAssignment, STACK_ARG_SENTINEL,
};

pub fn emit_call_label(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("bl {}", label));                      // branch-and-link to the named direct-call target
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("call {}", label));                    // call the named direct-call target through the native x86_64 instruction
        }
    }
}

pub fn emit_call_reg(emitter: &mut Emitter, reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("blr {}", reg));                       // branch to the indirect-call target held in the requested register
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("call {}", reg));                      // call the indirect target held in the requested register
        }
    }
}

pub fn emit_store_incoming_param(
    emitter: &mut Emitter,
    name: &str,
    ty: &PhpType,
    offset: usize,
    is_ref: bool,
    cursor: &mut IncomingArgCursor,
) {
    let ty = ty.codegen_repr();
    let float_spill_reg = match emitter.target.arch {
        Arch::AArch64 => "d15",
        Arch::X86_64 => "xmm15",
    };
    let int_spill_reg = secondary_scratch_reg(emitter);
    let int_hi_spill_reg = tertiary_scratch_reg(emitter);
    let int_reg_limit = int_arg_reg_limit(emitter.target);
    let float_reg_limit = float_arg_reg_limit(emitter.target);

    if is_ref {
        if !cursor.int_stack_only && cursor.int_reg_idx < int_reg_limit {
            let reg = int_arg_reg_name(emitter.target, cursor.int_reg_idx);
            emitter.comment(&format!("param &${} from {} (ref)", name, reg));
            store_at_offset(emitter, reg, offset);                                     // save the by-reference address from the incoming integer argument register
            cursor.int_reg_idx += 1;
        } else {
            emitter.comment(&format!(
                "param &${} from caller stack +{}",
                name,
                cursor.caller_stack_offset
            ));
            load_from_caller_stack(emitter, int_spill_reg, cursor.caller_stack_offset);
            store_at_offset(emitter, int_spill_reg, offset);                           // save the spilled by-reference address into the local param slot
            cursor.caller_stack_offset += 16;
            cursor.int_stack_only = true;
        }
        return;
    }

    match ty {
        PhpType::Bool | PhpType::Int => {
            if !cursor.int_stack_only && cursor.int_reg_idx < int_reg_limit {
                let reg = int_arg_reg_name(emitter.target, cursor.int_reg_idx);
                emitter.comment(&format!("param ${} from {}", name, reg));
                store_at_offset(emitter, reg, offset);                                 // save the scalar parameter from the incoming integer argument register
                cursor.int_reg_idx += 1;
            } else {
                emitter.comment(&format!(
                    "param ${} from caller stack +{}",
                    name,
                    cursor.caller_stack_offset
                ));
                load_from_caller_stack(emitter, int_spill_reg, cursor.caller_stack_offset);
                store_at_offset(emitter, int_spill_reg, offset);                       // save the spilled scalar parameter into the local param slot
                cursor.caller_stack_offset += 16;
                cursor.int_stack_only = true;
            }
        }
        PhpType::Float => {
            if !cursor.float_stack_only && cursor.float_reg_idx < float_reg_limit {
                let reg = float_arg_reg_name(emitter.target, cursor.float_reg_idx);
                emitter.comment(&format!("param ${} from {}", name, reg));
                store_at_offset(emitter, reg, offset);                                 // save the float parameter from the incoming floating-point argument register
                cursor.float_reg_idx += 1;
            } else {
                emitter.comment(&format!(
                    "param ${} from caller stack +{}",
                    name,
                    cursor.caller_stack_offset
                ));
                load_from_caller_stack(emitter, float_spill_reg, cursor.caller_stack_offset);
                store_at_offset(emitter, float_spill_reg, offset);                     // save the spilled float parameter into the local param slot
                cursor.caller_stack_offset += 16;
                cursor.float_stack_only = true;
            }
        }
        PhpType::Str => {
            if !cursor.int_stack_only && cursor.int_reg_idx + 1 < int_reg_limit {
                let ptr_reg = int_arg_reg_name(emitter.target, cursor.int_reg_idx);
                let len_reg = int_arg_reg_name(emitter.target, cursor.int_reg_idx + 1);
                emitter.comment(&format!(
                    "param ${} from {},{}",
                    name, ptr_reg, len_reg
                ));
                store_at_offset(emitter, ptr_reg, offset);                             // save the string pointer from the incoming integer-register pair
                store_at_offset(emitter, len_reg, offset - 8);                         // save the string length from the incoming integer-register pair
                cursor.int_reg_idx += 2;
            } else {
                emitter.comment(&format!(
                    "param ${} from caller stack +{}",
                    name,
                    cursor.caller_stack_offset
                ));
                load_from_caller_stack(emitter, int_spill_reg, cursor.caller_stack_offset);
                load_from_caller_stack(emitter, int_hi_spill_reg, cursor.caller_stack_offset + 8);
                store_at_offset(emitter, int_spill_reg, offset);                       // save the spilled string pointer into the local param slot
                store_at_offset(emitter, int_hi_spill_reg, offset - 8);                // save the spilled string length into the local param slot
                cursor.caller_stack_offset += 16;
                cursor.int_stack_only = true;
            }
        }
        PhpType::Void => {}
        PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_) => {
            if !cursor.int_stack_only && cursor.int_reg_idx < int_reg_limit {
                let reg = int_arg_reg_name(emitter.target, cursor.int_reg_idx);
                emitter.comment(&format!("param ${} from {}", name, reg));
                store_at_offset(emitter, reg, offset);                                 // save the pointer-like parameter from the incoming integer argument register
                cursor.int_reg_idx += 1;
            } else {
                emitter.comment(&format!(
                    "param ${} from caller stack +{}",
                    name,
                    cursor.caller_stack_offset
                ));
                load_from_caller_stack(emitter, int_spill_reg, cursor.caller_stack_offset);
                store_at_offset(emitter, int_spill_reg, offset);                       // save the spilled pointer-like parameter into the local param slot
                cursor.caller_stack_offset += 16;
                cursor.int_stack_only = true;
            }
        }
    }
}

pub fn emit_push_reg(emitter: &mut Emitter, reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("str {}, [sp, #-16]!", reg));          // push the requested integer or pointer register onto the temporary stack
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 16");                                 // reserve one temporary stack slot for the pushed integer or pointer value
            emitter.instruction(&format!("mov QWORD PTR [rsp], {}", reg));      // store the requested integer or pointer register into the new stack slot
        }
    }
}

pub fn emit_pop_reg(emitter: &mut Emitter, reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr {}, [sp], #16", reg));            // pop the requested integer or pointer register from the temporary stack
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, QWORD PTR [rsp]", reg));      // reload the requested integer or pointer register from the temporary stack slot
            emitter.instruction("add rsp, 16");                                 // release the temporary stack slot after the integer or pointer pop
        }
    }
}

pub fn emit_push_float_reg(emitter: &mut Emitter, reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("str {}, [sp, #-16]!", reg));          // push the requested floating-point register onto the temporary stack
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 16");                                 // reserve one temporary stack slot for the pushed floating-point value
            emitter.instruction(&format!("movsd QWORD PTR [rsp], {}", reg));    // store the requested floating-point register into the new stack slot
        }
    }
}

pub fn emit_pop_float_reg(emitter: &mut Emitter, reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr {}, [sp], #16", reg));            // pop the requested floating-point register from the temporary stack
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("movsd {}, QWORD PTR [rsp]", reg));    // reload the requested floating-point register from the temporary stack slot
            emitter.instruction("add rsp, 16");                                 // release the temporary stack slot after the floating-point pop
        }
    }
}

pub fn emit_push_reg_pair(emitter: &mut Emitter, lo_reg: &str, hi_reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("stp {}, {}, [sp, #-16]!", lo_reg, hi_reg)); // push the requested register pair into one temporary 16-byte stack slot
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 16");                                 // reserve one temporary stack slot for the pushed register pair
            emitter.instruction(&format!("mov QWORD PTR [rsp], {}", lo_reg));   // store the first register into the low half of the temporary slot
            emitter.instruction(&format!("mov QWORD PTR [rsp + 8], {}", hi_reg)); // store the second register into the high half of the temporary slot
        }
    }
}

pub fn emit_pop_reg_pair(emitter: &mut Emitter, lo_reg: &str, hi_reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldp {}, {}, [sp], #16", lo_reg, hi_reg)); // pop the requested register pair from one temporary 16-byte stack slot
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, QWORD PTR [rsp]", lo_reg));   // reload the first register from the low half of the temporary stack slot
            emitter.instruction(&format!("mov {}, QWORD PTR [rsp + 8]", hi_reg)); // reload the second register from the high half of the temporary stack slot
            emitter.instruction("add rsp, 16");                                 // release the temporary stack slot after the register-pair pop
        }
    }
}

pub fn emit_push_result_value(emitter: &mut Emitter, ty: &PhpType) {
    match ty.codegen_repr() {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_) => {
            emit_push_reg(emitter, int_result_reg(emitter));                            // push the current scalar or pointer result register onto the temporary arg stack
        }
        PhpType::Float => {
            emit_push_float_reg(emitter, float_result_reg(emitter));                    // push the current floating-point result register onto the temporary arg stack
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            emit_push_reg_pair(emitter, ptr_reg, len_reg);                              // push the current string result register pair onto the temporary arg stack
        }
        PhpType::Void => {}
    }
}

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

pub fn emit_release_temporary_stack(emitter: &mut Emitter, amount: usize) {
    emit_adjust_sp(emitter, amount, false);
}

pub fn emit_reserve_temporary_stack(emitter: &mut Emitter, amount: usize) {
    emit_adjust_sp(emitter, amount, true);
}

pub fn emit_temporary_stack_address(emitter: &mut Emitter, scratch: &str, offset: usize) {
    emit_sp_address(emitter, scratch, offset);
}

pub fn emit_load_temporary_stack_slot(emitter: &mut Emitter, reg: &str, offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if offset == 0 {
                emitter.instruction(&format!("ldr {}, [sp]", reg));             // load directly from the top of the temporary argument stack
            } else if offset <= 4095 {
                emitter.instruction(&format!("ldr {}, [sp, #{}]", reg, offset)); // load from a nearby temporary argument slot with an immediate offset
            } else {
                emit_sp_address(emitter, "x9", offset);
                emitter.instruction(&format!("ldr {}, [x9]", reg));             // load from a distant temporary argument slot through a scratch address
            }
        }
        Arch::X86_64 => {
            let slot = if offset == 0 {
                "[rsp]".to_string()
            } else {
                format!("[rsp + {}]", offset)
            };
            if reg.starts_with('d') || reg.starts_with("xmm") {
                emitter.instruction(&format!("movsd {}, QWORD PTR {}", reg, slot)); // load the floating-point payload from the temporary outgoing-arg stack
            } else {
                emitter.instruction(&format!("mov {}, QWORD PTR {}", reg, slot)); // load the integer or pointer payload from the temporary outgoing-arg stack
            }
        }
    }
}

fn emit_store_to_sp(emitter: &mut Emitter, reg: &str, offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if offset == 0 {
                emitter.instruction(&format!("str {}, [sp]", reg));             // store directly to the top of the outgoing stack-argument area
            } else if offset <= 4095 {
                emitter.instruction(&format!("str {}, [sp, #{}]", reg, offset)); // store to a nearby outgoing stack-argument slot with an immediate offset
            } else {
                emit_sp_address(emitter, "x9", offset);
                emitter.instruction(&format!("str {}, [x9]", reg));             // store to a distant outgoing stack-argument slot through a scratch address
            }
        }
        Arch::X86_64 => {
            let slot = if offset == 0 {
                "[rsp]".to_string()
            } else {
                format!("[rsp + {}]", offset)
            };
            if reg.starts_with('d') || reg.starts_with("xmm") {
                emitter.instruction(&format!("movsd QWORD PTR {}, {}", slot, reg)); // store the floating-point payload into the outgoing stack-argument area
            } else {
                emitter.instruction(&format!("mov QWORD PTR {}, {}", slot, reg)); // store the integer or pointer payload into the outgoing stack-argument area
            }
        }
    }
}

fn emit_copy_stack_arg_slot(emitter: &mut Emitter, ty: &PhpType, src_offset: usize, dst_offset: usize) {
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
            PhpType::Void => {}
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
