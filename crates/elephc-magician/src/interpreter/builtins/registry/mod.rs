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
    contracts, eval_support, BackendImplementation, BackendSupport, EvalExecution, RuntimeBuiltinId,
};

use super::super::*;
use super::spec::{EvalArea, EvalBuiltinBinding, EvalBuiltinSpec};

mod arity;
mod binding;
mod callable;
mod callable_validation;
mod dispatch;
mod dynamic_mutation;
mod names;
mod owned_arguments;
mod signature;

pub(in crate::interpreter) use arity::*;
pub(in crate::interpreter) use binding::*;
pub(in crate::interpreter) use callable::*;
pub(in crate::interpreter) use callable_validation::*;
pub(in crate::interpreter) use dispatch::*;
pub(in crate::interpreter) use dynamic_mutation::*;
pub(in crate::interpreter) use names::*;
pub(in crate::interpreter) use owned_arguments::*;
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
    assert!(
        !spec.source_arguments || spec.direct.is_some(),
        "eval builtin {} requires source arguments but has no direct hook",
        spec.name
    );
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
                .any(|param| param.name == *by_ref_name && param.by_ref)
                || spec.variadic == Some(*by_ref_name),
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
        if spec.by_ref_params.contains(&variadic) {
            assert!(
                !spec.params.iter().any(|param| param.name == variadic),
                "eval builtin {} represents its by-reference variadic outside fixed params",
                spec.name
            );
        }
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
        crate::mbregex_provider::available(),
    )
    .then_some(spec)
}

