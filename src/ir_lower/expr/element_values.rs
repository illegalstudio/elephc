//! Purpose:
//! Projects a boxed element-forming operand to the type the checker proved for it.
//!
//! Called from:
//! - Local array appends and keyed writes in `crate::ir_lower::stmt`.
//! - Local reads (`local_values`), which cover literal items and merge arms, and the storage
//!   prediction that pairs with them.
//!
//! Key details:
//! - A hash `foreach` binding, a guard-narrowed `mixed` local and a branch merge are boxed
//!   cells in EIR while the checker types them as one scalar. Writing the box widened the
//!   container to `mixed` payloads the checker never recorded, and a signature, declared
//!   local or typed property compiled against the checker's type then read boxed cells raw.
//! - Only a physically boxed operand is projected. A value that is not a plain local read is
//!   projected only into a container that is empty or already stores that type, so a genuine
//!   widening keeps the box and its runtime tag.
//! - String casts produce independent bytes and an extracted array owns its own reference; a
//!   borrowed loop-local box keeps its owner.

use super::*;

/// Returns the type a boxed operand at `span` is projected to, if the checker published one
/// for a container that stores `element` (or is still empty).
///
/// `local` is the variable the operand reads directly, `None` for a branch merge; it is part of
/// the site's identity. `physical` is the operand's own EIR storage: a concrete operand needs
/// no projection, and a container holding another payload widens exactly as before.
pub(crate) fn projected_element_type(
    ctx: &LoweringContext<'_, '_>,
    span: Span,
    local: Option<&str>,
    physical: &PhpType,
    element: Option<&PhpType>,
) -> Option<PhpType> {
    if physical.codegen_repr() != PhpType::Mixed {
        return None;
    }
    let view = ctx
        .element_value_views
        .get(&(ctx.loop_storage_scope.clone(), span, local.map(str::to_string)))?
        .as_ref()?;
    let target = view.codegen_repr();
    if !is_scalar_element(&target)
        && !matches!(target, PhpType::Array(_) | PhpType::AssocArray { .. })
    {
        return None;
    }
    let accepts = element.is_none_or(|element| {
        let element = element.codegen_repr();
        element == target || matches!(element, PhpType::Never | PhpType::Void)
    });
    accepts.then_some(target)
}

/// Extracts the checked value from a boxed operand before it becomes a container element.
///
/// A scalar is cast out of the box and a local's own array or hash is unboxed. `local` names
/// the variable the operand reads directly, `None` for a branch merge. `element` is the
/// payload the receiving container stores now, or `None` when the caller already chose its
/// storage from [`projected_local_element_type`]. Every other operand is returned unchanged,
/// so unproven, volatile and genuinely heterogeneous writes keep their old path.
pub(crate) fn project_element_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
    local: Option<&str>,
    element: Option<&PhpType>,
) -> LoweredValue {
    let physical = ctx.builder.value_php_type(value.value);
    let Some(target) = projected_element_type(ctx, span, local, &physical, element) else {
        return value;
    };
    if !is_scalar_element(&target) {
        // The box holds the local's own array or hash. Extraction retains that payload, so the
        // result owns its lease independently of the cell it came from.
        let effects = Op::mixed_unbox_effects(&target);
        let extracted =
            ctx.emit_owned_value(Op::MixedUnbox, vec![value.value], None, target, effects, Some(span));
        if ctx.value_needs_release_after_use(value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
        }
        return extracted;
    }
    let result = ctx.emit_value(
        Op::Cast,
        vec![value.value],
        Some(Immediate::CastTarget(crate::ir_lower::context::value_ir_type(&target))),
        target,
        Op::Cast.default_effects(),
        Some(span),
    );
    // A scalar cast cannot alias the source cell and a string cast detaches its bytes, so an
    // independent temporary source retires here while a borrowed local box keeps its owner.
    if ctx.value_needs_release_after_use(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    result
}

/// Returns whether a projection target is a scalar a boxed operand is cast to.
fn is_scalar_element(target: &PhpType) -> bool {
    matches!(target, PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Str)
}

/// Returns the payload type a local container value stores, when it has one concrete layout.
pub(crate) fn container_element_type(container: &PhpType) -> Option<PhpType> {
    match container.codegen_repr() {
        PhpType::Array(element) => Some(*element),
        PhpType::AssocArray { value, .. } => Some(*value),
        _ => None,
    }
}

/// Returns the type a boxed read of `name` at `span` is projected to, if one is published.
///
/// The read is a container element or an arm of a merge that is one. Literal storage is
/// predicted from the syntax before its items are lowered, so the read itself has to produce
/// the checked value: the prediction and every lowering path then agree without consulting
/// the view again.
pub(crate) fn projected_local_element_type(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    span: Span,
    physical: &PhpType,
) -> Option<PhpType> {
    projected_element_type(ctx, span, Some(name), physical, None)
}
