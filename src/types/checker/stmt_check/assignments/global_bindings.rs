//! Purpose:
//! Replaces an activation's old binding facts when a global declaration is reached.
//!
//! Called from:
//! - The assignment/declaration statement checker.
//!
//! Key details:
//! - Program source views replace old parameter, capture, static and ordinary local views.
//! - Physical canonical storage does not by itself authorize broader reference contracts.

use crate::{errors::CompileError, span::Span, types::{checker::Checker, PhpType, TypeEnv}};

/// Checks reached program sources independently of the local values they displace.
pub(super) fn check(checker: &mut Checker, vars: &[String], span: Span, env: &mut TypeEnv) -> Result<(), CompileError> {
    for name in vars {
        let retired = checker.binding_retirement_type(name, env);
        let source = checker.top_level_env.get(name).cloned().unwrap_or(PhpType::Mixed);
        checker.invalidate_local_flow_narrowing(name, env);
        checker.clear_local_binding_metadata(name);
        if !checker.null_probe_scope_is_top_level {
            checker.active_ref_params.remove(name);
            checker.active_external_ref_bindings.remove(name);
            checker.non_builtin_ref_aliased_locals.remove(name);
            checker.ref_assign_aliased_locals.remove(name);
            checker.ref_aliased_locals.remove(name);
            checker.static_local_names.remove(name);
            checker.active_statics.remove(name);
        }
        checker.active_globals.insert(name.clone());
        if checker.local_store_contracts.has_runtime_origin(&checker.current_loop_storage_scope, name) {
            checker.inferred_local_contracts.insert(name.clone(), source.clone());
            checker.record_declared_binding_contract(name, &source, span);
        }
        env.insert(name.clone(), source);
        checker.definitely_bound_locals.insert(name.clone());
        checker.record_binding_retirement_exception_flow(&retired, env);
    }
    Ok(())
}
