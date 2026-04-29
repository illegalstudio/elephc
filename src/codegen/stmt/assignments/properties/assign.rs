use super::{magic_set, references, storage, target};
use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_result_to_type, emit_expr};
use crate::codegen::stmt::helpers;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

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
    if let Some((current, default)) =
        crate::codegen::stmt::null_coalesce_property_target(object, property, value)
    {
        if matches!(default.kind, ExprKind::Null) {
            emitter.comment("literal null fallback leaves the property unchanged");
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
            emit_property_assign_stmt(object, property, default, emitter, ctx, data);
            emitter.label(&keep_label);
        } else {
            emit_property_assign_stmt(object, property, default, emitter, ctx, data);
        }
        return;
    }

    let magic_set_class = magic_set::resolve_magic_set_target(object, property, ctx);
    let declared_target_ty = declared_property_type(object, property, ctx);
    if references::is_reference_property(object, property, ctx) {
        if let Some(var_name) = references::promoted_reference_bind_var(object, property, value, ctx) {
            references::emit_property_reference_bind(&var_name, object, property, emitter, ctx, data);
        } else {
            references::emit_property_reference_write(value, object, property, emitter, ctx, data);
        }
        return;
    }

    let mut val_ty = emit_expr(value, emitter, ctx, data);
    let boxed_to_mixed = declared_target_ty.as_ref().is_some_and(|target_ty| {
        matches!(target_ty, PhpType::Mixed | PhpType::Union(_))
            && !matches!(val_ty, PhpType::Mixed | PhpType::Union(_))
    });
    if let Some(target_ty) = &declared_target_ty {
        coerce_result_to_type(emitter, ctx, data, &val_ty, target_ty);
        val_ty = target_ty.clone();
    }
    if magic_set_class.is_none() && !boxed_to_mixed {
        helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
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

fn declared_property_type(object: &Expr, property: &str, ctx: &Context) -> Option<PhpType> {
    let obj_ty = crate::codegen::functions::infer_contextual_type(object, ctx);
    let PhpType::Object(class_name) = obj_ty else {
        return None;
    };
    let class_info = ctx.classes.get(&class_name)?;
    if !class_info.declared_properties.contains(property) {
        return None;
    }
    class_info
        .properties
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, ty)| ty.clone())
}
