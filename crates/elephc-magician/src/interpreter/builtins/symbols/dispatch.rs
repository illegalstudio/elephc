//! Purpose:
//! Direct and evaluated-argument dispatch for symbol builtins declared in the eval registry.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks` for migrated symbol dispatch.
//!
//! Key details:
//! - Public dispatch routes through per-builtin leaf wrappers so declaration
//!   files own their registry entry and runtime adapter.
//! - Leaf builtin modules own the concrete direct and materialized-argument
//!   entry points so this dispatcher only routes public symbol-area calls.

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
