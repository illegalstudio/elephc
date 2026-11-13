//! Purpose:
//! Verifies semantic profiles in the actual cached native REPL bootstrap.
//!
//! Called from:
//! - The focused codegen REPL integration tests.
//!
//! Key details:
//! - Compiler-owned host tests use explicit booleans; submitted code retains user guarantees.

use super::Fixture;

/// Boolean restrictions survive host compilation, native context activation, and recovery.
#[test]
fn test_repl_strict_bools_preserves_guarantees_and_recovers() {
    let fixture = Fixture::new();
    let output = fixture.run("if (1) { echo 'bad'; }\nif (true) { echo 'B'; }\n", &["--strict-bools"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "B");
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(errors.contains("TypeError: --strict-bools"), "{errors}");
}
