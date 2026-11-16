//! Purpose:
//! Guards the split between the compiler's full parser and the tokenizer validator.
//!
//! Called from:
//! - The Rust integration test harness (`cargo test`).
//!
//! Key details:
//! - `elephc-syntax` keeps the feature for cfg compatibility, but only the
//!   `elephc-syntax-validate` wrapper may compile the parser with it enabled.
//! - Enabling it on the compiler's dependency silently strips assignment lowering while
//!   still compiling, so the manifest is checked rather than trusted.

use std::process::Command;

/// Resolves the workspace dependency graph and rejects a full-parser feature leak.
#[test]
fn compiler_syntax_dependency_never_enables_tokenizer_validation() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps", "--offline"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata runs");
    assert!(output.status.success(), "cargo metadata failed");
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("cargo metadata is JSON");
    for package in metadata["packages"].as_array().expect("packages array") {
        let package_name = package["name"].as_str().unwrap_or_default();
        for dependency in package["dependencies"].as_array().expect("dependencies array") {
            if dependency["name"] != "elephc-syntax" {
                continue;
            }
            let features = dependency["features"].as_array().expect("features array");
            assert!(
                !features.iter().any(|feature| feature == "tokenizer-validation"),
                "{package_name} enables tokenizer-validation on the compiler parser"
            );
        }
    }
}
