//! Purpose:
//! Serialises the single-source builtin registry to a JSON array for documentation tooling.
//! Every PHP-visible registered builtin is emitted as one object; internal builtins are skipped.
//!
//! Called from:
//! - `tools/gen_builtins.rs` (example target) via `elephc::builtins::docs::export_builtins_json()`.
//!
//! Key details:
//! - Uses `crate::builtins::registry::{names, lookup}` so `inventory::iter` runs in the same
//!   crate that submitted the `builtin!` entries — required for the iterator to see all specs.
//! - Registry scalar types are rendered with their PHP documentation spelling.
//! - Builtins with `internal: true` are excluded from the export.
//! - `#![allow(dead_code)]` suppresses warnings when the module is compiled in the context of
//!   the `elephc` binary (which never calls `export_builtins_json`); all items here are live
//!   from the `gen_builtins` binary's perspective.
#![allow(dead_code)]

use crate::builtins::registry::{lookup, names};
use crate::builtins::semantics::{
    BuiltinArgumentLowering, BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering,
    BuiltinRequirement, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy, BuiltinValidation,
};
use crate::builtins::spec::{Area, DefaultSpec, TypeSpec};
use serde_json::{json, Value};

/// Renders a `TypeSpec` as a PHP-style type string for documentation JSON.
fn type_spec_str(ty: &TypeSpec) -> String {
    match ty {
        TypeSpec::Int => "int".to_string(),
        TypeSpec::Float => "float".to_string(),
        TypeSpec::Str => "string".to_string(),
        TypeSpec::Bool => "bool".to_string(),
        TypeSpec::Mixed => "mixed".to_string(),
        TypeSpec::Void => "void".to_string(),
    }
}

/// Maps a builtin `Area` to its lowercase documentation category name.
fn area_str(area: Area) -> &'static str {
    match area {
        Area::String => "string",
        Area::Array => "array",
        Area::Math => "math",
        Area::Io => "io",
        Area::System => "system",
        Area::Types => "types",
        Area::Callables => "callables",
        Area::Spl => "spl",
        Area::Pointers => "pointers",
    }
}

/// Renders a parameter `DefaultSpec` as its documentation JSON value.
fn default_spec_json(default: &DefaultSpec) -> Value {
    match default {
        DefaultSpec::Null => Value::Null,
        DefaultSpec::Int(v) => json!(v),
        DefaultSpec::Bool(v) => json!(v),
        DefaultSpec::Float(v) => json!(v),
        DefaultSpec::Str(v) => json!(v),
        DefaultSpec::IntMax => json!("PHP_INT_MAX"),
        DefaultSpec::EmptyArray => json!([]),
    }
}

/// Renders one runtime or linker requirement for semantic inventory tooling.
fn requirement_json(requirement: BuiltinRequirement) -> Value {
    match requirement {
        BuiltinRequirement::Bridge(name) => json!({"kind": "bridge", "name": name}),
        BuiltinRequirement::SystemLibrary(name) => {
            json!({"kind": "system_library", "name": name})
        }
        BuiltinRequirement::MacOsLibrary(name) => {
            json!({"kind": "macos_library", "name": name})
        }
        BuiltinRequirement::WindowsLibrary(name) => {
            json!({"kind": "windows_library", "name": name})
        }
        BuiltinRequirement::RuntimeFeature(name) => {
            json!({"kind": "runtime_feature", "name": name})
        }
    }
}

