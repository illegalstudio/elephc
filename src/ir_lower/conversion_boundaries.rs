//! Purpose:
//! Enforces implicit value contracts before PHP conversions can erase original kinds.
//!
//! Called from:
//! - EIR parameter, return and declared-local value preparation.
//!
//! Key details:
//! - Physical boxing and unboxing remain legal after an exact value-kind check.
//! - The original owner stays rooted across a catchable failure.

use super::{context::{LoweredValue, LoweringContext}, semantic_guards};
use crate::{ir::{Op, PhpTypePredicate}, span::Span, types::PhpType};
use elephc_semantics::{conversions::{ConversionPlan, ValueKind}, StrictFeature};

/// Tests whether this user boundary needs the original value before destination lowering.
pub(super) fn active(ctx: &LoweringContext<'_, '_>, destination: &PhpType) -> bool {
    ctx.source_mode != crate::source::SourceMode::Internal
        && ctx.semantic_profile.contains(StrictFeature::Coercions)
        && crate::semantics::conversions::implicit_contract(destination).is_some()
}

/// Defers non-null parameter adaptation until original source operands have been evaluated.
pub(super) fn parameter_active(ctx: &LoweringContext<'_, '_>, destination: &PhpType) -> bool {
    active(ctx, destination)
        || (semantic_guards::source_feature_enabled(ctx, StrictFeature::Nullability)
            && (crate::semantics::conversions::scalar_contract(destination).is_some()
                || matches!(destination, PhpType::Callable | PhpType::DeclaredCallable | PhpType::Object(_)))
            && crate::semantics::nullability::non_null_contract(destination))
}

/// Retains the existing nominal check when boxed object adaptation moves after argument evaluation.
pub(super) fn object_parameter(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue,
    destination: &PhpType, span: Span) -> LoweredValue {
    let checked = membership(ctx, value, destination, StrictFeature::Nullability, span);
    value_storage(ctx, checked, destination, span)
}

/// Checks declared membership and returns the same value and ownership without conversion.
pub(super) fn implicit(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue,
    destination: &PhpType, span: Span) -> LoweredValue {
    membership(ctx, value, destination, StrictFeature::Coercions, span)
}

/// Reuses rooted kind checks while retaining the local store's independently owning guarantee.
fn membership(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue,
    destination: &PhpType, feature: StrictFeature, span: Span) -> LoweredValue {
    let source = ctx.source_mode;
    membership_at_source(ctx, value, destination, feature, source, span)
}

