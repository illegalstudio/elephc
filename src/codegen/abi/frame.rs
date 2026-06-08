//! Purpose:
//! Owns stack-frame setup, teardown, frame-slot addressing, and generic memory loads/stores.
//! Provides target-specific helpers for local slots, caller stack access, and cleanup callbacks.
//!
//! Called from:
//! - `crate::codegen::functions`, `crate::codegen::main_emission`, and ABI call helpers
//!
//! Key details:
//! - Frame offsets and stack alignment are shared contracts with local collection and call materialization.

use crate::codegen::{emit::Emitter, platform::Arch};
use crate::types::PhpType;

use super::registers::{
    float_result_reg, frame_pointer_reg, int_result_reg, is_float_register, string_result_regs,
};

/// Sets up the stack frame for a function body.
/// On AArch64: allocates `frame_size` bytes, saves x29/x30 in the footer, and establishes x29 as the frame pointer.
/// On x86_64: pushes rbp, establishes rsp as the frame base, and reserves `frame_size - 16` bytes for locals.
pub fn emit_frame_prologue(emitter: &mut Emitter, frame_size: usize) {
    debug_assert!(
        frame_size >= 16,
        "frame_size must reserve the 16-byte frame footer (x29/x30), got {frame_size}"
    );
    emitter.comment("prologue");
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_adjust_sp(emitter, frame_size, true);
            let footer_offset = frame_size - 16;
            if footer_offset <= 504 {
                emitter.instruction(&format!("stp x29, x30, [sp, #{}]", footer_offset)); // save frame pointer and return address in the fixed frame footer
            } else {
                emit_sp_address(emitter, "x9", footer_offset);
                emitter.instruction("stp x29, x30, [x9]");                      // save frame pointer and return address through the computed footer pointer
            }
            if footer_offset == 0 {
                emitter.instruction("mov x29, sp");                             // use the current stack pointer directly when the frame footer starts at sp
            } else if footer_offset <= 4095 {
                emitter.instruction(&format!("add x29, sp, #{}", footer_offset)); // point the frame pointer at the nearby fixed frame footer
            } else {
                emit_sp_address(emitter, "x29", footer_offset);
            }
        }
        Arch::X86_64 => {
            let local_bytes = frame_size.saturating_sub(16);
            emitter.instruction("push rbp");                                    // save the caller frame pointer on the stack
            emitter.instruction("mov rbp, rsp");                                // establish the current stack pointer as the new frame base
            if local_bytes > 0 {
                emitter.instruction(&format!("sub rsp, {}", local_bytes));      // reserve aligned stack space for local slots below rbp
            }
        }
    }
}

/// Tears down the stack frame and restores the caller's frame state.
/// On AArch64: restores x29/x30 from the footer and releases `frame_size` bytes.
/// On x86_64: releases local bytes and pops rbp.
pub fn emit_frame_restore(emitter: &mut Emitter, frame_size: usize) {
    debug_assert!(
        frame_size >= 16,
        "frame_size must reserve the 16-byte frame footer (x29/x30), got {frame_size}"
    );
    match emitter.target.arch {
        Arch::AArch64 => {
            let footer_offset = frame_size - 16;
            if footer_offset <= 504 {
                emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", footer_offset)); // restore frame pointer and return address from the fixed frame footer
            } else {
                emit_sp_address(emitter, "x9", footer_offset);
                emitter.instruction("ldp x29, x30, [x9]");                      // restore frame pointer and return address through the computed footer pointer
            }
            emit_adjust_sp(emitter, frame_size, false);
        }
        Arch::X86_64 => {
            let local_bytes = frame_size.saturating_sub(16);
            if local_bytes > 0 {
                emitter.instruction(&format!("add rsp, {}", local_bytes));      // release the aligned local-slot area below rbp
            }
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer from the stack
        }
    }
}

/// Emits the function return sequence using the platform `ret` instruction.
pub fn emit_return(emitter: &mut Emitter) {
    emitter.instruction("ret");                                                 // return to the caller using the platform return instruction
}

