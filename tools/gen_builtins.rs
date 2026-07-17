//! Purpose:
//! Standalone docs exporter that prints the single-source PHP builtin registry as JSON,
//! enriched with eval-interpreter (magician) support metadata for every builtin.
//!
//! Called from:
//! - `cargo run --example gen_builtins` (documentation generation / CI docs export).
//!
//! Key details:
//! - Declared as an example (not a bin) so it can read `elephc-magician`'s metadata —
//!   magician is a dev-dependency, keeping the eval interpreter out of the `elephc` binary.
//! - Static AOT records come from `elephc::builtins::docs`; each record gains an `eval`
//!   block (`kind`: `registry` | `date-alias` | `none`) and builtins only the eval
//!   registry exposes are appended with `eval_only: true`.
//! - `--include-internal` also emits `internal: true` builtins (the docs pipeline renders
//!   compiler-internals pages for the `__elephc_*` helpers).

use serde_json::{json, Value};

/// Prints the dual-support builtin documentation JSON (pretty-printed) to stdout.
fn main() {
    let include_internal = std::env::args().any(|a| a == "--include-internal");
    let value = if include_internal {
        elephc::builtins::docs::export_builtins_json_all()
    } else {
        elephc::builtins::docs::export_builtins_json()
    };
    let Value::Array(mut records) = value else {
        panic!("builtins JSON export must be a top-level array");
    };
    for record in &mut records {
        let name = record["name"]
            .as_str()
            .expect("builtin record carries a string name")
            .to_string();
        let entry = record
            .as_object_mut()
            .expect("builtin record is a JSON object");
        entry.insert("eval".to_string(), eval_support_json(&name));
    }
    append_eval_only_records(&mut records, include_internal);
    let json = serde_json::to_string_pretty(&Value::Array(records)).expect("serialize builtins JSON");
    println!("{}", json);
}

/// Builds the eval-interpreter (magician) support block for one builtin name.
///
/// `kind` distinguishes how eval'd code reaches the builtin: `registry` for
/// declarative `eval_builtin!` entries, `date-alias` for procedural date/time
/// aliases resolved by the alias dispatcher, `none` when eval'd code cannot
/// call it.
fn eval_support_json(name: &str) -> Value {
    if let Some(meta) = elephc_magician::builtin_metadata::builtin_docs_metadata(name) {
        let params: Vec<Value> = meta
            .params
            .iter()
            .map(|p| {
                json!({
                    "name": p.name,
                    "by_ref": p.by_ref,
                    "optional": p.default.is_some(),
                    "default": p.default,
                })
            })
            .collect();
        let mut hooks: Vec<&str> = Vec::new();
        if meta.has_direct_hook {
            hooks.push("direct");
        }
        if meta.has_values_hook {
            hooks.push("values");
        }
        return json!({
            "supported": true,
            "kind": "registry",
            "area": meta.area,
            "hooks": hooks,
            "params": params,
            "variadic": meta.variadic,
            "required_param_count": meta.required_param_count,
            "home_file": meta.home_file,
        });
    }
    let bare = name.trim_start_matches('\\').to_ascii_lowercase();
    if elephc_magician::builtin_metadata::date_procedural_alias_names()
        .iter()
        .any(|alias| *alias == bare)
    {
        return json!({
            "supported": true,
            "kind": "date-alias",
            "home_file": "crates/elephc-magician/src/interpreter/builtins/time/aliases.rs",
        });
    }
    json!({ "supported": false, "kind": "none" })
}

/// Appends eval-registry builtins with no static `builtin!` counterpart.
///
/// Two flavors: `aot_resident: true` when the static compiler still supports
/// the name as a compiler-resident construct or alias (`isset`, `strval`,
/// `is_integer`, ...) — the Python pipeline merges the eval block into its
/// hand-maintained pseudo-entries; `eval_only: true` when only eval'd code can
/// call the builtin. Static fields carry neutral placeholders (magician specs
/// are untyped).
fn append_eval_only_records(records: &mut Vec<Value>, include_internal: bool) {
    for eval_name in elephc_magician::builtin_metadata::php_visible_builtin_names() {
        if elephc::builtins::registry::lookup(eval_name).is_some() {
            continue;
        }
        let Some(meta) = elephc_magician::builtin_metadata::builtin_docs_metadata(eval_name)
        else {
            continue;
        };
        let internal = meta.name.starts_with("__elephc_");
        if internal && !include_internal {
            continue;
        }
        let aot_resident = elephc::builtins::docs::aot_php_visible_builtin_exists(&meta.name);
        let params: Vec<Value> = meta
            .params
            .iter()
            .map(|p| {
                json!({
                    "name": p.name,
                    "type": "mixed",
                    "by_ref": p.by_ref,
                    "optional": p.default.is_some(),
                    "default": p.default,
                })
            })
            .collect();
        // Appended records have no static `builtin!` spec, so the extension
        // classification comes from the eval registry's derived set (this is
        // how the catalog-name-only `buffer_new` gets flagged).
        let extension = elephc_magician::builtin_metadata::extension_builtin_names()
            .contains(&meta.name.as_str());
        records.push(json!({
            "name": meta.name,
            "area": meta.area,
            "internal": internal,
            "extension": extension,
            "params": params,
            "variadic": meta.variadic,
            "returns": "mixed",
            "by_ref_return": false,
            "min_args": Value::Null,
            "max_args": Value::Null,
            "arity_error": Value::Null,
            "summary": "",
            "examples": Vec::<Value>::new(),
            "php_manual": Value::Null,
            "deprecated": Value::Null,
            "eval_only": !aot_resident,
            "aot_resident": aot_resident,
            "eval": eval_support_json(&meta.name),
        }));
    }
}
