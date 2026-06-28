//! Purpose:
//! Integration tests for builtin catalog parity between static elephc and
//! elephc-magician's eval interpreter.
//!
//! Called from:
//! - `cargo test --test builtin_parity_tests` through Rust's test harness.
//!
//! Key details:
//! - Static builtin names and signatures are read from compiler metadata APIs.
//! - Eval builtin existence and named-argument parameters are read from magician metadata APIs.

use std::collections::BTreeSet;

/// Static-only raw memory helpers are elephc extensions tied to AOT FFI values.
const STATIC_ONLY_RAW_MEMORY_BUILTINS: &[&str] = &[
    "buffer_free",
    "buffer_len",
    "buffer_new",
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
];

/// Eval-only reflection probes exist because magician can inspect dynamic eval metadata before the AOT catalog exposes them.
const EVAL_ONLY_REFLECTION_BUILTINS: &[&str] = &[
    "get_called_class",
    "get_class_methods",
    "get_class_vars",
    "get_object_vars",
];

/// Verifies every non-raw-memory static builtin is visible through eval's function lookup.
#[test]
fn static_php_visible_builtins_are_visible_to_eval() {
    let static_only = STATIC_ONLY_RAW_MEMORY_BUILTINS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let missing = elephc::builtin_metadata::php_visible_builtin_names()
        .iter()
        .copied()
        .filter(|name| !static_only.contains(name))
        .filter(|name| !elephc_magician::builtin_metadata::php_visible_builtin_exists(name))
        .collect::<Vec<_>>();

    assert!(
        missing.is_empty(),
        "static builtins missing from eval function lookup: {missing:?}"
    );
}

/// Verifies eval has named-argument parameter metadata for each shared static builtin.
#[test]
fn shared_builtin_parameter_names_match_static_signatures() {
    let static_only = STATIC_ONLY_RAW_MEMORY_BUILTINS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let mut missing_static_signature = Vec::new();
    let mut missing_eval_signature = Vec::new();
    let mut mismatched_params = Vec::new();

    for name in elephc::builtin_metadata::php_visible_builtin_names() {
        if static_only.contains(name) {
            continue;
        }
        let Some(static_meta) = elephc::builtin_metadata::builtin_signature_metadata(name) else {
            missing_static_signature.push(*name);
            continue;
        };
        let Some(eval_meta) = elephc_magician::builtin_metadata::builtin_signature_metadata(name) else {
            missing_eval_signature.push(*name);
            continue;
        };
        if static_meta.params != eval_meta.params {
            mismatched_params.push((
                *name,
                static_meta.params.clone(),
                eval_meta.params.clone(),
            ));
        }
    }

    assert!(
        missing_static_signature.is_empty(),
        "static catalog entries without signature metadata: {missing_static_signature:?}"
    );
    assert!(
        missing_eval_signature.is_empty(),
        "shared builtins without eval parameter metadata: {missing_eval_signature:?}"
    );
    assert!(
        mismatched_params.is_empty(),
        "shared builtin parameter-name mismatches: {mismatched_params:#?}"
    );
}

/// Documents the current eval-only reflection builtins so the drift is explicit.
#[test]
fn eval_only_reflection_builtins_remain_visible_until_static_catalog_catches_up() {
    let static_names = elephc::builtin_metadata::php_visible_builtin_names()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();

    for name in EVAL_ONLY_REFLECTION_BUILTINS {
        assert!(
            !static_names.contains(name),
            "{name} moved into the static catalog; remove it from the eval-only allowlist"
        );
        assert!(
            elephc_magician::builtin_metadata::php_visible_builtin_exists(name),
            "{name} should stay visible to eval while it is documented as eval-only"
        );
    }
}
