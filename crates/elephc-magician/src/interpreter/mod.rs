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
mod call_argument_owners;

use call_argument_owners::*;
pub(crate) mod array_references;
pub mod builtin_metadata;
mod builtin_interfaces;
mod builtins;
mod constant_eval;
mod constants;
mod context_calls;
pub use context_calls::*;
mod control;
mod dynamic_functions;
mod expressions;
mod generics;
mod generators;
pub(in crate::interpreter) mod highlight;
mod globals;
mod include_exec;
mod operands;
mod libc_shims;
mod reflection;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) mod repl;
mod return_type_compat;
mod return_values;
mod runtime_ops;
pub(crate) use runtime_ops::{EVAL_TAG_BOOL, EVAL_TAG_FLOAT, EVAL_TAG_INT, EVAL_TAG_NULL, EVAL_TAG_STRING};
mod scope_cells;
mod persistent_references;
mod superglobals;
mod statements;
#[cfg(not(test))]
mod output_handlers;
#[cfg(not(test))]
mod pcntl_escape;
mod throwables;

use crate::context::{
    decode_callable_descriptor, eval_builtin_is_backtrace_visible, ElephcEvalContext,
    ElephcEvalExecutionScope, EvalArrayCursor, EvalArrayReferenceKey, EvalCallFrame,
    EvalCallFrameKind, EvalClassMethodRef, EvalClosure, EvalClosureCaptureBinding,
    EvalClosureObjectTarget, EvalReferenceTarget, NativeCallableDefault, NativeCallableSignature,
    NativeFunction,
};
use crate::errors::{report_fatal_diagnostic, EvalParseDiagnostic, EvalStatus};
use crate::eval_ir::{
    EvalArrayElement, EvalAttribute, EvalAttributeArg, EvalBinOp, EvalCallArg, EvalCatch,
    EvalCastType, EvalClass, EvalClassConstant, EvalClassMethod, EvalClassProperty, EvalConst,
    EvalDestructureSlot, EvalDestructureTarget,
    EvalEnum, EvalEnumBackingType, EvalEnumCase, EvalExpr, EvalFunction, EvalInstanceOfTarget,
    EvalInterface, EvalInterfaceMethod, EvalInterfaceProperty, EvalMagicConst, EvalMatchArm,
    EvalParameterType, EvalParameterTypeVariant, EvalProgram, EvalStmt, EvalSwitchCase, EvalTrait,
    EvalTraitAdaptation, EvalUnaryOp, EvalVisibility,
};
#[cfg(test)]
use crate::parser::parse_fragment;
use crate::scope::{ElephcEvalScope, ScopeCellOwnership, ScopeEntry};
use crate::value::RuntimeCellHandle;
use array_literals::*;
use array_references::eval_owned_reference_target_value;
use builtin_interfaces::*;
use builtins::*;
use constant_eval::*;
use constants::*;
pub use control::EvalOutcome;
use control::{
    BoundMethodArg, BoundNativeFunctionArgs, BoundNativeFunctionRefSlot, EvalArraySpliceDirectArgs,
    EvalByRefBindingMode, EvalControl, EvalPredefinedConstant, EvalSprintfSpec,
    EvaluatedCallArg, EvaluatedCallable,
};
use dynamic_functions::*;
use expressions::*;
use generics::*;
use generators::*;
use globals::*;
use include_exec::*;
use operands::*;
use libc_shims::*;
use reflection::*;
use return_type_compat::*;
use return_values::*;
pub use runtime_ops::{aot_member_names_uncached, AotMemberNameKind, RuntimeValueOps};
pub(crate) use builtins::eval_class_name_is_a_bridge;
pub(crate) use builtins::eval_highlight_render;
pub(crate) use dynamic_functions::current_native_call_context;
pub(crate) use builtins::eval_spl_autoload_class as eval_spl_autoload_class_bridge;

