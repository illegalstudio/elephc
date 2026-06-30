//! Purpose:
//! Collects all `BuiltinSpec` entries submitted via `builtin!` into a lazy registry,
//! and exposes lookup helpers used by the catalog, type checker, and codegen dispatcher.
//!
//! Called from:
//! - `crate::types::checker::builtins::catalog` for name-based lookup.
//! - `crate::codegen_ir::lower_inst::builtins` for lowering-hook dispatch.
//!
//! Key details:
//! - Registry is initialized once at first access via a `OnceLock`; subsequent calls
//!   are read-only and lock-free.
//! - Lookup is case-insensitive to match PHP's builtin name semantics.
//! - Duplicate builtin names panic at registry initialization time (link-time guard).

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::builtins::convert::{default_spec_to_expr, type_spec_to_php};
use crate::builtins::spec::BuiltinSpec;
use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::{callable_wrapper_sig, FunctionSig, PhpType};

/// The rich runtime form of a PHP builtin function descriptor.
///
/// Built from a `BuiltinSpec` by the registry at first access. The spec's static
/// `TypeSpec`/`DefaultSpec` fields are converted into `PhpType`/`Expr` via `convert.rs`.
/// The variadic parameter (if any) is appended to `params`/`defaults`/`ref_params`.
pub struct BuiltinDef {
    /// The canonical PHP function name (case-preserved, no leading backslash).
    pub name: &'static str,
    /// The PHP-level parameter list: `(name, type)` pairs in source order.
    /// Includes the variadic parameter (if any) appended as the last entry.
    pub params: Vec<(String, PhpType)>,
    /// Default values in the same order as `params`.
    /// `None` = required; `Some(expr)` = optional with that default.
    /// The variadic parameter always carries `Some(ArrayLiteral([]))`.
    pub defaults: Vec<Option<Expr>>,
    /// Per-parameter by-reference flag, in the same order as `params`.
    /// The variadic parameter is never by-reference (`false`).
    pub ref_params: Vec<bool>,
    /// Name of the variadic parameter, if any.
    pub variadic: Option<String>,
    /// The PHP-level return type, derived from the spec's `TypeSpec` via `type_spec_to_php`.
    pub return_type: PhpType,
    /// Whether this function returns by reference.
    pub by_ref_return: bool,
    /// Reference back to the original static `BuiltinSpec` for hooks and metadata.
    pub spec: &'static BuiltinSpec,
}

/// Global lazy registry: ASCII-lowercase-keyed map from builtin name to `BuiltinDef`.
static REGISTRY: OnceLock<HashMap<String, BuiltinDef>> = OnceLock::new();

/// Builds the registry by iterating all `BuiltinSpec`s collected by `inventory`.
///
/// Panics immediately if two specs register the same name (case-insensitive comparison),
/// so duplicate registrations are caught at program startup.
fn build_registry() -> HashMap<String, BuiltinDef> {
    let mut map: HashMap<String, BuiltinDef> = HashMap::new();
    for spec in inventory::iter::<BuiltinSpec> {
        let key = spec.name.to_ascii_lowercase();
        if map.contains_key(&key) {
            panic!(
                "duplicate builtin name registered in inventory: \"{}\"",
                spec.name
            );
        }

        // Convert the fixed parameter list.
        let param_count = spec.params.len();
        let variadic_count = if spec.variadic.is_some() { 1 } else { 0 };
        let total = param_count + variadic_count;

        let mut params: Vec<(String, PhpType)> = Vec::with_capacity(total);
        let mut defaults: Vec<Option<Expr>> = Vec::with_capacity(total);
        let mut ref_params: Vec<bool> = Vec::with_capacity(total);

        for p in spec.params {
            params.push((p.name.to_string(), type_spec_to_php(&p.ty)));
            defaults.push(p.default.as_ref().map(default_spec_to_expr));
            ref_params.push(p.by_ref);
        }

        // Append the variadic parameter with an empty-array default, matching the
        // convention used by the legacy `variadic()` helper in `src/types/signatures.rs`.
        if let Some(var_name) = spec.variadic {
            params.push((var_name.to_string(), PhpType::Mixed));
            defaults.push(Some(Expr::new(
                ExprKind::ArrayLiteral(Vec::new()),
                Span::dummy(),
            )));
            ref_params.push(false);
        }

        let def = BuiltinDef {
            name: spec.name,
            params,
            defaults,
            ref_params,
            variadic: spec.variadic.map(str::to_string),
            return_type: type_spec_to_php(&spec.returns),
            by_ref_return: spec.by_ref_return,
            spec,
        };
        map.insert(key, def);
    }
    map
}

