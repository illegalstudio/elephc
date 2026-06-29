//! Purpose:
//! Integration tests for builtin catalog parity between static elephc and
//! elephc-magician's eval interpreter.
//!
//! Called from:
//! - `cargo test --test builtin_parity_tests` through Rust's test harness.
//!
//! Key details:
//! - Static builtin names and signatures are read from compiler metadata APIs.
//! - Eval builtin existence and signature shape are read from magician metadata APIs.

use std::collections::BTreeSet;

/// Eval-only reflection probes exist because magician can inspect dynamic eval metadata before the AOT catalog exposes them.
const EVAL_ONLY_REFLECTION_BUILTINS: &[&str] = &[
    "get_called_class",
    "get_class_methods",
    "get_class_vars",
    "get_object_vars",
];

/// Eval supports these PHP optional parameters before the static backend does.
const EVAL_SIGNATURE_EXTENSION_BUILTINS: &[&str] = &[
    "array_reverse",
    "array_splice",
    "nl2br",
    "print_r",
];

/// Eval supports variadic debug output before the static backend does.
const EVAL_VARIADIC_SIGNATURE_EXTENSION_BUILTINS: &[&str] = &["var_dump"];

/// Verifies every static builtin is visible through eval's function lookup.
#[test]
fn static_php_visible_builtins_are_visible_to_eval() {
    let missing = elephc::builtin_metadata::php_visible_builtin_names()
        .iter()
        .copied()
        .filter(|name| !elephc_magician::builtin_metadata::php_visible_builtin_exists(name))
        .collect::<Vec<_>>();

    assert!(
        missing.is_empty(),
        "static builtins missing from eval function lookup: {missing:?}"
    );
}

/// Verifies eval has signature metadata for each shared static builtin.
#[test]
fn shared_builtin_signature_shape_matches_static_signatures() {
    let mut missing_static_signature = Vec::new();
    let mut missing_eval_signature = Vec::new();
    let mut mismatched_signatures = Vec::new();

    for name in elephc::builtin_metadata::php_visible_builtin_names() {
        let Some(static_meta) = elephc::builtin_metadata::builtin_signature_metadata(name) else {
            missing_static_signature.push(*name);
            continue;
        };
        let Some(eval_meta) = elephc_magician::builtin_metadata::builtin_signature_metadata(name) else {
            missing_eval_signature.push(*name);
            continue;
        };
        if EVAL_SIGNATURE_EXTENSION_BUILTINS.contains(name) {
            assert_eval_signature_extends_static_signature(name, &static_meta, &eval_meta);
            continue;
        }
        if EVAL_VARIADIC_SIGNATURE_EXTENSION_BUILTINS.contains(name) {
            assert_eval_variadic_signature_extends_static_signature(
                name,
                &static_meta,
                &eval_meta,
            );
            continue;
        }

        if static_meta.params != eval_meta.params
            || static_meta.required_param_count != eval_meta.required_param_count
            || static_meta.default_param_count != eval_meta.default_param_count
            || static_meta.variadic != eval_meta.variadic
            || static_meta.by_ref_params != eval_meta.by_ref_params
        {
            mismatched_signatures.push((*name, static_meta, eval_meta));
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
        mismatched_signatures.is_empty(),
        "shared builtin signature-shape mismatches: {mismatched_signatures:#?}"
    );
}

/// Verifies a documented eval signature extension keeps the static prefix contract.
fn assert_eval_signature_extends_static_signature(
    name: &str,
    static_meta: &elephc::builtin_metadata::BuiltinSignatureMetadata,
    eval_meta: &elephc_magician::builtin_metadata::BuiltinSignatureMetadata,
) {
    assert!(
        eval_meta.params.starts_with(&static_meta.params),
        "{name} eval extension must preserve static parameter prefix: static={static_meta:#?} eval={eval_meta:#?}"
    );
    assert_eq!(
        static_meta.required_param_count, eval_meta.required_param_count,
        "{name} eval extension must preserve required parameter count"
    );
    assert_eq!(
        static_meta.variadic, eval_meta.variadic,
        "{name} eval extension must not change variadic behavior"
    );
    assert_eq!(
        static_meta.by_ref_params, eval_meta.by_ref_params,
        "{name} eval extension must preserve by-reference parameters"
    );
    assert!(
        eval_meta.default_param_count >= static_meta.default_param_count,
        "{name} eval extension must not remove defaults"
    );
}

/// Verifies a documented eval variadic extension keeps the static prefix contract.
fn assert_eval_variadic_signature_extends_static_signature(
    name: &str,
    static_meta: &elephc::builtin_metadata::BuiltinSignatureMetadata,
    eval_meta: &elephc_magician::builtin_metadata::BuiltinSignatureMetadata,
) {
    assert!(
        eval_meta.params.starts_with(&static_meta.params),
        "{name} eval variadic extension must preserve static parameter prefix: static={static_meta:#?} eval={eval_meta:#?}"
    );
    assert_eq!(
        static_meta.required_param_count, eval_meta.required_param_count,
        "{name} eval variadic extension must preserve required parameter count"
    );
    assert!(
        static_meta.variadic.is_none() && eval_meta.variadic.is_some(),
        "{name} eval variadic extension must add, not remove, variadic behavior"
    );
    assert_eq!(
        static_meta.by_ref_params, eval_meta.by_ref_params,
        "{name} eval variadic extension must preserve by-reference parameters"
    );
    assert!(
        eval_meta.default_param_count >= static_meta.default_param_count,
        "{name} eval variadic extension must not remove defaults"
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

/// Verifies magician does not expose unexpected builtin names outside the static catalog.
#[test]
fn eval_php_visible_builtins_are_static_or_documented_eval_only() {
    let static_names = elephc::builtin_metadata::php_visible_builtin_names()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let eval_only = EVAL_ONLY_REFLECTION_BUILTINS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let unexpected = elephc_magician::builtin_metadata::php_visible_builtin_names()
        .iter()
        .copied()
        .filter(|name| !static_names.contains(name) && !eval_only.contains(name))
        .collect::<Vec<_>>();

    assert!(
        unexpected.is_empty(),
        "eval exposes builtins outside the static catalog and eval-only allowlist: {unexpected:?}"
    );
}