/// Renders the complete backend-neutral semantic descriptor for migration audits.
fn semantics_json(semantics: BuiltinSemantics) -> Value {
    let validation = match semantics.validation {
        BuiltinValidation::CheckerHook { lazy, .. } => {
            json!({"kind": "checker_hook", "lazy": lazy})
        }
        BuiltinValidation::SignatureOnly => json!({"kind": "signature"}),
        BuiltinValidation::Shared(_) => json!({"kind": "shared"}),
    };
    let result_type = match semantics.result_type {
        BuiltinResultType::Checked => "checked",
        BuiltinResultType::Declared => "declared",
        BuiltinResultType::Shared(_) => "shared",
    };
    let effects = match semantics.effects {
        BuiltinEffects::Static(effects) => {
            json!({"kind": "static", "names": effects.names()})
        }
        BuiltinEffects::Shared(_) => json!({"kind": "shared"}),
    };
    let ownership = match semantics.result_ownership {
        BuiltinResultOwnership::NonHeap => "non_heap",
        BuiltinResultOwnership::Fresh => "fresh",
        BuiltinResultOwnership::Borrowed => "borrowed",
        BuiltinResultOwnership::Independent => "independent",
        BuiltinResultOwnership::Aliases(_) => "aliases",
        BuiltinResultOwnership::MayAliasArguments => "may_alias_arguments",
    };
    let aliases = match semantics.result_ownership {
        BuiltinResultOwnership::Aliases(indexes) => indexes,
        _ => &[],
    };
    let requirements = match semantics.requirements {
        BuiltinRequirements::Static(requirements) => json!({
            "kind": "static",
            "values": requirements
                .iter()
                .copied()
                .map(requirement_json)
                .collect::<Vec<_>>(),
        }),
        BuiltinRequirements::Shared(_) => json!({"kind": "shared"}),
    };
    let target_strategy = match semantics.target_strategy {
        BuiltinTargetStrategy::EirPrimitive => "eir_primitive",
        BuiltinTargetStrategy::EirGraph => "eir_graph",
        BuiltinTargetStrategy::RuntimeCall => "runtime_call",
    };
    let target_support = match semantics.target_support {
        crate::builtins::semantics::BuiltinTargetSupport::All => {
            ["macos-aarch64", "linux-aarch64", "linux-x86_64"]
        }
    };
    let runtime_functions = match semantics.runtime_functions {
        BuiltinRuntimeFunctions::None => Vec::new(),
        BuiltinRuntimeFunctions::One(runtime_fn) => vec![runtime_fn.as_eir()],
    };
    let argument_lowering = match semantics.argument_lowering {
        BuiltinArgumentLowering::Standard => "standard",
        BuiltinArgumentLowering::Count => "count",
        BuiltinArgumentLowering::Date => "date",
        BuiltinArgumentLowering::JsonDecode => "json_decode",
        BuiltinArgumentLowering::ProcOpen => "proc_open",
        BuiltinArgumentLowering::PregReplaceCallback => "preg_replace_callback",
        BuiltinArgumentLowering::PositionalRegex => "positional_regex",
        BuiltinArgumentLowering::UserValueSort => "user_value_sort",
    };
    let callable = match semantics.callable {
        BuiltinCallablePolicy::Dynamic(_) => json!({"kind": "dynamic"}),
        BuiltinCallablePolicy::DynamicRuntime(target) => {
            json!({"kind": "dynamic_target", "target": target.as_eir()})
        }
        BuiltinCallablePolicy::StaticOnly(reason) => {
            json!({"kind": "static_only", "reason": reason})
        }
    };
    let lowering = match semantics.lowering {
        BuiltinLowering::Eir(_) | BuiltinLowering::TypePredicate(_) => json!({"kind": "eir"}),
        BuiltinLowering::Runtime(target) => {
            json!({"kind": "runtime_call", "target": target.as_eir()})
        }
    };
    json!({
        "validation": validation,
        "result_type": result_type,
        "effects": effects,
        "ownership": {"kind": ownership, "argument_indexes": aliases},
        "requirements": requirements,
        "target_strategy": target_strategy,
        "target_support": target_support,
        "runtime_functions": runtime_functions,
        "argument_lowering": argument_lowering,
        "callable": callable,
        "lowering": lowering,
    })
}

/// Returns true when a PHP-visible builtin exists in the static AOT surface:
/// `builtin!` registry entries plus compiler-resident constructs (`isset`,
/// `unset`, `empty`, `exit`, `die`, and dedicated `buffer_new`). Documentation tooling uses this to tell
/// resident names apart from genuinely eval-only builtins.
pub fn aot_php_visible_builtin_exists(name: &str) -> bool {
    crate::types::checker::builtins::is_php_visible_builtin_function(name)
}

