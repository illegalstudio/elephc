//! Purpose:
//! Provides small stack scratch helpers for preserving registers and staging temporary call values.
//! Handles typed result pushes, pops, address calculations, and stack slot transfers.
//!
//! Called from:
//! - `crate::codegen::abi::calls::outgoing` and expression/statement emitters
//!
//! Key details:
//! - Temporary stack adjustments must remain balanced across nested calls and target alignment rules.

use crate::codegen::{emit::Emitter, platform::Arch};
use crate::types::PhpType;

use super::super::frame::{emit_adjust_sp, emit_sp_address};
use super::super::registers::{
    float_result_reg, int_result_reg, string_result_regs,
};

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
            emit_push_reg(emitter, int_result_reg(emitter));                            // push the current scalar or pointer result register onto the temporary arg stack
        }
        PhpType::Float => {
            emit_push_float_reg(emitter, float_result_reg(emitter));                    // push the current floating-point result register onto the temporary arg stack
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            emit_push_reg_pair(emitter, ptr_reg, len_reg);                              // push the current string result register pair onto the temporary arg stack
        }
        PhpType::Void | PhpType::Never => {}
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

pub(super) fn emit_store_to_sp(emitter: &mut Emitter, reg: &str, offset: usize) {
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
