//! Purpose:
//! Groups builtin registry lookup, argument binding, callable dispatch, and
//! evaluated-argument builtin dispatch.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by core eval call paths.
//!
//! Key details:
//! - The large by-value dispatch match is isolated from argument planning and
//!   callable normalization.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::super::*;
use super::spec::EvalBuiltinSpec;

mod binding;
mod callable;
mod callable_validation;
mod dispatch;
mod dynamic_mutation;
mod names;
mod signature;

pub(in crate::interpreter) use binding::*;
pub(in crate::interpreter) use callable::*;
pub(in crate::interpreter) use callable_validation::*;
pub(in crate::interpreter) use dispatch::*;
pub(in crate::interpreter) use dynamic_mutation::*;
pub(in crate::interpreter) use names::*;
pub(in crate::interpreter) use signature::*;

/// Lazy registry of builtins migrated to declarative eval specs.
struct DeclaredBuiltinRegistry {
    /// Case-insensitive lookup keyed by canonical lowercase PHP builtin name.
    by_name: HashMap<String, &'static EvalBuiltinSpec>,
    /// Stable ordered list of registered canonical names.
    names: Vec<&'static str>,
}

/// Global eval builtin registry built from inventory submissions.
static DECLARED_BUILTIN_REGISTRY: OnceLock<DeclaredBuiltinRegistry> = OnceLock::new();

/// Builds the declarative registry and rejects duplicate builtin names.
fn build_declared_builtin_registry() -> DeclaredBuiltinRegistry {
    let mut by_name = HashMap::new();
    let mut names = Vec::new();

    for spec in inventory::iter::<EvalBuiltinSpec> {
        validate_declared_builtin_spec(spec);
        let key = spec.name.to_ascii_lowercase();
        if by_name.insert(key, spec).is_some() {
            panic!(
                "duplicate eval builtin name registered in inventory: \"{}\"",
                spec.name
            );
        }
        names.push(spec.name);
    }

    names.sort_unstable();
    DeclaredBuiltinRegistry { by_name, names }
}

/// Validates static spec invariants before the registry is exposed.
fn validate_declared_builtin_spec(spec: &EvalBuiltinSpec) {
    let expected_param_names = spec.params.len() + usize::from(spec.variadic.is_some());
    assert_eq!(
        expected_param_names,
        spec.param_names.len(),
        "eval builtin {} has mismatched params and param_names",
        spec.name
    );
    for (param, name) in spec.params.iter().zip(spec.param_names.iter()) {
        assert_eq!(
            param.name, *name,
            "eval builtin {} has a param_names entry out of sync",
            spec.name
        );
        if param.by_ref {
            assert!(
                spec.by_ref_params.contains(&param.name),
                "eval builtin {} marks {} by-ref without listing it",
                spec.name,
                param.name
            );
        }
    }
    for by_ref_name in spec.by_ref_params {
        assert!(
            spec.params
                .iter()
                .any(|param| param.name == *by_ref_name && param.by_ref),
            "eval builtin {} lists {} as by-ref without marking the parameter",
            spec.name,
            by_ref_name
        );
    }
    if let Some(variadic) = spec.variadic {
        assert_eq!(
            spec.param_names.last().copied(),
            Some(variadic),
            "eval builtin {} has a variadic name out of sync",
            spec.name
        );
    }
    if let Some(required_param_count) = spec.required_param_count {
        assert!(
            required_param_count <= spec.params.len(),
            "eval builtin {} has a required parameter count larger than its parameter list",
            spec.name
        );
    }
    let _ = spec.area();
}

/// Returns the declarative registry, initializing it on first access.
fn declared_builtin_registry() -> &'static DeclaredBuiltinRegistry {
    DECLARED_BUILTIN_REGISTRY.get_or_init(build_declared_builtin_registry)
}

/// Looks up a declaratively migrated eval builtin with PHP case-insensitive matching.
///
/// This is the single resolution choke point for eval builtin dispatch and
/// introspection (`function_exists`/`is_callable` probes), so the strict-PHP
/// filter lives here: in binaries compiled with `--strict-php`, extension
/// builtins resolve to `None` and eval'd code behaves as if the names did not
/// exist, exactly like the PHP interpreter.
pub(in crate::interpreter) fn eval_declared_builtin_spec(
    name: &str,
) -> Option<&'static EvalBuiltinSpec> {
    let key = name.trim_start_matches('\\').to_ascii_lowercase();
    let spec = declared_builtin_registry().by_name.get(&key).copied()?;
    if crate::strict_php_mode::strict_php_mode() && spec.is_extension() {
        return None;
    }
    Some(spec)
}

/// Looks up an eval builtin spec WITHOUT the strict-PHP filter.
///
/// Metadata derivations (the extension-name list itself, docs exporters) need
/// the raw registry regardless of the thread's strict state; every dispatch or
/// introspection path must use `eval_declared_builtin_spec` instead.
pub(in crate::interpreter) fn eval_raw_declared_builtin_spec(
    name: &str,
) -> Option<&'static EvalBuiltinSpec> {
    let key = name.trim_start_matches('\\').to_ascii_lowercase();
    declared_builtin_registry().by_name.get(&key).copied()
}

/// Returns whether a PHP-visible builtin has migrated into the declarative registry.
pub(in crate::interpreter) fn eval_declared_builtin_exists(name: &str) -> bool {
    eval_declared_builtin_spec(name).is_some()
}

/// Returns stable canonical names for builtins in the declarative registry.
pub(in crate::interpreter) fn eval_declared_builtin_function_names() -> &'static [&'static str] {
    declared_builtin_registry().names.as_slice()
}

/// Returns PHP parameter names for a declaratively migrated builtin.
pub(in crate::interpreter) fn eval_declared_builtin_param_names(
    name: &str,
) -> Option<&'static [&'static str]> {
    eval_declared_builtin_spec(name).map(|spec| spec.param_names)
}

/// Returns a default value from a declaratively migrated builtin spec.
pub(in crate::interpreter) fn eval_declared_builtin_default_value(
    name: &str,
    param_index: usize,
) -> Option<EvalBuiltinDefaultValue> {
    eval_declared_builtin_spec(name).and_then(|spec| spec.default_value(param_index))
}

/// Dispatches a declaratively migrated builtin from unevaluated positional expressions.
pub(in crate::interpreter) fn eval_declared_builtin_direct_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(spec) = eval_declared_builtin_spec(name) else {
        return Ok(None);
    };
    let Some(hook) = spec.direct else {
        return Ok(None);
    };
    hook.call(spec.name, args, context, scope, values).map(Some)
}

/// Dispatches a declaratively migrated builtin from already evaluated argument cells.
pub(in crate::interpreter) fn eval_declared_builtin_values_call(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(spec) = eval_declared_builtin_spec(name) else {
        return Ok(None);
    };
    let Some(hook) = spec.values else {
        return Ok(None);
    };
    hook.call(spec.name, evaluated_args, context, values)
        .map(Some)
}

#[cfg(test)]
mod tests;
