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

/// Pushes a general-purpose register onto the temporary stack (pre-decrement on AArch64, sub rsp on x86_64).
/// Used to preserve caller-saved registers across nested calls or stage arguments.
///
/// - `reg`: Register name to push (e.g., `"x0"`, `"rdi"`).
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

/// Pops a general-purpose register from the temporary stack (post-increment on AArch64, add rsp on x86_64).
/// Complements `emit_push_reg` to restore caller-saved registers after nested calls.
///
/// - `reg`: Register name to pop (e.g., `"x0"`, `"rdi"`).
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

/// Pushes a floating-point register onto the temporary stack (pre-decrement store-pair on AArch64, sub rsp + movsd on x86_64).
/// Used to preserve floating-point caller-saved registers across nested calls.
///
/// - `reg`: Floating-point register name (e.g., `"d0"`, `"xmm0"`).
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

/// Pops a floating-point register from the temporary stack (post-increment load-pair on AArch64, movsd + add rsp on x86_64).
/// Complements `emit_push_float_reg` to restore floating-point caller-saved registers after nested calls.
///
/// - `reg`: Floating-point register name (e.g., `"d0"`, `"xmm0"`).
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

/// Pushes a pair of general-purpose registers onto the temporary stack as a single 16-byte slot.
/// Used for 128-bit values (e.g., string pointer + length on AArch64) or paired scalar data.
///
/// - `lo_reg`: Low register (e.g., `"x0"`, `"rdi"`).
/// - `hi_reg`: High register (e.g., `"x1"`, `"rsi"`).
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

/// Pops a pair of general-purpose registers from the temporary stack as a single 16-byte slot.
/// Complements `emit_push_reg_pair` to restore paired registers after nested calls.
///
/// - `lo_reg`: Low register to reload (e.g., `"x0"`, `"rdi"`).
/// - `hi_reg`: High register to reload (e.g., `"x1"`, `"rsi"`).
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

/// Pushes the current result value onto the temporary stack based on its PHP type.
/// Dispatches to `emit_push_reg`, `emit_push_float_reg`, or `emit_push_reg_pair` depending on `ty`.
/// No-op for `void` and `never` types (which have no result).
///
/// - `ty`: PHP type of the result value to push.
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
        PhpType::TaggedScalar => {
            let tag_reg = crate::codegen::sentinels::tagged_scalar_tag_reg(emitter);
            emit_push_reg_pair(emitter, int_result_reg(emitter), tag_reg);              // push the current tagged scalar payload/tag register pair onto the temporary arg stack
        }
        PhpType::Void | PhpType::Never => {}
    }
}

/// Releases `amount` bytes from the temporary stack (adds to SP on x86_64, deallocates on AArch64).
/// Called after arguments have been consumed to clean up stacked values from outgoing calls.
///
/// - `amount`: Number of bytes to release (must be a multiple of 16).
pub fn emit_release_temporary_stack(emitter: &mut Emitter, amount: usize) {
    emit_adjust_sp(emitter, amount, false);
}

/// Reserves `amount` bytes on the temporary stack (subtracts from SP on x86_64, pre-allocates on AArch64).
/// Called before staging arguments for an outgoing call to ensure sufficient stack space.
///
/// - `amount`: Number of bytes to reserve (must be a multiple of 16).
pub fn emit_reserve_temporary_stack(emitter: &mut Emitter, amount: usize) {
    emit_adjust_sp(emitter, amount, true);
}

/// Computes the address of a temporary stack slot at a given `offset` and stores it in `scratch`.
/// AArch64 uses `adrp + add`; x86_64 uses `lea rsp + offset`. Used to prepare indirect memory access to stacked values.
///
/// - `scratch`: Output register for the computed address (e.g., `"x9"`, `"rcx"`).
/// - `offset`: Byte offset from the current stack pointer to the desired slot.
pub fn emit_temporary_stack_address(emitter: &mut Emitter, scratch: &str, offset: usize) {
    emit_sp_address(emitter, scratch, offset);
}

/// Loads a value from a temporary stack slot into `reg`, supporting all offsets and register classes.
/// Uses direct load for small offsets; goes through scratch register `x9` for large offsets on AArch64.
/// Handles both integer (mov) and floating-point (movsd) registers on x86_64.
///
/// - `reg`: Destination register (e.g., `"x0"`, `"d0"`, `"rdi"`).
/// - `offset`: Byte offset of the stack slot from the current SP.
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

/// Stores `reg` into a stack slot at a given `offset` in the outgoing stack-argument area.
/// Used for passing arguments that don't fit in registers and must be laid out in the call frame.
/// Uses direct store for small offsets; goes through scratch register `x9` for large offsets on AArch64.
/// Handles both integer (mov) and floating-point (movsd) registers on x86_64.
///
/// - `reg`: Source register to store (e.g., `"x0"`, `"d0"`, `"rdi"`).
/// - `offset`: Byte offset of the destination slot in the outgoing stack area.
pub fn emit_store_to_sp(emitter: &mut Emitter, reg: &str, offset: usize) {
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
