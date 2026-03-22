use super::emit::Emitter;
use crate::types::PhpType;

/// Store the current result registers to the stack.
/// Int: x0 → [x29, #-offset]
/// Str: x1 (ptr) → [x29, #-offset], x2 (len) → [x29, #-(offset-8)]
pub fn emit_store(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty {
        PhpType::Int => {
            emitter.instruction(&format!("stur x0, [x29, #-{}]", offset));
        }
        PhpType::Str => {
            emitter.instruction(&format!("stur x1, [x29, #-{}]", offset));
            emitter.instruction(&format!("stur x2, [x29, #-{}]", offset - 8));
        }
        PhpType::Void => {}
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
        PhpType::Int => {
            emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));
        }
        PhpType::Str => {
            emitter.instruction(&format!("ldur x1, [x29, #-{}]", offset));
            emitter.instruction(&format!("ldur x2, [x29, #-{}]", offset - 8));
        }
        PhpType::Void => {}
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
        PhpType::Int => {
            emitter.instruction("bl __rt_itoa");
            emitter.instruction("mov x0, #1");
            emitter.instruction("mov x16, #4");
            emitter.instruction("svc #0x80");
        }
        PhpType::Void | PhpType::Array(_) => {}
    }
}
