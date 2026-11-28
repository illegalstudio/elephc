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

/// String rules survive host caching and a failed compound write keeps the original binding.
#[test]
fn test_repl_strict_strings_public_profile_and_recovery() {
    let fixture = Fixture::new();
    let output = fixture.run("$value = 7;\ntry { $value .= 'x'; } catch (TypeError $error) { echo 'T'; }\necho \"[$value]\";\n", &["--strict-strings"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "int(7)\nT[7]");
}

/// Exact implicit bindings reach submitted functions while explicit casts remain intentional.
#[test]
fn test_repl_strict_coercions_public_profile_and_recovery() {
    let fixture = Fixture::new();
    let output = fixture.run("function replExactFloat(float $value): void { echo 'F'; }\ntry { replExactFloat(1); } catch (TypeError $contractError) { echo 'T'; } replExactFloat((float) 1);\n", &["--strict-coercions"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "TF");
}

/// Checked explicit conversions survive host caching and recover from a source-domain failure.
#[test]
fn test_repl_strict_casts_public_domains_and_recovery() {
    let fixture = Fixture::new();
    let output = fixture.run("try { echo (int) 'junk'; } catch (ValueError $castError) { echo 'V'; }\necho (int) '7.9';\n", &["--strict-casts"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "V7");
}

/// Bytewise ordering and exact kinds persist after a rejected loose-equality submission.
#[test]
fn test_repl_strict_comparisons_public_profile_and_recovery() {
    let fixture = Fixture::new();
    let output = fixture.run("echo '10' < '9' ? 'L' : 'X';\n1 == '1';\n\
        try { echo 1 < 2.0; } catch (TypeError $comparisonError) { echo 'T'; }\n\
        echo 1 === '1' ? 'X' : 'N';\n", &["--strict-comparisons"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "LTN");
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid"),
        "{}", String::from_utf8_lossy(&output.stderr));
}

/// Missing reads preserve the public profile and the cached host can execute later input.
#[test]
fn test_repl_strict_initialization_public_profile_and_recovery() {
    let fixture = Fixture::new();
    let output = fixture.run("try { echo $absent; } catch (Error $error) { echo 'I'; }\necho $absent ?? 'quiet';\n$value = null; echo gettype($value);\n", &["--strict-initialization"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "IquietNULL");
}

/// A cached public host preserves checked indexing and executes later input after recovery.
#[test]
fn test_repl_strict_indexing_public_profile_and_recovery() {
    let fixture = Fixture::new();
    let output = fixture.run("$a=[null,7];try{echo $a[9];}catch(OutOfBoundsException $error){echo 'I';}\necho $a[9]??'quiet';\necho gettype($a[0]), ':', $a[1];\n", &["--strict-indexing"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "IquietNULL:7");
}

/// A cached native host retains null checks and explicit probes across later submissions.
#[test]
fn test_repl_strict_nullability_public_profile_and_recovery() {
    let fixture = Fixture::new();
    let output = fixture.run("function requiredReplValue(mixed $value):string{return $value;}\ntry{requiredReplValue(null);}catch(TypeError $error){echo 'N';}\necho null[0]??'quiet';\n", &["--strict-nullability"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "Nquiet");
}
