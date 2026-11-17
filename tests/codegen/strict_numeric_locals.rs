//! Purpose:
//! Verifies stable numeric local bindings independently of coercion and overflow flags.
//!
//! Called from:
//! - Focused strict local CLI and native codegen tests.
//!
//! Key details:
//! - Overflow still produces a PHP float before local membership rejects the store.
//! - Rejected candidates preserve the previous occupant and balanced ownership.

use crate::support::*;

/// Explicit numeric changes are rejected at statement, expression, branch and parameter writes.
#[test]
fn test_strict_numeric_locals_reject_kind_changes_at_source() {
    for source in [
        "<?php $value = $argc; $value = $argc + 0.5;",
        "<?php $value = $argc + 0.5; $value = $argc;",
        "<?php $value = $argc; echo ($value = $argc + 0.5);",
        "<?php $value = $argc; if ($argc > 0) { $value = $argc + 0.5; }",
        "<?php $value = $argc; $other = ++$value; $other = 0.5;",
        "<?php $value = $argc; $value =& $value; $value = $argc + 0.5;",
        "<?php $value = null; $other = $value ?? ($temporary = 1); $temporary = 2.5;",
        "<?php class NumericProbe { public int $missing; } $probe = new NumericProbe(); $other = $probe->missing ?? ($temporary = 1); $temporary = 2.5;",
        "<?php $values = [1]; $other = $values[9] ?? ($temporary = 1); $temporary = 2.5;",
        "<?php function changeNumericParameter(int $value): void { $value = 0.5; } changeNumericParameter($argc);",
        "<?php $value = $argc; $closure = function() use ($value) { $value = 0.5; }; $closure();",
    ] {
        StrictFixture::new(source).assert_error(&["--strict-locals"], "--strict-locals");
        StrictFixture::new(source).assert_compiles(&[]);
    }
}

/// Dynamic assignments and updates validate original kinds before mutating a stable binding.
#[test]
fn test_strict_numeric_locals_native_guards_and_policy_independence() {
    let source = include_str!("../fixtures/strict/numeric_local_contracts.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for (strict, expected) in [
            (false, "Wdouble|9007199254740993|Cdouble|Rdouble|Sdouble|double|integer|1|2|E"),
            (true, "Tinteger|9007199254740993|Ainteger|Pinteger|Qinteger|double|integer|1|2|E"),
        ] {
            let mut flags = vec![mode, "--heap-debug"];
            if strict { flags.push("--strict-locals"); }
            let output = StrictFixture::new(source).capture(&flags);
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{flags:?}: {}",
                String::from_utf8_lossy(&output.stderr));
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

/// Explicit numeric unions and mixed declarations retain the alternatives the source permits.
#[test]
fn test_strict_numeric_locals_preserve_explicit_contracts_and_unset_rebind() {
    let source = r#"<?php
function numericValue(mixed $value): mixed { return $value; }
int|float $number = 1;
$number = numericValue(2.5); echo $number, '|';
$number = numericValue(9007199254740993); echo $number, '|';
mixed $open = 1; $open = 2.5; echo $open, '|';
$fresh = $argc; unset($fresh); $fresh = 2.5; echo $fresh, '|';
function numericScope(int $seed): void {
    $local = $seed;
    $closure = function() { $local = 2.5; return $local; };
    echo $closure(), '|', $local;
}
numericScope($argc);
"#;
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        assert_eq!(StrictFixture::new(source).run(&["--strict-locals", mode]),
            "2.5|9007199254740993|2.5|2.5|2.5|1");
    }
}


/// The documented example keeps integer stores stable while standalone overflow still promotes.
#[test]
fn test_strict_numeric_locals_example() {
    let source = include_str!("../../examples/strict-locals/main.php");
    assert_eq!(StrictFixture::new(source).run(&["--strict-locals"]),
        "Quantity requires an integer or null\nQuantity: 2\nQuantity: 3\nQuantity: not tracked\nThe integer binding kept its value\nArithmetic result: double\nFresh measurement: 2.5\n");
}


/// Rebinding by reference ends the displaced contract, and diagnostic-only stores create no binding.
#[test]
fn test_strict_numeric_locals_rebind_and_nonexecuting_store_contracts() {
    let source = include_str!("../fixtures/strict/numeric_local_rebinds.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for flags in [vec![mode, "--heap-debug"], vec!["--strict-locals", mode, "--heap-debug"]] {
            let output = StrictFixture::new(source).capture(&flags);
            assert_eq!(String::from_utf8_lossy(&output.stdout), "2.5|2.5|1|2.5|1|2.5|1|2.5", "{flags:?}: {}",
                String::from_utf8_lossy(&output.stderr));
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}


/// Nullable numeric writes retain null while updates reject promotion before touching shared cells.
#[test]
fn test_strict_nullable_numeric_locals_preserve_contracts_and_reference_storage() {
    let source = include_str!("../fixtures/strict/nullable_numeric_local_contracts.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for overflow in [false, true] {
            let mut flags = vec!["--strict-locals", "--heap-debug", mode];
            if overflow { flags.push("--strict-overflow"); }
            let failure = if overflow { "A" } else { "T" };
            let expected = format!("Tinteger:1|WN|Winteger:9007199254740993|{failure}integer:9223372036854775807|{failure}integer:-9223372036854775808|N|Sinteger:2|{failure}integer:9223372036854775807|{failure}integer:-9223372036854775808|N|Sinteger:2|2:2:3|2|0|");
            let output = StrictFixture::new(source).capture(&flags);
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{flags:?}: {}",
                String::from_utf8_lossy(&output.stderr));
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

/// Known nullable numeric stores reject float without forbidding null or integer alternatives.
#[test]
fn test_strict_nullable_numeric_locals_known_kinds_and_explicit_null() {
    for source in ["<?php function nullableStore(?int $value): void { $value = 2.5; } nullableStore($argc);",
        "<?php function nullableFloat(?float $value): void { $value = 2; } nullableFloat(2.5);"] {
        StrictFixture::new(source).assert_error(&["--strict-locals"], "--strict-locals");
        StrictFixture::new(source).assert_compiles(&[]);
    }
    let source = "<?php function nullableMember(?int $value): void { $value = null; echo $value === null ? 'N' : 'X'; $value = 9007199254740993; echo $value; } nullableMember($argc);";
    assert_eq!(StrictFixture::new(source).run(&["--strict-locals"]), "N9007199254740993");
}