/// Sets up the stack frame for a cleanup callback (e.g., from destructor unwinding).
/// On AArch64: allocates 16 bytes of spill space and saves x29/x30.
/// On x86_64: pushes rbp and establishes `frame_base_reg` as the temporary frame pointer.
pub fn emit_cleanup_callback_prologue(emitter: &mut Emitter, frame_base_reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");                             // reserve spill space for the callback's saved frame state
            emitter.instruction("stp x29, x30, [sp, #0]");                      // save the callback caller's frame pointer and return address
            emitter.instruction(&format!("mov x29, {}", frame_base_reg));       // treat the unwound frame base as the temporary frame pointer during cleanup
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the callback caller frame pointer before rebasing cleanup
            emitter.instruction(&format!("mov rbp, {}", frame_base_reg));       // treat the unwound frame base as the temporary cleanup frame pointer
        }
    }
}

/// Tears down the cleanup callback frame and returns.
/// On AArch64: restores x29/x30 from the 16-byte spill area and releases it.
/// On x86_64: pops rbp. Both targets then emit the platform `ret` instruction.
pub fn emit_cleanup_callback_epilogue(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldp x29, x30, [sp, #0]");                      // restore the callback caller's frame pointer and return address
            emitter.instruction("add sp, sp, #16");                             // release the callback spill space
        }
        Arch::X86_64 => {
            emitter.instruction("pop rbp");                                     // restore the callback caller frame pointer after cleanup work
        }
    }
    emit_return(emitter);
}

/// Emits code that computes the address of a local frame slot and stores it in `dest`.
/// Uses the frame pointer (x29/rbp) as the base. Large offsets on AArch64 are walked down in
/// 4095-byte chunks to stay within immediate-add instructions.
pub fn emit_frame_slot_address(emitter: &mut Emitter, dest: &str, offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if offset == 0 {
                emitter.instruction(&format!("mov {}, x29", dest));             // copy the frame pointer when the requested slot is the frame base itself
            } else if offset <= 4095 {
                emitter.instruction(&format!("sub {}, x29, #{}", dest, offset)); // compute the local-slot address directly from the frame pointer
            } else {
                emitter.instruction(&format!("mov {}, x29", dest));             // seed the destination register from the frame pointer for a far local-slot address
                let mut remaining = offset;
                while remaining > 0 {
                    let chunk = remaining.min(4095);
                    emitter.instruction(&format!("sub {}, {}, #{}", dest, dest, chunk)); // walk the destination register down toward the distant local-slot address
                    remaining -= chunk;
                }
            }
        }
        Arch::X86_64 => {
            if offset == 0 {
                emitter.instruction(&format!("mov {}, {}", dest, frame_pointer_reg(emitter))); // copy rbp when the requested slot is the frame base itself
            } else {
                emitter.instruction(&format!("lea {}, [{} - {}]", dest, frame_pointer_reg(emitter), offset)); // materialize the local-slot address relative to rbp
            }
        }
    }
}

/// Stores `reg` into the local frame slot at `offset` from the frame pointer, using x9 as scratch.
/// On AArch64: uses `stur` for offsets ≤ 255, otherwise computes the address first.
/// On x86_64: stores via `[rbp - offset]` with a mov instruction; float registers use movsd.
pub fn store_at_offset(emitter: &mut Emitter, reg: &str, offset: usize) {
    store_at_offset_scratch(emitter, reg, offset, "x9");
}

/// Stores `reg` into the local frame slot at `offset` from the frame pointer, using `scratch` as scratch.
/// This variant accepts a caller-specified scratch register to avoid conflicts in multi-register sequences.
pub fn store_at_offset_scratch(emitter: &mut Emitter, reg: &str, offset: usize, scratch: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if offset <= 255 {
                emitter.instruction(&format!("stur {}, [x29, #-{}]", reg, offset)); // store via unscaled immediate offset
            } else {
                emit_frame_slot_address(emitter, scratch, offset);
                emitter.instruction(&format!("str {}, [{}]", reg, scratch));    // store via computed address
            }
        }
        Arch::X86_64 => {
            let slot = if offset == 0 {
                format!("[{}]", frame_pointer_reg(emitter))
            } else {
                format!("[{} - {}]", frame_pointer_reg(emitter), offset)
            };
            if is_float_register(reg) {
                emitter.instruction(&format!("movsd QWORD PTR {}, {}", slot, reg)); // store the floating-point payload into the local frame slot
            } else {
                emitter.instruction(&format!("mov QWORD PTR {}, {}", slot, reg)); // store the integer or pointer payload into the local frame slot
            }
        }
    }
}

