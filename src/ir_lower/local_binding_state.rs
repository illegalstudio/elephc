//! Purpose:
//! Records source binding presence independently of native slot allocation and representation.
//!
//! Called from:
//! - Lowering context initialization, alias binding and binding retirement.
//!
//! Key details:
//! - Only user frames that synchronize eval locals need these EIR state writes.
//! - Representation-only stores preserve presence and cannot invent an imported binding.

use super::context::LoweringContext;
use crate::{ir::{Immediate, LocalKind, LocalSlotId, Op}, types::PhpType};

/// Publishes an actual binding event after storage initialization or before binding retirement.
pub(super) fn emit(ctx: &mut LoweringContext<'_, '_>, slot: LocalSlotId, initialized: bool) {
    if !ctx.builder.function().flags.tracks_local_binding_state || ctx.binding_state_suppressed
        || ctx.builder.local_kind(slot) != LocalKind::PhpLocal
        || ctx.builder.function().locals[slot.as_raw() as usize].name.as_deref()
            .is_none_or(crate::names::is_generated_local_name) { return; }
    let value = ctx.emit_value(Op::ConstBool, Vec::new(), Some(Immediate::Bool(initialized)),
        PhpType::Bool, Op::ConstBool.default_effects(), None);
    ctx.emit_void(Op::SetLocalBindingState, vec![value.value], Some(Immediate::LocalSlot(slot)),
        Op::SetLocalBindingState.default_effects(), None);
}
