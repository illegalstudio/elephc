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
fn declared_builtin_registry_derives_regex_metadata() {        assert_eq!(
            eval_declared_builtin_param_names("preg_match"),
            Some(["pattern", "subject", "matches", "flags"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("preg_match", 2),
            Some(EvalBuiltinDefaultValue::EmptyArray)
        );
        assert_eq!(
            eval_declared_builtin_default_value("preg_match_all", 3),
            Some(EvalBuiltinDefaultValue::Int(0))
        );
        assert_eq!(
            eval_builtin_signature_shape("preg_match").map(|shape| shape.by_ref_params),
            Some(["matches"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("preg_replace_callback"),
            Some(["pattern", "callback", "subject"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("preg_split", 2),
            Some(EvalBuiltinDefaultValue::Int(-1))
        );

}
