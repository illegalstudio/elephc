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

mod builtin_datetime;
mod context;
mod effects_lookup;
mod expr;
mod fibers;
mod function;
mod ownership;
mod program;
mod reflection;
mod stmt;

#[cfg(test)]
mod tests;

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
    program::lower(program, check_result, target, None, web)
}

/// Lowers `program` into an EIR module and records the main PHP source path.
pub fn lower_program_with_source_path(
    program: &Program,
    check_result: &CheckResult,
    target: Target,
    source_path: &Path,
) -> Result<Module, LoweringError> {
    program::lower(program, check_result, target, Some(source_path), false)
}

/// Lowers `program` into EIR while retaining both source-path and web-mode metadata.
pub fn lower_program_with_source_path_and_web(
    program: &Program,
    check_result: &CheckResult,
    target: Target,
    source_path: &Path,
    web: bool,
) -> Result<Module, LoweringError> {
    program::lower(program, check_result, target, Some(source_path), web)
}

/// Error produced while building or validating EIR.
#[derive(Debug)]
pub enum LoweringError {
    Validation(ValidationError),
}

impl fmt::Display for LoweringError {
    /// Formats the lowering error for CLI diagnostics.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoweringError::Validation(err) => write!(f, "EIR validation failed: {:?}", err),
        }
    }
}

impl From<ValidationError> for LoweringError {
    /// Converts an EIR validation error into a lowering error.
    fn from(value: ValidationError) -> Self {
        LoweringError::Validation(value)
    }
}
