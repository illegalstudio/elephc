//! Purpose:
//! Selects writable native reference layouts without mutating a detached value snapshot.
//!
//! Called from:
//! - Local append and offset assignment lowering.
//!
//! Key details:
//! - Canonical references lend their current boxed child to the existing Mixed writer.
//! - Headerless native cells keep the concrete writer and its original ABI writeback.
//! - Detached locals retain ordinary boxed storage, and by-value reads still own copies.

use super::*;

/// Runs an operation with the actual reference layout, preserving ordinary branch joins.
pub(in crate::ir_lower) fn lower(
    ctx: &mut LoweringContext<'_, '_>, name: &str, span: Span,
    mut operation: impl FnMut(&mut LoweringContext<'_, '_>),
) -> bool {
    if ctx.live_native_mutation_locals.contains_key(name) { return false; }
    let Some(payload) = ctx.native_mutation_payload_type(name) else { return false; };
    let snapshot = ctx.load_local(name, Some(span));
    let raw = ctx.emit_value(Op::RuntimeCall, vec![snapshot.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ReferenceCellUsesRawPayload)),
        PhpType::Bool, effects_lookup::runtime_effects(), Some(span));
    crate::ir_lower::ownership::release_if_owned(ctx, snapshot, Some(span));
    super::conditionals::lower_runtime_choice(ctx, raw.value, span, |ctx, raw| {
        if raw { ctx.set_local_logical_type(name, payload.clone()); }
        ctx.live_native_mutation_locals.insert(name.to_string(),
            if raw { payload.clone() } else { PhpType::Mixed });
        operation(ctx);
        ctx.live_native_mutation_locals.remove(name);
        // The entry declaration remains a physical contract, not a later value fact.
        ctx.set_local_logical_type(name, PhpType::Mixed);
    });
    true
}
