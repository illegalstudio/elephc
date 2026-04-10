use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::types::PhpType;

use super::calls::emit_call_label;
use super::frame::{load_at_offset, store_at_offset};
use super::registers::{float_result_reg, int_result_reg, string_result_regs};

pub fn emit_store(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty {
        PhpType::Bool | PhpType::Int => {
            store_at_offset(emitter, int_result_reg(emitter), offset);                  // store int/bool to stack
        }
        PhpType::Float => {
            store_at_offset(emitter, float_result_reg(emitter), offset);                // store float to stack
        }
        PhpType::Str => {
            emitter.instruction("bl __rt_str_persist");                                  // copy string to heap, x1=heap_ptr, x2=len
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            store_at_offset(emitter, ptr_reg, offset);                                  // store string pointer
            store_at_offset(emitter, len_reg, offset - 8);                              // store string length
        }
        PhpType::Void => {
            store_at_offset(emitter, int_result_reg(emitter), offset);                  // store null sentinel
        }
        PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_) => {
            store_at_offset(emitter, int_result_reg(emitter), offset);                  // store array/callable/object/pointer value
        }
    }
}

pub fn emit_incref_if_refcounted(emitter: &mut Emitter, ty: &PhpType) {
    if ty.is_refcounted() {
        emitter.instruction("str x0, [sp, #-16]!");                                     // preserve heap pointer across incref helper call
        emitter.instruction("bl __rt_incref");                                          // retain shared heap value before creating a new owner
        emitter.instruction("ldr x0, [sp], #16");                                       // restore original heap pointer after incref
    }
}

pub fn emit_decref_if_refcounted(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Mixed | PhpType::Union(_) => {
            emitter.instruction("bl __rt_decref_mixed");                                // release mixed cell reference
        }
        PhpType::Array(_) => {
            emitter.instruction("bl __rt_decref_array");                                // release indexed array reference
        }
        PhpType::AssocArray { .. } => {
            emitter.instruction("bl __rt_decref_hash");                                 // release associative array reference
        }
        PhpType::Object(_) => {
            emitter.instruction("bl __rt_decref_object");                               // release object reference
        }
        _ => {}
    }
}

pub fn emit_load(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty {
        PhpType::Bool | PhpType::Int => {
            load_at_offset(emitter, int_result_reg(emitter), offset);                   // load int/bool from stack
        }
        PhpType::Float => {
            load_at_offset(emitter, float_result_reg(emitter), offset);                 // load float from stack
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            load_at_offset(emitter, ptr_reg, offset);                                   // load string pointer
            load_at_offset(emitter, len_reg, offset - 8);                               // load string length
        }
        PhpType::Void => {
            load_at_offset(emitter, int_result_reg(emitter), offset);                   // load null sentinel
        }
        PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_) => {
            load_at_offset(emitter, int_result_reg(emitter), offset);                   // load array/callable/object/pointer value
        }
    }
}

pub fn emit_branch_if_int_result_zero(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction(&format!("cbz {}, {}", int_result_reg(emitter), label)); // branch when the coerced integer truthiness result is zero
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction(&format!("test {}, {}", int_result_reg(emitter), int_result_reg(emitter))); // test whether the coerced integer truthiness result is zero
            emitter.instruction(&format!("je {}", label));                            // branch when the coerced integer truthiness result is zero
        }
    }
}

pub fn emit_branch_if_int_result_nonzero(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction(&format!("cbnz {}, {}", int_result_reg(emitter), label)); // branch when the coerced integer truthiness result is non-zero
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction(&format!("test {}, {}", int_result_reg(emitter), int_result_reg(emitter))); // test whether the coerced integer truthiness result is non-zero
            emitter.instruction(&format!("jne {}", label));                            // branch when the coerced integer truthiness result is non-zero
        }
    }
}

pub fn emit_jump(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction(&format!("b {}", label));                       // jump unconditionally to the target label
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction(&format!("jmp {}", label));                     // jump unconditionally to the target label
        }
    }
}

