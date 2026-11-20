//! Purpose:
//! Captures reached predicate facts for stable physical properties and literal array projections.
//!
//! Called from:
//! - Condition evaluation before/after the predicate and logical source projection consumers.
//!
//! Key details:
//! - Facts belong to scope roots and generations, contain no PHP handles, and expire on effects.
//! - Getter/magic/ArrayAccess results never become repeatable physical storage proofs.

use super::*;
use crate::scope::{GuardRoot, GuardSelector};

/// A root and observation captured before evaluating one catalog predicate.
pub(in crate::interpreter) struct PreparedGuard {
    root: GuardRoot,
    selector: GuardSelector,
    generation: u64,
    epoch: u64,
    target: Type,
    original: Option<Type>,
    negated: bool,
}

/// Builds a stable syntactic projection without reading a PHP property or running a computed key.
fn projection(expr: &EvalExpr, context: &ElephcEvalContext) -> Option<(GuardRoot, GuardSelector)> {
    match expr {
        EvalExpr::PropertyGet { object, property } => match object.as_ref() {
            EvalExpr::LoadVar(name) => Some((GuardRoot::Variable(name.clone()), GuardSelector::Property(property.clone()))),
            _ => None,
        },
        EvalExpr::StaticPropertyGet { class_name, property } => {
            let class = resolve_eval_static_member_class_name(class_name, context).ok()?;
            Some((GuardRoot::Static(class.trim_start_matches('\\').to_ascii_lowercase()), GuardSelector::Property(property.clone())))
        }
        EvalExpr::ArrayGet { array, index } => {
            let EvalExpr::LoadVar(name) = array.as_ref() else { return None; };
            let key = match index.as_ref() {
                EvalExpr::Const(EvalConst::Int(value)) => EvalArrayReferenceKey::Int(*value),
                EvalExpr::Const(EvalConst::String(value)) => string_key(value.as_bytes(), context),
                EvalExpr::Const(EvalConst::Bytes(value)) => string_key(value, context),
                _ => return None,
            };
            Some((GuardRoot::Variable(name.clone()), GuardSelector::Index(key)))
        }
        _ => None,
    }
}

/// Uses the selected semantic key policy without evaluating or allocating a PHP operand.
fn string_key(bytes: &[u8], context: &ElephcEvalContext) -> EvalArrayReferenceKey {
    eval_numeric_string_array_key(bytes, context.semantic_profile())
        .map(EvalArrayReferenceKey::Int).unwrap_or_else(|| EvalArrayReferenceKey::String(bytes.to_vec()))
}

/// Snapshots only physical initialized storage before the predicate can execute PHP-visible work.
pub(in crate::interpreter) fn prepare(condition: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<Option<PreparedGuard>, EvalStatus> {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed) { return Ok(None); }
    let mut condition = condition;
    let mut negated = false;
    while let EvalExpr::Unary { op: EvalUnaryOp::LogicalNot, expr } = condition {
        condition = expr; negated = !negated;
    }
    let Some((argument, target)) = super::predicate(condition, context) else { return Ok(None); };
    let Some((root, selector)) = projection(argument, context) else { return Ok(None); };
    let Some(generation) = scope.guard_root_generation(&root) else { return Ok(None); };
    let stable = match (&root, &selector) {
        (GuardRoot::Variable(name), GuardSelector::Property(property)) => {
            if let Some(object) = visible_scope_cell(context, scope, name) {
                if values.type_tag(object)? == EVAL_TAG_OBJECT {
                    supports_source_guard(object, property, context, values)?
                } else { false }
            } else { false }
        }
        (GuardRoot::Variable(name), GuardSelector::Index(_)) => {
            visible_scope_cell(context, scope, name).map(|value| values.type_tag(value)).transpose()?
                .is_some_and(|tag| matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC))
        }
        (GuardRoot::Static(class), GuardSelector::Property(property)) => {
            if let Some((_, property)) = context.class_property(class, property) {
                property.is_static() && !property.has_get_hook() && !property.has_set_hook()
            } else { context.native_property_type(class, property).is_some() }
        }
        _ => false,
    };
    if !stable { return Ok(None); }
    let original = local_binding_origins::source_view(argument, context, scope).as_ref().map(generic_type);
    Ok(Some(PreparedGuard { root, selector, generation, epoch: context.source_view_epoch(), target, original, negated }))
}

/// Records the reached result only if no call, member mutation or shared write changed its observation.
pub(in crate::interpreter) fn record(prepared: Option<PreparedGuard>, truthy: bool,
    context: &ElephcEvalContext, scope: &mut ElephcEvalScope,
) {
    let Some(prepared) = prepared else { return; };
    if prepared.epoch != context.source_view_epoch() { return; }
    let truthy = truthy != prepared.negated;
    let ty = if truthy { Some(prepared.target.clone()) }
        else { prepared.original.and_then(|ty| super::exclude(ty, &prepared.target)) };
    if let Some(ty) = ty {
        scope.record_projection_source(prepared.root, prepared.selector, prepared.generation,
            prepared.epoch, parameter_type(&ty));
    }
}

/// Reads a scope-owned predicate fact independently from its binding's declared type.
pub(in crate::interpreter) fn source(expr: &EvalExpr, context: &ElephcEvalContext,
    scope: &ElephcEvalScope,
) -> Option<EvalParameterType> {
    let (root, selector) = projection(expr, context)?;
    scope.projection_source(&root, &selector, context).cloned()
}
