use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::ArrayAssignTarget;

pub(super) fn emit_assoc_array_assign(
    target: &ArrayAssignTarget<'_>,
    index: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    if target.is_ref {
        abi::load_at_offset(emitter, "x9", target.offset);                            // load ref pointer
        emitter.instruction("ldr x0, [x9]");                                       // dereference to get hash table pointer
    } else {
        abi::load_at_offset(emitter, "x0", target.offset);                            // load hash table pointer
    }
    emitter.instruction("str x0, [sp, #-16]!");                                    // save hash table pointer
    emit_expr(index, emitter, ctx, data);
    emitter.instruction("stp x1, x2, [sp, #-16]!");                                // save key ptr/len
    let mut val_ty = emit_expr(value, emitter, ctx, data);
    if matches!(target.elem_ty, PhpType::Mixed) && !matches!(val_ty, PhpType::Mixed | PhpType::Union(_)) {
        super::super::super::super::emit_box_current_value_as_mixed(emitter, &val_ty);
        val_ty = PhpType::Mixed;
    } else {
        super::super::super::helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }
    let (val_lo, val_hi) = match &val_ty {
        PhpType::Int | PhpType::Bool => ("x0", "xzr"),
        PhpType::Str => {
            emitter.instruction("bl __rt_str_persist");                             // copy string to heap, x1=heap_ptr, x2=len
            ("x1", "x2")
        }
        PhpType::Float => {
            emitter.instruction("fmov x9, d0");                                     // move float bits to integer register
            ("x9", "xzr")
        }
        _ => ("x0", "xzr"),
    };
    emitter.instruction(&format!("mov x3, {}", val_lo));                           // value_lo
    emitter.instruction(&format!("mov x4, {}", val_hi));                           // value_hi
    emitter.instruction(&format!(
        "mov x5, #{}",
        super::super::super::super::runtime_value_tag(&val_ty)
    )); // value_tag for this assoc entry
    emitter.instruction("ldp x1, x2, [sp], #16");                                  // pop key ptr/len
    emitter.instruction("ldr x0, [sp], #16");                                      // pop hash table pointer
    emitter.instruction("bl __rt_hash_set");                                       // insert/update key-value pair (x0 = table)
    if target.is_ref {
        abi::load_at_offset(emitter, "x9", target.offset);                            // load ref pointer
        emitter.instruction("str x0, [x9]");                                       // store new table ptr through ref
    } else {
        abi::store_at_offset(emitter, "x0", target.offset);                           // save possibly-new table pointer
    }
}
