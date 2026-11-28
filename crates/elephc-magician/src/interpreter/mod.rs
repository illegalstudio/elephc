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
mod array_entry_copies;
mod array_reference_contracts;
mod argument_value_owners;
mod array_argument_sources;
mod call_argument_owners;

use argument_value_owners::*;
use call_argument_owners::*;
pub(crate) mod array_references;
mod reference_keys;
pub mod builtin_metadata;
mod builtin_interfaces;
mod builtins;
mod constant_eval;
mod constants;
mod context_calls;
pub use context_calls::*;
mod bytecode;
mod control;
mod dynamic_functions;
mod expressions;
mod generics;
mod include_exec;
mod operands;
mod checked_numeric;
mod checked_comparisons;
mod checked_strings;
mod checked_array_keys;
mod checked_nullability;
mod checked_cast;
mod native_results;
mod numeric_domains;
mod libc_shims;
mod null_object_boundaries;
mod nullsafe_chains;
mod reflection;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) mod repl;
mod return_type_compat;
mod return_values;
mod value_contracts;
mod property_value_contracts;
mod write_source_domains;
mod member_write_sources;
mod property_reference_contracts;
mod property_reference_candidates;
mod runtime_ops;
pub(crate) use runtime_ops::{EVAL_TAG_BOOL, EVAL_TAG_FLOAT, EVAL_TAG_INT, EVAL_TAG_NULL, EVAL_TAG_STRING};
mod scope_cells;
mod local_binding_origins;
mod mixed_source_proofs;
mod mixed_eval;
mod mixed_result_sources;
mod mixed_callable_sources;
mod mixed_method_results;
mod mixed_foreach_sources;
mod mixed_class_sources;
mod mixed_guards;
mod initialization;
mod probe_reads;
use local_binding_origins::{bind_parameter_origin, local_producer_contract, store_local_with_origin};
mod persistent_references;
mod static_locals;
mod global_bindings;
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
pub(crate) use bytecode::with_executable_cache_key;
pub use control::EvalOutcome;
use control::{
    ArgumentSource, BoundMethodArg, BoundNativeFunctionArgs, BoundNativeFunctionRefSlot, EvalArraySpliceDirectArgs,
    EvalByRefBindingMode, EvalControl, EvalPredefinedConstant, EvalSprintfSpec,
    EvaluatedCallArg, EvaluatedCallable,
};
use dynamic_functions::*;
use expressions::*;
use generics::*;
use include_exec::*;
use operands::*;
use libc_shims::*;
use null_object_boundaries::*;
use reflection::*;
use return_type_compat::*;
use return_values::*;
use value_contracts::*;
use property_value_contracts::*;
use member_write_sources::*;
use property_reference_contracts::*;
#[cfg(not(test))]
pub(crate) use property_reference_contracts::eval_validate_property_reference;
pub(crate) use property_reference_candidates::{prepare_property_reference_candidates,
    coerce_property_reference_candidates};
use property_reference_candidates::coerce_property_value_candidate;
pub use runtime_ops::{
    NativeCallableParameterMetadata, NativeCallableSignatureMetadata, RuntimeValueOps,
};
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

/// Validates immutable parser/context compatibility before either execution engine runs.
pub(crate) fn validate_program_semantics(
    context: &mut ElephcEvalContext, program: &EvalProgram, values: &impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let profile = context.activate_semantic_profile();
    if profile.bits() & !elephc_semantics::INTEGRATED_FEATURE_BITS != 0 {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    if !program.matches_semantic_profile(profile) { return Err(EvalStatus::AbiMismatch); }
    if values.fixed_semantic_profile()?.is_some_and(|runtime| runtime != profile) {
        return Err(EvalStatus::AbiMismatch);
    }
    Ok(())
}

/// Executes an EvalIR program and preserves escaping Throwable cells.
///
/// A fragment that fits the bytecode slice runs there. Any named gap, or a cached
/// gap for the same source, keeps the tree walker.
pub fn execute_program_outcome_with_context(
    context: &mut ElephcEvalContext,
    program: &EvalProgram,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    with_source_return_contract(None, None, None, context, |context|
        execute_program_in_return_scope(context, program, scope, values))
}

/// Runs one program after its return declaration has been separated from its caller's scope.
fn execute_program_in_return_scope(context: &mut ElephcEvalContext, program: &EvalProgram,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    validate_program_semantics(context, program, values)?;
    let previous_strict_types = context.strict_types();
    context.set_strict_types(program.strict_types());
    let executed = match bytecode::try_execute(context, program, scope, values) {
        Ok(Some(control)) => Ok(control),
        Ok(None) => execute_program_statements(program, context, scope, values),
        Err(status) => Err(status),
    };
    context.set_strict_types(previous_strict_types);
    match executed {
        Ok(EvalControl::None | EvalControl::ReturnVoid) => values.null().map(EvalOutcome::Value),
        Ok(EvalControl::Return(result)) => {
            let result = if result.is_borrowed() { values.retain(result)? } else { result };
            Ok(EvalOutcome::Value(result))
        }
        Ok(EvalControl::Throw(result)) => {
            let result = if result.is_borrowed() { values.retain(result)? } else { result };
            Ok(EvalOutcome::Throwable(result))
        }
        Ok(EvalControl::Break | EvalControl::Continue | EvalControl::Goto(_)) => {
            Err(EvalStatus::UnsupportedConstruct)
        }
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Clears the eval `preg_last_error()` code at a `--web` request boundary.
///
/// A worker serves every request on one reused thread, so an error recorded by one request's
/// eval would otherwise be the next request's `preg_last_error()` before it ran any regex.
/// Once the regex provider registered it, the code is the compiled `_preg_last_error`, which
/// the generated request reset also clears; this covers the thread-local fallback slot.
pub(crate) fn reset_request_regex_state() {
    builtins::eval_preg_set_last_error(0);
}

/// Returns the current interpreter availability status for the ABI stub.
pub fn current_stub_status() -> EvalStatus {
    EvalStatus::UnsupportedConstruct
}

#[cfg(test)]
mod tests;