/// Applies global array key identity to trusted implementation paths as well as user source.
pub(super) fn array_key(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> LoweredValue {
    membership_at_source(ctx, value, &PhpType::Union(vec![PhpType::Int, PhpType::Str]),
        StrictFeature::ArrayKeys, crate::source::SourceMode::Php, span)
}

/// Keeps whole-row column selection separate from using null as an actual array key.
pub(super) fn array_key_or_null(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> LoweredValue {
    membership_at_source(ctx, value, &PhpType::Union(vec![PhpType::Int, PhpType::Str, PhpType::Void]),
        StrictFeature::ArrayKeys, crate::source::SourceMode::Php, span)
}

/// Shares guarded membership and owner handling while keeping source policy explicit.
fn membership_at_source(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue,
    destination: &PhpType, feature: StrictFeature, source: crate::source::SourceMode, span: Span) -> LoweredValue {
    let actual = ctx.builder.value_php_type(value.value);
    match crate::semantics::conversions::membership(ctx.semantic_profile, source, feature, destination, &actual) {
        ConversionPlan::ExistingBehavior | ConversionPlan::Identity => return value,
        _ => {}
    }
    let Some(allowed) = crate::semantics::conversions::implicit_contract(destination) else { return value; };
    let false_only = crate::semantics::conversions::only_false(destination);
    let predicates: Vec<_> = [(ValueKind::Integer, PhpTypePredicate::Int),
        (ValueKind::Float, PhpTypePredicate::Float), (ValueKind::Boolean, PhpTypePredicate::Bool),
        (ValueKind::String, PhpTypePredicate::String)]
        .into_iter().filter_map(|(kind, predicate)| (allowed.allows(kind)
            && !(false_only && kind == ValueKind::Boolean)).then_some(predicate)).collect();
    let nominal = crate::semantics::conversions::nominal_members(destination);
    let guard_box = (!nominal.is_empty() && !matches!(value.ir_type,
        crate::ir::IrType::Heap(crate::ir::IrHeapKind::Mixed | crate::ir::IrHeapKind::Union)))
        .then(|| ctx.emit_owned_value(Op::MixedBox, vec![value.value], None, PhpType::Mixed,
            Op::MixedBox.default_effects(), Some(span)));
    let guarded = guard_box.unwrap_or(value);
    let mut guarded_owners = vec![value.value];
    if let Some(boxed) = guard_box { guarded_owners.push(boxed.value); }
    let pins = super::expr::call_operand_owners::pin_in_flight_owners(ctx, &guarded_owners, span);
    let mut accepted = semantic_guards::matches_any_kind(ctx, guarded, &predicates, Some(span));
    for name in nominal {
        let matches = if name.is_empty() || name.eq_ignore_ascii_case("object") {
            semantic_guards::matches_any_kind(ctx, guarded, &[PhpTypePredicate::Object], Some(span))
        } else {
            let name = ctx.intern_class_name(name);
            ctx.emit_value(Op::InstanceOf, vec![guarded.value], Some(crate::ir::Immediate::Data(name)),
                PhpType::Bool, Op::InstanceOf.default_effects(), Some(span))
        };
        accepted = ctx.emit_value(Op::IBitOr, vec![accepted.value, matches.value], None,
            PhpType::Bool, Op::IBitOr.default_effects(), Some(span));
    }
    for constraint in crate::semantics::conversions::runtime_constraints(destination) {
        use crate::semantics::conversions::RuntimeConstraint;
        let matches = match constraint {
            RuntimeConstraint::Callable => {
                let target = crate::ir::RuntimeFnId::IsCallable;
                ctx.emit_value(Op::RuntimeCall, vec![guarded.value],
                    Some(crate::ir::Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(target))),
                    PhpType::Bool, target.effects(), Some(span))
            }
            constraint => semantic_guards::matches_any_kind(ctx, guarded, &[match constraint {
                RuntimeConstraint::Array => PhpTypePredicate::Array,
                RuntimeConstraint::Iterable => PhpTypePredicate::Iterable,
                RuntimeConstraint::Resource => PhpTypePredicate::Resource,
                RuntimeConstraint::Callable => unreachable!(),
            }], Some(span)),
        };
        accepted = ctx.emit_value(Op::IBitOr, vec![accepted.value, matches.value], None,
            PhpType::Bool, Op::IBitOr.default_effects(), Some(span));
    }
    if false_only {
        let no = ctx.emit_value(Op::ConstBool, Vec::new(), Some(crate::ir::Immediate::Bool(false)),
            PhpType::Bool, Op::ConstBool.default_effects(), Some(span));
        let is_false = ctx.emit_value(Op::StrictEq, vec![value.value, no.value], None,
            PhpType::Bool, Op::StrictEq.default_effects(), Some(span));
        accepted = ctx.emit_value(Op::IBitOr, vec![accepted.value, is_false.value], None,
            PhpType::Bool, Op::IBitOr.default_effects(), Some(span));
    }
    if allowed.allows(ValueKind::Null) {
        let null = ctx.emit_value(Op::IsNull, vec![value.value], None, PhpType::Bool,
            Op::IsNull.default_effects(), Some(span));
        accepted = ctx.emit_value(Op::IBitOr, vec![accepted.value, null.value], None, PhpType::Bool,
            Op::IBitOr.default_effects(), Some(span));
    }
    let message = if feature == StrictFeature::ArrayKeys {
        crate::semantics::array_keys::EXACT_KEY_MESSAGE.to_string()
    } else { format!("--{} requires {destination} without implicit conversion", feature.flag()) };
    semantic_guards::require(ctx, accepted, Op::ThrowTypeError, &message, Some(span));
    super::expr::call_operand_owners::unpin_in_flight_owners(ctx, pins, span);
    if let Some(boxed) = guard_box { super::ownership::release_if_owned(ctx, boxed, Some(span)); }
    value
}

/// Validates a later write using its checked declaration rather than a narrowed local read view.
pub(super) fn local_store(ctx: &mut LoweringContext<'_, '_>, name: &str, value: LoweredValue,
    span: Span) -> Option<(LoweredValue, PhpType)> {
    if ctx.local_store_contracts.origins.contains_key(
        &(ctx.loop_storage_scope.clone(), span, name.to_string())) {
        ctx.prepare_eval_native_binding_store(name, span);
        return Some((value, PhpType::Mixed));
    }
    let contract = ctx.local_store_contracts.get(
        &(ctx.loop_storage_scope.clone(), span, name.to_string())).cloned();
    if ctx.source_mode != crate::source::SourceMode::Internal
        && ctx.supports_eval_native_binding_store(name)
        && contract.as_ref().is_none_or(|contract| contract.feature == elephc_semantics::StrictFeature::Locals) {
        if let Some(contract) = &contract {
            let slot = ctx.declare_local(name, PhpType::Mixed);
            super::local_binding_state::unless_eval_bound(ctx, slot, span, |ctx| {
                membership(ctx, value, &contract.ty, contract.feature, span);
            });
        }
        ctx.prepare_eval_native_binding_store(name, span);
        // The canonical publisher validates the original candidate against the surviving origin.
        return Some((value, PhpType::Mixed));
    }
    let Some(contract) = contract else {
        ctx.prepare_eval_native_binding_store(name, span);
        return None;
    };
    if ctx.source_mode == crate::source::SourceMode::Internal
        || !ctx.semantic_profile.contains(contract.feature) { return None; }
    if contract.feature == StrictFeature::Nullability {
        super::nullability::require_declared_value(ctx, value, &contract.ty, span);
        ctx.prepare_eval_native_binding_store(name, span);
        // Nullability adds a check; ordinary local contextualization still owns conversion.
        return None;
    }
    let value = membership(ctx, value, &contract.ty, contract.feature, span);
    ctx.prepare_eval_native_binding_store(name, span);
    Some((value_storage(ctx, value, &contract.ty, span), contract.ty))
}

/// Extracts only a kind already accepted by the guard, preserving its PHP-visible value.
pub(super) fn value_storage(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue,
    destination: &PhpType, span: Span) -> LoweredValue {
    if *destination == PhpType::Void {
        // An accepted boxed null owns storage, while the null binding itself owns no box.
        let null = ctx.emit_value(Op::ConstNull, Vec::new(), None, PhpType::Void,
            Op::ConstNull.default_effects(), Some(span));
        if ctx.value_is_owning_temporary(value) { super::ownership::release_if_owned(ctx, value, Some(span)); }
        return null;
    }
    if crate::ir::IrType::from_php(&destination.codegen_repr()) == crate::ir::IrType::TaggedScalar {
        let result = super::stmt::coerce_to_tagged_scalar(ctx, value, Some(span));
        if result.value != value.value && ctx.value_is_owning_temporary(value) {
            super::ownership::release_if_owned(ctx, value, Some(span));
        }
        return result;
    }
    if *destination == PhpType::Callable && value.ir_type != crate::ir::IrType::from_php(destination) {
        let pins = super::expr::call_operand_owners::pin_in_flight_owners(ctx, &[value.value], span);
        let result = ctx.emit_owned_value(Op::NormalizeCallable, vec![value.value], None,
            PhpType::Callable, Op::NormalizeCallable.default_effects(), Some(span));
        super::expr::call_operand_owners::unpin_in_flight_owners(ctx, pins, span);
        if ctx.value_is_owning_temporary(value) { super::ownership::release_if_owned(ctx, value, Some(span)); }
        return result;
    }
    if matches!(destination, PhpType::Iterable | PhpType::Array(_) | PhpType::AssocArray { .. })
        && value.ir_type != crate::ir::IrType::from_php(&destination.codegen_repr()) {
        let boxed = if matches!(value.ir_type, crate::ir::IrType::Heap(crate::ir::IrHeapKind::Mixed | crate::ir::IrHeapKind::Union)) {
            value
        } else {
            ctx.emit_owned_value(Op::MixedBox, vec![value.value], None, PhpType::Mixed,
                Op::MixedBox.default_effects(), Some(span))
        };
        let result = ctx.emit_owned_value(Op::MixedUnbox, vec![boxed.value], None,
            destination.clone(), Op::mixed_unbox_effects(destination), Some(span));
        if ctx.value_is_owning_temporary(boxed) { super::ownership::release_if_owned(ctx, boxed, Some(span)); }
        if boxed.value != value.value && ctx.value_is_owning_temporary(value) {
            super::ownership::release_if_owned(ctx, value, Some(span));
        }
        return result;
    }
    if matches!(destination, PhpType::Object(_))
        && value.ir_type != crate::ir::IrType::from_php(destination) {
        let result = ctx.emit_owned_value(Op::MixedUnbox, vec![value.value], None,
            destination.clone(), Op::mixed_unbox_effects(destination), Some(span));
        if ctx.value_is_owning_temporary(value) { super::ownership::release_if_owned(ctx, value, Some(span)); }
        return result;
    }
    if !matches!(destination, PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False | PhpType::Str)
        || value.ir_type == crate::ir::IrType::from_php(destination) { return value; }
    // The guarded identity still needs its full scalar ABI, including F64 and string pairs.
    // MixedUnbox materializes a pointer-word payload and cannot replace these typed paths.
    let result = ctx.emit_value(Op::Cast, vec![value.value],
        Some(crate::ir::Immediate::CastTarget(crate::ir::IrType::from_php(destination))),
        destination.clone(), Op::Cast.default_effects(), Some(span));
    if ctx.value_is_owning_temporary(value) {
        super::ownership::release_if_owned(ctx, value, Some(span));
    }
    result
}
