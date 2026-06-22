//! Purpose:
//! Lowers property writes through reference-like lvalue contexts.
//! Shares receiver and property metadata with object expression lowering.
//!
//! Called from:
//! - `crate::codegen::stmt::assignments::properties`
//!
//! Key details:
//! - Property writes must respect declared types, visibility checks, and runtime object layout.

use super::{storage, target};
use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::calls::args as call_args;
use crate::codegen::expr::{coerce_result_to_type, emit_expr};
use crate::codegen::stmt::helpers;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Returns `true` when the receiver's inferred type is a declared object
/// class that has a reference property of the given name.
pub(super) fn is_reference_property(object: &Expr, property: &str, ctx: &Context) -> bool {
    let obj_ty = crate::codegen::functions::infer_contextual_type(object, ctx);
    let PhpType::Object(class_name) = obj_ty else {
        return false;
    };
    ctx.classes
        .get(&class_name)
        .is_some_and(|class_info| class_info.visible_property_is_reference(property))
}
/// Returns `Some(var_name)` when `object` is `$this`, `value` is a variable
/// with the same name as `property`, and that name is a registered reference
/// parameter in `ctx.ref_params`. Used for promoted constructor properties
/// that are declared as `&$param`.
pub(super) fn promoted_reference_bind_var(
    object: &Expr,
    property: &str,
    value: &Expr,
    ctx: &Context,
) -> Option<String> {
    if !matches!(object.kind, ExprKind::This) {
        return None;
    }
    let ExprKind::Variable(var_name) = &value.kind else {
        return None;
    };
    if property != var_name || !ctx.ref_params.contains(var_name) {
        return None;
    }
    Some(var_name.clone())
}

/// Emits a reference bind for a promoted constructor property: `&$prop = $this->prop`.
/// Emits the variable address via `emit_ref_arg_variable_address`, pushes the
/// result value, evaluates the receiver, resolves the property slot, and stores
/// the address of the property slot into the variable's storage. The property
/// must be a concrete (non-dynamic, non-magic) reference property.
pub(super) fn emit_property_reference_bind(
    var_name: &str,
    object: &Expr,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    if !call_args::emit_ref_arg_variable_address(var_name, "promoted property ref", emitter, ctx) {
        return;
    }
    abi::emit_push_result_value(emitter, &PhpType::Int);

    let obj_ty = emit_expr(object, emitter, ctx, data);
    let target = match target::resolve_property_assign_target(&obj_ty, property, None, emitter, ctx) {
        target::PropertyAssignResolution::Resolved(target) => target,
        target::PropertyAssignResolution::UseMagicSet(_) | target::PropertyAssignResolution::UseDynamicProperty { .. } | target::PropertyAssignResolution::Abort => {
            emitter.comment("WARNING: reference property bind requires a concrete property");
            return;
        }
    };

    let object_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!("mov {}, {}", object_reg, abi::int_result_reg(emitter))); //keep the object pointer while binding the promoted reference property slot
    storage::store_property_reference_address(emitter, object_reg, target.offset);
}

/// Lowers property assignment through a reference lvalue (e.g. `&$obj->prop = $value`).
/// Coerces the RHS to the property's declared type, retains borrowed heap results,
/// pushes the value, evaluates the receiver, resolves the concrete property slot,
/// releases the previous referenced value, and stores the new value through the
/// reference pointer.
pub(super) fn emit_property_reference_write(
    value: &Expr,
    object: &Expr,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let prop_ty = reference_property_type(object, property, ctx).unwrap_or(PhpType::Int);
    let mut val_ty = emit_expr(value, emitter, ctx, data);
    coerce_result_to_type(emitter, ctx, data, &val_ty, &prop_ty);
    val_ty = prop_ty.clone();
    helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    abi::emit_push_result_value(emitter, &val_ty);

    let obj_ty = emit_expr(object, emitter, ctx, data);
    let target = match target::resolve_property_assign_target(&obj_ty, property, None, emitter, ctx) {
        target::PropertyAssignResolution::Resolved(target) => target,
        target::PropertyAssignResolution::UseMagicSet(_) | target::PropertyAssignResolution::UseDynamicProperty { .. } | target::PropertyAssignResolution::Abort => {
            emitter.comment("WARNING: reference property write requires a concrete property");
            return;
        }
    };

    let object_reg = abi::symbol_scratch_reg(emitter);
    let pointer_reg = abi::temp_int_reg(emitter.target);
    emitter.instruction(&format!("mov {}, {}", object_reg, abi::int_result_reg(emitter))); //keep the object pointer while resolving the referenced property slot
    abi::emit_load_from_address(emitter, pointer_reg, object_reg, target.offset);
    storage::release_previous_referenced_value(emitter, pointer_reg, &target.prop_ty, &val_ty);
    storage::store_referenced_value(emitter, pointer_reg, &val_ty);
}

/// Looks up the declared PHP type of a named reference property on a given
/// object. Returns `None` if the receiver is not a unique object class, the
/// class has no such property, or the property is not declared as a reference.
fn reference_property_type(object: &Expr, property: &str, ctx: &Context) -> Option<PhpType> {
    let obj_ty = crate::codegen::functions::infer_contextual_type(object, ctx);
    let PhpType::Object(class_name) = obj_ty else {
        return None;
    };
    let class_info = ctx.classes.get(&class_name)?;
    if !class_info.visible_property_is_reference(property) {
        return None;
    }
    class_info
        .visible_property(property)
        .map(|(_, (_, ty))| ty.clone())
}
