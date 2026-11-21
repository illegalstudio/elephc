//! Purpose:
//! Reduces strict numeric array elements while preserving native numeric casts and owned leases.
//!
//! Called from:
//! - The shared walker and VM array aggregate builtin adapter.
//!
//! Key details:
//! - A detached array snapshot keeps iteration stable across native calls.
//! - Every element, numeric cast and partial accumulator has an explicit cleanup obligation.

use super::super::super::*;
use crate::runtime_hooks::owner_frames::{with_runtime_owners, RuntimeOwnerSlot};

/// Applies the same element domains and independent overflow rules as native reduction helpers.
pub(super) fn evaluate(name: &str, array: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps)
    -> Result<RuntimeCellHandle, EvalStatus> {
    let product = match name { "array_sum" => false, "array_product" => true, _ => return Err(EvalStatus::UnsupportedConstruct) };
    if !values.is_array_like(array)? {
        return throwables::eval_throw_type_error(&format!("{name}(): Argument #1 ($array) must be of type array"), context, values);
    }
    let source = RuntimeOwnerSlot::new(values.copy_array_value(array)?);
    let result = with_runtime_owners(&source, || {
        let length = values.array_len(source.borrowed())?;
        let mut carry = values.int(i64::from(product))?;
        for position in 0..length {
            let previous = RuntimeOwnerSlot::new(carry);
            let next = with_runtime_owners(&previous, || combine(source.borrowed(), position, previous.borrowed(), product, context, values));
            carry = finish_synthesized_method_arguments(next, [previous.take()], context, values)?;
        }
        Ok(carry)
    });
    finish_synthesized_method_arguments(result, [source.take()], context, values)
}

/// Resolves an explicit numeric class view before the scalar guard and retires both independent leases.
fn combine(array: RuntimeCellHandle, position: usize, carry: RuntimeCellHandle, product: bool,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps)
    -> Result<RuntimeCellHandle, EvalStatus> {
    let element = RuntimeOwnerSlot::new(values.array_iter_value(array, position)?);
    let result = with_runtime_owners(&element, || {
        let number = RuntimeOwnerSlot::new(values.number_value(element.borrowed())?);
        let result = with_runtime_owners(&number, || {
            numeric_domains::require_unary(elephc_semantics::numeric_domains::NumericOperator::Arithmetic,
                number.borrowed(), context, values)?;
            eval_binary_result(if product { EvalBinOp::Mul } else { EvalBinOp::Add }, carry, number.borrowed(), context, values)
        });
        finish_synthesized_method_arguments(result, [number.take()], context, values)
    });
    finish_synthesized_method_arguments(result, [element.take()], context, values)
}
