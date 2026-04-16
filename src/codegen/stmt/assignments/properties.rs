mod magic_set;
mod storage;
mod target;

use super::super::super::abi;
use super::super::super::context::Context;
use super::super::super::data_section::DataSection;
use super::super::super::emit::Emitter;
use super::super::super::expr::emit_expr;
use crate::parser::ast::Expr;

pub(crate) fn emit_property_assign_stmt(
    object: &Expr,
    property: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("->{}  = ...", property));

    let magic_set_class = magic_set::resolve_magic_set_target(object, property, ctx);
    let val_ty = emit_expr(value, emitter, ctx, data);
    if magic_set_class.is_none() {
        super::super::helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }
    abi::emit_push_result_value(emitter, &val_ty);

    let obj_ty = emit_expr(object, emitter, ctx, data);
    let target = match target::resolve_property_assign_target(
        &obj_ty,
        property,
        magic_set_class.as_deref(),
        emitter,
        ctx,
    ) {
        target::PropertyAssignResolution::Resolved(target) => target,
        target::PropertyAssignResolution::UseMagicSet(class_name) => {
            magic_set::emit_magic_set_call(&class_name, property, &val_ty, emitter, ctx, data);
            return;
        }
        target::PropertyAssignResolution::Abort => return,
    };

    if target.needs_deref {
        abi::emit_call_label(emitter, "__rt_ptr_check_nonnull");
        emitter.comment(&format!(
            "store extern field {}::{} at offset {}",
            target.class_name, property, target.offset
        ));
    }

    let object_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!("mov {}, {}", object_reg, abi::int_result_reg(emitter))); // keep the object pointer in a scratch register while property storage is updated
    if !target.needs_deref {
        storage::release_previous_property_value(emitter, object_reg, &target.prop_ty, target.offset);
    }

    storage::store_property_value(emitter, object_reg, &val_ty, target.offset);
}
