//! Purpose:
//! Integration tests for frozen PHP 8.5.6 stream build-manifest evidence.
//!
//! Called from:
//! - `cargo test --test php_oracle_manifest_tests` through Rust's test harness.
//!
//! Key details:
//! - These tests consume checked-in JSON and never depend on a host PHP installation.
//! - Gate 0 source evidence must cover every supported target without open requirements.

use serde_json::Value;
use std::fs;
use std::path::PathBuf;

const SUPPORTED_TARGETS: &[&str] =
    &["macos-aarch64", "linux-aarch64", "linux-x86_64"];

/// Returns the selected macOS PHP 8.5.6 profile path.
fn selected_manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_oracle/manifests/streams/php-8.5.6")
        .join("macos-aarch64/homebrew-no-ini.json")
}

/// Returns one source-built PHP 8.5.6 profile path.
fn source_manifest_path(target: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_oracle/manifests/streams/php-8.5.6")
        .join(target)
        .join("streams-full.json")
}

/// Parses the selected checked-in profile as JSON.
fn selected_manifest() -> Value {
    let path = selected_manifest_path();
    serde_json::from_slice(
        &fs::read(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

/// Parses one source-built checked-in profile as JSON.
fn source_manifest(target: &str) -> Value {
    let path = source_manifest_path(target);
    serde_json::from_slice(
        &fs::read(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

/// The manifest stays pinned to the exact PHP and php-src revisions from the locked spec.
#[test]
fn stream_manifest_pins_frozen_php_source_and_target() {
    let manifest = selected_manifest();
    assert_eq!(manifest["profile"]["php_release"], "8.5.6");
    assert_eq!(
        manifest["profile"]["php_src_commit"],
        "fcc29c8d6d6ee6f5ba2d941f0a2a6ea6aa6ee633"
    );
    assert_eq!(manifest["profile"]["target"], "macos-aarch64");
    assert_eq!(manifest["oracle"]["php_version"], "8.5.6");
    assert_eq!(manifest["oracle"]["os_family"], "Darwin");
}

/// PHP's observable wrapper, transport, and filter order remains exact for this build.
#[test]
fn stream_manifest_pins_configured_registry_order() {
    let manifest = selected_manifest();
    assert_eq!(
        manifest["surface"]["wrappers"],
        serde_json::json!([
            "https",
            "ftps",
            "compress.zlib",
            "compress.bzip2",
            "php",
            "file",
            "glob",
            "data",
            "http",
            "ftp",
            "phar",
            "zip",
        ])
    );
    assert_eq!(
        manifest["surface"]["transports"],
        serde_json::json!([
            "tcp", "udp", "unix", "udg", "ssl", "tls", "tlsv1.0", "tlsv1.1", "tlsv1.2",
            "tlsv1.3",
        ])
    );
    assert_eq!(
        manifest["surface"]["filters"],
        serde_json::json!([
            "zlib.*",
            "bzip2.*",
            "convert.iconv.*",
            "string.rot13",
            "string.toupper",
            "string.tolower",
            "convert.*",
            "consumed",
            "dechunk",
        ])
    );
}

/// The frozen constants expose php-src values and exclude PR-only internal names.
#[test]
fn stream_manifest_pins_audited_constant_blockers() {
    let manifest = selected_manifest();
    let constants = &manifest["surface"]["constants"];
    for (name, expected) in [
        ("STREAM_CLIENT_PERSISTENT", 1),
        ("STREAM_CLIENT_ASYNC_CONNECT", 2),
        ("STREAM_CLIENT_CONNECT", 4),
        ("FILE_BINARY", 0),
        ("FILE_TEXT", 0),
    ] {
        assert_eq!(
            constants[name],
            serde_json::json!({"type": "int", "value": expected}),
            "{name}"
        );
    }
    for name in [
        "STREAM_FROM_START",
        "STREAM_FROM_CUR",
        "STREAM_FROM_END",
        "STREAM_META_MODIFIED",
        "STREAM_OPTION_CHUNK_SIZE",
    ] {
        assert!(constants.get(name).is_none(), "{name} must stay internal");
    }
}

/// The full php-src user-wrapper callback protocol retains arity and reference evidence.
#[test]
fn stream_source_manifest_pins_user_wrapper_protocol() {
    let manifest = source_manifest("macos-aarch64");
    let callbacks = manifest["wrapper_protocol"]["callbacks"]
        .as_array()
        .expect("wrapper callback array");
    assert_eq!(callbacks.len(), 23);
    let stream_open = callbacks
        .iter()
        .find(|callback| callback["name"] == "stream_open")
        .expect("stream_open callback");
    let invocation = &stream_open["invocations"][0];
    assert_eq!(invocation["arity"], 4);
    let referenced = invocation["arguments"]
        .as_array()
        .expect("stream_open arguments")
        .iter()
        .filter(|argument| argument["by_reference"] == true)
        .map(|argument| argument["position"].as_u64().expect("argument position"))
        .collect::<Vec<_>>();
    assert_eq!(referenced, vec![3]);
}

/// Function and class-method aliases preserve their canonical php-src target.
#[test]
fn stream_source_manifest_pins_alias_relationships() {
    let manifest = source_manifest("macos-aarch64");
    let functions = manifest["surface"]["functions"]
        .as_array()
        .expect("stream function array");
    let fputs = functions
        .iter()
        .find(|function| function["canonical_name"] == "fputs")
        .expect("fputs manifest");
    assert_eq!(fputs["alias_of"], "fwrite");
    let classes = manifest["surface"]["classes"]
        .as_array()
        .expect("stream class array");
    let spl_file = classes
        .iter()
        .find(|class| class["canonical_name"] == "splfileobject")
        .expect("SplFileObject manifest");
    let current_line = spl_file["methods"]
        .as_array()
        .expect("SplFileObject methods")
        .iter()
        .find(|method| method["canonical_name"] == "getcurrentline")
        .expect("SplFileObject::getCurrentLine manifest");
    assert_eq!(current_line["alias_of"], "SplFileObject::fgets");
}

/// The initial host profile cannot be reported as full Gate 0 acceptance.
#[test]
fn stream_manifest_records_gate_zero_as_partial() {
    let manifest = selected_manifest();
    assert_eq!(manifest["gate"]["status"], "partial");
    let open = manifest["gate"]["open_requirements"]
        .as_array()
        .expect("open_requirements array");
    for required in [
        "profile-binary-source-attestation",
        "authoritative-clang-source-reachability",
        "differential-oracle-corpus",
        "elephc-classified-drift-ledger",
    ] {
        assert!(
            open.iter().any(|value| value == required),
            "missing open requirement {required}"
        );
    }
}

/// Every supported target binds a clean source build and all Gate 0 companions.
#[test]
fn stream_source_manifest_attests_build_reachability_corpus_and_drift() {
    for target in SUPPORTED_TARGETS {
        let manifest = source_manifest(target);
        assert_eq!(
            manifest["build"]["binary_source_attestation"],
            "source-build",
            "{target}"
        );
        assert_eq!(manifest["build"]["binary_path"], "sapi/cli/php", "{target}");
        let translation_units = manifest["build"]["compile_capture"]
            ["translation_units"]
            .as_u64()
            .expect("translation unit count");
        assert!(translation_units >= 560, "{target}");
        assert_eq!(
            manifest["build"]["compile_capture"]["unique_translation_units"],
            translation_units,
            "{target}"
        );
        assert!(manifest["reachability"].is_object(), "{target}");
        assert!(
            manifest["companion_evidence"]["corpus_index"].is_object(),
            "{target}"
        );
        assert!(
            manifest["companion_evidence"]["drift_ledger"].is_object(),
            "{target}"
        );
        assert_eq!(manifest["gate"]["status"], "candidate", "{target}");
        assert_eq!(
            manifest["gate"]["open_requirements"],
            serde_json::json!([]),
            "{target}"
        );
    }
}
