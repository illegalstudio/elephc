//! Purpose:
//! Provides type-directed load, store, branch, jump, conversion, and refcount helpers for result values.
//! Normalizes scalar, string, array, object, and Mixed value movement across emitters.
//!
//! Called from:
//! - `crate::codegen::expr`, `crate::codegen::stmt`, and function cleanup emitters
//!
//! Key details:
//! - Refcounted values require balanced retain/release behavior around borrowed and owned temporaries.

use crate::codegen::callable_descriptor;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::types::PhpType;

use super::calls::{emit_call_label, emit_pop_reg, emit_push_reg};
use super::frame::{emit_load_from_address, load_at_offset, store_at_offset};
use super::registers::{float_result_reg, int_result_reg, string_result_regs};
use crate::codegen::sentinels::tagged_scalar_tag_reg;

/// Stores the current result value (in result registers) of the given type at a stack frame offset.
///
/// For `PhpType::Str`, calls `__rt_str_persist` to copy the string into owned heap storage,
/// then stores the pointer and length as separate values. For `PhpType::Void`/`Never`, stores
/// a null sentinel. Refcounted types (array, object, etc.) are stored directly without
/// incrementing the refcount—the caller owns the result register value.
pub fn emit_store(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty {
        PhpType::Bool | PhpType::Int | PhpType::Resource(_) => {
            store_at_offset(emitter, int_result_reg(emitter), offset);                  // store scalar integer-like value to stack
        }
        PhpType::Float => {
            store_at_offset(emitter, float_result_reg(emitter), offset);                // store float to stack
        }
        PhpType::Str => {
            emit_call_label(emitter, "__rt_str_persist");                                // copy the current string payload into owned heap storage when needed
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            store_at_offset(emitter, ptr_reg, offset);                                  // store string pointer
            store_at_offset(emitter, len_reg, offset - 8);                              // store string length
        }
        PhpType::Void | PhpType::Never => {
            store_at_offset(emitter, int_result_reg(emitter), offset);                  // store null sentinel
        }
        PhpType::TaggedScalar => {
            store_at_offset(emitter, int_result_reg(emitter), offset);                  // store tagged scalar payload word
            store_at_offset(emitter, tagged_scalar_tag_reg(emitter), offset - 8);       // store tagged scalar tag word
        }
        PhpType::Iterable
        | PhpType::Mixed
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

/// Retains a refcounted result value before it is reused or overwritten.
///
/// If `ty` is a refcounted heap value or callable descriptor, emits a retain call after
/// preserving the heap pointer in a temporary slot. The target architecture dictates
/// register preservation conventions: AArch64 uses a pre-decrement stack store, while
/// x86_64 uses a 16-byte aligned push/pop pair to maintain SysV ABI alignment.
pub fn emit_incref_if_refcounted(emitter: &mut Emitter, ty: &PhpType) {
    if matches!(ty, PhpType::Callable) {
        callable_descriptor::emit_retain_current_descriptor(emitter);
        return;
    }
    if ty.is_refcounted() {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("str x0, [sp, #-16]!");                     // preserve heap pointer across incref helper call
                emitter.instruction("bl __rt_incref");                          // retain shared heap value before creating a new owner
                emitter.instruction("ldr x0, [sp], #16");                       // restore original heap pointer after incref
            }
            Arch::X86_64 => {
                emit_push_reg(emitter, "rax");                                          // preserve the heap pointer in a 16-byte temporary slot to keep the SysV stack aligned across the helper call
                emitter.instruction("call __rt_incref");                        // retain shared heap value before creating a new owner
                emit_pop_reg(emitter, "rax");                                           // restore the original heap pointer after the aligned incref helper call
            }
        }
    }
}

/// Releases a refcounted result value held in `x0`.
///
/// Dispatches to the appropriate runtime helper based on the PHP type:
/// - `Mixed`/`Union` → `__rt_decref_mixed`
/// - `Array` → `__rt_decref_array`
/// - `AssocArray` → `__rt_decref_hash`
/// - `Object` → `__rt_decref_object`
/// - `Iterable` → `__rt_decref_any` (inspects heap kind)
/// - `Callable` → `__rt_callable_descriptor_release`
/// - Non-refcounted types → no-op
pub fn emit_decref_if_refcounted(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Mixed | PhpType::Union(_) => {
            emit_call_label(emitter, "__rt_decref_mixed");                              // release mixed cell reference
        }
        PhpType::Array(_) => {
            emit_call_label(emitter, "__rt_decref_array");                              // release indexed array reference
        }
        PhpType::AssocArray { .. } => {
            emit_call_label(emitter, "__rt_decref_hash");                               // release associative array reference
        }
        PhpType::Object(_) => {
            emit_call_label(emitter, "__rt_decref_object");                             // release object reference
        }
        PhpType::Iterable => {
            emit_call_label(emitter, "__rt_decref_any");                                // release the erased iterable payload by inspecting its heap kind
        }
        PhpType::Callable => {
            callable_descriptor::emit_release_current_descriptor(emitter);
        }
        _ => {}
    }
}

