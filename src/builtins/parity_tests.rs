//! Purpose:
//! Parity gate between registry-derived builtin signatures and the legacy
//! `legacy_builtin_call_sig()` golden table. For every builtin registered in
//! the inventory that also has a legacy table entry, this file asserts that the
//! behavior-bearing fields of the two `FunctionSig`s agree.
//!
//! Called from:
//! - `cargo test` through Rust's test harness (unit test module).
//!
//! Key details:
//! - Placed here (not in `tests/`) because `legacy_builtin_call_sig` is
//!   `pub(crate)` and cannot be reached from an integration test without
//!   widening visibility.
//! - Type fields (`params[*].1`, `return_type`, `declared_return`,
//!   `declared_params`) are intentionally excluded from comparison: the registry
//!   derives precise types while the legacy table uses `PhpType::Mixed`
//!   placeholders. That precision is an intended improvement and is
//!   behavior-neutral (call-arg planning reads only param NAMES, never types).
//! - The gate compares against `legacy_builtin_call_sig` (not `builtin_call_sig`)
//!   so that the comparison is non-vacuous: `builtin_call_sig` checks the registry
//!   first, so comparing registry::function_sig against builtin_call_sig would
//!   simply compare a value against itself for migrated builtins.
//! - Migration rule: when a builtin is moved into `src/builtins/`, its arm is
//!   KEPT in `legacy_builtin_call_sig` as the parity golden. Remove the arm only
//!   after the parity gate has verified the registry matches and the golden is no
//!   longer needed.

use crate::builtins::registry;
use crate::types::{legacy_builtin_call_sig, FunctionSig};

/// Asserts that the behavior-bearing fields of `derived` and `legacy` agree.
///
/// Fields compared:
/// - param names (`.0` of each `(String, PhpType)` pair, in order)
/// - defaults (rendered via `{:?}` for a stable comparison of `Option<Expr>`)
/// - ref_params (per-parameter by-reference flags)
/// - variadic (the variadic parameter name, if any)
/// - by_ref_return
/// - total param count and required param count (arity)
///
/// Panics with a message naming the builtin and the diverging field.
fn assert_behavior_fields_match(name: &str, derived: &FunctionSig, legacy: &FunctionSig) {
    // Arity: total param count.
    assert_eq!(
        derived.params.len(),
        legacy.params.len(),
        "signature drift for {name}: param count differs (derived={}, legacy={})",
        derived.params.len(),
        legacy.params.len(),
    );

    // Required param count: params with no default.
    let derived_required = derived.defaults.iter().filter(|d| d.is_none()).count();
    let legacy_required = legacy.defaults.iter().filter(|d| d.is_none()).count();
    assert_eq!(
        derived_required,
        legacy_required,
        "signature drift for {name}: required param count differs (derived={derived_required}, legacy={legacy_required})",
    );

    // Param names (in order).
    let derived_names: Vec<&str> = derived.params.iter().map(|(n, _)| n.as_str()).collect();
    let legacy_names: Vec<&str> = legacy.params.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        derived_names,
        legacy_names,
        "signature drift for {name}: param names differ (derived={derived_names:?}, legacy={legacy_names:?})",
    );

    // Defaults (stable debug representation for Option<Expr>).
    let derived_defaults = format!("{:?}", derived.defaults);
    let legacy_defaults = format!("{:?}", legacy.defaults);
    assert_eq!(
        derived_defaults,
        legacy_defaults,
        "signature drift for {name}: defaults differ\n  derived={derived_defaults}\n  legacy={legacy_defaults}",
    );

    // Per-parameter by-reference flags.
    assert_eq!(
        derived.ref_params,
        legacy.ref_params,
        "signature drift for {name}: ref_params differ (derived={:?}, legacy={:?})",
        derived.ref_params,
        legacy.ref_params,
    );

    // Variadic parameter name.
    assert_eq!(
        derived.variadic,
        legacy.variadic,
        "signature drift for {name}: variadic differs (derived={:?}, legacy={:?})",
        derived.variadic,
        legacy.variadic,
    );

    // By-reference return flag.
    assert_eq!(
        derived.by_ref_return,
        legacy.by_ref_return,
        "signature drift for {name}: by_ref_return differs (derived={}, legacy={})",
        derived.by_ref_return,
        legacy.by_ref_return,
    );
}

/// Verifies that every registry-derived builtin signature agrees with the legacy
/// `legacy_builtin_call_sig()` golden table on all behavior-bearing fields.
///
/// Iterates all names registered in the inventory. For each name that also has
/// a golden legacy entry, runs `assert_behavior_fields_match`. Names with no
/// legacy entry (internal test probes, or builtins not yet assigned a golden)
/// are skipped — the gate activates incrementally as migration tasks register
/// real builtins and retain their legacy arms as goldens.
///
/// The comparison uses `legacy_builtin_call_sig` (NOT `builtin_call_sig`) so that
/// the test is non-vacuous: `builtin_call_sig` checks the registry first, so for
/// any migrated builtin both sides would resolve to the same registry value and
/// the assertion would always trivially pass.
#[test]
fn derived_signatures_match_legacy() {
    for name in registry::names() {
        // Skip internal test probes and builtins not yet assigned a legacy golden.
        let Some(legacy) = legacy_builtin_call_sig(name) else {
            continue;
        };

        let derived = registry::function_sig(name)
            .unwrap_or_else(|| panic!("registry::names() yielded {name} but function_sig returned None"));

        assert_behavior_fields_match(name, &derived, &legacy);
    }
}

