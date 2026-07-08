//! Purpose:
//! Orchestrates symbol, constant, class, and language-construct eval builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by core call dispatch.
//!
//! Key details:
//! - Concrete builtin behavior lives in focused `symbols/` modules so each
//!   source file stays cohesive and below the ordinary 500 LoC guideline.

use super::super::*;

mod class_alias;
mod class_attribute_args;
mod class_attribute_names;
mod class_exists;
mod class_get_attributes;
mod class_implements;
mod class_parents;
mod class_uses;
mod dispatch;
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

pub(in crate::interpreter) use dispatch::{eval_builtin_symbols_call, eval_symbols_values_result};
pub(in crate::interpreter) use class_implements::eval_class_relation_result;
pub(in crate::interpreter) use function_exists::eval_function_probe_exists;
pub(in crate::interpreter) use get_class::eval_get_class_result;
pub(in crate::interpreter) use get_parent_class::eval_get_parent_class_result;
pub(in crate::interpreter) use is_a::dynamic_object_is_a;
pub(in crate::interpreter) use is_callable::{
    eval_builtin_is_callable_call, eval_is_callable_call_with_evaluated_args,
    eval_is_callable_value,
};
