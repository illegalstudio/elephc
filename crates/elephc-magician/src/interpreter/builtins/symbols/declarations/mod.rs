//! Purpose:
//! Declarative eval registry entries and dispatch adapters for symbol and class metadata builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols` module loading.
//! - `crate::interpreter::builtins::hooks` for migrated symbol dispatch.
//!
//! Key details:
//! - Direct adapters preserve source-sensitive language constructs and
//!   `is_callable()` by-reference writeback.
//! - Values adapters replace the legacy dynamic symbols dispatcher.

use super::super::super::*;
use super::super::*;

mod class_alias;
mod class_attribute_args;
mod class_attribute_names;
mod class_exists;
mod class_get_attributes;
mod class_implements;
mod class_parents;
mod class_uses;
mod empty;
mod enum_exists;
mod function_exists;
mod get_called_class;
mod get_class;
mod get_class_methods;
mod get_class_vars;
mod get_declared_classes;
mod get_declared_interfaces;
mod get_declared_traits;
mod get_object_vars;
mod get_parent_class;
mod get_resource_id;
mod get_resource_type;
mod interface_exists;
mod is_a;
mod is_callable;
mod is_subclass_of;
mod isset;
mod method_exists;
mod property_exists;
mod spl_autoload;
mod spl_autoload_call;
mod spl_autoload_extensions;
mod spl_autoload_functions;
mod spl_autoload_register;
mod spl_autoload_unregister;
mod spl_classes;
mod spl_object_hash;
mod spl_object_id;
mod trait_exists;
mod unset;

/// Dispatches direct expression-level calls for declaratively migrated symbol builtins.
pub(in crate::interpreter) fn eval_builtin_symbols_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "class_alias" => eval_builtin_class_alias(args, context, scope, values),
        "class_attribute_args" | "class_attribute_names" | "class_get_attributes" => {
            eval_builtin_class_attribute_metadata(name, args, context, scope, values)
        }
        "class_exists" => eval_builtin_class_exists(args, context, scope, values),
        "class_implements" | "class_parents" | "class_uses" => {
            eval_builtin_class_relation(name, args, context, scope, values)
        }
        "empty" => eval_builtin_empty(args, context, scope, values),
        "enum_exists" | "trait_exists" => {
            eval_builtin_class_like_exists(name, args, context, scope, values)
        }
        "function_exists" | "is_callable" => {
            eval_builtin_function_probe(name, args, context, scope, values)
        }
        "get_called_class" => eval_builtin_get_called_class(args, context, values),
        "get_class" => eval_builtin_get_class(args, context, scope, values),
        "get_class_methods" => eval_builtin_get_class_methods(args, context, scope, values),
        "get_class_vars" => eval_builtin_get_class_vars(args, context, scope, values),
        "get_declared_classes" | "get_declared_interfaces" | "get_declared_traits" => {
            eval_builtin_get_declared_symbols(name, args, context, values)
        }
        "get_object_vars" => eval_builtin_get_object_vars(args, context, scope, values),
        "get_parent_class" => eval_builtin_get_parent_class(args, context, scope, values),
        "get_resource_id" | "get_resource_type" => {
            eval_builtin_resource_introspection(name, args, context, scope, values)
        }
        "interface_exists" => eval_builtin_interface_exists(args, context, scope, values),
        "is_a" | "is_subclass_of" => {
            eval_builtin_is_a_relation(name, args, context, scope, values)
        }
        "isset" => eval_builtin_isset(args, context, scope, values),
        "method_exists" | "property_exists" => {
            eval_builtin_member_exists(name, args, context, scope, values)
        }
        "spl_autoload" | "spl_autoload_call" => {
            eval_builtin_spl_autoload_void(name, args, context, scope, values)
        }
        "spl_autoload_extensions" => {
            eval_builtin_spl_autoload_extensions(args, context, scope, values)
        }
        "spl_autoload_functions" => {
            eval_builtin_spl_autoload_functions(args, context, scope, values)
        }
        "spl_autoload_register" | "spl_autoload_unregister" => {
            eval_builtin_spl_autoload_bool(name, args, context, scope, values)
        }
        "spl_classes" => eval_builtin_spl_classes(args, values),
        "spl_object_hash" | "spl_object_id" => {
            eval_builtin_spl_object_identity(name, args, context, scope, values)
        }
        "unset" => eval_builtin_unset(args, context, scope, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches evaluated-argument calls for declaratively migrated symbol builtins.
pub(in crate::interpreter) fn eval_symbols_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "class_alias" => eval_class_alias_result(evaluated_args, context, values),
        "class_attribute_args" | "class_attribute_names" | "class_get_attributes" => {
            eval_class_attribute_metadata_result(name, evaluated_args, context, values)
        }
        "class_exists" => eval_class_exists_result(evaluated_args, context, values),
        "class_implements" | "class_parents" | "class_uses" => {
            eval_class_relation_result(name, evaluated_args, context, values)
        }
        "empty" => eval_empty_result(evaluated_args, values),
        "enum_exists" | "trait_exists" => {
            eval_class_like_exists_result(name, evaluated_args, context, values)
        }
        "function_exists" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_function_probe_result(name, *value, context, values)
        }
        "get_called_class" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_get_called_class_result(context, values)
        }
        "get_class" => match evaluated_args {
            [] => eval_get_class_no_arg_result(context, values),
            [object] => eval_get_class_result(*object, context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "get_class_methods" => eval_get_class_methods_result(evaluated_args, context, values),
        "get_class_vars" => eval_get_class_vars_result(evaluated_args, context, values),
        "get_declared_classes" | "get_declared_interfaces" | "get_declared_traits" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_get_declared_symbols_result(name, context, values)
        }
        "get_object_vars" => eval_get_object_vars_result(evaluated_args, context, values),
        "get_parent_class" => match evaluated_args {
            [] => eval_get_parent_class_no_arg_result(context, values),
            [object_or_class] => eval_get_parent_class_result(*object_or_class, context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "get_resource_id" | "get_resource_type" => {
            let [resource] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_resource_introspection_result(name, *resource, values)
        }
        "interface_exists" => eval_interface_exists_result(evaluated_args, context, values),
        "is_a" | "is_subclass_of" => {
            eval_is_a_relation_result(name, evaluated_args, context, values)
        }
        "is_callable" => eval_is_callable_with_values(evaluated_args, context, values),
        "isset" => eval_isset_result(evaluated_args, values),
        "method_exists" | "property_exists" => {
            eval_member_exists_result(name, evaluated_args, context, values)
        }
        "spl_autoload" | "spl_autoload_call" => {
            eval_spl_autoload_void_result(name, evaluated_args, values)
        }
        "spl_autoload_extensions" => {
            eval_spl_autoload_extensions_result(evaluated_args, context, values)
        }
        "spl_autoload_functions" => eval_spl_autoload_functions_result(evaluated_args, values),
        "spl_autoload_register" | "spl_autoload_unregister" => {
            eval_spl_autoload_bool_result(name, evaluated_args, values)
        }
        "spl_classes" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_spl_classes_result(values)
        }
        "spl_object_hash" | "spl_object_id" => {
            let [object] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_spl_object_identity_result(name, *object, values)
        }
        "unset" => eval_unset_result(evaluated_args, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `spl_classes()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_spl_classes(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_spl_classes_result(values)
}

/// Builds the static class-name list returned by eval `spl_classes()`.
pub(in crate::interpreter) fn eval_spl_classes_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_static_string_array_result(EVAL_SPL_CLASS_NAMES, values)
}
