use crate::codegen::{emit::Emitter, platform::Arch};
use crate::types::PhpType;

use super::registers::{
    float_result_reg, frame_pointer_reg, int_result_reg, is_float_register, string_result_regs,
};

pub fn emit_frame_prologue(emitter: &mut Emitter, frame_size: usize) {
    emitter.comment("prologue");
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_adjust_sp(emitter, frame_size, true);
            let footer_offset = frame_size - 16;
            if footer_offset <= 504 {
                emitter.instruction(&format!("stp x29, x30, [sp, #{}]", footer_offset)); // save frame pointer and return address in the fixed frame footer
            } else {
                emit_sp_address(emitter, "x9", footer_offset);
                emitter.instruction("stp x29, x30, [x9]");                              // save frame pointer and return address through the computed footer pointer
            }
            if footer_offset == 0 {
                emitter.instruction("mov x29, sp");                                   // use the current stack pointer directly when the frame footer starts at sp
            } else if footer_offset <= 4095 {
                emitter.instruction(&format!("add x29, sp, #{}", footer_offset));      // point the frame pointer at the nearby fixed frame footer
            } else {
                emit_sp_address(emitter, "x29", footer_offset);
            }
        }
        Arch::X86_64 => {
            let local_bytes = frame_size.saturating_sub(16);
            emitter.instruction("push rbp");                                            // save the caller frame pointer on the stack
            emitter.instruction("mov rbp, rsp");                                        // establish the current stack pointer as the new frame base
            if local_bytes > 0 {
                emitter.instruction(&format!("sub rsp, {}", local_bytes));              // reserve aligned stack space for local slots below rbp
            }
        }
    }
}

pub fn emit_frame_restore(emitter: &mut Emitter, frame_size: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            let footer_offset = frame_size - 16;
            if footer_offset <= 504 {
                emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", footer_offset)); // restore frame pointer and return address from the fixed frame footer
            } else {
                emit_sp_address(emitter, "x9", footer_offset);
                emitter.instruction("ldp x29, x30, [x9]");                              // restore frame pointer and return address through the computed footer pointer
            }
            emit_adjust_sp(emitter, frame_size, false);
        }
        Arch::X86_64 => {
            let local_bytes = frame_size.saturating_sub(16);
            if local_bytes > 0 {
                emitter.instruction(&format!("add rsp, {}", local_bytes));              // release the aligned local-slot area below rbp
            }
            emitter.instruction("pop rbp");                                             // restore the caller frame pointer from the stack
        }
    }
}

pub fn emit_return(emitter: &mut Emitter) {
    emitter.instruction("ret");                                                         // return to the caller using the platform return instruction
}

pub fn emit_cleanup_callback_prologue(emitter: &mut Emitter, frame_base_reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");                                     // reserve spill space for the callback's saved frame state
            emitter.instruction("stp x29, x30, [sp, #0]");                              // save the callback caller's frame pointer and return address
            emitter.instruction(&format!("mov x29, {}", frame_base_reg));               // treat the unwound frame base as the temporary frame pointer during cleanup
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                            // preserve the callback caller frame pointer before rebasing cleanup
            emitter.instruction(&format!("mov rbp, {}", frame_base_reg));               // treat the unwound frame base as the temporary cleanup frame pointer
        }
    }
}

pub fn emit_cleanup_callback_epilogue(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore the callback caller's frame pointer and return address
            emitter.instruction("add sp, sp, #16");                                     // release the callback spill space
        }
        Arch::X86_64 => {
            emitter.instruction("pop rbp");                                             // restore the callback caller frame pointer after cleanup work
        }
    }
    emit_return(emitter);
}

pub fn emit_frame_slot_address(emitter: &mut Emitter, dest: &str, offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if offset == 0 {
                emitter.instruction(&format!("mov {}, x29", dest));                     // copy the frame pointer when the requested slot is the frame base itself
            } else if offset <= 4095 {
                emitter.instruction(&format!("sub {}, x29, #{}", dest, offset));        // compute the local-slot address directly from the frame pointer
            } else {
                emitter.instruction(&format!("mov {}, x29", dest));                     // seed the destination register from the frame pointer for a far local-slot address
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

pub fn store_at_offset(emitter: &mut Emitter, reg: &str, offset: usize) {
    store_at_offset_scratch(emitter, reg, offset, "x9");
}

pub fn store_at_offset_scratch(emitter: &mut Emitter, reg: &str, offset: usize, scratch: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if offset <= 255 {
                emitter.instruction(&format!("stur {}, [x29, #-{}]", reg, offset));     // store via unscaled immediate offset
            } else {
                emit_frame_slot_address(emitter, scratch, offset);
                emitter.instruction(&format!("str {}, [{}]", reg, scratch));            // store via computed address
            }
        }
        Arch::X86_64 => {
            let slot = if offset == 0 {
                format!("[{}]", frame_pointer_reg(emitter))
            } else {
                format!("[{} - {}]", frame_pointer_reg(emitter), offset)
            };
            if is_float_register(reg) {
                emitter.instruction(&format!("movsd QWORD PTR {}, {}", slot, reg));     // store the floating-point payload into the local frame slot
            } else {
                emitter.instruction(&format!("mov QWORD PTR {}, {}", slot, reg));       // store the integer or pointer payload into the local frame slot
            }
        }
    }
}

