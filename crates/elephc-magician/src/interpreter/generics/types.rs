//! Purpose:
//! Infers runtime generic type terms and validates retained typed array/callable contracts.
//!
//! Called from:
//! - Generic binding, parameter validation, and return validation.
//!
//! Key details:
//! - Array reads and iteration keys are owned temporaries and are released on every path.
//! - Heterogeneous element storage is represented by mixed, matching AOT inference.

use super::*;
use crate::eval_ir::parameter_type;

/// Infers concrete types from runtime cells without retaining values in template metadata.
pub(in crate::interpreter) fn runtime_generic_type(value: RuntimeCellHandle, context: &ElephcEvalContext, values: &mut impl RuntimeValueOps, depth: usize) -> Result<Type, EvalStatus> {
    if depth > elephc_generics::MAX_TYPE_ARGUMENT_DEPTH { return Ok(Type::atom("mixed")); }
    Ok(match values.type_tag(value)? {
        EVAL_TAG_NULL => Type::atom("null"), EVAL_TAG_BOOL => Type::atom("bool"),
        EVAL_TAG_INT => Type::atom("int"), EVAL_TAG_FLOAT => Type::atom("float"),
        EVAL_TAG_STRING => Type::atom("string"),
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            let associative = values.type_tag(value)? == EVAL_TAG_ASSOC;
            let mut key_type = None;
            let mut element_type = None;
            for index in 0..values.array_len(value)? {
                let key = values.array_iter_key(value, index)?;
                let key_result = runtime_generic_type(key, context, values, depth + 1);
                let key_release = values.release(key);
                let ty = key_result.and_then(|ty| key_release.map(|()| ty))?;
                merge_type(&mut key_type, ty);
                let element = values.array_iter_value(value, index)?;
                let result = runtime_generic_type(element, context, values, depth + 1);
                let release = values.release(element);
                let ty = result.and_then(|ty| release.map(|()| ty))?;
                merge_type(&mut element_type, ty);
            }
            let elem = element_type.unwrap_or_else(|| Type::atom("mixed"));
            Type::Apply("array".into(), if associative { vec![key_type.unwrap_or_else(|| Type::atom("mixed")), elem] } else { vec![elem] })
        }
        EVAL_TAG_OBJECT | EVAL_TAG_CALLABLE => {
            if let Ok(identity) = values.object_identity(value) {
                if let Some(EvalClosureObjectTarget::Named(name) | EvalClosureObjectTarget::BoundNamed { name, .. }) = context.closure_object_target(identity) {
                    if let Some(closure) = context.closure(name) {
                        let function = closure.function();
                        if let Some(ret) = function.return_type() {
                            return Ok(Type::Callable(function.parameter_types().iter().map(|ty| ty.as_ref().map(generic_type).unwrap_or_else(|| Type::atom("mixed"))).collect(), Box::new(generic_type(ret))));
                        }
                    }
                }
                if let Some(name) = context.dynamic_object_class_name(identity) {
                    return crate::parser::generics::parse_type_string(&name).map_err(|_| EvalStatus::ParseError);
                }
            }
            let class = values.object_class_name(value)?;
            let result = values.string_bytes(class).map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
            let release = values.release(class);
            let name = result.and_then(|name| release.map(|()| name))?;
            crate::parser::generics::parse_type_string(&name).unwrap_or_else(|_| Type::atom(name))
        }
        _ => Type::atom("mixed"),
    })
}

/// Merges runtime elements using AOT's mixed representation for heterogeneous storage.
fn merge_type(current: &mut Option<Type>, next: Type) {
    match current { None => *current = Some(next), Some(existing) if *existing != next => *existing = Type::atom("mixed"), _ => {} }
}

/// Tests a structural generic type without coercing array elements or mutating their storage.
pub(in crate::interpreter) fn generic_value_accepts(ty: &Type, value: RuntimeCellHandle,
    context: &ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    accepts_value(ty, value, None, context, values)
}

