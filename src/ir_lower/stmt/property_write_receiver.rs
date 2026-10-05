//! Purpose:
//! Keeps property-write receivers alive and visible to same-frame exception cleanup.
//!
//! Called from:
//! - Literal, runtime-name and array-element property write lowering.
//!
//! Key details:
//! - Static loads borrow either a concrete object or a boxed nullable value.
//! - Nullable array receivers are guarded before extracting an owned object payload.

use super::*;

/// One receiver lease covering RHS evaluation, coercion and the eventual property mutation.
pub(crate) struct PropertyWriteReceiver {
    pub(crate) value: LoweredValue,
    pins: Vec<crate::ir_lower::expr::PinnedInFlightOwner>,
    owners: Vec<LoweredValue>,
}

impl PropertyWriteReceiver {
    /// Acquires borrowed static storage and parks any independently owned receiver for unwind.
    pub(crate) fn new(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> Self {
        let value = if ctx.builder.value_defining_op(value.value) == Some(Op::LoadStaticProperty) {
            crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, value, Some(span))
        } else { value };
        let pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[value.value], span);
        let owners = if ctx.value_is_owning_temporary(value) { vec![value] } else { Vec::new() };
        Self { value, pins, owners }
    }

    /// Guards and extracts a nullable object while both owner forms remain unwind-visible.
    pub(super) fn for_array(
        ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, property: &str, span: Span,
    ) -> Self {
        let mut receiver = Self::new(ctx, value, span);
        let object = narrow_array_write_receiver(ctx, receiver.value, property, span);
        if object.value != receiver.value.value {
            receiver.pins.extend(crate::ir_lower::expr::pin_in_flight_owners(ctx, &[object.value], span));
            receiver.owners.push(object);
            receiver.value = object;
        }
        receiver
    }

    /// Retires the unwind record and releases precisely the receiver's independent lease.
    pub(crate) fn finish(self, ctx: &mut LoweringContext<'_, '_>, span: Span) {
        crate::ir_lower::expr::unpin_in_flight_owners(ctx, self.pins, span);
        for value in self.owners.into_iter().rev() {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
        }
    }
}

/// Narrows a statically known nullable class for fixed-slot array mutation, with PHP's null Error.
fn narrow_array_write_receiver(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, property: &str, span: Span,
) -> LoweredValue {
    let ty = ctx.builder.value_php_type(value.value);
    let Some((class, true)) = crate::ir_lower::expr::singular_object_class(&ty) else {
        return value;
    };
    let class = class.to_string();
    let is_null = ctx.emit_value(Op::IsNull, vec![value.value], None, PhpType::Bool,
        Op::IsNull.default_effects(), Some(span));
    let null = ctx.builder.create_named_block("property.write.null", Vec::new());
    let present = ctx.builder.create_named_block("property.write.present", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value, then_target: null, then_args: Vec::new(),
        else_target: present, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(null);
    lower_throw_access_error(ctx, &format!("Attempt to modify property \"{property}\" on null"), span);
    ctx.builder.position_at_end(present);
    let target = PhpType::Object(class);
    ctx.emit_owned_value(Op::MixedUnbox, vec![value.value], None, target.clone(),
        Op::mixed_unbox_effects(&target), Some(span))
}