pub fn load_at_offset(emitter: &mut Emitter, reg: &str, offset: usize) {
    load_at_offset_scratch(emitter, reg, offset, "x9");
}

pub fn load_at_offset_scratch(emitter: &mut Emitter, reg: &str, offset: usize, scratch: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if offset <= 255 {
                emitter.instruction(&format!("ldur {}, [x29, #-{}]", reg, offset));     // load via unscaled immediate offset
            } else {
                emit_frame_slot_address(emitter, scratch, offset);
                emitter.instruction(&format!("ldr {}, [{}]", reg, scratch));            // load via computed address
            }
        }
        Arch::X86_64 => {
            let slot = if offset == 0 {
                format!("[{}]", frame_pointer_reg(emitter))
            } else {
                format!("[{} - {}]", frame_pointer_reg(emitter), offset)
            };
            if is_float_register(reg) {
                emitter.instruction(&format!("movsd {}, QWORD PTR {}", reg, slot));     // load the floating-point payload from the local frame slot
            } else {
                emitter.instruction(&format!("mov {}, QWORD PTR {}", reg, slot));       // load the integer or pointer payload from the local frame slot
            }
        }
    }
}

pub fn emit_load_from_address(emitter: &mut Emitter, reg: &str, addr_reg: &str, byte_offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if byte_offset == 0 {
                emitter.instruction(&format!("ldr {}, [{}]", reg, addr_reg));          // load the requested value directly from the computed address register
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
                emitter.instruction(&format!("movsd {}, QWORD PTR {}", reg, slot));     // load the floating-point payload through the computed address register
            } else {
                emitter.instruction(&format!("mov {}, QWORD PTR {}", reg, slot));       // load the integer or pointer payload through the computed address register
            }
        }
    }
}

pub fn emit_store_to_address(
    emitter: &mut Emitter,
    reg: &str,
    addr_reg: &str,
    byte_offset: usize,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if byte_offset == 0 {
                emitter.instruction(&format!("str {}, [{}]", reg, addr_reg));          // store the requested value directly through the computed address register
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
                emitter.instruction(&format!("movsd QWORD PTR {}, {}", slot, reg));     // store the floating-point payload through the computed address register
            } else {
                emitter.instruction(&format!("mov QWORD PTR {}, {}", slot, reg));       // store the integer or pointer payload through the computed address register
            }
        }
    }
}

pub fn load_from_caller_stack(emitter: &mut Emitter, reg: &str, offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if offset <= 4095 {
                emitter.instruction(&format!("ldr {}, [x29, #{}]", reg, offset));       // load a spilled incoming argument from the caller stack
            } else {
                emitter.instruction("mov x9, x29");                                     // seed a scratch pointer from the current frame base
                let mut remaining = offset;
                while remaining > 0 {
                    let chunk = remaining.min(4080);
                    emitter.instruction(&format!("add x9, x9, #{}", chunk));            // advance the scratch pointer toward the distant caller-stack slot
                    remaining -= chunk;
                }
                emitter.instruction(&format!("ldr {}, [x9]", reg));                     // load the spilled incoming argument through the computed caller-stack pointer
            }
        }
        Arch::X86_64 => {
            let slot = if offset == 0 {
                format!("[{}]", frame_pointer_reg(emitter))
            } else {
                format!("[{} + {}]", frame_pointer_reg(emitter), offset)
            };
            if is_float_register(reg) {
                emitter.instruction(&format!("movsd {}, QWORD PTR {}", reg, slot));     // load a spilled floating-point argument from the caller stack area
            } else {
                emitter.instruction(&format!("mov {}, QWORD PTR {}", reg, slot));       // load a spilled integer or pointer argument from the caller stack area
            }
        }
    }
}

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

pub(crate) fn emit_adjust_sp(emitter: &mut Emitter, amount: usize, subtract: bool) {
    match emitter.target.arch {
        Arch::AArch64 => {
            let mut remaining = amount;
            while remaining > 0 {
                let chunk = remaining.min(4080);
                if subtract {
                    emitter.instruction(&format!("sub sp, sp, #{}", chunk));            // reserve stack space for spilled outgoing call arguments
                } else {
                    emitter.instruction(&format!("add sp, sp, #{}", chunk));            // release temporary outgoing call-argument stack space
                }
                remaining -= chunk;
            }
        }
        Arch::X86_64 => {
            if amount == 0 {
                return;
            }
            if subtract {
                emitter.instruction(&format!("sub rsp, {}", amount));                    // reserve stack space for spilled outgoing call arguments
            } else {
                emitter.instruction(&format!("add rsp, {}", amount));                    // release temporary outgoing call-argument stack space
            }
        }
    }
}

pub(crate) fn emit_sp_address(emitter: &mut Emitter, scratch: &str, offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov {}, sp", scratch));                       // seed a scratch pointer from the current stack pointer
            let mut remaining = offset;
            while remaining > 0 {
                let chunk = remaining.min(4080);
                emitter.instruction(&format!("add {}, {}, #{}", scratch, scratch, chunk)); // advance the scratch pointer toward the desired stack slot
                remaining -= chunk;
            }
        }
        Arch::X86_64 => {
            if offset == 0 {
                emitter.instruction(&format!("mov {}, rsp", scratch));                  // copy the current stack pointer when the requested stack slot is at rsp
            } else {
                emitter.instruction(&format!("lea {}, [rsp + {}]", scratch, offset));   // materialize the temporary stack-slot address relative to rsp
            }
        }
    }
}
