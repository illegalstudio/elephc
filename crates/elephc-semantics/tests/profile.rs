//! Purpose:
//! Tests strict option independence, explicit overrides, and ABI decoding.
//!
//! Called from:
//! - Cargo's elephc-semantics profile integration-test binary.
//!
//! Key details:
//! - Exercises every catalogued feature without native compiler or runtime dependencies.

use elephc_semantics::{SemanticProfile, StrictFeature, StrictRequests, FEATURES};

/// Resolves a sequence of recognized strict CLI arguments.
fn resolve(args: &[&str]) -> SemanticProfile {
    let mut requests = StrictRequests::default();
    for arg in args { assert!(requests.apply_arg(arg), "{arg}"); }
    requests.resolve()
}

/// Every independent flag enables only its own guarantee.
#[test]
fn strict_profile_features_are_independent() {
    for spec in FEATURES {
        assert_eq!(resolve(&[&format!("--{}", spec.flag)]).bits(), spec.feature.bit());
    }
    assert_eq!(resolve(&[]), SemanticProfile::PERMISSIVE);
}

/// Specific negative overrides beat the umbrella in either argument order.
#[test]
fn strict_profile_explicit_overrides_umbrella_in_either_order() {
    for spec in FEATURES {
        let disabled = format!("--no-{}", spec.flag);
        let expected = SemanticProfile::all().with(spec.feature, false);
        assert_eq!(resolve(&["--strict", &disabled]), expected);
        assert_eq!(resolve(&[&disabled, "--strict"]), expected);
    }
    assert!(resolve(&["--strict"]).contains(StrictFeature::Opcache));
}

/// Repeating explicit options retains the latest decision for that feature.
#[test]
fn strict_profile_last_explicit_override_wins() {
    assert!(resolve(&["--no-strict-overflow", "--strict-overflow"]).contains(StrictFeature::Overflow));
    assert!(!resolve(&["--strict-overflow", "--no-strict-overflow", "--strict"]).contains(StrictFeature::Overflow));
}

/// PHP-only syntax policy and misspelled options never become strict features.
#[test]
fn strict_profile_does_not_consume_other_options() {
    for arg in ["--php-only", "--strict-php", "--strict-unknown", "--no-strict-unknown", "--strict=1"] {
        assert!(!StrictRequests::default().apply_arg(arg), "{arg}");
    }
}

/// Cache/ABI round trips preserve profiles and reject future unknown bits.
#[test]
fn strict_profile_encoding_rejects_unknown_bits() {
    for bits in 0..=SemanticProfile::all().bits() {
        let profile = SemanticProfile::from_bits(bits).unwrap();
        assert_eq!(profile.bits(), bits);
    }
    assert_eq!(SemanticProfile::from_bits(1u64 << 63).unwrap_err().unknown_bits, 1u64 << 63);
}