pub fn emit_int_result_to_float_result(emitter: &mut Emitter) {
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            let inst = format!("scvtf {}, {}", float_result_reg(emitter), int_result_reg(emitter));
            emitter.instruction(&inst);                                         // promote the integer result into the floating-point result register
        }
        crate::codegen::platform::Arch::X86_64 => {
            let inst = format!("cvtsi2sd {}, {}", float_result_reg(emitter), int_result_reg(emitter));
            emitter.instruction(&inst);                                         // promote the integer result into the floating-point result register
        }
    }
}

pub fn emit_float_result_to_int_result(emitter: &mut Emitter) {
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            let inst = format!("fcvtzs {}, {}", int_result_reg(emitter), float_result_reg(emitter));
            emitter.instruction(&inst);                                         // truncate the floating-point result into the integer result register
        }
        crate::codegen::platform::Arch::X86_64 => {
            let inst = format!("cvttsd2si {}, {}", int_result_reg(emitter), float_result_reg(emitter));
            emitter.instruction(&inst);                                         // truncate the floating-point result into the integer result register
        }
    }
}

pub fn emit_load_int_immediate(emitter: &mut Emitter, reg: &str, value: i64) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if (0..=65535).contains(&value) {
                emitter.instruction(&format!("mov {}, #{}", reg, value));               // load a small non-negative immediate directly into the target register
            } else if (-65536..0).contains(&value) {
                emitter.instruction(&format!("mov {}, #{}", reg, value));               // load a small negative immediate directly into the target register
            } else {
                let uval = value as u64;
                emitter.instruction(&format!("movz {}, #0x{:x}", reg, uval & 0xFFFF)); // seed the low 16 bits of the wider immediate value
                if (uval >> 16) & 0xFFFF != 0 {
                    emitter.instruction(&format!(
                        "movk {}, #0x{:x}, lsl #16",
                        reg,
                        (uval >> 16) & 0xFFFF
                    )); // patch bits 16-31 of the wider immediate value
                }
                if (uval >> 32) & 0xFFFF != 0 {
                    emitter.instruction(&format!(
                        "movk {}, #0x{:x}, lsl #32",
                        reg,
                        (uval >> 32) & 0xFFFF
                    )); // patch bits 32-47 of the wider immediate value
                }
                if (uval >> 48) & 0xFFFF != 0 {
                    emitter.instruction(&format!(
                        "movk {}, #0x{:x}, lsl #48",
                        reg,
                        (uval >> 48) & 0xFFFF
                    )); // patch bits 48-63 of the wider immediate value
                }
            }
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, {}", reg, value));                    // load the immediate directly into the native x86_64 register
        }
    }
}

pub fn emit_write_stdout(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Str => {
            emit_write_current_string_stdout(emitter);
        }
        PhpType::Bool | PhpType::Int => {
            emit_call_label(emitter, "__rt_itoa");
            emit_write_current_string_stdout(emitter);
        }
        PhpType::Float => {
            emit_call_label(emitter, "__rt_ftoa");
            emit_write_current_string_stdout(emitter);
        }
        PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) => {
            emit_call_label(emitter, "__rt_ptoa");
            emit_write_current_string_stdout(emitter);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_call_label(emitter, "__rt_mixed_write_stdout");
        }
        PhpType::Void
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Callable
        | PhpType::Object(_) => {}
    }
}

fn emit_write_current_string_stdout(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #1");                                          // fd = stdout
            emitter.syscall(4);
        }
        Arch::X86_64 => {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            emitter.instruction(&format!("mov rsi, {}", ptr_reg));                      // move the current string pointer into the Linux write buffer register
            emitter.instruction(&format!("mov rdx, {}", len_reg));                      // move the current string length into the Linux write length register
            emitter.instruction("mov edi, 1");                                          // fd = stdout
            emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                             // write the current string payload to stdout
        }
    }
}