/// Returns the global registry, initializing it on first call.
///
/// The registry is built exactly once (via `OnceLock`); all subsequent accesses
/// are read-only and lock-free.
fn registry() -> &'static HashMap<String, BuiltinDef> {
    REGISTRY.get_or_init(build_registry)
}

/// Looks up a PHP builtin by name, using case-insensitive matching.
///
/// Returns `None` if the name is not registered in the inventory.
pub fn lookup(name: &str) -> Option<&'static BuiltinDef> {
    let lower = name.to_ascii_lowercase();
    registry().get(&lower)
}

/// Returns `true` if the given name is a known PHP builtin.
pub fn is_supported(name: &str) -> bool {
    lookup(name).is_some()
}

/// Returns an iterator over all registered canonical builtin names in sorted order.
///
/// Names are returned in stable lexicographic order (sorted by `&'static str`)
/// with case-preserved spelling (i.e., as originally supplied to `builtin!`).
/// Sorting ensures deterministic assembly layout across compiler builds.
/// Used primarily from test and documentation-generation contexts.
#[allow(dead_code)]
pub fn names() -> impl Iterator<Item = &'static str> {
    let mut sorted: Vec<&str> = registry().values().map(|def| def.name).collect();
    sorted.sort_unstable();
    sorted.into_iter()
}

/// Derives a `FunctionSig` for the named builtin from the registry.
///
/// The returned sig matches the field layout the legacy `builtin_call_sig()` arms
/// produce via `make_sig`, with the following field mapping:
///
/// | `FunctionSig` field   | Source                                         |
/// |------------------------|------------------------------------------------|
/// | `params`               | `BuiltinDef.params` (typed via `TypeSpec`)    |
/// | `defaults`             | `BuiltinDef.defaults` (via `DefaultSpec`)     |
/// | `return_type`          | `BuiltinDef.return_type` (via `TypeSpec`)     |
/// | `declared_return`      | `false` (matching `make_sig` convention)       |
/// | `by_ref_return`        | `BuiltinDef.by_ref_return` (from spec)        |
/// | `ref_params`           | `BuiltinDef.ref_params` (from spec)           |
/// | `declared_params`      | `vec![false; N]` (matching `make_sig`)        |
/// | `variadic`             | `BuiltinDef.variadic` (from spec)             |
/// | `deprecation`          | `spec.deprecation` mapped to `Option<String>` |
///
/// Returns `None` if the builtin is not registered.
pub fn function_sig(name: &str) -> Option<FunctionSig> {
    let def = lookup(name)?;
    Some(FunctionSig {
        params: def.params.clone(),
        defaults: def.defaults.clone(),
        return_type: def.return_type.clone(),
        declared_return: false,
        by_ref_return: def.by_ref_return,
        ref_params: def.ref_params.clone(),
        declared_params: vec![false; def.params.len()],
        variadic: def.variadic.clone(),
        deprecation: def.spec.deprecation.map(str::to_string),
    })
}

/// Derives a first-class-callable `FunctionSig` for the named builtin.
///
/// Applies `callable_wrapper_sig` to the base `function_sig`, upgrading the
/// variadic parameter (if any) to `Array<Mixed>` as required for first-class use.
/// This reuses the same upgrade logic applied by the legacy `callable_wrapper_sig`
/// helper in `src/types/signatures.rs` rather than reinventing it.
///
/// Sets `declared_return: true` on the resulting signature, mirroring the
/// `typed_first_class_builtin_sig` convention used by the legacy table. First-class
/// callable sigs have a known, declared return type (they are typed wrappers, not
/// type-erased callables), so `declared_return` must be `true`.
///
/// Returns `None` if the builtin is not registered.
pub fn first_class_callable_sig(name: &str) -> Option<FunctionSig> {
    let sig = function_sig(name)?;
    let mut fcc_sig = callable_wrapper_sig(&sig);
    fcc_sig.declared_return = true;
    Some(fcc_sig)
}

