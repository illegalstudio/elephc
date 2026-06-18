//! Purpose:
//! Dispatches already evaluated symbol, class, object, and resource introspection builtins by dynamic callable name.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry::dispatch`.
//!
//! Key details:
//! - Returns `Ok(None)` for names outside this domain so the parent dispatcher can
//!   continue probing other builtin families.

use super::super::super::super::*;
use super::super::super::*;

/// Attempts to dispatch evaluated symbol, class, object, and resource introspection builtins.
pub(in crate::interpreter) fn eval_symbols_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "spl_classes" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_spl_classes_result(values)?
        }
        "spl_object_id" | "spl_object_hash" => {
            let [object] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_spl_object_identity_result(name, *object, values)?
        }
        "function_exists" | "is_callable" => {
            let [name] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let name = values.string_bytes(*name)?;
            let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
            let name = name.trim_start_matches('\\').to_ascii_lowercase();
            values.bool_value(eval_function_probe_exists(context, &name))?
        }
        "class_exists" => eval_class_exists_result(evaluated_args, context, values)?,
        "class_alias" => eval_class_alias_result(evaluated_args, context, values)?,
        "enum_exists" | "trait_exists" => {
            eval_class_like_exists_result(name, evaluated_args, values)?
        }
        "interface_exists" => eval_interface_exists_result(evaluated_args, values)?,
        "is_a" | "is_subclass_of" => {
            eval_is_a_relation_result(name, evaluated_args, context, values)?
        }
        "get_class" => {
            let [object] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_get_class_result(*object, context, values)?
        }
        "get_parent_class" => {
            let [object_or_class] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_get_parent_class_result(*object_or_class, values)?
        }
        "get_resource_id" | "get_resource_type" => {
            let [resource] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_resource_introspection_result(name, *resource, values)?
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}
