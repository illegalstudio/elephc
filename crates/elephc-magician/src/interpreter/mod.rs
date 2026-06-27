//! Purpose:
//! Interprets EvalIR against a materialized caller scope.
//! The interpreter is generic over runtime value operations so it can execute
//! by manipulating opaque elephc runtime-cell handles.
//!
//! Called from:
//! - Future `crate::__elephc_eval_execute()` implementation.
//! - `cargo test -p elephc-magician` for scope/value-flow validation.
//!
//! Key details:
//! - This module does not own PHP values. Constants and operations are delegated
//!   to `RuntimeValueOps`, which will be backed by elephc runtime hooks.

mod array_literals;
mod builtin_interfaces;
mod builtins;
mod constant_eval;
mod constants;
mod control;
mod core_builtins;
mod debug_output;
mod dynamic_functions;
mod expressions;
mod include_exec;
mod json;
mod libc_shims;
mod reflection;
mod return_type_compat;
mod return_values;
mod runtime_ops;
mod scope_cells;
mod statements;
mod throwables;

use crate::context::{
    ElephcEvalContext, ElephcEvalExecutionScope, EvalReferenceTarget, NativeCallableDefault,
    NativeCallableSignature, NativeFunction,
};
use crate::errors::{EvalParseError, EvalStatus};
use crate::eval_ir::{
    EvalArrayElement, EvalAttribute, EvalAttributeArg, EvalBinOp, EvalCallArg, EvalCatch,
    EvalCastType, EvalClass, EvalClassConstant, EvalClassMethod, EvalClassProperty, EvalConst,
    EvalEnum, EvalEnumBackingType, EvalEnumCase, EvalExpr, EvalFunction, EvalInstanceOfTarget,
    EvalInterface, EvalInterfaceMethod, EvalInterfaceProperty, EvalMagicConst, EvalMatchArm,
    EvalParameterType, EvalParameterTypeVariant, EvalProgram, EvalStmt, EvalSwitchCase, EvalTrait,
    EvalTraitAdaptation, EvalUnaryOp, EvalVisibility,
};
use crate::json_validate::{self, JsonParseError, JsonParseErrorKind, JsonValue};
use crate::parser::parse_fragment;
use crate::scope::{ElephcEvalScope, ScopeCellOwnership, ScopeEntry};
use crate::value::RuntimeCellHandle;
use array_literals::*;
use builtin_interfaces::*;
use builtins::*;
use constant_eval::*;
use constants::*;
pub use control::EvalOutcome;
use control::{
    BoundMethodArg, EvalArraySpliceDirectArgs, EvalControl, EvalPredefinedConstant,
    EvalSprintfSpec, EvaluatedCallArg, EvaluatedCallable,
};
use core_builtins::*;
use debug_output::*;
use dynamic_functions::*;
use expressions::*;
use include_exec::*;
use json::*;
use libc_shims::*;
use reflection::*;
use regex::bytes::{Captures, Regex, RegexBuilder};
use return_type_compat::*;
use return_values::*;
pub use runtime_ops::RuntimeValueOps;
use runtime_ops::*;
use scope_cells::*;
#[cfg(not(test))]
pub(crate) use statements::eval_dynamic_destructor_for_object_cell;
use statements::*;
use throwables::*;
use std::ffi::{CStr, CString};
use std::mem::MaybeUninit;
use std::net::ToSocketAddrs;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

/// Executes an EvalIR program and returns the eval result cell.
pub fn execute_program(
    program: &EvalProgram,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut context = ElephcEvalContext::new();
    execute_program_with_context(&mut context, program, scope, values)
}

/// Executes an EvalIR program with a persistent eval context for dynamic declarations.
pub fn execute_program_with_context(
    context: &mut ElephcEvalContext,
    program: &EvalProgram,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match execute_program_outcome_with_context(context, program, scope, values)? {
        EvalOutcome::Value(result) => Ok(result),
        EvalOutcome::Throwable(error) => {
            context.set_pending_throw(error);
            Err(EvalStatus::UncaughtThrowable)
        }
    }
}

