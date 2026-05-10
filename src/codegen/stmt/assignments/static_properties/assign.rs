//! Purpose:
//! Lowers direct static property value assignment.
//! Works with static property symbols and class metadata instead of local frame slots.
//!
//! Called from:
//! - `crate::codegen::stmt::assignments::static_properties`
//!
//! Key details:
//! - Late-bound receivers and visibility checks must match PHP inheritance semantics before storage is updated.

use super::late_bound;
use super::resolve;
use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_result_to_type, emit_expr};
use crate::codegen::stmt::helpers;
use crate::names::static_property_symbol;
use crate::parser::ast::{Expr, ExprKind, StaticReceiver};
use crate::types::PhpType;

pub(crate) fn emit_static_property_assign_stmt(
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("::${} = ...", property));
    if let Some((current, default)) =
        crate::codegen::stmt::null_coalesce_static_property_target(receiver, property, value)
    {
        if matches!(default.kind, ExprKind::Null) {
            emitter.comment("literal null fallback leaves the static property unchanged");
            return;
        }
        let current_ty = emit_expr(current, emitter, ctx, data);
        if current_ty != PhpType::Void {
            let keep_label = ctx.next_label("nca_keep");
            crate::codegen::stmt::emit_branch_if_result_non_null(
                &current_ty,
                &keep_label,
                emitter,
            );
            emit_static_property_assign_stmt(receiver, property, default, emitter, ctx, data);
            emitter.label(&keep_label);
        } else {
            emit_static_property_assign_stmt(receiver, property, default, emitter, ctx, data);
        }
        return;
    }

    let Some((class_name, declaring_class, prop_ty, declared)) =
        resolve::resolve_static_property(receiver, property, ctx, emitter)
    else {
        return;
    };
    let branches =
        late_bound::dynamic_static_property_branches(receiver, property, &declaring_class, ctx);
    let class_id_saved = late_bound::emit_and_push_called_class_id_if_needed(
        &branches,
        emitter,
        ctx,
    );

    let mut val_ty = emit_expr(value, emitter, ctx, data);
    let boxed_to_mixed = declared
        && matches!(prop_ty, PhpType::Mixed | PhpType::Union(_))
        && !matches!(val_ty, PhpType::Mixed | PhpType::Union(_));
    if declared {
        coerce_result_to_type(emitter, ctx, data, &val_ty, &prop_ty);
        val_ty = prop_ty.clone();
    }
    if !boxed_to_mixed {
        helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }

    emitter.comment(&format!("store {}::${}", class_name, property));
    if class_id_saved {
        let class_id_reg = late_bound::class_id_work_reg(emitter);
        abi::emit_pop_reg(emitter, class_id_reg);                              // restore the late-bound called class id for static property storage dispatch
        late_bound::emit_dynamic_store_result_to_static_property(
            property,
            class_id_reg,
            &declaring_class,
            &branches,
            &val_ty,
            true,
            emitter,
            ctx,
        );
    } else {
        let symbol = static_property_symbol(&declaring_class, property);
        abi::emit_store_result_to_symbol(emitter, &symbol, &val_ty, true);
    }
}
