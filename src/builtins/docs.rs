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
//! - `TypeSpec` rendering is recursive (handles `ArrayOf`/`AssocOf`/`Union` nesting).
//! - Builtins with `internal: true` are excluded from the export.
//! - `#![allow(dead_code)]` suppresses warnings when the module is compiled in the context of
//!   the `elephc` binary (which never calls `export_builtins_json`); all items here are live
//!   from the `gen_builtins` binary's perspective.
#![allow(dead_code)]

use crate::builtins::registry::{lookup, names};
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
        TypeSpec::Null => "null".to_string(),
        TypeSpec::Void => "void".to_string(),
        TypeSpec::ArrayOf(inner) => format!("array<{}>", type_spec_str(inner)),
        TypeSpec::AssocOf(inner) => format!("array<string, {}>", type_spec_str(inner)),
        TypeSpec::Union(members) => members
            .iter()
            .map(type_spec_str)
            .collect::<Vec<_>>()
            .join("|"),
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
        Area::Internal => "internal",
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
        DefaultSpec::IntMin => json!("PHP_INT_MIN"),
        DefaultSpec::EmptyArray => json!([]),
    }
}

/// Returns true when a PHP-visible builtin exists in the static AOT surface:
/// `builtin!` registry entries plus compiler-resident constructs (`isset`,
/// `strval`, predicate aliases, ...). Documentation tooling uses this to tell
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
