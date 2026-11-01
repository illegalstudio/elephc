//! Purpose:
//! Registry metadata tests for regex builtin signatures and defaults.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Assertions use registry metadata APIs rather than dispatcher literals.

use super::*;

/// Verifies migrated builtin metadata for this registry area.
#[test]
fn declared_builtin_registry_derives_regex_metadata() {
    let public_match_params = ["pattern", "subject", "matches", "flags", "offset"];
    assert_eq!(
        eval_declared_builtin_param_names("preg_match"),
        Some(public_match_params.as_slice())
    );
    assert_eq!(
        eval_declared_builtin_param_names("preg_match_all"),
        Some(public_match_params.as_slice())
    );
    assert_eq!(
        eval_declared_builtin_default_value("preg_match", 2),
        Some(EvalBuiltinDefaultValue::Null)
    );
    assert_eq!(
        eval_declared_builtin_default_value("preg_match_all", 2),
        Some(EvalBuiltinDefaultValue::Null)
    );
    assert_eq!(
        eval_declared_builtin_default_value("preg_match", 4),
        Some(EvalBuiltinDefaultValue::Int(0))
    );
    assert_eq!(
        eval_declared_builtin_default_value("preg_match_all", 4),
        Some(EvalBuiltinDefaultValue::Int(0))
    );
    assert_eq!(
        eval_builtin_signature_shape("preg_match").map(|shape| shape.by_ref_params),
        Some(["matches"].as_slice())
    );
    assert_eq!(
        eval_declared_builtin_param_names("preg_replace"),
        Some(["pattern", "replacement", "subject", "limit", "count"].as_slice())
    );
    assert_eq!(
        eval_declared_builtin_default_value("preg_replace", 3),
        Some(EvalBuiltinDefaultValue::Int(-1))
    );
    assert_eq!(
        eval_builtin_signature_shape("preg_replace").map(|shape| shape.by_ref_params),
        Some(["count"].as_slice())
    );
    assert_eq!(
        eval_declared_builtin_param_names("preg_replace_callback"),
        Some(["pattern", "callback", "subject", "limit", "count", "flags"].as_slice())
    );
    assert_eq!(
        eval_declared_builtin_default_value("preg_replace_callback", 3),
        Some(EvalBuiltinDefaultValue::Int(-1))
    );
    assert_eq!(
        eval_builtin_signature_shape("preg_replace_callback")
            .map(|shape| shape.by_ref_params),
        Some(["count"].as_slice())
    );
    assert_eq!(
        eval_declared_builtin_default_value("preg_split", 2),
        Some(EvalBuiltinDefaultValue::Int(-1))
    );
}
