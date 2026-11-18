//! Purpose:
//! Binds reached function-global declarations through independent activation leases.
//!
//! Called from:
//! - Global declaration lowering for ordinary boxed PHP program symbols.
//!
//! Key details:
//! - The program symbol and frame cell share a PHP reference wrapper, not ownership of a slot.
//! - Staging protects the incoming cell while replacing an older local can execute PHP.

use super::*;

/// Acquires the shared value before replacing this activation's previous binding.
pub(super) fn lower(ctx: &mut LoweringContext<'_, '_>, name: &str, span: Span) {
    let (staged, owner) = ctx.predeclare_returned_ref_cell_staging();
    crate::ir_lower::expr::register_owned_call_operand(ctx, owner, span);
    let data = ctx.intern_global_name(name);
    let pointer = ctx.emit_value(Op::LoadGlobalRefCell, Vec::new(),
        Some(Immediate::GlobalName(data)), PhpType::Pointer(None),
        Op::LoadGlobalRefCell.default_effects(), Some(span));
    ctx.adopt_returned_ref_cell_into(&staged, pointer, PhpType::Mixed, Some(span));
    crate::ir_lower::local_binding_origins::end(ctx, name, Some(span));
    ctx.bind_persistent_ref_cell_ptr(name, pointer, span);
    crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span);
    ctx.clear_static_callable_locals();
}
