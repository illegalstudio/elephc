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
pub(in crate::interpreter) fn generic_value_accepts(ty: &Type, value: RuntimeCellHandle, context: &ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<bool, EvalStatus> {
    match ty {
        Type::Atom(name) if name == "false" => Ok(values.type_tag(value)? == EVAL_TAG_BOOL && !values.truthy(value)?),
        Type::Apply(name, args) if name == "array" => {
            let tag = values.type_tag(value)?;
            if !matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) || args.len() == 1 && tag != EVAL_TAG_ARRAY { return Ok(false); }
            let Some(element_type) = args.last() else { return Ok(false); };
            for index in 0..values.array_len(value)? {
                if args.len() == 2 {
                    let key = values.array_iter_key(value, index)?;
                    let accepted = generic_value_accepts(&args[0], key, context, values);
                    let released = values.release(key);
                    if !accepted.and_then(|ok| released.map(|()| ok))? { return Ok(false); }
                }
                let element = values.array_iter_value(value, index)?;
                let accepted = generic_value_accepts(element_type, element, context, values);
                let released = values.release(element);
                if !accepted.and_then(|ok| released.map(|()| ok))? { return Ok(false); }
            }
            Ok(true)
        }
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
