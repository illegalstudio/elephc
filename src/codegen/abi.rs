use super::emit::Emitter;
use crate::types::PhpType;

/// Store the current result registers to a local variable on the stack.
///
/// ARM64 register conventions for each PHP type:
///   Int/Bool: value in x0 (64-bit general register)
///   Float:    value in d0 (64-bit FP register)
///   Str:      pointer in x1, length in x2 (two 8-byte slots)
///   Null:     sentinel value in x0
///   Array:    heap pointer in x0
pub fn emit_store(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty {
        PhpType::Bool | PhpType::Int => {
            emitter.instruction(&format!("stur x0, [x29, #-{}]", offset));      // store int/bool to stack
        }
        PhpType::Float => {
            emitter.instruction(&format!("stur d0, [x29, #-{}]", offset));      // store float to stack
        }
        PhpType::Str => {
            // Strings use 16 bytes: pointer at offset, length at offset-8
            emitter.instruction(&format!("stur x1, [x29, #-{}]", offset));      // store string pointer
            emitter.instruction(&format!("stur x2, [x29, #-{}]", offset - 8));  // store string length
        }
        PhpType::Void => {
            emitter.instruction(&format!("stur x0, [x29, #-{}]", offset));      // store null sentinel
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            emitter.instruction(&format!("stur x0, [x29, #-{}]", offset));      // store array heap pointer
        }
    }
}

/// Load a local variable from the stack into result registers.
///
/// Restores the value into the same registers used by emit_store.
pub fn emit_load(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty {
        PhpType::Bool | PhpType::Int => {
            emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));      // load int/bool from stack
        }
        PhpType::Float => {
            emitter.instruction(&format!("ldur d0, [x29, #-{}]", offset));      // load float from stack
        }
        PhpType::Str => {
            emitter.instruction(&format!("ldur x1, [x29, #-{}]", offset));      // load string pointer
            emitter.instruction(&format!("ldur x2, [x29, #-{}]", offset - 8));  // load string length
        }
        PhpType::Void => {
            emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));      // load null sentinel
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));      // load array heap pointer
        }
    }
}

/// Emit sys_write(stdout, ptr, len) to print a value.
///
/// Uses macOS syscall convention:
///   x0 = fd (1 = stdout), x1 = buffer pointer, x2 = buffer length
///   x16 = syscall number (4 = write), then svc #0x80 to invoke kernel.
///
/// For non-string types, converts to string first via runtime helpers.
pub fn emit_write_stdout(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Str => {
            // x1=ptr, x2=len already set by the expression evaluator
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall 4 = write
            emitter.instruction("svc #0x80");                                   // invoke kernel
        }
        PhpType::Bool | PhpType::Int => {
            // Convert integer in x0 to decimal string, then write
            emitter.instruction("bl __rt_itoa");                                // x0 → x1=ptr, x2=len
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall 4 = write
            emitter.instruction("svc #0x80");                                   // invoke kernel
        }
        PhpType::Float => {
            // Convert float in d0 to string via snprintf, then write
            emitter.instruction("bl __rt_ftoa");                                // d0 → x1=ptr, x2=len
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall 4 = write
            emitter.instruction("svc #0x80");                                   // invoke kernel
        }
        PhpType::Void | PhpType::Array(_) | PhpType::AssocArray { .. } => {}      // null/array: nothing to print
    }
}
