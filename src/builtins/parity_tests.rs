//! Purpose:
//! Structural parity gates for registry visibility, extension classification,
//! strict-PHP behavior, and compiler prelude usage.
//!
//! Called from:
//! - `cargo test` through Rust's test harness (unit test module).
//!
//! Key details:
//! - Registry signatures are authoritative; no parallel name-based golden table exists.
//! - Extension and internal visibility sets remain explicitly pinned.

use crate::builtins::registry;

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