/// Loads the local frame slot at `offset` from the frame pointer into `reg`, using x9 as scratch.
/// On AArch64: uses `ldur` for offsets ≤ 255, otherwise computes the address first.
/// On x86_64: loads via `[rbp - offset]` with a mov instruction; float registers use movsd.
pub fn load_at_offset(emitter: &mut Emitter, reg: &str, offset: usize) {
    load_at_offset_scratch(emitter, reg, offset, "x9");
}

/// Loads the local frame slot at `offset` from the frame pointer into `reg`, using `scratch` as scratch.
/// This variant accepts a caller-specified scratch register to avoid conflicts in multi-register sequences.
pub fn load_at_offset_scratch(emitter: &mut Emitter, reg: &str, offset: usize, scratch: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if offset <= 255 {
                emitter.instruction(&format!("ldur {}, [x29, #-{}]", reg, offset)); // load via unscaled immediate offset
            } else {
                emit_frame_slot_address(emitter, scratch, offset);
                emitter.instruction(&format!("ldr {}, [{}]", reg, scratch));    // load via computed address
            }
        }
        Arch::X86_64 => {
            let slot = if offset == 0 {
                format!("[{}]", frame_pointer_reg(emitter))
            } else {
                format!("[{} - {}]", frame_pointer_reg(emitter), offset)
            };
            if is_float_register(reg) {
                emitter.instruction(&format!("movsd {}, QWORD PTR {}", reg, slot)); // load the floating-point payload from the local frame slot
            } else {
                emitter.instruction(&format!("mov {}, QWORD PTR {}", reg, slot)); // load the integer or pointer payload from the local frame slot
            }
        }
    }
}

/// Loads a value from an arbitrary address in memory into `reg`.
/// `addr_reg` holds the base address; `byte_offset` is added (AArch64 scaled immediate, x86_64 additive).
/// On x86_64, float registers use movsd; integers use mov.
pub fn emit_load_from_address(emitter: &mut Emitter, reg: &str, addr_reg: &str, byte_offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if byte_offset == 0 {
                emitter.instruction(&format!("ldr {}, [{}]", reg, addr_reg));   // load the requested value directly from the computed address register
            } else {
                emitter.instruction(&format!("ldr {}, [{}, #{}]", reg, addr_reg, byte_offset)); // load the requested value from the computed address register plus byte offset
            }
        }
        Arch::X86_64 => {
            let slot = if byte_offset == 0 {
                format!("[{}]", addr_reg)
            } else {
                format!("[{} + {}]", addr_reg, byte_offset)
            };
            if is_float_register(reg) {
                emitter.instruction(&format!("movsd {}, QWORD PTR {}", reg, slot)); // load the floating-point payload through the computed address register
            } else {
                emitter.instruction(&format!("mov {}, QWORD PTR {}", reg, slot)); // load the integer or pointer payload through the computed address register
            }
        }
    }
}

/// Stores `reg` to an arbitrary address in memory.
/// `addr_reg` holds the base address; `byte_offset` is added (AArch64 scaled immediate, x86_64 additive).
/// On x86_64, float registers use movsd; integers use mov.
pub fn emit_store_to_address(
    emitter: &mut Emitter,
    reg: &str,
    addr_reg: &str,
    byte_offset: usize,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if byte_offset == 0 {
                emitter.instruction(&format!("str {}, [{}]", reg, addr_reg));   // store the requested value directly through the computed address register
            } else {
                emitter.instruction(&format!("str {}, [{}, #{}]", reg, addr_reg, byte_offset)); // store the requested value through the computed address register plus byte offset
            }
        }
        Arch::X86_64 => {
            let slot = if byte_offset == 0 {
                format!("[{}]", addr_reg)
            } else {
                format!("[{} + {}]", addr_reg, byte_offset)
            };
            if is_float_register(reg) {
                emitter.instruction(&format!("movsd QWORD PTR {}, {}", slot, reg)); // store the floating-point payload through the computed address register
            } else {
                emitter.instruction(&format!("mov QWORD PTR {}, {}", slot, reg)); // store the integer or pointer payload through the computed address register
            }
        }
    }
}

