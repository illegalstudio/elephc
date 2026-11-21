//! Purpose:
//! Prepares independent weak declaration candidates for eval-owned property references.
//!
//! Called from:
//! - Interpreter publication and the native reference coordinator's external-source callback.
//!
//! Key details:
//! - Every declaration is planned from the original RHS before any PHP callback runs.
//! - Plans own immutable metadata, never property receivers or live alias-map borrows.
//! - Scalar binding follows implicit PHP rules independently of checked explicit casts.

use super::*;
use elephc_semantics::{conversions::{AllowedKinds, ValueKind}, weak_scalar_binding::{
    self, ScalarBindingInput, WeakScalarBinding}};
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

/// One copied declaration decision, independent of later source retirement or rebinding.
pub(crate) struct PropertyReferenceCandidate {
    action: Option<WeakScalarBinding>,
    stringable: bool,
    failure: String,
}

/// Copies source decisions while nominal metadata and aliases can be inspected without PHP.
pub(crate) fn prepare_property_reference_candidates(
    reference: RuntimeCellHandle, original: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Vec<PropertyReferenceCandidate>, EvalStatus> {
    if !context.semantic_profile().checked_property_references() { return Ok(Vec::new()); }
    let mut prepared = Vec::new();
    let mut sources=context.property_reference_types(reference);
    sources.extend(context.claim_property_reference_admission(reference));
    for source in sources {
        array_reference_contracts::validate_new_owner(reference, original, std::slice::from_ref(&source.declared), context, values)?;
        if crate::eval_ir::array_contents_schema(&crate::eval_ir::generic_type(&source.declared)).is_some() {
            validate_generic_local(&source.declared, &source.property, original, context, values)?;
            continue;
        }
        prepared.push(prepare_property_value_candidate(&source.owner,&source.property,&source.declared,original,context,values)?);
    }
    Ok(prepared)
}

/// Plans one immutable property declaration from the original value without consuming it.
pub(crate) fn prepare_property_value_candidate(owner: &str,property: &str,declared: &EvalParameterType,
    original: RuntimeCellHandle,context: &mut ElephcEvalContext,values: &mut impl RuntimeValueOps,
) -> Result<PropertyReferenceCandidate,EvalStatus> {
    let exact = eval_declared_value_type_accepts_exact(declared, Some(owner),
        Some(owner), original, context, values)?;
    let expected = eval_null_boundary_type_label(declared,
        Some(owner), Some(owner), context)?;
    let failure = format!("Cannot assign value to property {}::${} of type {expected}",
        owner.trim_start_matches('\\'), property);
    let mut allowed = AllowedKinds::EMPTY;
    if !declared.is_intersection() {
        for variant in declared.variants() {
            let kind = match variant {
                EvalParameterTypeVariant::Int => ValueKind::Integer,
                EvalParameterTypeVariant::Float => ValueKind::Float,
                EvalParameterTypeVariant::Bool => ValueKind::Boolean,
                EvalParameterTypeVariant::String => ValueKind::String,
                _ => continue,
            };
            allowed = allowed.union(AllowedKinds::one(kind));
        }
    }
    let weak = !context.semantic_profile().contains(elephc_semantics::StrictFeature::Coercions);
    let tag = values.type_tag(original)?;
    let stringable = weak && !exact && tag == EVAL_TAG_OBJECT
        && allowed.allows(ValueKind::String) && has_to_string(original, context, values)?;
    let action = if exact { Some(WeakScalarBinding::Identity) }
        else if weak { scalar_plan(allowed, original, tag, values)? } else { None };
    Ok(PropertyReferenceCandidate {action,stringable,failure})
}

/// Normalizes an ordinary unaliased property write without applying explicit-cast policy.
pub(in crate::interpreter) fn coerce_property_value_candidate(owner: &str,property: &EvalClassProperty,
    original: RuntimeCellHandle,context: &mut ElephcEvalContext,values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>,EvalStatus> {
    if !context.semantic_profile().checked_property_references() {return Ok(None);}
    let Some(declared)=property.property_type() else {return Ok(None);};
    if eval_declared_value_type_accepts_exact(declared,Some(owner),Some(owner),original,context,values)? {return Ok(None);}
    let plan=prepare_property_value_candidate(owner,property.name(),declared,original,context,values)?;
    coerce_property_reference_candidates(vec![plan],original,context,values)
}

/// Probes escaped dynamic objects against their own immutable method metadata without PHP.
fn has_to_string(value: RuntimeCellHandle, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps) -> Result<bool, EvalStatus> {
    #[cfg(not(test))]
    if let Some(owner) = crate::ffi::dynamic_destructors::dynamic_object_owner_context(values.object_identity(value)?) {
        if owner != context as *mut ElephcEvalContext {
            let owner = unsafe { owner.as_mut() }.ok_or(EvalStatus::RuntimeFatal)?;
            return eval_object_has_to_string(value, owner, values);
        }
    }
    eval_object_has_to_string(value, context, values)
}

/// Reads scalar payloads without applying explicit-cast policy or invoking object adapters.
fn scalar_plan(allowed: AllowedKinds, value: RuntimeCellHandle, tag: u64,
    values: &mut impl RuntimeValueOps) -> Result<Option<WeakScalarBinding>, EvalStatus> {
    let bytes;
    let input = match tag {
        EVAL_TAG_INT => ScalarBindingInput::Integer(values.raw_value_word(value)? as i64),
        EVAL_TAG_FLOAT => ScalarBindingInput::Float(f64::from_bits(values.raw_value_word(value)?)),
        EVAL_TAG_BOOL => ScalarBindingInput::Boolean(values.raw_value_word(value)? != 0),
        EVAL_TAG_NULL => ScalarBindingInput::Null,
        EVAL_TAG_STRING => { bytes = values.string_bytes(value)?; ScalarBindingInput::Bytes(&bytes) }
        _ => return Ok(None),
    };
    Ok(weak_scalar_binding::plan(allowed, input))
}

/// Coerces each copied source independently and transfers one agreeing candidate owner.
pub(crate) fn coerce_property_reference_candidates(
    prepared: Vec<PropertyReferenceCandidate>, original: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if prepared.is_empty() { return Ok(None); }
    let empty = RuntimeCellHandle::from_raw(std::ptr::null_mut()).borrowed();
    let selected = RuntimeOwnerSlot::new(empty);
    let candidate = RuntimeOwnerSlot::new(empty);
    with_runtime_owners(&(&selected, &candidate), || {
        let result = (|| {
            for plan in prepared {
                let value = match plan.action {
                    Some(WeakScalarBinding::Identity) => values.retain(original)?,
                    Some(WeakScalarBinding::Integer { value, loses_precision }) => {
                        if loses_precision {
                            precision_diagnostic(original, &plan.failure, context, values)?;
                        }
                        values.int(value)?
                    }
                    Some(WeakScalarBinding::Float(value)) => values.float(value)?,
                    Some(WeakScalarBinding::Boolean(value)) => values.bool_value(value)?,
                    Some(WeakScalarBinding::String) => values.cast_string(original)?,
                    None if plan.stringable => eval_string_context_value(original, context, values)?,
                    None => return eval_throw_type_error(&plan.failure, context, values),
                };
                candidate.initialize(value);
                if selected.borrowed().is_null() {
                    selected.initialize(candidate.take());
                    continue;
                }
                if selected.borrowed().as_ptr() != candidate.borrowed().as_ptr() {
                    let equal = values.compare(EvalBinOp::StrictEq, selected.borrowed(), candidate.borrowed())?;
                    let same = values.truthy(equal);
                    eval_release_value(context, values, equal)?;
                    if !same? { return eval_throw_type_error(
                        "Cannot assign to a reference held by properties with incompatible types", context, values); }
                }
                eval_release_value(context, values, candidate.take())?;
            }
            Ok(selected.take())
        })();
        finish_synthesized_method_arguments(result, [candidate.take(), selected.take()], context, values).map(Some)
    })
}

/// Preserves PHP's precision diagnostic and chained declaration error before publication.
fn precision_diagnostic(original: RuntimeCellHandle, failure: &str,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<(), EvalStatus> {
    let string = values.type_tag(original)? == EVAL_TAG_STRING;
    let bytes = values.string_bytes(original)?;
    let text = String::from_utf8_lossy(&bytes);
    let source = if string { format!("float-string \"{text}\"") } else { format!("float {text}") };
    let message = format!("Implicit conversion from {source} to int loses precision");
    let dispatched = eval_dispatch_php_error(&message, E_DEPRECATED, context, values);
    match native_results::finish(dispatched, context, values) {
        Ok(_) => Ok(()),
        Err(EvalStatus::UncaughtThrowable) => {
            let Some(previous) = context.take_pending_throw() else { return Err(EvalStatus::UncaughtThrowable); };
            eval_throw_type_error_with_previous(failure, previous, context, values)
        }
        Err(status) => Err(status),
    }
}
