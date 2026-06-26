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
        "spl_autoload_register" | "spl_autoload_unregister" => {
            eval_spl_autoload_bool_result(name, evaluated_args, values)?
        }
        "spl_autoload" | "spl_autoload_call" => {
            eval_spl_autoload_void_result(name, evaluated_args, values)?
        }
        "spl_autoload_functions" => eval_spl_autoload_functions_result(evaluated_args, values)?,
        "spl_autoload_extensions" => {
            eval_spl_autoload_extensions_result(evaluated_args, context, values)?
        }
        "spl_object_id" | "spl_object_hash" => {
            let [object] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_spl_object_identity_result(name, *object, values)?
        }
        "function_exists" | "is_callable" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_function_probe_result(name, *value, context, values)?
        }
        "empty" => eval_empty_result(evaluated_args, values)?,
        "isset" => eval_isset_result(evaluated_args, values)?,
        "unset" => eval_unset_result(evaluated_args, values)?,
        "class_exists" => eval_class_exists_result(evaluated_args, context, values)?,
        "class_alias" => eval_class_alias_result(evaluated_args, context, values)?,
        "class_attribute_args" | "class_attribute_names" | "class_get_attributes" => {
            eval_class_attribute_metadata_result(name, evaluated_args, context, values)?
        }
        "class_implements" | "class_parents" | "class_uses" => {
            eval_class_relation_result(name, evaluated_args, context, values)?
        }
        "method_exists" | "property_exists" => {
            eval_member_exists_result(name, evaluated_args, context, values)?
        }
        "get_class_methods" => eval_get_class_methods_result(evaluated_args, context, values)?,
        "get_object_vars" => eval_get_object_vars_result(evaluated_args, context, values)?,
        "enum_exists" | "trait_exists" => {
            eval_class_like_exists_result(name, evaluated_args, context, values)?
        }
        "interface_exists" => eval_interface_exists_result(evaluated_args, context, values)?,
        "is_a" | "is_subclass_of" => {
            eval_is_a_relation_result(name, evaluated_args, context, values)?
        }
        "get_called_class" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_get_called_class_result(context, values)?
        }
        "get_class" => {
            let [object] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_get_class_result(*object, context, values)?
        }
        "get_declared_classes" | "get_declared_interfaces" | "get_declared_traits" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_get_declared_symbols_result(name, context, values)?
        }
        "get_parent_class" => {
            let [object_or_class] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_get_parent_class_result(*object_or_class, context, values)?
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