/// Releases the payload of a local reference-counted cell and the cell itself.
///
/// Pushes `cell_reg` as a temporary, then:
/// - For `PhpType::Str`: loads the string payload and calls `__rt_heap_free_safe`.
/// - For other refcounted types: loads the heap pointer and calls `emit_decref_if_refcounted`.
/// Pops the preserved cell pointer and calls `__rt_heap_free` to release the cell.
/// Used during function epilogue for local variables that held borrowed or owned refs.
pub fn emit_release_local_ref_cell(emitter: &mut Emitter, cell_reg: &str, value_ty: &PhpType) {
    emit_push_reg(emitter, cell_reg);                                           // preserve the owned reference cell pointer while releasing its payload
    match value_ty.codegen_repr() {
        PhpType::Str => {
            emit_load_from_address(emitter, int_result_reg(emitter), cell_reg, 0);
            emit_call_label(emitter, "__rt_heap_free_safe");                   // release the owned string payload stored inside the local reference cell
        }
        ty if ty.is_refcounted() => {
            emit_load_from_address(emitter, int_result_reg(emitter), cell_reg, 0);
            emit_decref_if_refcounted(emitter, &ty);
        }
        PhpType::Callable => {
            emit_load_from_address(emitter, int_result_reg(emitter), cell_reg, 0);
            callable_descriptor::emit_release_current_descriptor(emitter);
        }
        _ => {}
    }
    emit_pop_reg(emitter, int_result_reg(emitter));                             // restore the owned reference cell pointer for heap release
    emit_call_label(emitter, "__rt_heap_free");                                // release the local reference cell itself
}

/// Loads a value of the given type from a stack frame offset into result registers.
///
/// For `PhpType::Str`, loads both the string pointer and length. For scalar types (bool, int,
/// float, resource), loads into the appropriate result register. For void/never, loads a null
/// sentinel. For compound types (array, object, callable, pointer, etc.), loads the heap pointer.
pub fn emit_load(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty {
        PhpType::Bool | PhpType::Int | PhpType::Resource(_) => {
            load_at_offset(emitter, int_result_reg(emitter), offset);                   // load scalar integer-like value from stack
        }
        PhpType::Float => {
            load_at_offset(emitter, float_result_reg(emitter), offset);                 // load float from stack
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            load_at_offset(emitter, ptr_reg, offset);                                   // load string pointer
            load_at_offset(emitter, len_reg, offset - 8);                               // load string length
        }
        PhpType::Void | PhpType::Never => {
            load_at_offset(emitter, int_result_reg(emitter), offset);                   // load null sentinel
        }
        PhpType::TaggedScalar => {
            load_at_offset(emitter, int_result_reg(emitter), offset);                   // load tagged scalar payload word
            load_at_offset(emitter, tagged_scalar_tag_reg(emitter), offset - 8);        // load tagged scalar tag word
        }
        PhpType::Iterable
        | PhpType::Mixed
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

/// Branches to `label` when the integer result register is zero (coerced truthiness).
///
/// AArch64: `cbz` (compare and branch if zero). x86_64: `test` + `je` (set cc + conditional jump).
/// The integer result represents a coerced PHP truthiness value used in conditional contexts.
pub fn emit_branch_if_int_result_zero(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction(&format!("cbz {}, {}", int_result_reg(emitter), label)); //branch when the coerced integer truthiness result is zero
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction(&format!("test {}, {}", int_result_reg(emitter), int_result_reg(emitter))); //test whether the coerced integer truthiness result is zero
            emitter.instruction(&format!("je {}", label));                      // branch when the coerced integer truthiness result is zero
        }
    }
}

/// Branches to `label` when the integer result register is non-zero (coerced truthiness).
///
/// AArch64: `cbnz` (compare and branch if non-zero). x86_64: `test` + `jne` (set cc + conditional jump).
/// The integer result represents a coerced PHP truthiness value used in conditional contexts.
pub fn emit_branch_if_int_result_nonzero(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction(&format!("cbnz {}, {}", int_result_reg(emitter), label)); //branch when the coerced integer truthiness result is non-zero
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction(&format!("test {}, {}", int_result_reg(emitter), int_result_reg(emitter))); //test whether the coerced integer truthiness result is non-zero
            emitter.instruction(&format!("jne {}", label));                     // branch when the coerced integer truthiness result is non-zero
        }
    }
}

/// Unconditionally jumps to `label` for control flow transfer.
///
/// AArch64 uses `b label`. x86_64 uses `jmp label`.
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

/// Promotes the integer result register value to the floating-point result register.
///
/// AArch64: `scvtf` (signed convert to floating). x86_64: `cvtsi2sd` (signed integer to scalar double).
/// Used when a PHP int must be coerced to a float in mixed arithmetic contexts.
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

/// Truncates the floating-point result register value to the integer result register.
///
/// AArch64: `fcvtzs` (floating-point convert to signed fixed-point). x86_64: `cvttsd2si` (convert with truncation).
/// Used when a PHP float must be coerced to int in mixed arithmetic contexts.
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

