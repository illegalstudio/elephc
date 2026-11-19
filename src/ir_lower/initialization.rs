//! Purpose:
//! Proves ordinary native bindings present before their values are observed.
//!
//! Called from:
//! - Projected local reads shared by expressions and value captures.
//!
//! Key details:
//! - Intentional absence probes and trusted generated code retain their existing paths.
//! - The shared native/eval presence bits distinguish missing bindings from initialized null.

use super::context::LoweringContext;
use crate::{ir::{Immediate, LocalKind, Op, Terminator}, span::Span};
use elephc_semantics::StrictFeature;

/// Selects initialized-read restrictions for user lowering, independently of PHP-only syntax.
pub(super) fn enabled(ctx: &LoweringContext<'_, '_>) -> bool {
    super::semantic_guards::source_feature_enabled(ctx, StrictFeature::Initialization)
}

/// Finds canonical program presence or frame presence bits without consulting its incidental value representation.
pub(super) fn binding_presence(ctx: &mut LoweringContext<'_, '_>, name: &str, span: Span)
    -> Option<super::context::LoweredValue> {
    if !enabled(ctx) || name == "GLOBALS"
        || crate::names::is_generated_local_name(name)
        || ctx.extern_globals.contains_key(name) { return None; }
    if ctx.reads_from_eval_scope(name) {
        return Some(ctx.eval_scope_binding_present(name, span));
    }
    if ctx.local_uses_global_storage(name) {
        let data = ctx.intern_global_name(name);
        return Some(ctx.emit_value(Op::GlobalBindingPresent, Vec::new(),
            Some(Immediate::GlobalName(data)), crate::types::PhpType::Bool,
            Op::GlobalBindingPresent.default_effects(), Some(span)));
    }
    let ty = ctx.local_type(name);
    let slot = ctx.local_slots.get(name).copied().unwrap_or_else(|| ctx.declare_local(name, ty));
    if ctx.builder.local_kind(slot) != LocalKind::PhpLocal { return None; }
    Some(super::local_binding_state::binding_is_present(ctx, slot, span))
}

/// Guards a user read without changing its value representation or producer origin.
pub(super) fn guard_read(ctx: &mut LoweringContext<'_, '_>, name: &str, span: Span) {
    if ctx.suppresses_undefined_reads(name, span) { return; }
    let Some(present) = binding_presence(ctx, name, span) else { return; };
    let ready = ctx.builder.create_named_block("initialized.read", Vec::new());
    let missing = ctx.builder.create_named_block("uninitialized.read", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: present.value,
        then_target: ready, then_args: Vec::new(), else_target: missing, else_args: Vec::new() });
    ctx.builder.position_at_end(missing);
    let message = ctx.intern_string(&elephc_semantics::initialization::variable_message(name));
    ctx.emit_void(Op::ThrowError, Vec::new(), Some(Immediate::Data(message)),
        Op::ThrowError.default_effects(), Some(span));
    ctx.builder.terminate(Terminator::Unreachable);
    ctx.builder.position_at_end(ready);
}

/// Protects independent property operands while an initialized read or its operand evaluation can throw.
pub(super) fn pin_read_operands(ctx: &mut LoweringContext<'_, '_>, operands: &[crate::ir::ValueId],
    span: Span,
) -> Vec<super::expr::call_operand_owners::PinnedInFlightOwner> {
    if !enabled(ctx) { return Vec::new(); }
    super::expr::pin_in_flight_owners(ctx, operands, span)
}
