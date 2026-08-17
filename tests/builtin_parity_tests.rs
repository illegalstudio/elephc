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
    aot_signature_profile, aot_support, contracts, eval_execution, eval_signature,
    eval_signature_profile, eval_support, Area, AotSignatureOverrideReason,
    BackendImplementation, BackendSupport, BuiltinContract, BuiltinKind, BuiltinSignature,
    EvalAdapterReason, EvalExecution,
};

/// PHP-visible `curl_*` contracts the shared catalog publishes in this configuration.
///
/// `elephc-builtin-contract` compiles `catalog_curl.rs` only under its own `curl`
/// feature, and the root package's `curl` feature is the relay that turns it on for
/// this test binary together with Magician's `ext/curl` eval bindings (see the root
/// `Cargo.toml`). Feature-off the number is zero and every assertion below reduces to
/// "the surface is absent"; feature-on it is the complete surface, and the curl
/// contracts are audited by exactly the same machinery as every other builtin.
const CURL_SURFACE_LEN: usize = if cfg!(feature = "curl") { 34 } else { 0 };

/// The two curl contracts whose eval route needs caller-addressable storage.
///
/// `curl_multi_exec` writes its running-transfer count and `curl_multi_info_read` its
/// queue depth back through a by-reference parameter; every other curl contract is an
/// ordinary prelude-provided surface.
const CURL_BY_REF_CONTRACTS: &[&str] = &["curl_multi_exec", "curl_multi_info_read"];

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

    // The prelude-provided `curl_*` contracts are counted separately so the fixed
    // totals below stay about the non-curl surface in both configurations; the curl
    // surface's own per-contract audit is `curl_php_surface_is_a_full_parity_citizen`.
    let (curl_surface, exceptional): (Vec<&BuiltinContract>, Vec<&BuiltinContract>) = exceptional
        .into_iter()
        .partition(|contract| matches!(contract.area, Area::Curl));
    assert_eq!(
        curl_surface.len(),
        CURL_SURFACE_LEN,
        "the curl PHP surface is published all-or-nothing by the `curl` feature"
    );
    for contract in &curl_surface {
        assert_eq!(
            aot_support(contract),
            BackendSupport::Implemented(BackendImplementation::Prelude),
            "{} must reach AOT through the curl prelude",
            contract.name
        );
    }
    assert_eq!(exceptional.len(), 13);

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
        BTreeSet::from(["hash_copy", "hash_final", "hash_init", "hash_update"])
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

/// Verifies the PHP-visible curl surface is audited like every other builtin.
///
/// Feature-off this proves the surface is wholly absent — no `curl_*` contract and no
/// Magician binding claiming one. Feature-on it applies, per contract, every check the
/// shared machinery can express for a `PreludeProvided` entry:
///
/// - catalog classification: `Area::Curl`, `BuiltinKind::PreludeProvided`, neither
///   `internal` nor `extension`, and a `curl_` PHP name;
/// - the AOT route (`Prelude`) with no deliberate signature divergence;
/// - the eval route (`Registry`) with no deliberate signature divergence, plus the
///   documented `EvalExecution` classification each contract must carry;
/// - signature agreement between the catalog and Magician's real binding — parameter
///   names, required and default counts, variadic name, by-reference markers;
/// - both backends' public name sets: a prelude-routed name is deliberately absent from
///   the compiler's registry-derived name set and present in Magician's.
///
/// STRUCTURALLY INAPPLICABLE, and why. There is no AOT *registry* signature to compare
/// against: a prelude-provided contract has no `builtin!` binding by definition, so
/// `elephc::builtin_metadata::builtin_signature_metadata` answers `None`. This is not a
/// curl exemption — `backend_signature_shapes_derive_from_shared_contracts` skips the
/// four `hash_*` prelude contracts for exactly the same reason, and has since the
/// shared-contract migration. The compiler-side signature these contracts DO have is the
/// PHP function declaration inside the injected prelude, and that is compared against the
/// catalog by `builtins::parity_tests::prelude_contracts_match_their_injected_signatures`
/// — a lib test rather than one here, because the prelude sources are `pub(crate)`.
#[test]
fn curl_php_surface_is_a_full_parity_citizen() {
    let curl_surface = contracts()
        .iter()
        .filter(|contract| matches!(contract.area, Area::Curl) && !contract.internal)
        .collect::<Vec<_>>();
    assert_eq!(
        curl_surface.len(),
        CURL_SURFACE_LEN,
        "PHP-visible curl contract count"
    );

    let aot_names = elephc::builtin_metadata::php_visible_builtin_names()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let eval_names = elephc_magician::builtin_metadata::php_visible_builtin_names()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();

    for contract in &curl_surface {
        let name = contract.name;
        assert!(name.starts_with("curl_"), "{name} is not a curl PHP name");
        assert_eq!(contract.kind, BuiltinKind::PreludeProvided, "{name} kind");
        assert!(!contract.extension, "{name} is not a strict-PHP extension");
        assert_eq!(
            aot_support(contract),
            BackendSupport::Implemented(BackendImplementation::Prelude),
            "{name} AOT route"
        );
        assert_eq!(
            aot_signature_profile(contract).override_reason,
            None,
            "{name} must not diverge from the canonical AOT signature"
        );
        assert_eq!(
            eval_support(contract),
            BackendSupport::Implemented(BackendImplementation::Registry),
            "{name} eval route"
        );
        assert_eq!(
            eval_signature_profile(contract).override_reason,
            None,
            "{name} must not diverge from the canonical eval signature"
        );

        let expected_reason = if CURL_BY_REF_CONTRACTS.contains(&name) {
            EvalAdapterReason::ByReferenceOrLvalue
        } else {
            EvalAdapterReason::DynamicLanguageSurface
        };
        assert_eq!(
            eval_execution(contract),
            Some(EvalExecution::Adapter {
                runtime_builtin: None,
                reason: expected_reason,
            }),
            "{name} eval execution route"
        );

        let actual = elephc_magician::builtin_metadata::builtin_signature_metadata(name)
            .unwrap_or_else(|| panic!("missing eval signature for {name}"));
        assert_signature_shape(
            name,
            eval_signature(contract),
            &actual.params,
            actual.required_param_count,
            actual.default_param_count,
            actual.variadic.as_deref(),
            &actual.by_ref_params,
        );

        assert!(
            !aot_names.contains(name),
            "{name} reaches AOT through the prelude, not the builtin registry"
        );
        assert!(eval_names.contains(name), "{name} missing from Magician");
    }

    let by_ref_seen = curl_surface
        .iter()
        .filter(|contract| contract.params.iter().any(|param| param.by_ref))
        .map(|contract| contract.name)
        .collect::<Vec<_>>();
    let expected_by_ref = if CURL_SURFACE_LEN == 0 {
        Vec::new()
    } else {
        CURL_BY_REF_CONTRACTS.to_vec()
    };
    assert_eq!(by_ref_seen, expected_by_ref, "curl by-reference surface");

    let stray = eval_names
        .iter()
        .filter(|name| name.starts_with("curl_"))
        .count();
    assert_eq!(
        stray, CURL_SURFACE_LEN,
        "Magician must expose exactly the curl contracts the catalog publishes"
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
