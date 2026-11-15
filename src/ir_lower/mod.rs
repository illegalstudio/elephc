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
mod closure_param_hints;
mod closure_scopes;
mod context;
mod diagnostics;
mod effect_refinement;
mod effects_lookup;
mod eval_reference_layouts;
mod entry_locals;
mod expr;
mod fibers;
mod function;
mod integer_local_contracts;
mod loop_closures;
mod numeric_storage;
/// The padding-thunk symbol codegen calls for a dynamic `new` that omits defaulted arguments.
pub(crate) use function::dynamic_constructor_thunk_name;
pub(crate) use function::eval_native_default_helper_name;
pub(crate) use function::{lower_array_merge_callable, lower_boxed_usort_callable, lower_prepared_builtin_callable};
mod ownership;
mod object_boundaries;
mod program;
mod boxed_operators;
mod reference_property_validators;
pub use reference_property_validators::enable_native_reference_validation;
mod property_initializers;
mod property_default_errors;
mod reflection;
mod stmt;
mod scoped_writes;
mod throwable_constructors;
mod validation_context;
mod core_methods;

#[cfg(test)]
pub(crate) mod tests;

pub(crate) use expr::body_contains_eval_call;
#[allow(unused_imports)] // used by the binary's pipeline only
pub(crate) use program::module_runs_interpreted_code;

use std::cell::Cell;
use std::fmt;
use std::path::Path;

use crate::codegen::platform::Target;
use crate::ir::{Module, ValidationError};
use crate::parser::ast::Program;
use crate::types::CheckResult;

/// Lowers `program` into an EIR module for `target`.
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
    program::lower(program, check_result, target, None, web, false)
}

/// Lowers `program` into an EIR module and records the main PHP source path.
pub fn lower_program_with_source_path(
    program: &Program,
    check_result: &CheckResult,
    target: Target,
    source_path: &Path,
) -> Result<Module, LoweringError> {
    program::lower(program, check_result, target, Some(source_path), false, false)
}

/// Lowers `program` into EIR while retaining both source-path and web-mode metadata.
pub fn lower_program_with_source_path_and_web(
    program: &Program,
    check_result: &CheckResult,
    target: Target,
    source_path: &Path,
    web: bool,
) -> Result<Module, LoweringError> {
    program::lower(program, check_result, target, Some(source_path), web, false)
}

/// Lowers a `--debug` build, enabling debug-only instrumentation such as
/// full logical/switch path coverage.
#[allow(dead_code)] // the binary pipeline selects the build policy; the embedder library does not
pub(crate) fn lower_program_for_build(
    program: &Program,
    check_result: &CheckResult,
    target: Target,
    source_path: &Path,
    web: bool,
    debug_build: bool,
) -> Result<Module, LoweringError> {
    program::lower(
        program,
        check_result,
        target,
        Some(source_path),
        web,
        debug_build,
    )
}

thread_local! {
    /// Whether the current thread's lowering belongs to a `--debug` build.
    static DEBUG_BUILD: Cell<bool> = const { Cell::new(false) };
}

/// Returns whether the current lowering should emit debug-only instrumentation.
pub(crate) fn debug_build_enabled() -> bool {
    DEBUG_BUILD.with(Cell::get)
}

/// Runs `f` with the thread's debug-build lowering policy set and restored after.
fn with_debug_build<T>(debug_build: bool, f: impl FnOnce() -> T) -> T {
    /// Restores the previous policy even when lowering returns early.
    struct Restore(bool);

    impl Drop for Restore {
        /// Restores the thread-local debug policy.
        fn drop(&mut self) {
            DEBUG_BUILD.with(|flag| flag.set(self.0));
        }
    }

    let previous = DEBUG_BUILD.with(|flag| flag.replace(debug_build));
    let _restore = Restore(previous);
    f()
}

/// Error produced while building or validating EIR.
#[derive(Debug)]
pub enum LoweringError {
    Validation(ValidationError),
    /// Retains the failing function and value metadata for remote validation failures.
    ValidationInFunction { function: String, error: ValidationError, detail: String },
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
            LoweringError::Validation(err) => write!(f, "EIR validation failed: {:?}", err),
            LoweringError::ValidationInFunction { function, error, detail } => {
                write!(f, "EIR validation failed in {function}: {error:?}{detail}")
            }
            LoweringError::Unsupported(err) => write!(
                f,
                "unsupported EIR lowering at {}:{}: {}",
                err.span.line, err.span.col, err.message
            ),
        }
    }
}

impl From<ValidationError> for LoweringError {
    /// Converts an EIR validation error into a lowering error.
    fn from(value: ValidationError) -> Self {
        LoweringError::Validation(value)
    }
}
