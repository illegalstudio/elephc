//! Purpose:
//! Consumes the owned cell transferred by a reference-returning PHP call.
//!
//! Called from:
//! - Direct function, method and statically resolved callable lowering.
//!
//! Key details:
//! - Reference assignment transfers the cell into its local alias owner.
//! - Ordinary value use acquires the contained value before retiring the cell owner.

use super::*;

/// Selects reference transfer or by-value dereference before the call's temporary owners are released.
pub(super) fn finish_reference_return_call(
    ctx: &mut LoweringContext<'_, '_>,
    call: LoweredValue,
    signature: Option<&FunctionSig>,
    span: Span,
) -> LoweredValue {
    if !signature.is_some_and(|signature| signature.by_ref_return) {
        return call;
    }
    let php_type = ctx.builder.value_php_type(call.value);
    if let Some((depth, result_type)) = &mut ctx.reference_call_context {
        if *depth == ctx.expression_depth {
            *result_type = Some(php_type);
            return call;
        }
    }
    let staged = ctx.adopt_returned_ref_cell(call, php_type, Some(span));
    let value = ctx.load_local(&staged, Some(span));
    let owned = crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span));
    ctx.release_ref_cell_owner(&staged, Some(span));
    owned
}