/// Substitutes a proposed referent at every structural depth, reporting only associated roots.
pub(in crate::interpreter) fn generic_value_accepts_with_reference(ty: &Type, value: RuntimeCellHandle,
    reference: RuntimeCellHandle, proposed: RuntimeCellHandle, context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<bool>, EvalStatus> {
    let candidate = ReferenceCandidate { reference, proposed, matched: std::cell::Cell::new(false) };
    let accepted = accepts_value(ty, value, Some(&candidate), context, values)?;
    Ok(candidate.matched.get().then_some(accepted))
}

/// Borrowed substitution state belongs to this validation, never to a runtime handle registry.
struct ReferenceCandidate {
    reference: RuntimeCellHandle,
    proposed: RuntimeCellHandle,
    matched: std::cell::Cell<bool>,
}

/// Shares ordinary generic membership and prospective reference membership without publication.
fn accepts_value(ty: &Type, value: RuntimeCellHandle, candidate: Option<&ReferenceCandidate>,
    context: &ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if let Some(candidate) = candidate {
        if value.as_ptr() == candidate.reference.as_ptr() && values.is_reference(value)? {
            candidate.matched.set(true);
            let next = (candidate.proposed.as_ptr() != candidate.reference.as_ptr()).then_some(candidate);
            return accepts_value(ty, candidate.proposed, next, context, values);
        }
    }
    if crate::eval_ir::pending_array_contents(ty) {
        return Ok(candidate.is_none_or(|candidate| !candidate.matched.get()));
    }
    if let Some(schema) = crate::eval_ir::array_contents_schema(ty) {
        return if matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
            accepts_value(schema, value, candidate, context, values)
        } else { Ok(true) };
    }
    match ty {
        Type::Intersection(members) => {
            for member in members { if !accepts_value(member, value, candidate, context, values)? { return Ok(false); } }
            Ok(true)
        }
        Type::Union(members) => {
            for member in members {
                if candidate.is_some() && !generic_value_accepts(member, value, context, values)? { continue; }
                if accepts_value(member, value, candidate, context, values)? { return Ok(true); }
            }
            Ok(false)
        }
        Type::Nullable(inner) => if values.type_tag(value)? == EVAL_TAG_NULL { Ok(true) }
            else { accepts_value(inner, value, candidate, context, values) },
        Type::Atom(name) if name == "false" => Ok(values.type_tag(value)? == EVAL_TAG_BOOL && !values.truthy(value)?),
        // The native bridge boxes a pointer word as an integer. Its nominal pointee
        // is not present in the runtime tag, and null is handled by the enclosing type.
        Type::Atom(name) | Type::Apply(name, _) if name == "ptr" => {
            Ok(values.type_tag(value)? == EVAL_TAG_INT)
        }
        Type::Apply(name, args) if name == "array" => accepts_array(args, value, candidate, context, values),
        Type::Apply(_, _) => {
            let actual = runtime_generic_type(value, context, values, 0)?;
            generic_type_is_a(&actual, ty, context, values)
        }
        Type::Callable(expected_params, expected_ret) => {
            let actual = runtime_generic_type(value, context, values, 0)?;
            let Type::Callable(actual_params, actual_ret) = actual else { return Ok(false); };
            if expected_params.len() != actual_params.len() { return Ok(false); }
            for (expected, actual) in expected_params.iter().zip(&actual_params) {
                if !generic_type_is_a(expected, actual, context, values)? { return Ok(false); }
            }
            generic_type_is_a(&actual_ret, expected_ret, context, values)
        }
        _ => eval_method_parameter_type_accepts_exact(&parameter_type(ty), value, context, values),
    }
}

/// Pins each key and element read while checking structural membership and reference substitutions.
fn accepts_array(args: &[Type], value: RuntimeCellHandle, candidate: Option<&ReferenceCandidate>,
    context: &ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};
    let tag = values.type_tag(value)?;
    if !matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) || args.len() == 1 && tag != EVAL_TAG_ARRAY { return Ok(false); }
    let Some(element_type) = args.last() else { return Ok(false); };
    for index in 0..values.array_len(value)? {
        let key = RuntimeOwnerSlot::new(values.array_iter_key(value, index)?);
        let checked = with_runtime_owners(&key, || {
            let result = (|| {
                if args.len() == 2 && !accepts_value(&args[0], key.borrowed(), None, context, values)? { return Ok(false); }
                let element = RuntimeOwnerSlot::new(if candidate.is_some() {
                    values.array_get_preserving_references(value, key.borrowed())?
                } else { values.array_iter_value(value, index)? });
                with_runtime_owners(&element, || {
                    let accepted = accepts_value(element_type, element.borrowed(), candidate, context, values);
                    let owned = element.take();
                    let released = if owned.is_borrowed() { Ok(()) } else { values.release(owned) };
                    accepted.and_then(|accepted| released.map(|()| accepted))
                })
            })();
            let owned = key.take();
            let released = if owned.is_borrowed() { Ok(()) } else { values.release(owned) };
            result.and_then(|accepted| released.map(|()| accepted))
        })?;
        if !checked { return Ok(false); }
    }
    Ok(true)
}
