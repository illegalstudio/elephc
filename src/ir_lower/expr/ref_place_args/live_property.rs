//! Purpose:
//! Capture the live reference of an array_unshift property receiver before its value arguments.
//!
//! Called from:
//! - `super::lower_builtin_ref_place_call()` before snapshot materialization.
//!
//! Key details:
//! - Retain the property cell independently of its object, preserving PHP destructor order.
//! - Publish the hidden cell owner before capturing it so argument exceptions retire the lease.

use crate::ir::{RuntimeCallTarget, RuntimeFnId};
use crate::ir_lower::context::{LoweredValue, LoweringContext};
use crate::names::Name;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::FunctionSig;

/// Binds the receiver to a managed cell whose payload can change during argument evaluation.
pub(in crate::ir_lower::expr) fn lower_unshift_property_reference(
    ctx: &mut LoweringContext<'_, '_>,
    name: &Name,
    sig: &FunctionSig,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let definition = crate::builtins::registry::lookup(name.as_str())?;
    if !matches!(definition.spec.semantics.lowering,
        crate::builtins::semantics::BuiltinLowering::Runtime(
            RuntimeCallTarget::Function(RuntimeFnId::ArrayUnshift)))
    {
        return None;
    }
    let original = args.first()?;
    let place = super::ref_param_place(sig, 0, original)?;
    match &place.kind {
        ExprKind::PropertyAccess { object, property } => {
            let Some(class) = super::place_object_class_name(ctx, object) else {
                crate::ir_lower::diagnostics::refuse(place.span,
                    "Unsupported array_unshift property reference: the object's class is not statically known");
                return Some(super::super::lower_null(ctx, expr));
            };
            let info = ctx.classes.get(class.as_str())?;
            if info.readonly_properties.contains(property) {
                crate::ir_lower::diagnostics::refuse(place.span,
                    &format!("Cannot modify readonly property {}::${} by reference", class, property));
                return Some(super::super::lower_null(ctx, expr));
            }
            if !info.reference_properties.contains(property) {
                return None;
            }
        }
        ExprKind::StaticPropertyAccess { .. } => {}
        _ => return None,
    }

    let (alias, owner) = ctx.predeclare_returned_ref_cell_staging();
    super::super::call_operand_owners::register_owned_call_operand(ctx, owner, place.span);
    if matches!(place.kind, ExprKind::StaticPropertyAccess { .. }) {
        super::super::property_access::lower_ref_assign_static_property(ctx, &alias, place, place.span);
    } else {
        super::super::property_access::lower_ref_assign_property(ctx, &alias, place, place.span);
    }
    let variable = Expr::new(ExprKind::Variable(alias.clone()), place.span);
    let mut arguments = args.to_vec();
    arguments[0] = match &original.kind {
        ExprKind::NamedArg { name, .. } => Expr::new(ExprKind::NamedArg {
            name: name.clone(), value: Box::new(variable),
        }, original.span),
        _ => variable,
    };
    let result = super::lower_function_call(ctx, name, &arguments, expr);
    super::super::call_operand_owners::unregister_owned_call_operand(ctx, owner, place.span);
    ctx.release_ref_cell_owner(&alias, Some(place.span));
    Some(result)
}
