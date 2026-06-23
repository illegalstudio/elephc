//! Purpose:
//! Implements the checker driver top level phase.
//! Owns one ordered step in building checker state and validating the program before optimization/codegen.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl()`
//!
//! Key details:
//! - Phase order controls diagnostics, available declarations, required libraries, and function-local environments.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::Program;
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

impl Checker {
    /// Runs the top-level type-checking pass over the full program.
    ///
    /// Processes each statement in order, maintaining a shared `global_env` that accumulates
    /// declarations across the entire program. Each statement is checked in a fresh `top_level_env`
    /// cloned from the current global state. Returns the final `TypeEnv` and a vector of error
    /// vectors (one per statement) for structured diagnostics.
    pub(super) fn check_top_level_program(
        &mut self,
        program: &Program,
    ) -> (TypeEnv, Vec<Vec<CompileError>>) {
        let mut global_env = self.seed_global_env();
        let mut all_errors = Vec::with_capacity(program.len());
        for stmt in program {
            self.top_level_env = global_env.clone();
            let stmt_errors = self
                .check_stmt(stmt, &mut global_env)
                .err()
                .map(|error| error.flatten())
                .unwrap_or_default();
            all_errors.push(stmt_errors);
        }
        (global_env, all_errors)
    }

    /// Determines whether top-level errors for a statement can be suppressed.
    ///
    /// Only reached when the final fixpoint pass produced no error for this statement, so any
    /// remaining initial-pass error is stale by construction — the post-stability method/function
    /// signatures resolved the type. Suppression is gated on the message whitelist
    /// (`is_suppressible_initial_top_level_error`), which is itself proof that the statement contains
    /// the relevant construct: the index/property/callable diagnostics are emitted only by
    /// array-access, property-access, and callable inference. The whitelist therefore subsumes any
    /// structural check and, unlike an "erroring statement must itself contain a method/property
    /// access" gate, it also covers the result being bound to a local in an earlier statement and
    /// merely indexed here (e.g. `$r = $o->get(); echo $r[0];`).
    pub(super) fn can_suppress_initial_top_level_errors(errors: &[CompileError]) -> bool {
        if Self::can_suppress_stale_undefined_variable_errors(errors) {
            return true;
        }
        if Self::can_suppress_late_callable_metadata_errors(errors) {
            return true;
        }
        !errors.is_empty()
            && errors
                .iter()
                .all(|error| Self::is_suppressible_initial_top_level_error(&error.message))
    }

    /// Returns true for initial-pass callable metadata errors that disappeared in the final pass.
    fn can_suppress_late_callable_metadata_errors(errors: &[CompileError]) -> bool {
        errors
            .iter()
            .any(|error| Self::is_late_callable_metadata_error(&error.message))
            && errors.iter().all(|error| {
                Self::is_late_callable_metadata_error(&error.message)
                    || error.message.starts_with("Undefined variable: $")
            })
    }

    /// Returns true for undefined-variable cascades that disappeared in the final pass.
    fn can_suppress_stale_undefined_variable_errors(errors: &[CompileError]) -> bool {
        !errors.is_empty()
            && errors
                .iter()
                .all(|error| error.message.starts_with("Undefined variable: $"))
    }

    /// Returns true for stale diagnostics caused by method-return callable metadata.
    fn is_late_callable_metadata_error(message: &str) -> bool {
        message.contains("must have a statically known callable signature")
    }

    /// Returns `true` if the given error message is in the suppressible set for initial top-level errors.
    ///
    /// Suppressible messages include array-index, property-access, and callable-related diagnostics
    /// that commonly arise when a class is referenced before its definition.
    fn is_suppressible_initial_top_level_error(message: &str) -> bool {
        matches!(
            message,
            "Array index must be integer"
                | "Cannot index non-array"
                | "Property access requires an object or typed pointer"
        ) || (message.starts_with("Cannot call $") && message.contains("not a callable"))
    }

    /// Builds the initial `TypeEnv` with built-in globals `$argc`, `$argv`, and external globals.
    ///
    /// `$argc` is typed as `Int`; `$argv` is typed as `Array<Str>`. External globals from
    /// `self.extern_globals` are inserted verbatim. The returned environment serves as the
    /// starting point for top-level type checking.
    fn seed_global_env(&self) -> TypeEnv {
        let mut global_env: TypeEnv = HashMap::new();
        global_env.insert("argc".to_string(), PhpType::Int);
        global_env.insert("argv".to_string(), PhpType::Array(Box::new(PhpType::Str)));
        for name in crate::superglobals::SUPERGLOBALS {
            global_env.insert((*name).to_string(), crate::superglobals::superglobal_type());
        }
        for (name, ty) in &self.extern_globals {
            global_env.insert(name.clone(), ty.clone());
        }
        global_env
    }
}
