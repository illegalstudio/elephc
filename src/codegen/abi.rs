use super::emit::Emitter;
use crate::types::PhpType;

/// Store the current result registers to the stack.
/// Int: x0 → [x29, #-offset]
/// Str: x1 (ptr) → [x29, #-offset], x2 (len) → [x29, #-(offset-8)]
pub fn emit_store(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty {
        PhpType::Bool | PhpType::Int => {
            emitter.instruction(&format!("stur x0, [x29, #-{}]", offset));
        }
        PhpType::Float => {
            emitter.instruction(&format!("stur d0, [x29, #-{}]", offset));
        }
        PhpType::Str => {
            emitter.instruction(&format!("stur x1, [x29, #-{}]", offset));
            emitter.instruction(&format!("stur x2, [x29, #-{}]", offset - 8));
        }
        PhpType::Void => {
            // null sentinel stored as int
            emitter.instruction(&format!("stur x0, [x29, #-{}]", offset));
        }
        PhpType::Array(_) => {
            emitter.instruction(&format!("stur x0, [x29, #-{}]", offset));
        }
    }
}

/// Load a variable from the stack into result registers.
/// Int: [x29, #-offset] → x0
/// Str: [x29, #-offset] → x1 (ptr), [x29, #-(offset-8)] → x2 (len)
pub fn emit_load(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty {
        PhpType::Bool | PhpType::Int => {
            emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));
        }
        PhpType::Float => {
            emitter.instruction(&format!("ldur d0, [x29, #-{}]", offset));
        }
        PhpType::Str => {
            emitter.instruction(&format!("ldur x1, [x29, #-{}]", offset));
            emitter.instruction(&format!("ldur x2, [x29, #-{}]", offset - 8));
        }
        PhpType::Void => {
            emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));
        }
        PhpType::Array(_) => {
            emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));
        }
    }
}

/// Emit sys_write(stdout, ...) for the given type.
/// Str: x1/x2 already set, just emit syscall.
/// Int: call itoa first, then emit syscall.
pub fn emit_write_stdout(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Str => {
            emitter.instruction("mov x0, #1");
            emitter.instruction("mov x16, #4");
            emitter.instruction("svc #0x80");
        }
        PhpType::Bool => {
            // echo true → "1", echo false → nothing (PHP behavior)
            // Handled by the caller in stmt.rs which checks the type
            // and skips for false. Here we just print if nonzero.
            emitter.instruction("bl __rt_itoa");
            emitter.instruction("mov x0, #1");
            emitter.instruction("mov x16, #4");
            emitter.instruction("svc #0x80");
        }
        PhpType::Int => {
            emitter.instruction("bl __rt_itoa");
            emitter.instruction("mov x0, #1");
            emitter.instruction("mov x16, #4");
            emitter.instruction("svc #0x80");
        }
        PhpType::Float => {
            emitter.instruction("bl __rt_ftoa");
            emitter.instruction("mov x0, #1");
            emitter.instruction("mov x16, #4");
            emitter.instruction("svc #0x80");
        }
        PhpType::Void | PhpType::Array(_) => {}
    }
}
