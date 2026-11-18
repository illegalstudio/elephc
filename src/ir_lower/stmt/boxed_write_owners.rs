//! Purpose:
//! Manages caller-owned operands borrowed by shared boxed offset writes.
//!
//! Called from:
//! - Boxed, property, static-property and nested array-write lowering.
//!
//! Key details:
//! - The caller keeps normal cleanup; temporary guards cover exceptional exits.
//! - Original object and array key kinds survive representation-only boxing.
//! - Key/receiver leases are independent of an assignment result borrowed by the RHS.

use super::*;

/// Protects caller-owned operands while the boxed writer borrows independent views of them.
/// The caller retains its existing normal-path cleanup and every earlier pending record.
pub(super) fn lower_set(
    ctx: &mut LoweringContext<'_, '_>, array: LoweredValue,
    index: LoweredValue, value: LoweredValue,
    previous: &[crate::ir_lower::expr::PinnedInFlightOwner],
    span: Span, key_already_diagnosed: bool,
) {
    let value = ctx.borrow_write_operand_if_needed(value, span);
    ctx.with_independent_write_receiver(|ctx| {
        let pins = crate::ir_lower::expr::protect_additional_pending_operands(
            ctx, &[array, index, value], previous, span,
        );
        let array = crate::ir_lower::ownership::borrow_value(ctx, array, Some(span));
        let index = crate::ir_lower::ownership::borrow_value(ctx, index, Some(span));
        let value = crate::ir_lower::ownership::borrow_value(ctx, value, Some(span));
        super::array_write_core::lower_boxed_array_set_values(ctx, array, index, value, span, key_already_diagnosed);
        if !ctx.builder.insertion_block_is_terminated() {
            crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
        }
    })
}

/// Borrows rooted caller operands through the shared captured boxed append kernel.
pub(super) fn lower_append(
    ctx: &mut LoweringContext<'_, '_>, array: LoweredValue, value: LoweredValue, span: Span,
) {
    let value = ctx.borrow_write_operand_if_needed(value, span);
    ctx.with_independent_write_receiver(|ctx| {
        let pins = crate::ir_lower::expr::protect_additional_pending_operands(ctx, &[array, value], &[], span);
        let array = crate::ir_lower::ownership::borrow_value(ctx, array, Some(span));
        let value = crate::ir_lower::ownership::borrow_value(ctx, value, Some(span));
        super::nested_plain_writes::append_prepared(ctx, array, value, span);
        if !ctx.builder.insertion_block_is_terminated() {
            crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
        }
    });
}

/// Retires a source whose borrowed store took a separate owner, respecting borrowed RHS mode.
pub(super) fn release_value(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span,
) {
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    } else {
        super::array_write_core::release_persisted_string_operand(ctx, value, span);
    }
}

/// Retires an original key independently of an assignment result borrowed by its RHS.
pub(super) fn release_key(
    ctx: &mut LoweringContext<'_, '_>, key: LoweredValue, span: Span,
) {
    ctx.with_independent_write_receiver(|ctx| release_value(ctx, key, span));
}