/// Returns whether one builtin is visible under the active runtime capabilities.
fn builtin_is_available(
    spec: &EvalBuiltinSpec,
    strict_php: bool,
    regex_available: bool,
    mbregex_available: bool,
) -> bool {
    !(strict_php && spec.is_extension())
        && (!matches!(spec.area(), EvalArea::Regex) || regex_available)
        && (!spec.runtime_builtin.is_some_and(RuntimeBuiltinId::is_mbregex) || mbregex_available)
        && (!matches!(spec.area(), EvalArea::Pcntl)
            || eval_pcntl_builtin_is_available(spec.name))
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

/// Returns whether direct syntax must preserve this builtin's source expressions.
pub(in crate::interpreter) fn eval_declared_builtin_requires_source_arguments(
    name: &str,
) -> bool {
    eval_declared_builtin_spec(name).is_some_and(|spec| spec.source_arguments)
}

/// Returns whether `arg_count` positional arguments reach a by-reference parameter of a builtin
/// that has a direct (source-argument) hook to bind it.
///
/// The evaluated-argument route reads every argument as a value, so a by-reference destination
/// has nowhere to be written: `str_replace($s, $r, $subject, $count)` inside an eval fragment
/// reached the three-argument value hook and failed with a bare `RuntimeFatal`. The direct hook
/// is the one that binds the destination.
pub(in crate::interpreter) fn eval_declared_builtin_call_binds_reference(
    name: &str,
    arg_count: usize,
) -> bool {
    eval_declared_builtin_spec(name).is_some_and(|spec| {
        spec.direct.is_some()
            && spec
                .params
                .iter()
                .take(arg_count)
                .any(|param| spec.by_ref_param_names().contains(&param.name))
    })
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
    // A call that reaches a by-reference parameter (`headers_sent($file, $line)`) cannot go
    // through the by-value runtime builtin: it would read the destination as a value (warning on
    // an undefined one) and have nowhere to write the result. The direct hook binds it.
    let reaches_by_ref_param = spec
        .params
        .iter()
        .take(args.len())
        .any(|param| spec.by_ref_param_names().contains(&param.name));
    if let Some(runtime_builtin) = spec.runtime_builtin.filter(|_| !reaches_by_ref_param) {
        if runtime_builtin.is_mbstring() {
            return call_shared_mbstring_direct(runtime_builtin, args, context, scope, values);
        }
        if !runtime_builtin.supports_arity(args.len()) && spec.values.is_none() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let operands = args.iter().collect::<Vec<_>>();
        return with_eval_operands(&operands, context, scope, values, |args, context, scope, values| {
            let borrowed = args.iter().map(|value| value.borrowed()).collect::<Vec<_>>();
            // Runtime capability misses and deliberate arity adapters reuse the same
            // evaluated cells. Falling back to expression hooks would repeat side effects.
            let result = eval_declared_builtin_values_call_from_scope(name, &borrowed, Some(scope), context, values)?
                .ok_or(EvalStatus::UnsupportedConstruct)?;
            if result.is_borrowed() { values.retain(result) } else { Ok(result) }
        }).map(Some);
    }
    let Some(hook) = spec.direct else {
        return Ok(None);
    };
    // A `string` parameter accepts any Stringable object, converted through `__toString()`
    // (`file_get_contents($splFileInfo)` is how Symfony's `lint:twig` reads each template). A
    // direct hook evaluates its own arguments and reads the raw bytes, which are empty for an
    // object, so a builtin that has an evaluated-argument hook is routed through it instead,
    // where the conversion happens once for every builtin.
    // The arity guard leaves a wrong argument count to the direct hook, which is what raises
    // PHP's catchable ArgumentCountError.
    let required = spec.required_param_count.unwrap_or_else(|| {
        spec.params
            .iter()
            .take_while(|param| param.default.is_none())
            .count()
    });
    let arity_fits = args.len() >= required
        && (spec.variadic.is_some() || args.len() <= spec.params.len());
    if spec.has_string_param()
        && arity_fits
        && spec.by_ref_param_names().is_empty()
        && !eval_builtin_is_backtrace_visible(spec.name)
    {
        if let Some(values_hook) = spec.values {
            let mut evaluated_args = Vec::with_capacity(args.len());
            for arg in args {
                evaluated_args.push(eval_expr(arg, context, scope, values)?);
            }
            let originals = evaluated_args.clone();
            let evaluated_args =
                eval_coerce_string_param_objects(spec, evaluated_args, context, values)?;
            let result = values_hook.call(spec.name, &evaluated_args, Some(&*scope), context, values)?;
            release_builtin_arg_temporaries(args, &originals, &evaluated_args, Some(result), values)?;
            return Ok(Some(result));
        }
    }
    if eval_builtin_is_backtrace_visible(spec.name) {
        // The class probes must be visible to a userland autoloader they start: Symfony's
        // `ClassExistenceResource::throwOnRequiredClass` returns silently only when it finds one
        // of these frames above itself. PHP shows the probe's arguments too, so a probe whose
        // parameters are all by value is re-routed through its evaluated-argument hook to make
        // real handles available; one with a by-reference parameter keeps the direct hook and
        // pushes a frame with no `args` key, which is a shape PHP itself produces (under
        // `DEBUG_BACKTRACE_IGNORE_ARGS`) rather than an invented empty list.
        if spec.by_ref_param_names().is_empty() {
            if let Some(values_hook) = spec.values {
                let mut evaluated_args = Vec::with_capacity(args.len());
                for arg in args {
                    evaluated_args.push(eval_expr(arg, context, scope, values)?);
                }
                let frame =
                    EvalCallFrame::function(spec.name, Some(evaluated_args.clone()), context);
                context.push_call_frame(frame);
                let result = values_hook.call(spec.name, &evaluated_args, Some(&*scope), context, values);
                context.pop_call_frame();
                let result = result?;
                release_builtin_arg_temporaries(
                    args,
                    &evaluated_args,
                    &evaluated_args,
                    Some(result),
                    values,
                )?;
                return Ok(Some(result));
            }
        }
        let frame = EvalCallFrame::function(spec.name, None, context);
        context.push_call_frame(frame);
        let result = hook.call(spec.name, args, context, scope, values);
        context.pop_call_frame();
        return result.map(Some);
    }
    hook.call(spec.name, args, context, scope, values).map(Some)
}

/// Releases the argument cells a builtin call evaluated for itself and no longer needs.
///
/// `eval_expr` hands back either a cell storage still owns (`$var`, a constant, a static
/// property: a BORROWED handle, or `eval_expr_result_aliases_storage`) or a fresh temporary nobody
/// else holds. The builtins only READ their arguments -- they must, since a variable's cell is
/// borrowed -- so a temporary was simply dropped: every `mt_rand(1, 1)` or `strlen("ab")` in eval
/// code leaked its literal cells. `coerced` is the list after `__toString()` conversion; a
/// replacement it holds is a fresh string of its own. The result is never released: a builtin may
/// answer with an argument.
fn release_builtin_arg_temporaries(
    args: &[EvalExpr],
    originals: &[RuntimeCellHandle],
    coerced: &[RuntimeCellHandle],
    result: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for (index, original) in originals.iter().enumerate() {
        let replacement = coerced.get(index).copied().filter(|cell| cell != original);
        if let Some(replacement) = replacement {
            if Some(replacement) != result {
                values.release(replacement)?;
            }
        }
        let temporary = !original.is_borrowed()
            && args.get(index).is_some_and(|arg| {
                !crate::interpreter::expressions::eval_expr_result_aliases_storage(arg)
            });
        if temporary && Some(*original) != result {
            values.release(*original)?;
        }
    }
    Ok(())
}

/// Dispatches evaluated builtin cells with optional lexical scope for callback resolution.
pub(in crate::interpreter) fn eval_declared_builtin_values_call_from_scope(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(spec) = eval_declared_builtin_spec(name) else {
        return Ok(None);
    };
    if let Some(runtime_builtin) = spec.runtime_builtin {
        if runtime_builtin.supports_arity(evaluated_args.len()) || RuntimeBuiltinId::MBSTRING.contains(&runtime_builtin) {
            if let Some(result) = call_shared_runtime_builtin(runtime_builtin, evaluated_args, context, values)? {
                return Ok(Some(result));
            }
        } else if spec.values.is_none() {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    let Some(hook) = spec.values else {
        return Ok(None);
    };
    let coerced;
    let evaluated_args = if spec.has_string_param() {
        coerced = eval_coerce_string_param_objects(spec, evaluated_args.to_vec(), context, values)?;
        coerced.as_slice()
    } else {
        evaluated_args
    };
    if eval_builtin_is_backtrace_visible(spec.name) {
        let frame =
            EvalCallFrame::function(spec.name, Some(evaluated_args.to_vec()), context);
        context.push_call_frame(frame);
        let result = hook.call(spec.name, evaluated_args, lexical_scope, context, values);
        context.pop_call_frame();
        return result.map(Some);
    }
    hook.call(spec.name, evaluated_args, lexical_scope, context, values)
        .map(Some)
}

/// Converts each object passed to a `string` parameter through `__toString()`.
///
/// Only objects are touched: a scalar keeps its own coercion inside the implementation, which
/// is where PHP's per-type rules already live.
fn eval_coerce_string_param_objects(
    spec: &EvalBuiltinSpec,
    mut evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    for (index, arg) in evaluated_args.iter_mut().enumerate() {
        let Some(param) = spec.params.get(index) else {
            break;
        };
        if !param.string_param || values.type_tag(*arg)? != EVAL_TAG_OBJECT {
            continue;
        }
        // A `string` parameter given an object without `__toString()` is php's TypeError
        // (`strlen(): Argument #1 ($string) must be of type string, stdClass given`), not the
        // string-context `Object of class stdClass could not be converted to string` Error.
        if !crate::interpreter::dynamic_functions::eval_object_has_to_string(*arg, context, values)? {
            let given = crate::interpreter::eval_given_type_spelling(*arg, values)?;
            let parameter = spec.param_names.get(index).copied().unwrap_or("value");
            return eval_throw_type_error(
                &format!(
                    "{}(): Argument #{} (${parameter}) must be of type string, {given} given",
                    spec.name,
                    index + 1,
                ),
                context,
                values,
            );
        }
        *arg = crate::interpreter::dynamic_functions::eval_string_context_value(
            *arg, context, values,
        )?;
    }
    Ok(evaluated_args)
}

#[cfg(test)]
mod tests;

/// Transfers pending native builtin exceptions into the context consumed by eval catch clauses.
fn call_shared_runtime_builtin(
    id: elephc_builtin_contract::RuntimeBuiltinId,
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    // `pow()` is PHP's one math builtin whose operand overload reaches internal value objects.
    // The generated runtime adapter works on scalars only, so route an object operand through
    // the same `RuntimeValueOps::pow` default that `**` uses.
    if id == elephc_builtin_contract::RuntimeBuiltinId::Unserialize {
        if let [data] = args {
            if let Some(function) = context.native_function("__elephc_gmp_unserialize") {
                let arguments = vec![EvaluatedCallArg {
                    name: None,
                    value: data.borrowed(),
                    ref_target: None,
                }];
                let bound =
                    bind_evaluated_native_function_args(&function, arguments, context, values)?;
                return eval_native_function_with_values(function, bound, context, values).map(Some);
            }
        }
    }
    if id == elephc_builtin_contract::RuntimeBuiltinId::Pow {
        if let [base, exponent] = args {
            if values.type_tag(*base)? == EVAL_TAG_OBJECT
                || values.type_tag(*exponent)? == EVAL_TAG_OBJECT
            {
                return values.pow(*base, *exponent).map(Some);
            }
        }
    }
    if id.is_mbstring() {
        let contract = elephc_builtin_contract::lookup_id(id.builtin_id()).expect("shared mbstring contract");
        for (parameter, value) in contract.params.iter().zip(args) {
            if parameter.by_ref && !values.is_reference(*value)? {
                return Err(EvalStatus::UnsupportedConstruct);
            }
        }
    }
    match values.runtime_builtin_call(id, args) {
        Err(EvalStatus::UncaughtThrowable) => {
            if let Some(thrown) = values.take_pending_runtime_throwable()? {
                context.set_pending_throw(thrown);
            }
            Err(EvalStatus::UncaughtThrowable)
        }
        result => result,
    }
}

/// Sends positional mbstring expressions through the same owner-aware builtin argument boundary.
fn call_shared_mbstring_direct(
    id: RuntimeBuiltinId, args: &[EvalExpr], context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let arguments = args.iter().cloned().map(EvalCallArg::positional).collect::<Vec<_>>();
    let contract = elephc_builtin_contract::lookup_id(id.builtin_id()).expect("shared mbstring contract");
    eval_builtin_call(contract.name, &arguments, context, scope, values).map(Some)
}