/// Executes an EvalIR program and preserves escaping Throwable cells.
pub fn execute_program_outcome_with_context(
    context: &mut ElephcEvalContext,
    program: &EvalProgram,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    match execute_statements(program.statements(), context, scope, values) {
        Ok(EvalControl::None | EvalControl::ReturnVoid) => values.null().map(EvalOutcome::Value),
        Ok(EvalControl::Return(result)) => Ok(EvalOutcome::Value(result)),
        Ok(EvalControl::Throw(result)) => Ok(EvalOutcome::Throwable(result)),
        Ok(EvalControl::Break | EvalControl::Continue) => Err(EvalStatus::UnsupportedConstruct),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Executes a zero-argument function declared in the shared eval context.
pub fn execute_context_function_zero_args(
    context: &mut ElephcEvalContext,
    name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    execute_context_function(context, name, Vec::new(), values)
}

/// Executes a function declared in the shared eval context with prepared argument cells.
pub fn execute_context_function(
    context: &mut ElephcEvalContext,
    name: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match execute_context_function_outcome(context, name, args, values)? {
        EvalOutcome::Value(result) => Ok(result),
        EvalOutcome::Throwable(error) => {
            context.set_pending_throw(error);
            Err(EvalStatus::UncaughtThrowable)
        }
    }
}

/// Executes a function declared in the shared eval context and preserves thrown cells.
pub fn execute_context_function_outcome(
    context: &mut ElephcEvalContext,
    name: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    context
        .function(name)
        .cloned()
        .map_or(Err(EvalStatus::UnsupportedConstruct), |function| {
            match eval_dynamic_function_with_values(&function, args, context, values) {
                Ok(result) => Ok(EvalOutcome::Value(result)),
                Err(EvalStatus::UncaughtThrowable) => context
                    .take_pending_throw()
                    .map(EvalOutcome::Throwable)
                    .ok_or(EvalStatus::UncaughtThrowable),
                Err(status) => Err(status),
            }
        })
}

/// Executes a named eval-context callable with arguments from a PHP array container.
pub fn execute_context_function_call_array(
    context: &mut ElephcEvalContext,
    name: &str,
    arg_array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match execute_context_function_call_array_outcome(context, name, arg_array, values)? {
        EvalOutcome::Value(result) => Ok(result),
        EvalOutcome::Throwable(error) => {
            context.set_pending_throw(error);
            Err(EvalStatus::UncaughtThrowable)
        }
    }
}

/// Executes a named eval-context callable from an argument array and preserves thrown cells.
pub fn execute_context_function_call_array_outcome(
    context: &mut ElephcEvalContext,
    name: &str,
    arg_array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    if !values.is_array_like(arg_array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let evaluated_args = eval_array_call_arg_values(arg_array, values)?;
    match eval_callable_with_call_array_args(name, evaluated_args, context, values) {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Constructs a class declared in the shared eval context with prepared positional arguments.
pub fn execute_context_new_object_outcome(
    context: &mut ElephcEvalContext,
    name: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    let Some(class) = context.class(name).cloned() else {
        return Err(EvalStatus::UnsupportedConstruct);
    };
    let evaluated_args = args
        .into_iter()
        .map(|value| EvaluatedCallArg {
            name: None,
            value,
            ref_target: None,
        })
        .collect();
    let mut scope = ElephcEvalScope::new();
    match eval_dynamic_class_new_object(&class, evaluated_args, context, &mut scope, values) {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Calls a method on a value that may be an eval-created object.
pub fn execute_context_method_call_outcome(
    context: &mut ElephcEvalContext,
    object: RuntimeCellHandle,
    method: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    match eval_method_call_result(object, method, args, context, values) {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Resolves object class-name builtins against eval dynamic-object metadata first.
pub fn execute_context_object_class_name(
    context: &mut ElephcEvalContext,
    lookup: &str,
    object_or_class: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match lookup {
        "get_class" => eval_get_class_result(object_or_class, context, values),
        "get_parent_class" => eval_get_parent_class_result(object_or_class, context, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Tests an object relation against eval dynamic-object metadata before AOT metadata.
pub fn execute_context_object_is_a(
    context: &mut ElephcEvalContext,
    object: RuntimeCellHandle,
    target_class: &str,
    exclude_self: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Ok(false);
    }
    let target_class = target_class.trim_start_matches('\\');
    let resolved_target_class = context
        .resolve_class_like_name(target_class)
        .unwrap_or_else(|| target_class.to_string());
    dynamic_object_is_a(
        object,
        &resolved_target_class,
        exclude_self,
        context,
        values,
    )?
    .map_or_else(
        || values.object_is_a(object, &resolved_target_class, exclude_self),
        Ok,
    )
}

/// Returns the current interpreter availability status for the ABI stub.
pub fn current_stub_status() -> EvalStatus {
    EvalStatus::UnsupportedConstruct
}

#[cfg(test)]
mod tests;
