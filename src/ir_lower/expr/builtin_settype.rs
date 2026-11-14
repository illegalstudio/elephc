//! Purpose:
//! Converts evaluated settype values before publishing to their original reference binding.
//!
//! Called from:
//! - The settype semantic home through a prepared full EIR lowering context.
//!
//! Key details:
//! - Runtime targets use the neutral alias catalog, with original source and target owners rooted.
//! - Ordinary assignment owns reference publication and failed conversions keep tagged storage.

use super::*;
use crate::builtins::semantics::{BuiltinLoweringContext, BuiltinLoweringError};
use elephc_semantics::settype_target::{SettypeTarget, SettypeTargetFailure};

/// Applies scalar/null conversion policy and stores only after the selected conversion succeeds.
pub(in crate::ir_lower) fn lower(ctx: &mut LoweringContext<'_, '_>, value: ValueId,
    target: ValueId, span: Span) -> Result<LoweredValue, BuiltinLoweringError> {
    let name = ctx.operand_local_name(value).ok_or_else(|| BuiltinLoweringError::new(
        "settype requires a writable local or reference binding"))?;
    preserve_binding(ctx, &name);
    let original = pin_in_flight_owners(ctx, &[value, target], span);
    let text = ctx.emit_value(Op::RuntimeCall, vec![target], Some(Immediate::RuntimeCall(
        crate::ir::RuntimeCallTarget::UnaryString(crate::ir::UnaryStringRuntime::StrToLower))),
        PhpType::Str, Effects::READS_HEAP | Effects::ALLOC_HEAP | Effects::MAY_FATAL, Some(span));
    let text_owner = pin_in_flight_owners(ctx, &[text.value], span);
    let initial = ctx.initialized_slots_snapshot();
    let done = ctx.builder.create_named_block("settype.converted", Vec::new());
    for target in [SettypeTarget::Bool, SettypeTarget::Int, SettypeTarget::Float, SettypeTarget::String, SettypeTarget::Null] {
        let accepted = matches_names(ctx, text, target.names(), span);
        let convert = ctx.builder.create_named_block("settype.convert", Vec::new());
        let next = ctx.builder.create_named_block("settype.next_target", Vec::new());
        ctx.builder.terminate(Terminator::CondBr { cond: accepted.value, then_target: convert,
            then_args: Vec::new(), else_target: next, else_args: Vec::new() });
        ctx.builder.position_at_end(convert);
        let input = super::builtin_numeric::borrowed_operand(ctx, value, span);
        let expression = Expr::new(ExprKind::Variable("__settype_original".into()), span);
        let converted = if target == SettypeTarget::Null {
            ctx.emit_value(Op::ConstNull, Vec::new(), None, PhpType::Void, Op::ConstNull.default_effects(), Some(span))
        } else {
            let cast = match target { SettypeTarget::Bool => CastType::Bool, SettypeTarget::Int => CastType::Int,
                SettypeTarget::Float => CastType::Float, SettypeTarget::String => CastType::String,
                _ => unreachable!("this graph currently converts scalar/null targets") };
            super::ternary_cast::lower_scalar_cast_value(ctx, &cast, input, &expression)
        };
        let converted = ctx.box_value_as_mixed(converted, PhpType::Mixed, Some(span));
        if !ctx.store_operand_local(value, converted.value, PhpType::Mixed, Some(span)) {
            return Err(BuiltinLoweringError::new("checked settype lost its original writable binding"));
        }
        ctx.set_local_logical_type(&name, PhpType::Mixed);
        ctx.builder.terminate(Terminator::Br { target: done, args: Vec::new() });
        ctx.builder.position_at_end(next); ctx.restore_initialized_slots(initial.clone());
    }
    for target in [SettypeTarget::Array, SettypeTarget::Object] {
        let accepted = matches_names(ctx, text, target.names(), span);
        crate::ir_lower::semantic_guards::require_false(ctx, accepted, Op::ThrowError,
            "settype(): conversion to this target is not yet supported", Some(span));
    }
    let resource = matches_names(ctx, text, &["resource"], span);
    let resource_message = failure_message(ctx, SettypeTargetFailure::Resource);
    crate::ir_lower::semantic_guards::require_false(ctx, resource, Op::ThrowValueError,
        resource_message, Some(span));
    let message = ctx.intern_string(failure_message(ctx, SettypeTargetFailure::Invalid));
    ctx.emit_void(Op::ThrowValueError, Vec::new(), Some(Immediate::Data(message)),
        Op::ThrowValueError.default_effects(), Some(span));
    ctx.builder.terminate(Terminator::Unreachable);
    ctx.builder.position_at_end(done); ctx.restore_initialized_slots(initial);
    unpin_in_flight_owners(ctx, text_owner, span); release_binary_operand_temporary(ctx, text, span);
    unpin_in_flight_owners(ctx, original, span);
    ctx.set_local_logical_type(&name, PhpType::Mixed);
    Ok(emit_bool_literal(ctx, true, Some(span)))
}

/// Labels checked diagnostics while retaining ordinary PHP messages in permissive prepared bodies.
fn failure_message(ctx: &LoweringContext<'_, '_>, failure: SettypeTargetFailure) -> &'static str {
    if crate::semantics::conversions::checked_casts(ctx.semantic_profile, ctx.source_mode) { failure.message() }
    else { failure.php_message() }
}

/// Keeps failed conversions readable through the original tag without widening an existing reference ABI.
pub(super) fn preserve_binding(ctx: &mut LoweringContext<'_, '_>, name: &str) {
    if !ctx.is_ref_bound_local(name) {
        let slot = ctx.declare_local(name, PhpType::Mixed);
        ctx.builder.widen_local_storage_type(slot, PhpType::Mixed);
    }
    ctx.set_local_logical_type(name, PhpType::Mixed);
}

/// Matches complete normalized target strings, retaining byte length and embedded-NUL distinction.
fn matches_names(ctx: &mut LoweringContext<'_, '_>, text: LoweredValue,
    names: &[&str], span: Span) -> LoweredValue {
    let mut accepted = emit_bool_literal(ctx, false, Some(span));
    for name in names {
        let data = ctx.intern_string(name);
        let literal = ctx.emit_value(Op::ConstStr, Vec::new(), Some(Immediate::Data(data)),
            PhpType::Str, Op::ConstStr.default_effects(), Some(span));
        let matches = ctx.emit_value(Op::StrictEq, vec![text.value, literal.value], None,
            PhpType::Bool, Op::StrictEq.default_effects(), Some(span));
        accepted = ctx.emit_value(Op::IBitOr, vec![accepted.value, matches.value], None,
            PhpType::Bool, Op::IBitOr.default_effects(), Some(span));
    }
    accepted
}