/// Stores zero to an arbitrary address in memory using the architectural zero register.
/// On AArch64 uses xzr; on x86_64 stores an explicit 0. `byte_offset` is added to `addr_reg`.
pub fn emit_store_zero_to_address(emitter: &mut Emitter, addr_reg: &str, byte_offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if byte_offset == 0 {
                emitter.instruction(&format!("str xzr, [{}]", addr_reg));       // store architectural zero directly through the computed address register
            } else {
                emitter.instruction(&format!("str xzr, [{}, #{}]", addr_reg, byte_offset)); // store architectural zero through the computed address register plus byte offset
            }
        }
        Arch::X86_64 => {
            let slot = if byte_offset == 0 {
                format!("[{}]", addr_reg)
            } else {
                format!("[{} + {}]", addr_reg, byte_offset)
            };
            emitter.instruction(&format!("mov QWORD PTR {}, 0", slot));         // store an integer zero through the computed address register
        }
    }
}

/// Loads a spilled incoming call argument from the caller stack into `reg`.
/// On AArch64 uses the frame pointer (x29) as base with positive offset; large offsets are walked
/// through a scratch register in 4080-byte chunks. On x86_64 uses rbp with positive offset.
pub fn load_from_caller_stack(emitter: &mut Emitter, reg: &str, offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if offset <= 4095 {
                emitter.instruction(&format!("ldr {}, [x29, #{}]", reg, offset)); // load a spilled incoming argument from the caller stack
            } else {
                emitter.instruction("mov x9, x29");                             // seed a scratch pointer from the current frame base
                let mut remaining = offset;
                while remaining > 0 {
                    let chunk = remaining.min(4080);
                    emitter.instruction(&format!("add x9, x9, #{}", chunk));    // advance the scratch pointer toward the distant caller-stack slot
                    remaining -= chunk;
                }
                emitter.instruction(&format!("ldr {}, [x9]", reg));             // load the spilled incoming argument through the computed caller-stack pointer
            }
        }
        Arch::X86_64 => {
            let slot = if offset == 0 {
                format!("[{}]", frame_pointer_reg(emitter))
            } else {
                format!("[{} + {}]", frame_pointer_reg(emitter), offset)
            };
            if is_float_register(reg) {
                emitter.instruction(&format!("movsd {}, QWORD PTR {}", reg, slot)); // load a spilled floating-point argument from the caller stack area
            } else {
                emitter.instruction(&format!("mov {}, QWORD PTR {}", reg, slot)); // load a spilled integer or pointer argument from the caller stack area
            }
        }
    }
}

/// Zero-initializes the local frame slot at `offset` from the frame pointer.
/// On AArch64 uses the xzr register via `store_at_offset`; on x86_64 emits a mov with immediate 0.
pub fn emit_store_zero_to_local_slot(emitter: &mut Emitter, offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            store_at_offset(emitter, "xzr", offset);                                     // zero-initialize the local slot with the architectural zero register
        }
        Arch::X86_64 => {
            if offset == 0 {
                emitter.instruction(&format!("mov QWORD PTR [{}], 0", frame_pointer_reg(emitter))); // zero-initialize the frame-base slot directly through rbp
            } else {
                emitter.instruction(&format!("mov QWORD PTR [{} - {}], 0", frame_pointer_reg(emitter), offset)); // zero-initialize the requested local slot relative to rbp
            }
        }
    }
}

