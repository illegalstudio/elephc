//! Purpose:
//! Checks literal argument cleanup across method dispatch and partial argument evaluation.
//!
//! Called from:
//! - Magician's interpreter expression tests.
//!
//! Key details:
//! - Release logs distinguish literal owners from borrowed scope values and nullsafe skips.

use super::super::super::*;
use super::super::support::*;

/// Releases an evaluated literal when the next argument fails for every method call spelling.
#[test]
fn method_ownership_releases_literals_on_argument_failure() {
    for call in [
        "$owner->accept(731, missing_argument())",
        "$owner?->accept(731, missing_argument())",
        "$owner->$method(731, missing_argument())",
        "$owner?->$method(731, missing_argument())",
        "KnownClass::accept(731, missing_argument())",
        "$class::$method(731, missing_argument())",
    ] {
        let source = format!("$owner = new KnownClass(); $method = 'accept'; $class = 'KnownClass'; return {call};");
        let program = parse_fragment(source.as_bytes()).unwrap();
        let mut values = FakeOps::default();
        let mut scope = ElephcEvalScope::new();
        assert!(execute_program(&program, &mut scope, &mut values).is_err(), "{call}");
        let owners: Vec<_> = values.values.iter().filter(|(_, value)| **value == FakeValue::Int(731))
            .map(|(id, _)| *id).collect();
        assert_eq!(owners.len(), 1, "{call}");
        assert_eq!(values.releases.iter().filter(|value| value.as_ptr() as usize == owners[0]).count(), 1, "{call}");
    }
}

/// Leaves a nullsafe call's arguments unevaluated when the receiver is null.
#[test]
fn method_ownership_nullsafe_skips_argument_evaluation() {
    let program = parse_fragment(b"$owner = null; return $owner?->accept(731, missing_argument());").unwrap();
    let mut values = FakeOps::default();
    let mut scope = ElephcEvalScope::new();
    let result = execute_program(&program, &mut scope, &mut values).unwrap();
    assert_eq!(values.get(result), FakeValue::Null);
    assert!(!values.values.values().any(|value| *value == FakeValue::Int(731)));
}
