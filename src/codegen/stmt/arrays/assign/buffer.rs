use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::ArrayAssignTarget;

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
        crate::codegen::platform::Arch::AArch64 => "x11",
        crate::codegen::platform::Arch::X86_64 => "rcx",
    };
    if target.is_ref {
        abi::load_at_offset(emitter, buffer_reg, target.offset);                        // load the ref slot that points at the buffer local
        abi::emit_load_from_address(emitter, buffer_reg, buffer_reg, 0);                // dereference the ref slot to get the buffer header pointer
    } else {
        abi::load_at_offset(emitter, buffer_reg, target.offset);                        // load the buffer header pointer from the local slot
    }
    abi::emit_push_reg(emitter, buffer_reg);                                           // preserve the buffer pointer while evaluating the index
    emit_expr(index, emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                        // preserve the computed element index across value evaluation
    let val_ty = emit_expr(value, emitter, ctx, data);
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
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction(&format!("ldr {}, [sp, #16]", index_reg));      // reload the target index without disturbing the saved value slot
            emitter.instruction(&format!("ldr {}, [sp, #32]", buffer_reg));     // reload the buffer header pointer without disturbing the saved value slot
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, QWORD PTR [rsp + 16]", index_reg)); // reload the target index without disturbing the saved value slot
            emitter.instruction(&format!("mov {}, QWORD PTR [rsp + 32]", buffer_reg)); // reload the buffer header pointer without disturbing the saved value slot
        }
    }
    let uaf_ok = ctx.next_label("buf_st_uaf_ok");
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction(&format!("cbnz {}, {}", buffer_reg, uaf_ok));   // skip the fatal helper when the buffer header pointer is still live
            emitter.instruction("b __rt_buffer_use_after_free");                // abort immediately when the buffer local was nulled by buffer_free()
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction(&format!("test {}, {}", buffer_reg, buffer_reg)); // check whether the restored buffer header pointer is null
            emitter.instruction(&format!("jne {}", uaf_ok));                    // continue only when the buffer header pointer is still live
            emitter.instruction("jmp __rt_buffer_use_after_free");              // abort immediately when the buffer local was nulled by buffer_free()
        }
    }
    emitter.label(&uaf_ok);
    let bounds_ok = ctx.next_label("buffer_store_ok");
    let oob_ok = ctx.next_label("buf_st_oob_ok");
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
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
        crate::codegen::platform::Arch::X86_64 => {
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
