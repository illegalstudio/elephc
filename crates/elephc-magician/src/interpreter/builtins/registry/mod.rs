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

use elephc_builtin_contract::{
    contracts, eval_support, BackendImplementation, BackendSupport, EvalExecution,
};

use super::super::*;
use super::spec::{EvalArea, EvalBuiltinBinding, EvalBuiltinSpec};

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
    by_name: HashMap<String, usize>,
    /// Runtime-ready joins of shared contracts and Magician bindings.
    specs: Vec<EvalBuiltinSpec>,
    /// Stable ordered list of registered canonical names.
    names: Vec<&'static str>,
}

/// Global eval builtin registry built from inventory submissions.
static DECLARED_BUILTIN_REGISTRY: OnceLock<DeclaredBuiltinRegistry> = OnceLock::new();

/// Builds the declarative registry and rejects duplicate builtin names.
fn build_declared_builtin_registry() -> DeclaredBuiltinRegistry {
    let mut by_name = HashMap::new();
    let mut specs = Vec::new();
    let mut names = Vec::new();

    for binding in inventory::iter::<EvalBuiltinBinding> {
        let spec = EvalBuiltinSpec::from_binding(binding);
        validate_declared_builtin_spec(&spec);
        let key = spec.name.to_ascii_lowercase();
        let index = specs.len();
        if by_name.insert(key, index).is_some() {
            panic!(
                "duplicate eval builtin name registered in inventory: \"{}\"",
                spec.name
            );
        }
        names.push(spec.name);
        specs.push(spec);
    }

    validate_shared_eval_coverage(&by_name);

    names.sort_unstable();
    DeclaredBuiltinRegistry {
        by_name,
        specs,
        names,
    }
}

/// Proves every shared contract has exactly its declared Magician implementation route.
fn validate_shared_eval_coverage(by_name: &HashMap<String, usize>) {
    for contract in contracts() {
        let registered = by_name.contains_key(contract.name);
        match eval_support(contract) {
            BackendSupport::Implemented(BackendImplementation::Registry) => assert!(
                registered,
                "shared builtin contract {} requires an eval registry binding",
                contract.name
            ),
            BackendSupport::Implemented(route) => panic!(
                "shared builtin contract {} declares unsupported eval route {route:?}",
                contract.name
            ),
            BackendSupport::Unsupported(_) => assert!(
                !registered,
                "shared builtin contract {} must not have an eval registry binding",
                contract.name
            ),
        }
    }
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
    for by_ref_name in spec.by_ref_params.iter() {
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
    match spec.execution {
        EvalExecution::SharedRuntime(runtime_builtin) => {
            assert_eq!(spec.runtime_builtin, Some(runtime_builtin));
            assert!(
                spec.direct.is_none() && spec.values.is_none(),
                "eval builtin {} must not retain hooks after shared-runtime migration",
                spec.name
            );
        }
        EvalExecution::Adapter {
            runtime_builtin: Some(runtime_builtin),
            ..
        } => {
            assert_eq!(spec.runtime_builtin, Some(runtime_builtin));
            assert!(
                spec.direct.is_some() && spec.values.is_some(),
                "hybrid eval builtin {} must retain both fallback adapters",
                spec.name
            );
        }
        EvalExecution::Adapter {
            runtime_builtin: None,
            ..
        } => {
            assert!(
                spec.direct.is_some() || spec.values.is_some(),
                "eval builtin {} has no execution hook",
                spec.name
            );
        }
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
/// filters live here: strict-PHP binaries hide extension builtins, while
/// binaries without a registered regex provider hide `preg_*` builtins.
pub(in crate::interpreter) fn eval_declared_builtin_spec(
    name: &str,
) -> Option<&'static EvalBuiltinSpec> {
    let key = name.trim_start_matches('\\').to_ascii_lowercase();
    let registry = declared_builtin_registry();
    let index = *registry.by_name.get(&key)?;
    let spec = &registry.specs[index];
    builtin_is_available(
        spec,
        crate::strict_php_mode::strict_php_mode(),
        crate::regex_provider::regex_provider_available(),
    )
    .then_some(spec)
}

/// Returns whether one builtin is visible under the active runtime capabilities.
fn builtin_is_available(
    spec: &EvalBuiltinSpec,
    strict_php: bool,
    regex_available: bool,
) -> bool {
    !(strict_php && spec.is_extension())
        && (!matches!(spec.area(), EvalArea::Regex) || regex_available)
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
    let registry = declared_builtin_registry();
    let index = *registry.by_name.get(&key)?;
    Some(&registry.specs[index])
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
    eval_declared_builtin_spec(name).map(|spec| spec.param_names.as_ref())
}

/// Returns a default value from a declaratively migrated builtin spec.
pub(in crate::interpreter) fn eval_declared_builtin_default_value(
    name: &str,
    param_index: usize,
) -> Option<EvalBuiltinDefaultValue> {
    eval_declared_builtin_spec(name).and_then(|spec| spec.default_value(param_index))
}

/// Applies php's argument TypeErrors before a shared-runtime dispatch.
///
/// The generated runtime's boxed-cell builtins assume well-typed arguments — the
/// compiled checker enforces their contracts at compile time — so `eval()` must
/// enforce the same contract, with php's exact wording, before crossing the ABI.
fn eval_runtime_builtin_arg_check(
    runtime_builtin: elephc_builtin_contract::RuntimeBuiltinId,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if matches!(
        runtime_builtin,
        elephc_builtin_contract::RuntimeBuiltinId::ArrayKeyExists
    ) {
        super::array::array_arg_check::eval_check_array_args(
            "array_key_exists",
            evaluated_args,
            context,
            values,
        )?;
    }
    Ok(())
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
    if let Some(runtime_builtin) = spec.runtime_builtin {
        if runtime_builtin.supports_arity(args.len()) {
            let mut evaluated_args = Vec::with_capacity(args.len());
            for arg in args {
                evaluated_args.push(eval_expr(arg, context, scope, values)?);
            }
            eval_runtime_builtin_arg_check(runtime_builtin, &evaluated_args, context, values)?;
            if let Some(result) = values.runtime_builtin_call(runtime_builtin, &evaluated_args)? {
                return Ok(Some(result));
            }
        } else if spec.direct.is_none() {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
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
    if let Some(runtime_builtin) = spec.runtime_builtin {
        if runtime_builtin.supports_arity(evaluated_args.len()) {
            eval_runtime_builtin_arg_check(runtime_builtin, evaluated_args, context, values)?;
            if let Some(result) = values.runtime_builtin_call(runtime_builtin, evaluated_args)? {
                return Ok(Some(result));
            }
        } else if spec.values.is_none() {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    let Some(hook) = spec.values else {
        return Ok(None);
    };
    hook.call(spec.name, evaluated_args, context, values)
        .map(Some)
}

#[cfg(test)]
mod tests;
