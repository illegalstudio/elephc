//! Purpose:
//! Initializes physical instance-property slots for runtime by-name allocation.
//!
//! Called from:
//! - `super::function::lower_property_init_thunk` through the shared function lowerer.
//!
//! Key details:
//! - PropertyRef identifies a layout slot, so private shadows never resolve by name.
//! - Defaults bypass hooks; declared slots without defaults receive the uninitialized marker.

use crate::ir::{Effects, Immediate, Op};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{ClassInfo, PhpType};
use super::context::{LoweredValue, LoweringContext};

/// Returns whether allocation must initialize defaults or typed-property markers.
pub(super) fn needs_initializer(class: &ClassInfo) -> bool {
    class.defaults.iter().any(Option::is_some)
        || class.properties.iter().enumerate().any(|(index, (name, _))| {
            class.property_slot_is_declared(index, name)
                || (class.property_slot_is_reference(index, name)
                    && class.owned_reference_properties.contains(name))
        })
}

/// Emits default expressions and raw slot writes into a freshly zeroed object.
pub(super) fn lower(ctx: &mut LoweringContext<'_, '_>, class: &ClassInfo) {
    let object = ctx.load_local("this", None);
    for (index, (name, ty)) in class.properties.iter().enumerate() {
        let slot = Immediate::PropertyRef {
            class: u32::try_from(class.class_id).expect("class id exceeds EIR property reference"),
            property: u32::try_from(index).expect("property index exceeds EIR property reference"),
        };
        let Some(default) = class.defaults.get(index).and_then(Option::as_ref) else {
            if class.property_slot_is_declared(index, name)
                || (class.property_slot_is_reference(index, name)
                    && class.owned_reference_properties.contains(name))
            {
                ctx.emit_void(
                    Op::PropUnset, vec![object.value], Some(slot),
                    Op::PropUnset.default_effects() | Effects::ALLOC_HEAP, None,
                );
            }
            continue;
        };
        if matches!(default.kind, ExprKind::Null) && !ty.null_property_default_required() {
            continue;
        }
        let value = lower_default_value(ctx, default, ty);
        let value = super::stmt::coerce_typed_assign_value(ctx, value, ty, default.span);
        ctx.emit_void(
            Op::PropSet, vec![object.value, value.value], Some(slot),
            Op::PropSet.default_effects() | Effects::ALLOC_HEAP, Some(default.span),
        );
        super::stmt::release_property_assignment_source_after_retaining_store(ctx, ty, value, default.span);
    }
}

/// Builds defaults in the physical slot's representation without resolving private slots by name.
fn lower_default_value(
    ctx: &mut LoweringContext<'_, '_>,
    default: &Expr,
    ty: &PhpType,
) -> LoweredValue {
    let target = ty.codegen_repr();
    if matches!(target, PhpType::AssocArray { .. })
        && matches!(&default.kind, ExprKind::ArrayLiteral(items) if items.is_empty())
    {
        return ctx.emit_value(
            Op::HashNew, Vec::new(), Some(Immediate::Capacity(0)), target,
            Op::HashNew.default_effects(), Some(default.span),
        );
    }
    let value = super::expr::lower_expr(ctx, default);
    super::stmt::contextualize_property_array_value(ctx, value, default, ty, default.span)
}
