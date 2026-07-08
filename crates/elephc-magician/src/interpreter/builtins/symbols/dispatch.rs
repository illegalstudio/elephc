//! Purpose:
//! Direct and evaluated-argument dispatch for symbol builtins declared in the eval registry.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks` for migrated symbol dispatch.
//!
//! Key details:
//! - Public dispatch routes through per-builtin leaf wrappers so declaration
//!   files own their registry entry and runtime adapter.
//! - Internal impl dispatch keeps grouped helper behavior unchanged.

use super::*;

/// Routes direct expression-level symbol builtin calls through per-builtin leaf wrappers.
pub(in crate::interpreter) fn eval_builtin_symbols_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "class_alias" => super::class_alias::eval_class_alias_declared_call(args, context, scope, values),
        "class_attribute_args" => super::class_attribute_args::eval_class_attribute_args_declared_call(args, context, scope, values),
        "class_attribute_names" => super::class_attribute_names::eval_class_attribute_names_declared_call(args, context, scope, values),
        "class_exists" => super::class_exists::eval_class_exists_declared_call(args, context, scope, values),
        "class_get_attributes" => super::class_get_attributes::eval_class_get_attributes_declared_call(args, context, scope, values),
        "class_implements" => super::class_implements::eval_class_implements_declared_call(args, context, scope, values),
        "class_parents" => super::class_parents::eval_class_parents_declared_call(args, context, scope, values),
        "class_uses" => super::class_uses::eval_class_uses_declared_call(args, context, scope, values),
        "empty" => super::empty::eval_empty_declared_call(args, context, scope, values),
        "enum_exists" => super::enum_exists::eval_enum_exists_declared_call(args, context, scope, values),
        "function_exists" => super::function_exists::eval_function_exists_declared_call(args, context, scope, values),
        "get_called_class" => super::get_called_class::eval_get_called_class_declared_call(args, context, scope, values),
        "get_class" => super::get_class::eval_get_class_declared_call(args, context, scope, values),
        "get_class_methods" => super::get_class_methods::eval_get_class_methods_declared_call(args, context, scope, values),
        "get_class_vars" => super::get_class_vars::eval_get_class_vars_declared_call(args, context, scope, values),
        "get_declared_classes" => super::get_declared_classes::eval_get_declared_classes_declared_call(args, context, scope, values),
        "get_declared_interfaces" => super::get_declared_interfaces::eval_get_declared_interfaces_declared_call(args, context, scope, values),
        "get_declared_traits" => super::get_declared_traits::eval_get_declared_traits_declared_call(args, context, scope, values),
        "get_object_vars" => super::get_object_vars::eval_get_object_vars_declared_call(args, context, scope, values),
        "get_parent_class" => super::get_parent_class::eval_get_parent_class_declared_call(args, context, scope, values),
        "get_resource_id" => super::get_resource_id::eval_get_resource_id_declared_call(args, context, scope, values),
        "get_resource_type" => super::get_resource_type::eval_get_resource_type_declared_call(args, context, scope, values),
        "interface_exists" => super::interface_exists::eval_interface_exists_declared_call(args, context, scope, values),
        "is_a" => super::is_a::eval_is_a_declared_call(args, context, scope, values),
        "is_callable" => super::is_callable::eval_is_callable_declared_call(args, context, scope, values),
        "is_subclass_of" => super::is_subclass_of::eval_is_subclass_of_declared_call(args, context, scope, values),
        "isset" => super::isset::eval_isset_declared_call(args, context, scope, values),
        "method_exists" => super::method_exists::eval_method_exists_declared_call(args, context, scope, values),
        "property_exists" => super::property_exists::eval_property_exists_declared_call(args, context, scope, values),
        "spl_autoload" => super::spl_autoload::eval_spl_autoload_declared_call(args, context, scope, values),
        "spl_autoload_call" => super::spl_autoload_call::eval_spl_autoload_call_declared_call(args, context, scope, values),
        "spl_autoload_extensions" => super::spl_autoload_extensions::eval_spl_autoload_extensions_declared_call(args, context, scope, values),
        "spl_autoload_functions" => super::spl_autoload_functions::eval_spl_autoload_functions_declared_call(args, context, scope, values),
        "spl_autoload_register" => super::spl_autoload_register::eval_spl_autoload_register_declared_call(args, context, scope, values),
        "spl_autoload_unregister" => super::spl_autoload_unregister::eval_spl_autoload_unregister_declared_call(args, context, scope, values),
        "spl_classes" => super::spl_classes::eval_spl_classes_declared_call(args, context, scope, values),
        "spl_object_hash" => super::spl_object_hash::eval_spl_object_hash_declared_call(args, context, scope, values),
        "spl_object_id" => super::spl_object_id::eval_spl_object_id_declared_call(args, context, scope, values),
        "trait_exists" => super::trait_exists::eval_trait_exists_declared_call(args, context, scope, values),
        "unset" => super::unset::eval_unset_declared_call(args, context, scope, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Routes evaluated-argument symbol builtin calls through per-builtin leaf wrappers.
pub(in crate::interpreter) fn eval_symbols_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "class_alias" => super::class_alias::eval_class_alias_declared_values_result(evaluated_args, context, values),
        "class_attribute_args" => super::class_attribute_args::eval_class_attribute_args_declared_values_result(evaluated_args, context, values),
        "class_attribute_names" => super::class_attribute_names::eval_class_attribute_names_declared_values_result(evaluated_args, context, values),
        "class_exists" => super::class_exists::eval_class_exists_declared_values_result(evaluated_args, context, values),
        "class_get_attributes" => super::class_get_attributes::eval_class_get_attributes_declared_values_result(evaluated_args, context, values),
        "class_implements" => super::class_implements::eval_class_implements_declared_values_result(evaluated_args, context, values),
        "class_parents" => super::class_parents::eval_class_parents_declared_values_result(evaluated_args, context, values),
        "class_uses" => super::class_uses::eval_class_uses_declared_values_result(evaluated_args, context, values),
        "empty" => super::empty::eval_empty_declared_values_result(evaluated_args, context, values),
        "enum_exists" => super::enum_exists::eval_enum_exists_declared_values_result(evaluated_args, context, values),
        "function_exists" => super::function_exists::eval_function_exists_declared_values_result(evaluated_args, context, values),
        "get_called_class" => super::get_called_class::eval_get_called_class_declared_values_result(evaluated_args, context, values),
        "get_class" => super::get_class::eval_get_class_declared_values_result(evaluated_args, context, values),
        "get_class_methods" => super::get_class_methods::eval_get_class_methods_declared_values_result(evaluated_args, context, values),
        "get_class_vars" => super::get_class_vars::eval_get_class_vars_declared_values_result(evaluated_args, context, values),
        "get_declared_classes" => super::get_declared_classes::eval_get_declared_classes_declared_values_result(evaluated_args, context, values),
        "get_declared_interfaces" => super::get_declared_interfaces::eval_get_declared_interfaces_declared_values_result(evaluated_args, context, values),
        "get_declared_traits" => super::get_declared_traits::eval_get_declared_traits_declared_values_result(evaluated_args, context, values),
        "get_object_vars" => super::get_object_vars::eval_get_object_vars_declared_values_result(evaluated_args, context, values),
        "get_parent_class" => super::get_parent_class::eval_get_parent_class_declared_values_result(evaluated_args, context, values),
        "get_resource_id" => super::get_resource_id::eval_get_resource_id_declared_values_result(evaluated_args, context, values),
        "get_resource_type" => super::get_resource_type::eval_get_resource_type_declared_values_result(evaluated_args, context, values),
        "interface_exists" => super::interface_exists::eval_interface_exists_declared_values_result(evaluated_args, context, values),
        "is_a" => super::is_a::eval_is_a_declared_values_result(evaluated_args, context, values),
        "is_callable" => super::is_callable::eval_is_callable_declared_values_result(evaluated_args, context, values),
        "is_subclass_of" => super::is_subclass_of::eval_is_subclass_of_declared_values_result(evaluated_args, context, values),
        "isset" => super::isset::eval_isset_declared_values_result(evaluated_args, context, values),
        "method_exists" => super::method_exists::eval_method_exists_declared_values_result(evaluated_args, context, values),
        "property_exists" => super::property_exists::eval_property_exists_declared_values_result(evaluated_args, context, values),
        "spl_autoload" => super::spl_autoload::eval_spl_autoload_declared_values_result(evaluated_args, context, values),
        "spl_autoload_call" => super::spl_autoload_call::eval_spl_autoload_call_declared_values_result(evaluated_args, context, values),
        "spl_autoload_extensions" => super::spl_autoload_extensions::eval_spl_autoload_extensions_declared_values_result(evaluated_args, context, values),
        "spl_autoload_functions" => super::spl_autoload_functions::eval_spl_autoload_functions_declared_values_result(evaluated_args, context, values),
        "spl_autoload_register" => super::spl_autoload_register::eval_spl_autoload_register_declared_values_result(evaluated_args, context, values),
        "spl_autoload_unregister" => super::spl_autoload_unregister::eval_spl_autoload_unregister_declared_values_result(evaluated_args, context, values),
        "spl_classes" => super::spl_classes::eval_spl_classes_declared_values_result(evaluated_args, context, values),
        "spl_object_hash" => super::spl_object_hash::eval_spl_object_hash_declared_values_result(evaluated_args, context, values),
        "spl_object_id" => super::spl_object_id::eval_spl_object_id_declared_values_result(evaluated_args, context, values),
        "trait_exists" => super::trait_exists::eval_trait_exists_declared_values_result(evaluated_args, context, values),
        "unset" => super::unset::eval_unset_declared_values_result(evaluated_args, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches direct expression-level calls for declaratively migrated symbol builtins.
pub(in crate::interpreter) fn eval_builtin_symbols_call_impl(
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
        "spl_classes" => super::spl_classes::eval_builtin_spl_classes(args, values),
        "spl_object_hash" | "spl_object_id" => {
            eval_builtin_spl_object_identity(name, args, context, scope, values)
        }
        "unset" => eval_builtin_unset(args, context, scope, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches evaluated-argument calls for declaratively migrated symbol builtins.
pub(in crate::interpreter) fn eval_symbols_values_result_impl(
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
            super::spl_classes::eval_spl_classes_result(values)
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