/// Builds the PHP `spl_autoload_functions()` array for generated (AOT) callers.
pub(crate) fn registered_spl_autoload_functions(
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    builtins::eval_registered_spl_autoload_functions_result(context, values)
}
pub(crate) use builtins::eval_spl_autoload_classlike_definition;
use runtime_ops::*;
#[cfg(not(test))]
pub(crate) use pcntl_escape::value_contains_foreign_pcntl_callable;
use scope_cells::*;
use persistent_references::*;
use superglobals::*;
#[cfg(not(test))]
pub(crate) use statements::eval_dynamic_destructor_for_object_cell;
pub(crate) use builtins::eval_json_encode_serialized_object;
#[cfg(not(test))]
pub(crate) use generators::eval_generator_protocol_result;
#[cfg(not(test))]
pub(crate) use builtins::eval_unserialize_declared_object_from_hash;
#[cfg(not(test))]
pub(crate) use builtins::eval_serialize_object_fragment;
#[cfg(not(test))]
pub(crate) use builtins::eval_throwable_trace_value;
#[cfg(not(test))]
pub(crate) use statements::eval_object_clone_with_properties_for_ffi;
#[cfg(not(test))]
pub(crate) use statements::eval_property_set_for_ffi;
#[cfg(not(test))]
pub(crate) use output_handlers::eval_ob_handler_callback;
#[cfg(not(test))]
pub(crate) use output_handlers::release_ob_handler_callbacks;
use statements::*;
use throwables::*;
use std::ffi::{CStr, CString};
use std::mem::MaybeUninit;
use std::net::ToSocketAddrs;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

/// Registers one AOT-provided SPL callback through the interpreter's shared callback semantics.
pub(crate) fn register_runtime_spl_autoload_callback(
    callback: RuntimeCellHandle,
    prepend: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // `context` is usually one the bridge just made for this call, so PHP's by-value dedupe
    // has to look across every context that holds loaders.
    #[cfg(not(test))]
    if runtime_spl_autoload_callback_is_registered(callback, values)? {
        return values.bool_value(true);
    }
    register_spl_autoload_callback_unchecked(callback, prepend, context, values)
}

/// Removes one AOT-provided SPL callback from whichever eval context retains it.
///
/// The symmetric partner of [`register_runtime_spl_autoload_callback`]. Reports whether a
/// callback was actually removed, which is PHP's `spl_autoload_unregister` return value.
#[cfg(not(test))]
pub(crate) fn remove_runtime_spl_autoload_callback(
    callback: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    unregister_runtime_spl_autoload_callback(callback, values)
}

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
    match execute_program_statements(program, context, scope, values) {
        Ok(EvalControl::None | EvalControl::ReturnVoid) => values.null().map(EvalOutcome::Value),
        Ok(EvalControl::Return(result)) => {
            let result = if result.is_borrowed() { values.retain(result)? } else { result };
            Ok(EvalOutcome::Value(result))
        }
        Ok(EvalControl::Throw(result)) => {
            let result = if result.is_borrowed() { values.retain(result)? } else { result };
            Ok(EvalOutcome::Throwable(result))
        }
        Ok(EvalControl::Break(_) | EvalControl::Continue(_) | EvalControl::Goto(_)) => {
            Err(EvalStatus::UnsupportedConstruct)
        }
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Executes an already materialized runtime include and preserves escaping Throwable cells.
/// Context-free answer for a compiled include the interpreter would not run; see
/// `include_exec::fast_include_answer`.
pub(crate) fn fast_include_answer(path: &str, once: bool, value_discarded: bool) -> bool {
    include_exec::fast_include_answer(path, once, value_discarded)
}

pub fn execute_include_outcome_with_context(
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    path: RuntimeCellHandle,
    required: bool,
    once: bool,
    value_discarded: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    match include_exec::eval_include_value_with_use(
        path,
        required,
        once,
        !value_discarded,
        context,
        scope,
        values,
    ) {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Returns the current interpreter availability status for the ABI stub.
pub fn current_stub_status() -> EvalStatus {
    EvalStatus::UnsupportedConstruct
}

#[cfg(test)]
mod tests;