/// Loads a 64-bit immediate integer `value` into `reg`.
///
/// AArch64: uses `mov` for values in [−65536, 65535]; otherwise constructs the value using
/// `movz`/`movk` pairs for each 16-bit chunk (low, bits 16–31, 32–47, 48–63).
/// x86_64: a single `mov` instruction handles any immediate since x86_64 immediates are 32-bit
/// sign-extended to 64-bit.
pub fn emit_load_int_immediate(emitter: &mut Emitter, reg: &str, value: i64) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if (0..=65535).contains(&value) {
                emitter.instruction(&format!("mov {}, #{}", reg, value));       // load a small non-negative immediate directly into the target register
            } else if (-65536..0).contains(&value) {
                emitter.instruction(&format!("mov {}, #{}", reg, value));       // load a small negative immediate directly into the target register
            } else {
                let uval = value as u64;
                emitter.instruction(&format!("movz {}, #0x{:x}", reg, uval & 0xFFFF)); //seed the low 16 bits of the wider immediate value
                if (uval >> 16) & 0xFFFF != 0 {
                    emitter.instruction(&format!(                               // patch bits 16-31 of the wider immediate value
                        "movk {}, #0x{:x}, lsl #16",
                        reg,
                        (uval >> 16) & 0xFFFF
                    ));
                }
                if (uval >> 32) & 0xFFFF != 0 {
                    emitter.instruction(&format!(                               // patch bits 32-47 of the wider immediate value
                        "movk {}, #0x{:x}, lsl #32",
                        reg,
                        (uval >> 32) & 0xFFFF
                    ));
                }
                if (uval >> 48) & 0xFFFF != 0 {
                    emitter.instruction(&format!(                               // patch bits 48-63 of the wider immediate value
                        "movk {}, #0x{:x}, lsl #48",
                        reg,
                        (uval >> 48) & 0xFFFF
                    ));
                }
            }
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, {}", reg, value));            // load the immediate directly into the native x86_64 register
        }
    }
}

/// Writes a result value of the given PHP type to stdout.
///
/// Dispatches to the appropriate runtime helper:
/// - `Str` → `emit_write_current_string_stdout` directly
/// - `Bool`/`Int` → `__rt_itoa` then `emit_write_current_string_stdout`
/// - `Float` → `__rt_ftoa` then `emit_write_current_string_stdout`
/// - `Pointer`/`Buffer`/`Packed` → `__rt_ptoa` then `emit_write_current_string_stdout`
/// - `Resource` → `__rt_resource_write_stdout`
/// - `Mixed` → `__rt_mixed_write_stdout`
/// - `Iterable` → `__rt_iterable_write_stdout`
/// - Other types (void, array, callable, object) → no-op
pub fn emit_write_stdout(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Str => {
            emit_write_current_string_stdout(emitter);
        }
        PhpType::Bool | PhpType::Int => {
            emit_call_label(emitter, "__rt_itoa");
            emit_write_current_string_stdout(emitter);
        }
        PhpType::TaggedScalar => {
            emit_call_label(emitter, "__rt_itoa");                                      // convert the tagged scalar payload; callers suppress the null case first
            emit_write_current_string_stdout(emitter);
        }
        PhpType::Resource(_) => {
            emit_call_label(emitter, "__rt_resource_write_stdout");
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
        PhpType::Iterable => {
            emit_call_label(emitter, "__rt_iterable_write_stdout");                     // dispatch echo iterable through the heap-kind-aware writer instead of the mixed-cell writer
        }
        PhpType::Void
        | PhpType::Never
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Callable
        | PhpType::Object(_) => {}
    }
}

/// Writes the current string result registers to stdout through `__rt_stdout_write`.
///
/// Moves the string pointer/length out of the platform string result registers and
/// into the `__rt_stdout_write` calling convention (byte pointer in `x0`/`rdi`,
/// length in `x1`/`rsi`), then calls the runtime indirection. That routine performs
/// the actual `write(1, ptr, len)` syscall (or, in `--web` builds with capture
/// enabled, hands the bytes to `elephc_web_write`).
///
/// AArch64: string result regs are `(x1, x2)` → set `x0=x1`, `x1=x2`.
/// x86_64: string result regs are `(rax, rdx)` → set `rdi=rax`, `rsi=rdx`.
fn emit_write_current_string_stdout(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            emitter.instruction(&format!("mov x0, {}", ptr_reg));               // stdout_write ptr arg = current string pointer (copy before x1 is overwritten with the length, since ptr lives in x1)
            emitter.instruction(&format!("mov x1, {}", len_reg));               // stdout_write len arg = current string length
            emit_call_label(emitter, "__rt_stdout_write");                              // route the terminal write through the stdout-write indirection
        }
        Arch::X86_64 => {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            emitter.instruction(&format!("mov rsi, {}", len_reg));              // stdout_write len arg = current string length
            emitter.instruction(&format!("mov rdi, {}", ptr_reg));              // stdout_write ptr arg = current string pointer
            emit_call_label(emitter, "__rt_stdout_write");                              // route the terminal write through the stdout-write indirection
        }
    }
}
