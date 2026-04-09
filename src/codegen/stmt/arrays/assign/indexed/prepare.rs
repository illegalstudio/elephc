use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::super::super::super::helpers;
use super::super::ArrayAssignTarget;

pub(super) struct IndexedAssignState {
    pub(super) val_ty: PhpType,
    pub(super) effective_store_ty: PhpType,
    pub(super) stores_refcounted_pointer: bool,
}

pub(super) fn prepare_indexed_array_assign(
    target: &ArrayAssignTarget<'_>,
    index: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> IndexedAssignState {
    if target.is_ref {
        abi::load_at_offset(emitter, "x9", target.offset);                            // load ref pointer
        emitter.instruction("ldr x0, [x9]");                                       // dereference to get array heap pointer
    } else {
        abi::load_at_offset(emitter, "x0", target.offset);                            // load array heap pointer from stack frame
    }
    emitter.instruction("bl __rt_array_ensure_unique");                            // split shared indexed arrays before direct indexed writes mutate storage
    if target.is_ref {
        abi::load_at_offset(emitter, "x13", target.offset);                           // load ref pointer
        emitter.instruction("str x0, [x13]");                                      // persist the unique array pointer through the reference slot
    } else {
        abi::store_at_offset(emitter, "x0", target.offset);                           // persist the unique array pointer in the local slot
    }
    emitter.instruction("str x0, [sp, #-16]!");                                    // push array pointer onto stack
    emit_expr(index, emitter, ctx, data);
    emitter.instruction("str x0, [sp, #-16]!");                                    // push computed index onto stack
    let val_ty = emit_expr(value, emitter, ctx, data);
    helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    match &val_ty {
        PhpType::Str => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                        // preserve string pointer/length across growth helpers
        }
        PhpType::Float => {
            emitter.instruction("fmov x12, d0");                                   // move float bits into an integer register for stack preservation
            emitter.instruction("str x12, [sp, #-16]!");                           // preserve float bits across growth helpers
        }
        _ => {
            emitter.instruction("str x0, [sp, #-16]!");                            // preserve scalar or heap pointer value across growth helpers
        }
    }
    let effective_store_ty = if matches!(target.elem_ty, PhpType::Mixed) {
        PhpType::Mixed
    } else if target.elem_ty != val_ty {
        val_ty.clone()
    } else {
        target.elem_ty.clone()
    };
    if effective_store_ty != target.elem_ty {
        let updated_ty = PhpType::Array(Box::new(effective_store_ty.clone()));
        ctx.update_var_type_and_ownership(
            target.array,
            updated_ty.clone(),
            helpers::local_slot_ownership_after_store(&updated_ty),
        );
    }
    let stores_refcounted_pointer = matches!(
        effective_store_ty,
        PhpType::Mixed | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_)
    );
    emitter.instruction("ldr x9, [sp, #16]");                                      // reload index without disturbing the preserved value on top of the stack
    emitter.instruction("ldr x10, [sp, #32]");                                     // reload array pointer without disturbing the preserved value on top of the stack
    emitter.instruction("ldr x11, [x10]");                                         // load the original array length before any growth or extension
    emitter.instruction("ldr x12, [x10, #8]");                                     // load the current array capacity before any growth
    let grow_check = ctx.next_label("array_assign_grow_check");
    let grow_ready = ctx.next_label("array_assign_grow_ready");
    emitter.label(&grow_check);
    emitter.instruction("cmp x9, x12");                                            // does the target index fit within the current capacity?
    emitter.instruction(&format!("b.lo {}", grow_ready));                          // skip growth once the target slot is addressable
    emitter.instruction("str x9, [sp, #-16]!");                                    // preserve the target index across the growth helper
    emitter.instruction("mov x0, x10");                                            // move the current array pointer into the growth helper argument register
    emitter.instruction("bl __rt_array_grow");                                     // grow the indexed array until the target slot fits
    emitter.instruction("mov x10, x0");                                            // keep the possibly-reallocated array pointer in x10
    emitter.instruction("ldr x9, [sp], #16");                                      // restore the target index after growth
    emitter.instruction("ldr x12, [x10, #8]");                                     // reload the new array capacity after growth
    emitter.instruction(&format!("b {}", grow_check));                             // continue growing until the target slot fits
    emitter.label(&grow_ready);
    if target.is_ref {
        abi::load_at_offset_scratch(emitter, "x13", target.offset, "x14");           // load ref pointer (x14 scratch avoids clobbering x9 = index)
        emitter.instruction("str x10, [x13]");                                     // store the possibly-grown array pointer through the ref
    } else {
        abi::store_at_offset_scratch(emitter, "x10", target.offset, "x14");          // save possibly-grown array pointer (x14 scratch avoids clobbering x9)
    }
    match &val_ty {
        PhpType::Str => {
            emitter.instruction("ldp x1, x2, [sp], #16");                          // restore string pointer/length after growth helpers
        }
        PhpType::Float => {
            emitter.instruction("ldr x12, [sp], #16");                             // restore preserved float bits after growth helpers
            emitter.instruction("fmov d0, x12");                                   // move preserved float bits back into the float result register
        }
        _ => {
            emitter.instruction("ldr x0, [sp], #16");                              // restore scalar or heap pointer value after growth helpers
        }
    }
    emitter.instruction("add sp, sp, #32");                                        // drop the original saved index and array pointer after they have been restored
    IndexedAssignState {
        val_ty,
        effective_store_ty,
        stores_refcounted_pointer,
    }
}
