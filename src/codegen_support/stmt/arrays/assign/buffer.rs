//! Purpose:
//! Lowers typed buffer element assignment using element-size addressing.
//! Evaluates assignment operands and writes the coerced value into the selected container.
//!
//! Called from:
//! - `crate::codegen_support::stmt::arrays::assign`
//!
//! Key details:
//! - Container mutation must follow copy-on-write and element ownership expectations.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::{coerce_result_to_type, emit_expr};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::ArrayAssignTarget;

/// Emits machine code to assign a value to a typed buffer element at a given index.
///
/// # Arguments
/// - `target`: the buffer local slot and its element type
/// - `index`: expression yielding the element index (coerced to `i64`)
/// - `value`: expression yielding the assigned value (coerced to `target.elem_ty`)
///
/// # Generated sequence
/// 1. Loads the buffer header pointer (respects `target.is_ref` for ref slots)
/// 2. Evaluates `index`, pushes the result, evaluates `value`, pushes the value
/// 3. Reloads the preserved index and buffer pointer from the stack
/// 4. Guards against use-after-free: aborts via `__rt_buffer_use_after_free` if the
///    buffer header pointer is null (nulled by `buffer_free()`)
/// 5. Guards against out-of-bounds: rejects negative indices and index ≥ logical length,
///    aborts via `__rt_buffer_bounds_fail` with the index and length arguments
/// 6. Computes the element address as `buffer_base + 16 + index * stride`
/// 7. Pops the preserved value and stores it directly to the element slot
/// 8. Cleans up the two preserved slots (index and buffer pointer)
///
/// # Register usage
/// - `buffer_reg`: scratch symbol register holding the buffer header pointer
/// - `index_reg`: temporary integer register for the scaled index
/// - `len_reg`: fixed by architecture (`x11` on AArch64, `rcx` on x86_64) for the length/stride
pub(super) fn emit_buffer_array_assign(
    target: &ArrayAssignTarget<'_>,
    index: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let buffer_reg = abi::symbol_scratch_reg(emitter);
    let index_reg = abi::temp_int_reg(emitter.target);
    let len_reg = match emitter.target.arch {
        crate::codegen_support::platform::Arch::AArch64 => "x11",
        crate::codegen_support::platform::Arch::X86_64 => "rcx",
    };
    if target.is_ref {
        abi::load_at_offset(emitter, buffer_reg, target.offset);                        // load the ref slot that points at the buffer local
        abi::emit_load_from_address(emitter, buffer_reg, buffer_reg, 0);                // dereference the ref slot to get the buffer header pointer
    } else {
        abi::load_at_offset(emitter, buffer_reg, target.offset);                        // load the buffer header pointer from the local slot
    }
    abi::emit_push_reg(emitter, buffer_reg);                                           // preserve the buffer pointer while evaluating the index
    let index_ty = emit_expr(index, emitter, ctx, data);
    coerce_result_to_type(emitter, ctx, data, &index_ty, &PhpType::Int);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                        // preserve the computed element index across value evaluation
    let mut val_ty = emit_expr(value, emitter, ctx, data);
    if matches!(val_ty, PhpType::Mixed | PhpType::Union(_))
        && !matches!(target.elem_ty, PhpType::Mixed | PhpType::Union(_))
        && crate::codegen_support::expr::can_coerce_result_to_type(&val_ty, &target.elem_ty)
    {
        coerce_result_to_type(emitter, ctx, data, &val_ty, &target.elem_ty);
        val_ty = target.elem_ty.clone();
    }
    match &val_ty {
        PhpType::Float => {
            abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));         // preserve the float payload across address computation
        }
        PhpType::Str => {
            let (ptr_reg, len_result_reg) = abi::string_result_regs(emitter);
            abi::emit_push_reg_pair(emitter, ptr_reg, len_result_reg);                // preserve the unsupported string payload for consistent stack cleanup
        }
        _ => {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                // preserve the scalar or pointer payload across address computation
        }
    }
    match emitter.target.arch {
        crate::codegen_support::platform::Arch::AArch64 => {
            emitter.instruction(&format!("ldr {}, [sp, #16]", index_reg));      // reload the target index without disturbing the saved value slot
            emitter.instruction(&format!("ldr {}, [sp, #32]", buffer_reg));     // reload the buffer header pointer without disturbing the saved value slot
        }
        crate::codegen_support::platform::Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, QWORD PTR [rsp + 16]", index_reg)); // reload the target index without disturbing the saved value slot
            emitter.instruction(&format!("mov {}, QWORD PTR [rsp + 32]", buffer_reg)); // reload the buffer header pointer without disturbing the saved value slot
        }
    }
    let uaf_ok = ctx.next_label("buf_st_uaf_ok");
    match emitter.target.arch {
        crate::codegen_support::platform::Arch::AArch64 => {
            emitter.instruction(&format!("cbnz {}, {}", buffer_reg, uaf_ok));   // skip the fatal helper when the buffer header pointer is still live
            emitter.instruction("b __rt_buffer_use_after_free");                // abort immediately when the buffer local was nulled by buffer_free()
        }
        crate::codegen_support::platform::Arch::X86_64 => {
            emitter.instruction(&format!("test {}, {}", buffer_reg, buffer_reg)); // check whether the restored buffer header pointer is null
            emitter.instruction(&format!("jne {}", uaf_ok));                    // continue only when the buffer header pointer is still live
            emitter.instruction("jmp __rt_buffer_use_after_free");              // abort immediately when the buffer local was nulled by buffer_free()
        }
    }
    emitter.label(&uaf_ok);
    let bounds_ok = ctx.next_label("buffer_store_ok");
    let oob_ok = ctx.next_label("buf_st_oob_ok");
    match emitter.target.arch {
        crate::codegen_support::platform::Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #0", index_reg));             // reject negative buffer indexes before touching the payload
            emitter.instruction(&format!("b.ge {}", oob_ok));                   // continue once the target index is non-negative
            emitter.instruction("b __rt_buffer_bounds_fail");                   // abort immediately on negative buffer indexes
            emitter.label(&oob_ok);
            abi::emit_load_from_address(emitter, len_reg, buffer_reg, 0);             // load the logical buffer length from the header
            emitter.instruction(&format!("cmp {}, {}", index_reg, len_reg));    // compare the target index against the logical buffer length
            emitter.instruction(&format!("b.lo {}", bounds_ok));                // continue once the write target is still in bounds
            emitter.instruction(&format!("mov x0, {}", index_reg));             // pass the out-of-bounds index to the fatal helper for parity with the ARM path
            emitter.instruction(&format!("mov x1, {}", len_reg));               // pass the logical buffer length to the fatal helper for parity with the ARM path
            emitter.instruction("bl __rt_buffer_bounds_fail");                  // abort the program on invalid buffer writes
            emitter.label(&bounds_ok);
            abi::emit_load_from_address(emitter, len_reg, buffer_reg, 8);             // load the element stride from the buffer header
            emitter.instruction(&format!("add {}, {}, #16", buffer_reg, buffer_reg)); // skip the buffer header to reach the payload base
            emitter.instruction(&format!("madd {}, {}, {}, {}", buffer_reg, index_reg, len_reg, buffer_reg)); // compute payload base + index*stride for the target element
        }
        crate::codegen_support::platform::Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, 0", index_reg));              // reject negative buffer indexes before touching the payload
            emitter.instruction(&format!("jge {}", oob_ok));                    // continue once the target index is non-negative
            emitter.instruction("jmp __rt_buffer_bounds_fail");                 // abort immediately on negative buffer indexes
            emitter.label(&oob_ok);
            abi::emit_load_from_address(emitter, len_reg, buffer_reg, 0);             // load the logical buffer length from the header
            emitter.instruction(&format!("cmp {}, {}", index_reg, len_reg));    // compare the target index against the logical buffer length
            emitter.instruction(&format!("jl {}", bounds_ok));                  // continue once the write target is still in bounds
            emitter.instruction("jmp __rt_buffer_bounds_fail");                 // abort the program on invalid buffer writes
            emitter.label(&bounds_ok);
            abi::emit_load_from_address(emitter, len_reg, buffer_reg, 8);             // load the element stride from the buffer header
            emitter.instruction(&format!("imul {}, {}", index_reg, len_reg));   // scale the target index by the element stride in bytes
            emitter.instruction(&format!("add {}, 16", buffer_reg));            // skip the buffer header to reach the payload base
            emitter.instruction(&format!("add {}, {}", buffer_reg, index_reg)); // compute payload base + index*stride for the target element
        }
    }
    match &target.elem_ty {
        PhpType::Float => {
            abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter));         // restore the float payload before the direct store
            abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), buffer_reg, 0); // store the float payload directly into the contiguous element slot
        }
        PhpType::Packed(_) => {
            emitter.comment("WARNING: packed buffer whole-element stores are not supported");
            abi::emit_release_temporary_stack(emitter, 16);                           // drop the preserved placeholder payload for unsupported packed stores
        }
        _ => {
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                // restore the scalar or pointer payload before the direct store
            abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), buffer_reg, 0); // store the scalar or pointer payload directly into the contiguous element slot
        }
    }
    abi::emit_release_temporary_stack(emitter, 32);                                  // drop the preserved index and buffer pointer slots after completing the write
}
