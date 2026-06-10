//! Purpose:
//! Emits PHP `array_pop` builtin calls that mutate array arguments in place.
//! Handles COW preparation and writes any replacement array pointer back to caller storage.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Mutating/ref-like arguments must avoid value-temp preevaluation so PHP-visible storage is updated.

use super::ensure_unique_arg::emit_ensure_unique_arg;
use super::store_mutating_arg::emit_store_mutating_arg;
use crate::codegen::abi;
use crate::codegen::NULL_SENTINEL;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the PHP `array_pop` builtin: removes and returns the last element of an indexed array.
///
/// For x86_64: calls `ensure_unique_arg` and `store_mutating_arg` to handle COW and write back
/// the modified array pointer to caller storage, then loads the array header from `rax` to
/// obtain/modify the length and load the popped element into registers.
///
/// For ARM64: loads array header from `x0`, decrements length, and materializes the popped
/// element into return registers (`x0`/`d0` for scalars, `x1`/`x2` for strings).
///
/// On empty array: returns a null sentinel (i64::MAX - 1 on x86_64, 0x7FFFFFFFFFFFFFFE on ARM64).
///
/// # Arguments
/// * `_name` - unused (builtin dispatches by name)
/// * `args` - must contain exactly one argument: the array expression
/// * `emitter` - target-aware code emitter
/// * `ctx` - codegen context (labels, frame layout, variable allocation)
/// * `data` - data section for constants
///
/// # Returns
/// `Some(elem_ty)` where `elem_ty` is the element type (PhpType::Int, PhpType::Str, or PhpType::Array
/// for arrays-of-arrays), or `None` if the array type could not be determined.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_pop()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let elem_ty = match &arr_ty {
        PhpType::Array(t) => *t.clone(),
        _ => PhpType::Int,
    };

    let empty_label = ctx.next_label("array_pop_empty");
    let end_label = ctx.next_label("array_pop_end");
    let tagged_int_result =
        crate::codegen::sentinels::null_repr_is_tagged() && matches!(elem_ty, PhpType::Int);

    if emitter.target.arch == Arch::X86_64 {
        emit_ensure_unique_arg(emitter, &arr_ty);
        emit_store_mutating_arg(emitter, ctx, &args[0]);
        emitter.instruction("mov r10, QWORD PTR [rax]");                        // load the current indexed-array length before checking whether the pop operation is empty
        emitter.instruction(&format!("test r10, r10"));                         // check whether the indexed array currently stores any elements
        emitter.instruction(&format!("jz {}", empty_label));                    // return null when array_pop runs on an empty indexed array
        emitter.instruction("sub r10, 1");                                      // decrement the indexed-array length to point at the removed last element
        emitter.instruction("mov QWORD PTR [rax], r10");                        // persist the decremented indexed-array length back into the array header
        match &elem_ty {
            PhpType::Str => {
                emitter.instruction("lea r11, [rax + 24]");                     // compute the first string-slot payload address in the source indexed array
                emitter.instruction("shl r10, 4");                              // scale the removed-element index by the 16-byte string-slot size
                emitter.instruction("add r11, r10");                            // advance to the removed string-slot payload within the indexed array
                emitter.instruction("mov rax, QWORD PTR [r11]");                // load the removed string pointer into the primary x86_64 string result register
                emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");            // load the removed string length into the secondary x86_64 string result register
            }
            _ => {
                emitter.instruction("lea r11, [rax + 24]");                     // compute the first scalar-slot payload address in the source indexed array
                emitter.instruction("mov rax, QWORD PTR [r11 + r10 * 8]");      // load the removed scalar payload from the last live indexed-array slot
            }
        }
        if tagged_int_result {
            crate::codegen::sentinels::emit_tagged_scalar_from_int_result(emitter);
        }
        emitter.instruction(&format!("jmp {}", end_label));                     // skip the empty-array null sentinel path after loading the removed payload

        emitter.label(&empty_label);
        if tagged_int_result {
            crate::codegen::sentinels::emit_tagged_scalar_null(emitter);
        } else {
            abi::emit_load_int_immediate(emitter, "rax", NULL_SENTINEL);       // materialize the shared null sentinel as the empty-array result on x86_64
        }
        emitter.label(&end_label);

        if tagged_int_result {
            return Some(PhpType::TaggedScalar);
        }
        return Some(elem_ty);
    }

    // -- check if array is empty --
    emitter.instruction("ldr x9, [x0]");                                        // load current array length into x9
    emitter.instruction(&format!("cbz x9, {}", empty_label));                   // if length == 0, jump to empty handler

    // -- decrement array length to remove last element --
    emitter.instruction("sub x9, x9, #1");                                      // decrement length by 1
    emitter.instruction("str x9, [x0]");                                        // store decremented length back to array header
    match &elem_ty {
        PhpType::Int => {
            // -- load the popped integer element --
            emitter.instruction("add x0, x0, #24");                             // advance past array header (24 bytes) to data area
            emitter.instruction("ldr x0, [x0, x9, lsl #3]");                    // load int at index x9 (offset = x9 * 8 bytes)
        }
        PhpType::Str => {
            // -- load the popped string element (ptr + len) --
            emitter.instruction("lsl x10, x9, #4");                             // multiply index by 16 (each string entry = 16 bytes)
            emitter.instruction("add x0, x0, x10");                             // advance pointer by element offset
            emitter.instruction("add x0, x0, #24");                             // skip past array header to data area
            emitter.instruction("ldr x1, [x0]");                                // load string pointer from element
            emitter.instruction("ldr x2, [x0, #8]");                            // load string length from element + 8
        }
        _ => {}
    }
    if tagged_int_result {
        crate::codegen::sentinels::emit_tagged_scalar_from_int_result(emitter);
    }
    emitter.instruction(&format!("b {}", end_label));                           // skip empty handler

    // -- empty array: return null sentinel --
    emitter.label(&empty_label);
    if tagged_int_result {
        crate::codegen::sentinels::emit_tagged_scalar_null(emitter);
    } else {
        let sentinel = NULL_SENTINEL as u64;
        emitter.instruction(&format!("movz x0, #0x{:X}", sentinel & 0xFFFF));   // load null sentinel bits [15:0]
        emitter.instruction(&format!("movk x0, #0x{:X}, lsl #16", (sentinel >> 16) & 0xFFFF)); // load null sentinel bits [31:16]
        emitter.instruction(&format!("movk x0, #0x{:X}, lsl #32", (sentinel >> 32) & 0xFFFF)); // load null sentinel bits [47:32]
        emitter.instruction(&format!("movk x0, #0x{:X}, lsl #48", (sentinel >> 48) & 0xFFFF)); // load null sentinel bits [63:48] = 0x7FFFFFFFFFFFFFFE
    }

    emitter.label(&end_label);

    if tagged_int_result {
        return Some(PhpType::TaggedScalar);
    }
    Some(elem_ty)
}
