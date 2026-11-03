//! Purpose:
//! Lowers a checked and optimized PHP AST into EIR for the active backend.
//! Owns the AST-to-IR semantic boundary before validation and EIR codegen.
//!
//! Called from:
//! - `crate::pipeline::compile()` before optimization, register allocation, and codegen.
//!
//! Key details:
//! - Lowering preserves PHP source evaluation order by walking the AST in
//!   source order and emitting high-level EIR operations.
//! - EIR is the only production backend; unsupported lowering must fail explicitly.

mod array_pointer_scan;
mod boxed_array_appends;
mod builtin_datetime;
mod clone_overrides;
mod closure_scopes;
mod context;
mod diagnostics;
mod effect_refinement;
mod effects_lookup;
mod entry_locals;
mod expr;
mod fibers;
mod function;
mod integer_local_contracts;
mod loop_closures;
mod numeric_storage;
pub(crate) mod gradual_coercions;
/// The padding-thunk symbol codegen calls for a dynamic `new` that omits defaulted arguments.
pub(crate) use clone_overrides::driver_function_name as clone_override_driver_function_name;
pub(crate) use function::{dynamic_constructor_thunk_name, dynamic_constructor_thunk_owner};
pub(crate) use function::eval_native_default_helper_name;
pub(crate) use function::{lower_array_merge_callable, lower_boxed_usort_callable, lower_prepared_builtin_callable};
mod ownership;
mod program;
mod property_initializers;
mod property_default_errors;
mod reflection;
pub(crate) mod static_refs;
mod stmt;
mod throwable_constructors;
mod core_methods;

#[cfg(test)]
pub(crate) mod tests;

pub(crate) use expr::body_contains_eval_call;
/// Whether an op writes its string result into the shared concat scratch arena.
///
/// Codegen's synthetic builtin callable wrappers ask the same question return lowering does,
/// so they persist a scratch-backed result instead of handing the arena pointer to a caller.
pub(crate) use expr::body_writes_local;
pub(crate) use expr::cast_string_needs_owned_persist;
pub(crate) use expr::runtime_call_string_is_fresh;
#[allow(unused_imports)] // used by the binary's pipeline only
pub(crate) use program::module_runs_interpreted_code;
#[allow(unused_imports)] // used by the binary's pipeline only
pub(crate) use program::{closed_world_bridge_reasons, module_has_dynamic_include};

use std::fmt;
use std::path::Path;

use crate::codegen::platform::Target;
use crate::ir::{Module, ValidationError};
use crate::parser::ast::Program;
use crate::types::CheckResult;

/// Lowers `program` into an EIR module for `target`.
/// Resolver-produced inclusion markers require `lower_program_with_source_catalog`;
/// missing source metadata is rejected during validation rather than guessed.
///
/// `web` is the CLI `--web` flag; it is stored on the returned module (see
/// `crate::ir::Module::web`) so lowering can gate request-superglobal
/// (`$_SERVER`/`$_SESSION`/…) type seeding on it, mirroring the `web` gate
/// `codegen_ir::block_emit::emit_module` already applies to `.comm` storage.
pub fn lower_program(
    program: &Program,
    check_result: &CheckResult,
    target: Target,
    web: bool,
) -> Result<Module, LoweringError> {
    program::lower(program, check_result, target, None, web, None)
}

/// Lowers `program` into an EIR module and records the main PHP source path.
pub fn lower_program_with_source_path(
    program: &Program,
    check_result: &CheckResult,
    target: Target,
    source_path: &Path,
) -> Result<Module, LoweringError> {
    program::lower(program, check_result, target, Some(source_path), false, None)
}

/// Lowers `program` into EIR while retaining both source-path and web-mode metadata.
pub fn lower_program_with_source_path_and_web(
    program: &Program,
    check_result: &CheckResult,
    target: Target,
    source_path: &Path,
    web: bool,
) -> Result<Module, LoweringError> {
    program::lower(program, check_result, target, Some(source_path), web, None)
}

/// Lowers with immutable source identities available to every emitted body.
pub fn lower_program_with_source_catalog(
    program: &Program,
    check_result: &CheckResult,
    target: Target,
    source_path: &Path,
    web: bool,
    catalog: crate::ir::SourceCatalog,
) -> Result<Module, LoweringError> {
    program::lower(program, check_result, target, Some(source_path), web, Some(catalog))
}

/// Error produced while building or validating EIR.
#[derive(Debug)]
pub enum LoweringError {
    Validation(ValidationError),
    /// Some bodies failed validation while [`collect_invalid_bodies`] was on: the lowered module
    /// comes back with every failing body, so the caller can hand the class files those bodies
    /// belong to back to the runtime autoloader, exactly as it does for backend refusals.
    InvalidBodies {
        /// The first validator error, reported when nothing can be handed back.
        first: ValidationError,
        /// Every refused body and its validator error.
        failures: Vec<(String, String)>,
        /// The lowered module the failures were found in.
        module: Box<crate::ir::Module>,
    },
    /// A PHP shape this lowering deliberately refuses, reported as a source diagnostic.
    ///
    /// Lowering refuses instead of emitting code whenever the selected path cannot honor an
    /// ABI contract, most notably a reference binding that would otherwise treat an ordinary
    /// result value as a raw reference-cell pointer.
    Unsupported(crate::errors::CompileError),
}

impl fmt::Display for LoweringError {
    /// Formats the lowering error for CLI diagnostics.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoweringError::Validation(err) | LoweringError::InvalidBodies { first: err, .. } => {
                write!(f, "EIR validation failed: {:?}", err)
            }
            LoweringError::Unsupported(err) => write!(
                f,
                "unsupported EIR lowering at {}:{}: {}",
                err.span.line, err.span.col, err.message
            ),
        }
    }
}

/// Records that some class file was handed to the runtime autoloader, so the interpreter will
/// declare classes the compiled program never saw (see `program::CLASS_FILES_DEFERRED`).
pub fn set_class_files_deferred(deferred: bool) {
    program::CLASS_FILES_DEFERRED.store(deferred, std::sync::atomic::Ordering::Relaxed);
}

/// Turns on collection of every body that fails EIR validation (see
/// [`LoweringError::InvalidBodies`]); off by default, which keeps fail-fast validation.
pub fn collect_invalid_bodies(enabled: bool) {
    program::COLLECT_INVALID_BODIES.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

impl From<ValidationError> for LoweringError {
    /// Converts an EIR validation error into a lowering error.
    fn from(value: ValidationError) -> Self {
        LoweringError::Validation(value)
    }
}
