use super::emit::Emitter;
use crate::types::PhpType;

/// Emit a store of `reg` at `[x29, #-offset]`, handling large offsets.
/// Uses x9 as scratch register for offsets > 255.
///
/// For offsets <= 255: single `stur` instruction (9-bit signed immediate).
/// For offsets 256-4095: `sub x9, x29, #offset` then `str reg, [x9]` (12-bit unsigned immediate).
pub fn store_at_offset(emitter: &mut Emitter, reg: &str, offset: usize) {
    store_at_offset_scratch(emitter, reg, offset, "x9");
}

/// Emit a store of `reg` at `[x29, #-offset]` using a custom scratch register.
///
/// For offsets <= 255: single `stur` instruction.
/// For offsets 256-4095: `sub scratch, x29, #offset` then `str reg, [scratch]`.
pub fn store_at_offset_scratch(emitter: &mut Emitter, reg: &str, offset: usize, scratch: &str) {
    if offset <= 255 {
        emitter.instruction(&format!("stur {}, [x29, #-{}]", reg, offset));     // store via unscaled immediate offset
    } else {
        emitter.instruction(&format!("sub {}, x29, #{}", scratch, offset));     // compute stack address for large offset
        emitter.instruction(&format!("str {}, [{}]", reg, scratch));            // store via computed address
    }
}

/// Emit a load into `reg` from `[x29, #-offset]`, handling large offsets.
/// Uses x9 as scratch register for offsets > 255.
///
/// For offsets <= 255: single `ldur` instruction.
/// For offsets 256-4095: `sub x9, x29, #offset` then `ldr reg, [x9]`.
pub fn load_at_offset(emitter: &mut Emitter, reg: &str, offset: usize) {
    load_at_offset_scratch(emitter, reg, offset, "x9");
}

/// Emit a load into `reg` from `[x29, #-offset]` using a custom scratch register.
///
/// For offsets <= 255: single `ldur` instruction.
/// For offsets 256-4095: `sub scratch, x29, #offset` then `ldr reg, [scratch]`.
pub fn load_at_offset_scratch(emitter: &mut Emitter, reg: &str, offset: usize, scratch: &str) {
    if offset <= 255 {
        emitter.instruction(&format!("ldur {}, [x29, #-{}]", reg, offset));     // load via unscaled immediate offset
    } else {
        emitter.instruction(&format!("sub {}, x29, #{}", scratch, offset));     // compute stack address for large offset
        emitter.instruction(&format!("ldr {}, [{}]", reg, scratch));            // load via computed address
    }
}

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
            store_at_offset(emitter, "x0", offset);                             // store int/bool to stack
        }
        PhpType::Float => {
            store_at_offset(emitter, "d0", offset);                             // store float to stack
        }
        PhpType::Str => {
            // Persist string to heap so it survives concat_buf resets
            emitter.instruction("bl __rt_str_persist");                         // copy string to heap, x1=heap_ptr, x2=len
            // Strings use 16 bytes: pointer at offset, length at offset-8
            store_at_offset(emitter, "x1", offset);                             // store string pointer
            store_at_offset(emitter, "x2", offset - 8);                         // store string length
        }
        PhpType::Void => {
            store_at_offset(emitter, "x0", offset);                             // store null sentinel
        }
        PhpType::Mixed
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_) => {
            store_at_offset(emitter, "x0", offset);                             // store array/callable/object/pointer value
        }
    }
}

/// Retain the current value in x0 if it is runtime-refcounted.
pub fn emit_incref_if_refcounted(emitter: &mut Emitter, ty: &PhpType) {
    if ty.is_refcounted() {
        emitter.instruction("str x0, [sp, #-16]!");                             // preserve heap pointer across incref helper call
        emitter.instruction("bl __rt_incref");                                  // retain shared heap value before creating a new owner
        emitter.instruction("ldr x0, [sp], #16");                               // restore original heap pointer after incref
    }
}

/// Release the current value in x0 if it is runtime-refcounted.
pub fn emit_decref_if_refcounted(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Mixed => {
            emitter.instruction("bl __rt_decref_mixed");                        // release mixed cell reference
        }
        PhpType::Array(_) => {
            emitter.instruction("bl __rt_decref_array");                        // release indexed array reference
        }
        PhpType::AssocArray { .. } => {
            emitter.instruction("bl __rt_decref_hash");                         // release associative array reference
        }
        PhpType::Object(_) => {
            emitter.instruction("bl __rt_decref_object");                       // release object reference
        }
        _ => {}
    }
}

/// Load a local variable from the stack into result registers.
///
/// Restores the value into the same registers used by emit_store.
pub fn emit_load(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty {
        PhpType::Bool | PhpType::Int => {
            load_at_offset(emitter, "x0", offset);                              // load int/bool from stack
        }
        PhpType::Float => {
            load_at_offset(emitter, "d0", offset);                              // load float from stack
        }
        PhpType::Str => {
            load_at_offset(emitter, "x1", offset);                              // load string pointer
            load_at_offset(emitter, "x2", offset - 8);                          // load string length
        }
        PhpType::Void => {
            load_at_offset(emitter, "x0", offset);                              // load null sentinel
        }
        PhpType::Mixed
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_) => {
            load_at_offset(emitter, "x0", offset);                              // load array/callable/object/pointer value
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
        PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) => {
            // Convert pointer address in x0 to hex string, then write
            emitter.instruction("bl __rt_ptoa");                                // x0 → x1=ptr, x2=len
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall 4 = write
            emitter.instruction("svc #0x80");                                   // invoke kernel
        }
        PhpType::Mixed => {
            emitter.instruction("bl __rt_mixed_write_stdout");                  // inspect boxed mixed payload and print if scalar/string
        }
        PhpType::Void | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Callable | PhpType::Object(_) => {} // null/array/callable/object: nothing to print
    }
}
