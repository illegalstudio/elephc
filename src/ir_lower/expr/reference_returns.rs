//! Purpose:
//! Consumes the owned cell transferred by a reference-returning PHP call.
//!
//! Called from:
//! - Direct function, method and statically resolved callable lowering.
//!
//! Key details:
//! - A reference assignment adopts the lease into staging it published BEFORE the call.
//! - Ordinary value use acquires the contained value before retiring its own staged owner.

use super::*;

/// Selects reference transfer or by-value dereference before the call's temporary owners are released.
///
/// Both outcomes adopt the transferred cell into a hidden owner slot here, which is immediately
/// after the call and before argument temporaries, evaluation intermediates or an owning receiver
/// are retired.
///
/// The two outcomes differ in how long that lease has to survive:
///
/// - A reference assignment still has to retire the previous binding of its target and publish
///   the alias, and either step can run a destructor that throws. Its staging was therefore
///   declared and published in the unwind chain by `lower_ref_assign_call` before the source
///   expression was lowered, so this adoption drops the lease straight into a record that is
///   already live and nests OUTSIDE every argument root the call published.
/// - A by-value use loads the payload, acquires it and retires the lease immediately. No PHP
///   code runs between those steps, so that lease cannot be stranded and needs no record.
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
    let staging = ctx
        .reference_call_context
        .as_ref()
        .filter(|context| context.depth == ctx.expression_depth)
        .map(|context| context.staged.clone());
    if let Some(staged) = staging {
        ctx.adopt_returned_ref_cell_into(&staged, call, php_type, Some(span));
        if let Some(context) = ctx.reference_call_context.as_mut() {
            context.adopted = true;
        }
        return call;
    }
    let staged = ctx.adopt_returned_ref_cell(call, php_type, Some(span));
    let value = ctx.load_local(&staged, Some(span));
    let owned = crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span));
    ctx.release_ref_cell_owner(&staged, Some(span));
    owned
}
