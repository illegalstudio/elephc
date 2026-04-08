use crate::codegen::emit::Emitter;
use crate::types::PhpType;

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

pub fn emit_write_stdout(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Str => {
            emitter.instruction("mov x0, #1");                                          // fd = stdout
            emitter.syscall(4);
        }
        PhpType::Bool | PhpType::Int => {
            emitter.instruction("bl __rt_itoa");                                        // x0 → x1=ptr, x2=len
            emitter.instruction("mov x0, #1");                                          // fd = stdout
            emitter.syscall(4);
        }
        PhpType::Float => {
            emitter.instruction("bl __rt_ftoa");                                        // d0 → x1=ptr, x2=len
            emitter.instruction("mov x0, #1");                                          // fd = stdout
            emitter.syscall(4);
        }
        PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) => {
            emitter.instruction("bl __rt_ptoa");                                        // x0 → x1=ptr, x2=len
            emitter.instruction("mov x0, #1");                                          // fd = stdout
            emitter.syscall(4);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emitter.instruction("bl __rt_mixed_write_stdout");                          // inspect boxed mixed payload and print if scalar/string
        }
        PhpType::Void
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Callable
        | PhpType::Object(_) => {}
    }
}