/// Saves the return value into a hidden frame slot so it survives a tail-call or callback frame switch.
/// Float values use the float result register; strings use string_result_regs (pointer + length);
/// scalars use the integer result register. `return_offset` is the slot for the primary value; string
/// length is stored 8 bytes before it.
pub fn emit_preserve_return_value(
    emitter: &mut Emitter,
    return_ty: &PhpType,
    return_offset: usize,
) {
    match return_ty.codegen_repr() {
        PhpType::Float => {
            store_at_offset(emitter, float_result_reg(emitter), return_offset);         // preserve the float return value in the hidden frame slot
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            store_at_offset(emitter, ptr_reg, return_offset);                           // preserve the string return pointer in the hidden frame slot
            store_at_offset(emitter, len_reg, return_offset - 8);                       // preserve the string return length in the hidden frame slot
        }
        _ => {
            store_at_offset(emitter, int_result_reg(emitter), return_offset);           // preserve the scalar or pointer-like return value in the hidden frame slot
        }
    }
}

/// Restores the return value from a hidden frame slot after a tail-call or callback frame switch.
/// Reverse of `emit_preserve_return_value`: loads based on `return_ty` codegen repr into the
/// appropriate result registers.
pub fn emit_restore_return_value(
    emitter: &mut Emitter,
    return_ty: &PhpType,
    return_offset: usize,
) {
    match return_ty.codegen_repr() {
        PhpType::Float => {
            load_at_offset(emitter, float_result_reg(emitter), return_offset);          // restore the preserved float return value from the hidden frame slot
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            load_at_offset(emitter, ptr_reg, return_offset);                            // restore the preserved string return pointer from the hidden frame slot
            load_at_offset(emitter, len_reg, return_offset - 8);                        // restore the preserved string return length from the hidden frame slot
        }
        _ => {
            load_at_offset(emitter, int_result_reg(emitter), return_offset);            // restore the preserved scalar or pointer-like return value from the hidden frame slot
        }
    }
}

/// Allocates or releases `amount` bytes from the stack pointer.
/// On AArch64 emits at most 4080-byte chunks to stay within sub/add immediate limits.
/// On x86_64 emits a single sub or add. `subtract=true` reserves space; `subtract=false` releases.
pub(crate) fn emit_adjust_sp(emitter: &mut Emitter, amount: usize, subtract: bool) {
    match emitter.target.arch {
        Arch::AArch64 => {
            let mut remaining = amount;
            while remaining > 0 {
                let chunk = remaining.min(4080);
                if subtract {
                    emitter.instruction(&format!("sub sp, sp, #{}", chunk));    // reserve stack space for spilled outgoing call arguments
                } else {
                    emitter.instruction(&format!("add sp, sp, #{}", chunk));    // release temporary outgoing call-argument stack space
                }
                remaining -= chunk;
            }
        }
        Arch::X86_64 => {
            if amount == 0 {
                return;
            }
            if subtract {
                emitter.instruction(&format!("sub rsp, {}", amount));           // reserve stack space for spilled outgoing call arguments
            } else {
                emitter.instruction(&format!("add rsp, {}", amount));           // release temporary outgoing call-argument stack space
            }
        }
    }
}

/// Computes the address of a temporary stack slot relative to the current stack pointer and
/// stores it in `scratch`. Used for stack positions that are not part of the fixed frame layout.
/// On AArch64 walks up from sp in 4080-byte chunks; on x86_64 uses lea with rsp base.
pub(crate) fn emit_sp_address(emitter: &mut Emitter, scratch: &str, offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov {}, sp", scratch));               // seed a scratch pointer from the current stack pointer
            let mut remaining = offset;
            while remaining > 0 {
                let chunk = remaining.min(4080);
                emitter.instruction(&format!("add {}, {}, #{}", scratch, scratch, chunk)); // advance the scratch pointer toward the desired stack slot
                remaining -= chunk;
            }
        }
        Arch::X86_64 => {
            if offset == 0 {
                emitter.instruction(&format!("mov {}, rsp", scratch));          // copy the current stack pointer when the requested stack slot is at rsp
            } else {
                emitter.instruction(&format!("lea {}, [rsp + {}]", scratch, offset)); // materialize the temporary stack-slot address relative to rsp
            }
        }
    }
}
