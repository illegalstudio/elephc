//! Purpose:
//! Dispatches EvalIR statements and propagates structured control flow.
//! Statement families and runtime subsystems live in focused child modules.
//!
//! Called from:
//! - `crate::interpreter::execute_program_outcome_with_context()` and dynamic function execution.
//!
//! Key details:
//! - Statement execution propagates `EvalControl` instead of flattening returns, throws, breaks, or continues.
//! - Scope writes flow through shared scope-cell helpers so global aliases and reference aliases stay coherent.

mod abstract_requirements;
mod array_updates;
mod attributes_magic_validation;
mod callable_objects;
mod class_declarations;
mod class_resolution;
mod closure_binding;
mod dispatch;
mod dynamic_method_execution;
mod enum_declarations;
mod exceptions;
mod instance_property_access;
mod interface_contracts;
mod interface_member_validation;
mod loop_statements;
mod method_dispatch;
mod native_argument_binding;
mod native_constructor_defaults;
mod native_method_execution;
mod native_static_dispatch;
mod property_constant_validation;
mod property_validation;
mod reference_writeback;
mod reflection_instantiation;
mod static_method_dispatch;
mod static_property_access;
mod trait_declarations;

use super::*;
use crate::context::{
    NativeCallableArrayDefaultElement, NativeCallableArrayDefaultKey,
    NativeCallableObjectDefaultArg, NativeCallableSignature,
    push_native_frame_called_class_override,
};

use abstract_requirements::*;
pub(crate) use array_updates::*;
use attributes_magic_validation::*;
pub(in crate::interpreter) use callable_objects::*;
pub(in crate::interpreter) use class_declarations::*;
pub(in crate::interpreter) use class_resolution::*;
use closure_binding::*;
pub(in crate::interpreter) use dispatch::*;
pub(in crate::interpreter) use dynamic_method_execution::*;
pub(in crate::interpreter) use enum_declarations::*;
pub(in crate::interpreter) use exceptions::*;
pub(in crate::interpreter) use instance_property_access::*;
use interface_contracts::*;
use interface_member_validation::*;
pub(in crate::interpreter) use loop_statements::*;
pub(in crate::interpreter) use method_dispatch::*;
pub(in crate::interpreter) use native_argument_binding::*;
pub(in crate::interpreter) use native_constructor_defaults::*;
pub(in crate::interpreter) use native_method_execution::*;
pub(in crate::interpreter) use native_static_dispatch::*;
use property_constant_validation::*;
pub(in crate::interpreter) use property_validation::*;
pub(in crate::interpreter) use reference_writeback::*;
pub(in crate::interpreter) use reflection_instantiation::*;
pub(in crate::interpreter) use static_method_dispatch::*;
pub(in crate::interpreter) use static_property_access::*;
pub(in crate::interpreter) use trait_declarations::*;