/// Builds the documentation JSON array for every PHP-visible registered builtin.
///
/// Iterates the registry in sorted name order, skips `internal` builtins, and emits one object per
/// builtin (see [`build_json`] for the object shape). Consumed by the `gen_builtins` binary for
/// documentation generation.
pub fn export_builtins_json() -> Value {
    build_json(false)
}

/// Builds the documentation JSON array for every registered builtin, INCLUDING `internal` ones.
///
/// Same object shape as [`export_builtins_json`]; used by the docs pipeline, which renders
/// compiler-internals pages for internal `__elephc_*` helpers as well as the PHP-visible surface.
pub fn export_builtins_json_all() -> Value {
    build_json(true)
}

/// Builds the builtin documentation JSON array, optionally including `internal` builtins.
///
/// Iterates the registry in sorted name order and emits one object per builtin with its area,
/// `internal` flag, parameters (name/type/by_ref/optional/default), variadic name, arity overrides,
/// return type, summary, examples, PHP-manual fragment, and deprecation. When `include_internal` is
/// false, builtins flagged `internal` are skipped.
fn build_json(include_internal: bool) -> Value {
    let mut out: Vec<Value> = Vec::new();
    for name in names() {
        let Some(def) = lookup(name) else { continue };
        let spec = def.spec;
        if spec.internal && !include_internal {
            continue;
        }
        let params: Vec<Value> = spec
            .params
            .iter()
            .map(|p| {
                json!({
                    "name": p.name,
                    "type": type_spec_str(&p.ty),
                    "by_ref": p.by_ref,
                    // `optional` disambiguates a required param (no default) from an
                    // optional param whose default value is literally `null`; both would
                    // otherwise render as JSON `null` under the `default` key.
                    "optional": p.default.is_some(),
                    "default": p.default.as_ref().map(default_spec_json).unwrap_or(Value::Null),
                })
            })
            .collect();
        out.push(json!({
            "name": spec.name,
            "area": area_str(spec.area),
            "internal": spec.internal,
            "extension": spec.extension,
            "params": params,
            "variadic": spec.variadic,
            "returns": type_spec_str(&spec.returns),
            "by_ref_return": spec.by_ref_return,
            "min_args": spec.min_args,
            "max_args": spec.max_args,
            "arity_error": spec.arity_error,
            "semantics": semantics_json(spec.semantics),
            "summary": spec.summary,
            "examples": spec.examples,
            "php_manual": spec.php_manual,
            "deprecated": spec.deprecation,
        }));
    }
    Value::Array(out)
}

#[cfg(test)]
mod tests {
    /// Verifies the exporter emits a non-empty array and a known builtin (`strlen`) with its
    /// documented shape (required string param, int return, non-internal).
    #[test]
    fn export_contains_strlen_with_expected_shape() {
        let v = super::export_builtins_json();
        let arr = v.as_array().expect("top-level array");
        assert!(!arr.is_empty());
        let strlen = arr
            .iter()
            .find(|e| e["name"] == "strlen")
            .expect("strlen present");
        assert_eq!(strlen["area"], "string");
        assert_eq!(strlen["returns"], "int");
        assert_eq!(strlen["params"][0]["name"], "string");
        // `strlen`'s sole param is required, so `optional` must be false (it has no default).
        assert_eq!(strlen["params"][0]["optional"], false);
        // No internal builtins leak into the docs export.
        assert!(arr.iter().all(|e| e["name"].as_str().map_or(false, |n| !n.starts_with("__elephc_"))));
        // The default export carries the `internal` flag, always false here.
        assert_eq!(strlen["internal"], false);
    }

    /// Verifies the include-internal export is a strict superset of the PHP-visible one and
    /// surfaces at least one `internal` builtin flagged `internal: true`.
    #[test]
    fn export_all_includes_internal_builtins() {
        let visible = super::export_builtins_json();
        let all = super::export_builtins_json_all();
        let visible_len = visible.as_array().expect("array").len();
        let all_arr = all.as_array().expect("array");
        assert!(all_arr.len() >= visible_len);
        // Every builtin flagged internal is present only in the include-internal export.
        assert!(all_arr.iter().any(|e| e["internal"] == true));
        assert!(visible
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["internal"] == false));
    }
}
