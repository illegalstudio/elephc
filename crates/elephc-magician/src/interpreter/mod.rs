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
mod include_exec;
mod operands;
mod libc_shims;
mod reflection;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) mod repl;
mod return_type_compat;
mod return_values;
mod runtime_ops;
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
    ElephcEvalContext, ElephcEvalExecutionScope, EvalArrayCursor, EvalArrayReferenceKey,
    EvalReferenceTarget, EvalClosure, EvalClosureCaptureBinding, EvalClosureObjectTarget,
    NativeCallableDefault, NativeCallableSignature, NativeFunction,
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
use include_exec::*;
use operands::*;
use libc_shims::*;
use reflection::*;
use return_type_compat::*;
use return_values::*;
pub use runtime_ops::RuntimeValueOps;
use runtime_ops::*;
#[cfg(not(test))]
pub(crate) use pcntl_escape::value_contains_foreign_pcntl_callable;
use scope_cells::*;
use persistent_references::*;
use superglobals::*;
#[cfg(not(test))]
pub(crate) use statements::eval_dynamic_destructor_for_object_cell;
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
        Ok(EvalControl::Break | EvalControl::Continue) => Err(EvalStatus::UnsupportedConstruct),
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
