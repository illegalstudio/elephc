//! Purpose:
//! Integration tests for the compiler and Magician joins over the shared
//! builtin contract catalog.
//!
//! Called from:
//! - `cargo test --test builtin_parity_tests` through Rust's test harness.
//!
//! Key details:
//! - Backend coverage and signature profiles come from `elephc-builtin-contract`.
//! - No independent static-only, eval-only, or signature-extension allowlist exists here.

use std::collections::BTreeSet;

use elephc_builtin_contract::{
    aot_signature_profile, aot_support, contracts, eval_signature, eval_support,
    AotSignatureOverrideReason, BackendImplementation, BackendSupport, BuiltinSignature,
};

/// Verifies all contract surfaces outside the ordinary AOT registry have typed routes.
#[test]
fn non_registry_surfaces_have_complete_backend_contracts() {
    let exceptional = contracts()
        .iter()
        .filter(|contract| {
            !matches!(
                aot_support(contract),
                BackendSupport::Implemented(BackendImplementation::Registry)
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(exceptional.len(), 15);

    let mut language_constructs = 0;
    let mut dedicated_syntax = 0;
    let mut preludes = BTreeSet::new();
    let mut unsupported = 0;
    for contract in exceptional {
        match aot_support(contract) {
            BackendSupport::Implemented(BackendImplementation::LanguageConstruct) => {
                language_constructs += 1;
            }
            BackendSupport::Implemented(BackendImplementation::DedicatedSyntax) => {
                dedicated_syntax += 1;
            }
            BackendSupport::Implemented(BackendImplementation::Prelude) => {
                preludes.insert(contract.name);
            }
            BackendSupport::Unsupported(_) => unsupported += 1,
            BackendSupport::Implemented(BackendImplementation::Registry) => unreachable!(),
        }
    }
    assert_eq!(language_constructs, 5);
    assert_eq!(dedicated_syntax, 1);
    assert_eq!(unsupported, 3);
    assert_eq!(
        preludes,
        BTreeSet::from(["dir", "hash_copy", "hash_final", "hash_init", "hash_update"])
    );

    let hash_init = contracts()
        .iter()
        .find(|contract| contract.name == "hash_init")
        .expect("hash_init contract must exist");
    let profile = aot_signature_profile(hash_init);
    assert_eq!(profile.signature.params.len(), 1);
    assert_eq!(
        profile.override_reason,
        Some(AotSignatureOverrideReason::PreludeSignatureSubset)
    );
}

/// Verifies the public compiler and Magician name sets match shared support records.
#[test]
fn backend_public_name_sets_derive_from_shared_support() {
    let expected_aot = contracts()
        .iter()
        .filter(|contract| !contract.internal)
        .filter(|contract| {
            matches!(
                aot_support(contract),
                BackendSupport::Implemented(
                    BackendImplementation::Registry
                        | BackendImplementation::LanguageConstruct
                        | BackendImplementation::DedicatedSyntax
                )
            )
        })
        .map(|contract| contract.name)
        .collect::<BTreeSet<_>>();
    let actual_aot = elephc::builtin_metadata::php_visible_builtin_names()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    assert_eq!(actual_aot, expected_aot);

    let expected_eval = contracts()
        .iter()
        .filter(|contract| {
            matches!(
                eval_support(contract),
                BackendSupport::Implemented(BackendImplementation::Registry)
            )
        })
        .map(|contract| contract.name)
        .collect::<BTreeSet<_>>();
    let actual_eval = elephc_magician::builtin_metadata::php_visible_builtin_names()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    assert_eq!(actual_eval, expected_eval);
}

/// Verifies each backend exposes exactly the signature profile selected by the contract.
#[test]
fn backend_signature_shapes_derive_from_shared_contracts() {
    for contract in contracts() {
        if matches!(
            aot_support(contract),
            BackendSupport::Implemented(BackendImplementation::Registry)
        ) {
            let actual = elephc::builtin_metadata::builtin_signature_metadata(contract.name)
                .unwrap_or_else(|| panic!("missing AOT signature for {}", contract.name));
            assert_signature_shape(
                contract.name,
                contract.signature(),
                &actual.params,
                actual.required_param_count,
                actual.default_param_count,
                actual.variadic.as_deref(),
                &actual.by_ref_params,
            );
        }

        if matches!(
            eval_support(contract),
            BackendSupport::Implemented(BackendImplementation::Registry)
        ) {
            let actual =
                elephc_magician::builtin_metadata::builtin_signature_metadata(contract.name)
                    .unwrap_or_else(|| panic!("missing eval signature for {}", contract.name));
            assert_signature_shape(
                contract.name,
                eval_signature(contract),
                &actual.params,
                actual.required_param_count,
                actual.default_param_count,
                actual.variadic.as_deref(),
                &actual.by_ref_params,
            );
        }
    }
}

/// Verifies strict-PHP extension visibility is selected by the shared contract.
#[test]
fn extension_builtin_sets_derive_from_shared_contracts() {
    let expected_aot = contracts()
        .iter()
        .filter(|contract| contract.extension && !contract.internal)
        .filter(|contract| {
            matches!(
                aot_support(contract),
                BackendSupport::Implemented(
                    BackendImplementation::Registry | BackendImplementation::DedicatedSyntax
                )
            )
        })
        .map(|contract| contract.name)
        .collect::<BTreeSet<_>>();
    let actual_aot = elephc::builtin_metadata::extension_builtin_names()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    assert_eq!(actual_aot, expected_aot);

    let expected_eval = contracts()
        .iter()
        .filter(|contract| contract.extension)
        .filter(|contract| {
            matches!(
                eval_support(contract),
                BackendSupport::Implemented(BackendImplementation::Registry)
            )
        })
        .map(|contract| contract.name)
        .collect::<BTreeSet<_>>();
    let actual_eval = elephc_magician::builtin_metadata::extension_builtin_names()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    assert_eq!(actual_eval, expected_eval);
}

/// Verifies implementation metadata exists exactly when the shared eval support says it must.
#[test]
fn eval_registry_coverage_matches_shared_support_records() {
    for contract in contracts() {
        let registered =
            elephc_magician::builtin_metadata::php_visible_builtin_is_registry_declared(
                contract.name,
            );
        let expected = matches!(
            eval_support(contract),
            BackendSupport::Implemented(BackendImplementation::Registry)
        );
        assert_eq!(registered, expected, "{} eval coverage", contract.name);
    }
}

/// Compares one backend metadata view with a neutral signature profile.
fn assert_signature_shape(
    name: &str,
    expected: BuiltinSignature,
    actual_params: &[String],
    actual_required: usize,
    actual_defaults: usize,
    actual_variadic: Option<&str>,
    actual_by_ref: &[String],
) {
    let expected_params = expected
        .params
        .iter()
        .map(|param| param.name)
        .chain(expected.variadic)
        .collect::<Vec<_>>();
    let expected_defaults = expected
        .params
        .iter()
        .filter(|param| param.default.is_some())
        .count()
        + usize::from(expected.variadic.is_some());
    let expected_by_ref = expected
        .params
        .iter()
        .filter(|param| param.by_ref)
        .map(|param| param.name)
        .collect::<Vec<_>>();

    assert_eq!(actual_params, expected_params, "{name} parameter names");
    assert_eq!(
        actual_required,
        expected.required_param_count(),
        "{name} required parameter count"
    );
    assert_eq!(actual_defaults, expected_defaults, "{name} default count");
    assert_eq!(actual_variadic, expected.variadic, "{name} variadic name");
    assert_eq!(actual_by_ref, expected_by_ref, "{name} by-reference params");
}
