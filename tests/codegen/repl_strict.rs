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

/// Arithmetic applies to submitted code, survives the cached host and recovers after a caught error.
#[test]
fn test_repl_strict_arithmetic_public_profile_and_recovery() {
    let fixture = Fixture::new();
    let output = fixture.run("echo is_float(4 / 2) ? 'F' : 'X';\ntry { echo '2' + 1; } catch (TypeError $error) { echo 'T'; }\n", &["--strict-arithmetic"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "FT");
}

/// The public overflow host keeps a failed update's original value available after recovery.
#[test]
fn test_repl_strict_overflow_public_profile_and_recovery() {
    let fixture = Fixture::new();
    let output = fixture.run("try { $value = PHP_INT_MAX; $value++; } catch (ArithmeticError $error) { echo 'A'; }\necho $value === PHP_INT_MAX ? 'K' : 'X';\n", &["--strict-overflow"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "AK");
}