/// The exact set of PHP-visible builtins that are elephc extensions (no PHP
/// equivalent), pinned so that adding or reclassifying a builtin is a conscious,
/// reviewable decision. `--strict-php` hides exactly this set (plus the
/// `buffer_new` catalog-name-only entry) from user programs.
const EXPECTED_EXTENSION_BUILTINS: &[&str] = &[
    "buffer_free",
    "buffer_len",
    "class_attribute_args",
    "class_attribute_names",
    "class_get_attributes",
    "ptr",
    "ptr_get",
    "ptr_is_null",
    "ptr_null",
    "ptr_offset",
    "ptr_read16",
    "ptr_read32",
    "ptr_read8",
    "ptr_read_string",
    "ptr_set",
    "ptr_sizeof",
    "ptr_write16",
    "ptr_write32",
    "ptr_write8",
    "ptr_write_string",
    "zval_free",
    "zval_pack",
    "zval_type",
    "zval_unpack",
];

/// Verifies no injected compiler prelude calls a PHP-visible extension builtin.
///
/// `--strict-php` hides extension builtins at the catalog level with no notion
/// of code origin, so a prelude calling one (instead of its `internal: true`
/// `__elephc_*` alias) would break strict-mode compiles of programs that
/// trigger that prelude's injection. Scans every prelude PHP source for
/// `<name>(` call sites; bare mentions inside comments are tolerated.
#[test]
fn preludes_never_call_php_visible_extension_builtins() {
    let mut extension_names: Vec<String> = vec!["buffer_new".to_string()];
    for name in registry::names() {
        let def = registry::lookup(name).expect("names() yields registered builtins");
        if def.spec.extension && !def.spec.internal {
            extension_names.push(def.name.to_string());
        }
    }

    let prelude_sources: &[(&str, &str)] = &[
        ("pdo_prelude", crate::pdo_prelude::PDO_PRELUDE_SRC),
        ("tz_prelude", crate::tz_prelude::TZ_PRELUDE_SRC),
        ("list_id_prelude", crate::list_id_prelude::LIST_ID_PRELUDE_TEMPLATE),
        ("var_export_prelude", crate::var_export_prelude::VAR_EXPORT_PRELUDE_SRC),
        ("image_prelude", crate::image_prelude::IMAGE_PRELUDE_SRC),
        ("web_prelude", crate::web_prelude::WEB_PRELUDE_SRC),
        ("web_prelude(wrap)", crate::web_prelude::WEB_WRAP_SRC),
    ];

    let mut violations: Vec<String> = Vec::new();
    for (prelude, source) in prelude_sources {
        for name in &extension_names {
            if source_calls_function(source, name) {
                violations.push(format!("{prelude} calls {name}()"));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "preludes must call `__elephc_*` internal aliases, not PHP-visible extension builtins:\n{}",
        violations.join("\n"),
    );
}

/// Returns true when `source` contains a plain function-call site `name(`.
///
/// A match is a call site only when the preceding character is not part of a
/// longer identifier (`elephc_pdo_column_data_ptr(`), a variable (`$ptr(`), or
/// a method/static access (`->ptr(`, `::ptr(`), so extern helpers whose names
/// merely end with a builtin name do not count.
fn source_calls_function(source: &str, name: &str) -> bool {
    let needle = format!("{name}(");
    source.match_indices(&needle).any(|(index, _)| {
        match source[..index].chars().next_back() {
            None => true,
            Some(prev) => {
                !prev.is_ascii_alphanumeric() && !matches!(prev, '_' | '$' | '>' | ':')
            }
        }
    })
}

/// Verifies the registry's PHP-visible `extension: true` set matches the pinned
/// list exactly, in both directions: no extension builtin missing the flag, no
/// PHP builtin carrying it by mistake. Internal builtins are skipped: they are
/// not PHP-visible, strict mode never hides them, and cfg(test) probes may
/// combine `internal` with `extension` to exercise the macro.
#[test]
fn extension_builtin_set_is_pinned() {
    let mut tagged: Vec<&str> = Vec::new();
    for name in registry::names() {
        let def = registry::lookup(name).expect("names() yields registered builtins");
        if def.spec.extension && !def.spec.internal {
            tagged.push(def.name);
        }
    }
    tagged.sort_unstable();
    assert_eq!(
        tagged, EXPECTED_EXTENSION_BUILTINS,
        "extension builtin set drifted from the pinned list",
    );
}