/// Returns the minimum and maximum arity for the named builtin.
///
/// - `min`: count of parameters with no default (i.e., required).
/// - `max`: `None` for variadic functions, `Some(n)` for fixed-arity functions
///   where `n` is the total parameter count including optional ones.
///
/// Returns `None` if the builtin is not registered.
pub fn arity_bounds(name: &str) -> Option<(usize, Option<usize>)> {
    let def = lookup(name)?;
    let min = def.defaults.iter().filter(|d| d.is_none()).count();
    let max = if def.variadic.is_some() {
        None
    } else {
        Some(def.params.len())
    };
    Some((min, max))
}

/// Validates the argument count for a named builtin and returns a standard arity error on mismatch.
///
/// Uses `arity_bounds(name)` to determine the expected arity and compares it against
/// `arg_count`. Returns `Ok(())` when the count is in range. Returns a `CompileError`
/// with `span` and a message matching the dominant legacy `"<name>() takes …"` phrasing:
///
/// - `min == max == 0`: `"<name>() takes no arguments"`
/// - `min == max == 1`: `"<name>() takes exactly 1 argument"` (singular)
/// - `min == max > 1`: `"<name>() takes exactly N arguments"` (plural)
/// - `max == None` (variadic), `min == 1`: `"<name>() takes at least 1 argument"` (singular)
/// - `max == None` (variadic), `min > 1`: `"<name>() takes at least N arguments"` (plural)
/// - `max == Some(M)`, `min == 0`, `M == 1`: `"<name>() takes at most 1 argument"` (singular)
/// - `max == Some(M)`, `min == 0`, `M > 1`: `"<name>() takes at most M arguments"` (plural)
/// - `max == Some(M)`, `min < M`, `M == min + 1`: `"<name>() takes N or M arguments"` (e.g., `"substr() takes 2 or 3 arguments"`)
/// - `max == Some(M)`, `min < M`, `M > min + 1`: `"<name>() takes N to M arguments"` (e.g., `"str_pad() takes 2 to 4 arguments"`)
///
/// Returns `Ok(())` without error if `name` is not registered (unknown builtins are handled
/// upstream by the catalog / type checker, which provides its own unknown-name diagnostic).
///
/// When the registered spec carries a `max_args` override, that value caps the maximum
/// accepted argument count for this check only. `function_sig`, `arity_bounds`, and the
/// parity gate keep the full param-derived bounds, so the override never affects argument
/// normalization or the registry/legacy signature parity comparison.
pub fn check_arity(name: &str, arg_count: usize, span: Span) -> Result<(), CompileError> {
    // Param-derived bounds, identical to what the parity gate compares against.
    // `min` is the count of required params; `param_max` is the declared maximum.
    let (min, param_max) = match arity_bounds(name) {
        Some(bounds) => bounds,
        None => return Ok(()),
    };
    // Apply the `max_args` override (if any) to the maximum only. The minimum stays
    // param-derived; the override exists to tighten the accepted maximum for builtins
    // whose legacy CHECK arm was stricter than their declared (golden) signature.
    let max = match lookup(name).and_then(|def| def.spec.max_args) {
        Some(capped) => Some(capped),
        None => param_max,
    };

    let in_range = match max {
        None => arg_count >= min,
        Some(m) => arg_count >= min && arg_count <= m,
    };

    if in_range {
        return Ok(());
    }

    let msg = match (min, max) {
        (0, Some(0)) => format!("{}() takes no arguments", name),
        (n, Some(m)) if n == m && n == 1 => {
            format!("{}() takes exactly 1 argument", name)
        }
        (n, Some(m)) if n == m => {
            format!("{}() takes exactly {} arguments", name, n)
        }
        (n, None) if n == 1 => format!("{}() takes at least 1 argument", name),
        (n, None) => format!("{}() takes at least {} arguments", name, n),
        (0, Some(1)) => format!("{}() takes at most 1 argument", name),
        (0, Some(m)) => format!("{}() takes at most {} arguments", name, m),
        // Consecutive two-value range: use "N or M" to match PHP's natural phrasing
        // (e.g. "substr() takes 2 or 3 arguments", "substr_replace() takes 3 or 4 arguments").
        (n, Some(m)) if m == n + 1 => format!("{}() takes {} or {} arguments", name, n, m),
        (n, Some(m)) => format!("{}() takes {} to {} arguments", name, n, m),
    };

    Err(CompileError::new(span, &msg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins::spec::DefaultSpec;

    /// No-op lowering hook used by test probe builtins; does nothing and succeeds.
    fn noop_lower(
        _c: &mut crate::codegen_ir::context::FunctionContext,
        _i: &crate::ir::Instruction,
    ) -> Result<(), crate::codegen_ir::CodegenIrError> {
        Ok(())
    }

    // Register a registry-specific probe so tests do not depend solely on the
    // spec-module probe (which lives in a different cfg(test) module).
    builtin! {
        name: "__registry_probe_opt",
        area: Internal,
        params: [a: Int, b: Str = DefaultSpec::Null],
        returns: Bool,
        lower: noop_lower,
        summary: "registry arity probe",
        internal: true,
    }

    builtin! {
        name: "__registry_probe_variadic",
        area: Internal,
        params: [fmt: Str],
        variadic: "__registry_values",
        returns: Str,
        lower: noop_lower,
        summary: "registry variadic probe",
        internal: true,
    }

    // Probe whose `max_args` (2) is smaller than its declared param count (3, since
    // `c` is optional). Used to verify the override caps `check_arity` without
    // affecting `function_sig`'s full param count.
    builtin! {
        name: "__registry_probe_capped",
        area: Internal,
        params: [a: Int, b: Int, c: Int = DefaultSpec::Int(0)],
        max_args: 2,
        returns: Int,
        lower: noop_lower,
        summary: "registry capped-arity probe",
        internal: true,
    }

    /// Verifies the registry derives FunctionSig arity/return for a registered builtin.
    #[test]
    fn registry_derives_signature() {
        // assumes a `substr`-shaped probe is registered in this build
        let sig = function_sig("__macro_probe").expect("probe registered");
        assert_eq!(sig.params.len(), 1);
        assert_eq!(sig.return_type, crate::types::PhpType::Int);
    }

    /// Verifies `lookup` returns a `BuiltinDef` for a registered builtin.
    #[test]
    fn lookup_finds_registered_builtin() {
        let def = lookup("__macro_probe").expect("probe must be in registry");
        assert_eq!(def.name, "__macro_probe");
    }

    /// Verifies case-insensitive lookup works (PHP builtin name semantics).
    #[test]
    fn lookup_is_case_insensitive() {
        assert!(lookup("__MACRO_PROBE").is_some());
        assert!(lookup("__Macro_Probe").is_some());
    }

    /// Verifies `is_supported` returns true for registered builtins.
    #[test]
    fn is_supported_returns_true_for_known_builtin() {
        assert!(is_supported("__macro_probe"));
    }

    /// Verifies `is_supported` returns false for unknown names.
    #[test]
    fn is_supported_returns_false_for_unknown() {
        assert!(!is_supported("__not_a_real_builtin_xyz"));
    }

    /// Verifies `names()` includes the probe builtin.
    #[test]
    fn names_includes_registered_builtin() {
        let all: Vec<&str> = names().collect();
        assert!(
            all.contains(&"__macro_probe"),
            "names() must yield all registered builtins"
        );
    }

    /// Verifies `names()` returns builtin names in sorted order for determinism.
    #[test]
    fn names_returns_sorted_order() {
        let names_vec: Vec<&str> = names().collect();
        let mut sorted_vec = names_vec.clone();
        sorted_vec.sort();
        assert_eq!(
            names_vec, sorted_vec,
            "names() must return sorted order for deterministic assembly layout"
        );
    }

    /// Verifies the derived arity error mirrors the legacy "<name>() takes …" messages.
    #[test]
    fn arity_messages_match_legacy() {
        // probe: exactly 1 arg
        let err = check_arity("__macro_probe", 2, crate::span::Span::dummy()).unwrap_err();
        assert!(err.message.contains("__macro_probe() takes exactly 1 argument"));
    }

    /// Verifies the `max_args` override caps `check_arity` (here to 2, below the
    /// 3-param declared signature) while `function_sig` still reports the full
    /// param count. The override must affect only arity validation, never the
    /// derived signature consumed by argument normalization and the parity gate.
    #[test]
    fn max_args_caps_check_arity_but_not_function_sig() {
        // __registry_probe_capped: params [a, b, c=0], max_args=2.
        // Calling with 3 args exceeds the capped max → arity error.
        let err = check_arity("__registry_probe_capped", 3, crate::span::Span::dummy())
            .expect_err("3 args must exceed the max_args=2 cap");
        assert!(
            err.message
                .contains("__registry_probe_capped() takes exactly 2 arguments"),
            "capped arity error mismatch: {}",
            err.message,
        );
        // function_sig is unaffected by the override: it reports all 3 params.
        let sig = function_sig("__registry_probe_capped").expect("probe registered");
        assert_eq!(
            sig.params.len(),
            3,
            "function_sig must report the full param count, ignoring max_args",
        );
        // A call within the cap (2 args) is accepted.
        assert!(check_arity("__registry_probe_capped", 2, crate::span::Span::dummy()).is_ok());
    }

    /// Verifies arity_bounds for a fixed-arity builtin with one optional param.
    #[test]
    fn arity_bounds_fixed_with_optional() {
        // __registry_probe_opt: params [a: Int, b: Str = Null], variadic: None
        // min = 1 (a is required), max = Some(2)
        let (min, max) = arity_bounds("__registry_probe_opt").expect("probe registered");
        assert_eq!(min, 1, "one required param");
        assert_eq!(max, Some(2), "two params total, not variadic");
    }

    /// Verifies arity_bounds for a variadic builtin returns None as max.
    #[test]
    fn arity_bounds_variadic() {
        // __registry_probe_variadic: fixed [fmt: Str], variadic: "__registry_values"
        // min = 1 (fmt required), max = None
        let (min, max) = arity_bounds("__registry_probe_variadic").expect("probe registered");
        assert_eq!(min, 1, "one required fixed param");
        assert_eq!(max, None, "variadic → unbounded max");
    }

    /// Verifies `function_sig` fields match the expected FunctionSig layout.
    #[test]
    fn function_sig_fields_match_layout() {
        let sig = function_sig("__registry_probe_opt").expect("probe registered");
        assert_eq!(sig.params.len(), 2);
        assert!(!sig.declared_return, "declared_return must be false for builtins");
        assert_eq!(sig.declared_params, vec![false, false]);
        assert_eq!(sig.ref_params, vec![false, false]);
        assert!(sig.variadic.is_none());
        assert!(sig.deprecation.is_none());
        assert_eq!(sig.return_type, PhpType::Bool);
    }

    /// Verifies `first_class_callable_sig` applies the variadic-upgrade for variadic builtins.
    #[test]
    fn first_class_callable_sig_upgrades_variadic() {
        let sig = first_class_callable_sig("__registry_probe_variadic")
            .expect("probe registered");
        // After callable_wrapper_sig, the variadic param type becomes Array<Mixed>.
        let variadic_name = sig.variadic.as_deref().expect("variadic name preserved");
        let var_param = sig.params.iter().find(|(n, _)| n == variadic_name);
        let (_, var_ty) = var_param.expect("variadic param must be in params");
        assert_eq!(*var_ty, PhpType::Array(Box::new(PhpType::Mixed)));
    }

    /// Verifies `function_sig` returns None for an unknown builtin.
    #[test]
    fn function_sig_returns_none_for_unknown() {
        assert!(function_sig("__nonexistent_builtin_xyz").is_none());
    }

    /// Verifies `arity_bounds` returns None for an unknown builtin.
    #[test]
    fn arity_bounds_returns_none_for_unknown() {
        assert!(arity_bounds("__nonexistent_builtin_xyz").is_none());
    }
}
