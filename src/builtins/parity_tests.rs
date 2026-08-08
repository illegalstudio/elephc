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

/// Returns the PHP-visible extension builtins a prelude must never call directly.
fn php_visible_extension_builtins() -> Vec<String> {
    let mut names: Vec<String> = vec!["buffer_new".to_string()];
    for name in registry::names() {
        let def = registry::lookup(name).expect("names() yields registered builtins");
        if def.spec.extension && !def.spec.internal {
            names.push(def.name.to_string());
        }
    }
    names
}

/// Verifies no injected compiler prelude calls a PHP-visible extension builtin.
///
/// `--strict-php` hides extension builtins at the catalog level with no notion of code origin,
/// so a prelude calling one (instead of its `internal: true` `__elephc_*` alias) would break
/// strict-mode compiles of programs that trigger that prelude's injection.
///
/// THIS USED TO BE HALF A GATE. Every prelude was PHP text, and the audit was a `grep` for
/// `name(` that tolerated bare mentions in comments and had to reason about the character before
/// the match to tell a call from `elephc_pdo_column_data_ptr(` or `->ptr(`. Now that every
/// prelude builds its declarations in Rust, the audit just reads the call sites off the AST —
/// and `called_function_names` panics on any node it does not model, so a prelude that grows a
/// construct this audit cannot see fails loudly instead of silently leaving the net.
#[test]
fn preludes_built_in_rust_never_call_php_visible_extension_builtins() {
    let extension_names = php_visible_extension_builtins();

    let built: &[(&str, crate::parser::ast::Program)] = &[
        ("hash_prelude", crate::hash_prelude::hash_declarations()),
        ("tz_prelude", crate::tz_prelude::tz_declarations()),
        (
            "var_export_prelude",
            crate::var_export_prelude::var_export_declarations(),
        ),
        (
            "list_id_prelude",
            crate::list_id_prelude::list_id_declarations(),
        ),
        (
            "pdo_prelude",
            crate::pdo_prelude::build::pdo_declarations(
                crate::php_version::PhpVersion::default(),
                crate::pdo_prelude::OptionalDrivers::from_build_environment(),
            ),
        ),
        ("image_prelude", crate::image_prelude::image_declarations()),
        (
            "version_prelude",
            crate::version_prelude::version_declarations(
                &["zend_version", "php_sapi_name", "ini_restore"],
                crate::web_prelude::PhpVersion::Php85,
            ),
        ),
        (
            "opcache_prelude(env)",
            crate::opcache_prelude::env_override_declarations(),
        ),
        (
            "opcache_prelude(ini)",
            crate::opcache_prelude::ini_helper_declarations(
                crate::web_prelude::PhpVersion::Php85,
                &[],
            ),
        ),
        (
            "opcache_prelude(state)",
            crate::opcache_prelude::build::state_helper_decls(),
        ),
        (
            "web_prelude",
            crate::web_prelude::build::web_declarations(
                crate::web_prelude::PhpVersion::Php85,
                &[],
            ),
        ),
        ("web_prelude(wrap)", vec![crate::web_prelude::web_wrap_stmt()]),
    ];

    let mut violations: Vec<String> = Vec::new();
    for (prelude, program) in built {
        let called = crate::synthetic_class::called_function_names(program);
        for name in &extension_names {
            if called.iter().any(|call| call.eq_ignore_ascii_case(name)) {
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
