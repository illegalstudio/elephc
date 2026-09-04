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
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

impl Checker {
    /// Runs the top-level type-checking pass over the full program.
    ///
    /// Processes each statement in order, maintaining a shared `global_env` that accumulates
    /// declarations across the entire program. Each statement is checked in a fresh `top_level_env`
    /// cloned from the current global state. Returns the final `TypeEnv` and a vector of error
    /// vectors (one per statement) for structured diagnostics.
    /// The third member of the tuple marks, per statement, whether typing it reached THROUGH a
    /// null receiver. In the initial pass that answer is provisional — see
    /// `Checker::tolerated_null_receiver` — so the caller uses it to decide whose errors count.
    pub(super) fn check_top_level_program(
        &mut self,
        program: &Program,
    ) -> (TypeEnv, Vec<Vec<CompileError>>, Vec<bool>) {
        let saved_eval_barrier_active = self.eval_barrier_active;
        self.eval_barrier_active = false;
        let saved_null_probe_scope = self.null_probe_scope_is_top_level;
        self.null_probe_scope_is_top_level = true;
        self.pending_null_probe_roots.clear();
        // Top level is a body like any other, and this pass runs TWICE. Without a reset the
        // second pass would start with the first pass's aliases and binding depths already in
        // place, so the same `unset` could be eligible in one pass and not the other.
        let saved_local_binding_scope = self.enter_local_binding_scope(Vec::new(), Vec::new());
        // `enter_local_binding_scope` does NOT touch `active_ref_params`, so the top level installs
        // it here the way `with_local_storage_context` does for every other body: saved, emptied,
        // restored. Top level declares no by-reference parameters and captures nothing, so the set
        // is empty for it — and the pre-scan reads it (a reference-aliased name is never markable),
        // which is what makes an empty set the correct state rather than merely the tidy one. This
        // pass runs TWICE, so an inherited or leftover entry would also make the same name markable
        // in one pass and not the other.
        let saved_ref_params = std::mem::take(&mut self.active_ref_params);
        let mut global_env = self.seed_global_env();
        // The pre-scan has to decide before the first statement is checked: a marked local binds
        // boxed `Mixed` at its FIRST store. Top level has no parameters, so the by-reference and
        // declared-type exclusion sets the scan consults are empty — `enter_local_binding_scope`
        // installed the declared-type one and the line above the by-reference one — and there is no
        // pre-bound storage OF ITS OWN either: `$argc`, `$argv`, the superglobals and the extern C
        // globals all live in program storage, and `name_is_seeded_program_storage` keeps the
        // marking off them outright.
        self.run_mixed_storage_scan(program, &std::collections::HashMap::new());
        let mut all_errors = Vec::with_capacity(program.len());
        let mut through_null = Vec::with_capacity(program.len());
        // `(statement index, name, span)` for every null probe this pass tolerated, so the
        // deferred diagnostic lands on the statement that contains the probe.
        let mut probe_roots: Vec<(usize, String, Span)> = Vec::new();
        for (index, stmt) in program.iter().enumerate() {
            self.top_level_env = global_env.clone();
            self.tolerated_null_receiver = false;
            let stmt_errors = self
                .check_stmt(stmt, &mut global_env)
                .err()
                .map(|error| error.flatten())
                .unwrap_or_default();
            through_null.push(self.tolerated_null_receiver);
            probe_roots.extend(
                self.pending_null_probe_roots
                    .drain(..)
                    .map(|(name, span)| (index, name, span)),
            );
            all_errors.push(stmt_errors);
        }
        self.resolve_null_probe_roots(probe_roots, &mut global_env, &mut all_errors);
        self.top_level_env = global_env.clone();
        self.eval_barrier_active = saved_eval_barrier_active;
        self.null_probe_scope_is_top_level = saved_null_probe_scope;
        self.active_ref_params = saved_ref_params;
        self.exit_local_binding_scope(saved_local_binding_scope);
        // `resolve_null_probe_roots` can append to `all_errors` for statements the loop already
        // visited, so the two vectors are aligned by padding rather than by construction.
        through_null.resize(all_errors.len(), false);
        (global_env, all_errors, through_null)
    }

    /// Decides, with the finished `global_env` in hand, whether each tolerated null-probe root
    /// was legitimate.
    ///
    /// A name still absent from `global_env` was never assigned anywhere at top level, so it is
    /// `null` for the whole scope: binding it to `PhpType::Void` both matches PHP and gives EIR
    /// lowering a slot type it can answer `isset`/`empty`/`??` from without reading storage that
    /// no store ever initializes. A name that *is* bound was assigned somewhere in the same
    /// scope, so its slot carries that assigned type and the probe would read it before the
    /// store — the original `Undefined variable` diagnostic is restored for those.
    fn resolve_null_probe_roots(
        &mut self,
        probe_roots: Vec<(usize, String, Span)>,
        global_env: &mut TypeEnv,
        all_errors: &mut [Vec<CompileError>],
    ) {
        let mut reported: std::collections::HashSet<(String, u32, u32)> =
            std::collections::HashSet::new();
        for (index, name, span) in probe_roots {
            match global_env.get(&name) {
                // Never assigned at top level: seed the `null` binding lowering needs. The same
                // name can be probed several times (and each probe is seen by both the
                // assignment-effect walk and expression inference), so an already-seeded `Void`
                // is just a repeat of this decision and stays accepted.
                None => {
                    global_env.insert(name, PhpType::Void);
                }
                Some(PhpType::Void) => {}
                Some(_) => {
                    // One probe is visited by both the assignment-effect walk and expression
                    // inference, so the same (name, position) can arrive several times.
                    if !reported.insert((name.clone(), span.line, span.col)) {
                        continue;
                    }
                    if let Some(stmt_errors) = all_errors.get_mut(index) {
                        stmt_errors.push(
                            super::super::null_probe::unrepresentable_probe_root_error(&name, span),
                        );
                    }
                }
            }
        }
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
        global_env.insert("http_response_header".to_string(), PhpType::Array(Box::new(PhpType::Str)));
        for name in crate::superglobals::SUPERGLOBALS {
            global_env.insert((*name).to_string(), crate::superglobals::superglobal_type());
        }
        for (name, ty) in &self.extern_globals {
            global_env.insert(name.clone(), ty.clone());
        }
        global_env
    }
}
