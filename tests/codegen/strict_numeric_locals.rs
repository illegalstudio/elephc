//! Purpose:
//! Verifies stable local value contracts independently of conversion and element policies.
//!
//! Called from:
//! - Focused strict local CLI and native codegen tests.
//!
//! Key details:
//! - Overflow still produces a PHP float before local membership rejects the store.
//! - Rejected candidates preserve the previous occupant and balanced ownership.

use crate::support::*;

/// Runtime source establishes stable origins that survive native re-entry, aliases and unset.
#[test]
fn test_strict_runtime_eval_binding_origins_bundle() {
    let source = format!("{}\n{}", include_str!("../fixtures/strict/eval_runtime_binding_origin.php"),
        include_str!("../fixtures/strict/eval_runtime_producer_contracts.php").strip_prefix("<?php").unwrap());
    let source = format!("{source}\n{}", include_str!("../fixtures/strict/eval_conditional_native_origins.php")
        .strip_prefix("<?php").unwrap());
    let source = format!("{source}\n{}", include_str!("../fixtures/strict/eval_native_rebinding_stores.php")
        .strip_prefix("<?php").unwrap());
    let source = format!("{source}\n{}", include_str!("../fixtures/strict/eval_native_copy_origins.php")
        .strip_prefix("<?php").unwrap());
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for strict in [false, true] {
            let mut flags = vec![mode, "--heap-debug"];
            if strict { flags.push("--strict-locals"); }
            let output = StrictFixture::new(&source).capture(&flags);
            let expected = if strict { "Tdouble|2.5|T1|T3.5|open|Tarray|TNULL|T1|new|Tstart|U|T|R|B|" }
                else { "Winteger|1|Wbad|W2|open|Winteger|Wstring|Wbad|new|W1|U|W|R|W|" };
            let conditional = if strict { concat!(
                "Tinteger:1|Wdouble:2.5|Tinteger:1|Wdouble:2.5|",
                "Tinteger:1|Tinteger:1|Wdouble:2.5|Wdouble:2.5|",
                "Tinteger:1|Tinteger:1|Wdouble:2.5|Wdouble:2.5|P|Ddouble:3.5|") }
                else { concat!("Wdouble:2.5|Wdouble:2.5|Wdouble:2.5|Wdouble:2.5|",
                    "Wdouble:2.5|Wdouble:2.5|Wdouble:2.5|Wdouble:2.5|",
                    "Wdouble:2.5|Wdouble:2.5|Wdouble:2.5|Wdouble:2.5|W|Ddouble:3.5|") };
            let current = if strict { concat!("Wdouble:3.5|Tdouble:2.5|",
                "Wdouble:3.5:double:3.5|T:double:2.5|OWstring:after!|OTdouble:2.5|",
                "Warray|Tarray|WNULL:|TNULL:|integer:1|Kdouble:4.5|Tinteger:1|") }
                else { concat!("Wdouble:3.5|Winteger:1|Wdouble:3.5:double:3.5|Winteger:1:integer:1|",
                    "OWstring:after!|OWstring:after!|Warray|Winteger|WNULL:|Winteger:1|integer:1|Kdouble:4.5|Kdouble:4.5|") };
            let copies = if strict { concat!("Wdouble:3.5|Tdouble:2.5|Tdouble:2.5|",
                "Tinteger:1|Tdouble:3.5|integer:1|Tdouble:3.5|integer:1|Tdouble:3.5:double:3.5|T2.5:2.5|Tarray|") }
                else { concat!("Wdouble:3.5|Winteger:1|Winteger:1|",
                    "Wdouble:3.5|Wstring:bad|integer:1|Wstring:bad|integer:1|Wdouble:3.5:integer:1|W2.5:1|Winteger|") };
            assert_eq!(String::from_utf8_lossy(&output.stdout), format!("{expected}{conditional}{current}{copies}"), "{flags:?}: {}",
                String::from_utf8_lossy(&output.stderr));
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

/// Pure literal scopes distinguish a surviving native binding from a new eval origin at native stores.
#[test]
fn test_strict_literal_eval_current_origin_native_stores() {
    let source = include_str!("../fixtures/strict/literal_eval_conditional_native_stores.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for strict in [false, true] {
            let mut flags = vec![mode, "--heap-debug", "--strict-dynamic-code"];
            if strict { flags.push("--strict-locals"); }
            let output = StrictFixture::new(source).capture(&flags);
            let expected = if strict { "AWdouble:3.5|ATdouble:2.5|EWinteger:1|ETinteger:1|Kdouble:4.5|ETinteger:1|" }
                else { "AWdouble:3.5|AWinteger:1|AWinteger:1|AWdouble:3.5|Kdouble:4.5|Kdouble:4.5|" };
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{flags:?}: {}",
                String::from_utf8_lossy(&output.stderr));
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

/// Native stores and typed reference calls preserve eval-created origins and the original owned RHS.
#[test]
fn test_strict_literal_eval_preserves_origins_during_native_stores() {
    let source = include_str!("../fixtures/strict/literal_eval_native_store_origin.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for bridge in [false, true] {
            let source = if bridge { format!("{source}\n$other = 1; eval('unset($other);');") }
                else { source.to_string() };
            let mut flags = vec![mode, "--heap-debug", "--strict-locals"];
            if !bridge { flags.push("--strict-dynamic-code"); }
            let output = StrictFixture::new(&source).capture(&flags);
            assert_eq!(String::from_utf8_lossy(&output.stdout), concat!(
                "Tdouble:2.5|Tdouble:3.5|OTdouble:2.5|Tarray|",
                "integer:1|2.5:3.5|Tdouble:4.5|Tarray:ok|"), "{flags:?}: {}",
                String::from_utf8_lossy(&output.stderr));
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
        let permissive = StrictFixture::new("<?php eval('$created=2.5;'); $created=$argc; echo gettype($created), ':', $created;")
            .capture(&[mode, "--heap-debug"]);
        assert_eq!(String::from_utf8_lossy(&permissive.stdout), "integer:1");
        assert!(String::from_utf8_lossy(&permissive.stderr).contains("HEAP DEBUG: leak summary: clean"));
    }
}

/// Suspended pure and bridge scopes enumerate exact owners and preserve their origins across GC.
#[test]
fn test_strict_literal_eval_origins_survive_generator_suspension_and_gc() {
    let source = include_str!("../fixtures/strict/literal_eval_suspended_scope.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for strict in [false, true] {
            for bridge in [false, true] {
                let source = if bridge { format!("{source}\n$other = 1; eval('unset($other);');") }
                    else { source.to_string() };
                let mut flags = vec![mode, "--heap-debug"];
                if strict { flags.push("--strict-locals"); }
                if !bridge { flags.push("--strict-dynamic-code"); }
                let output = StrictFixture::new(&source).capture(&flags);
                assert_eq!(String::from_utf8_lossy(&output.stdout), if strict { "1T" } else { "1W" },
                    "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
                assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                    "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
            }
        }
    }
}

/// A reserved native slot is not an imported binding until its source path actually initializes it.
#[test]
fn test_strict_literal_eval_preserves_origins_across_native_observation() {
    let source = include_str!("../fixtures/strict/literal_eval_native_origin.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for strict in [false, true] {
            for bridge in [false, true] {
                let source = if bridge { format!("{source}\n$other = 1; eval('unset($other);');") }
                    else { source.to_string() };
                let mut flags = vec![mode, "--heap-debug"];
                if strict { flags.push("--strict-locals"); }
                if !bridge { flags.push("--strict-dynamic-code"); }
                let output = StrictFixture::new(&source).capture(&flags);
                let expected = if strict { "integer:Tinteger:1|changedW|T|" }
                    else { "integer:Wstring:bad|changedW|changedW|" };
                assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{flags:?}: {}",
                    String::from_utf8_lossy(&output.stderr));
                assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                    "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
            }
        }
    }
}

/// Shared fragment bodies retain fresh, imported and explicit mixed producer origins per activation.
#[test]
fn test_strict_literal_eval_establishes_fresh_binding_origins() {
    let source = include_str!("../fixtures/strict/eval_fresh_binding_origin.php");
    let retirement = include_str!("../fixtures/strict/eval_fresh_binding_retirement.php")
        .strip_prefix("<?php").unwrap();
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for strict in [false, true] {
            for bridge in [false, true] {
                let source = if bridge { format!("{source}\n{retirement}") } else { source.to_string() };
                let mut flags = vec![mode, "--heap-debug"];
                if strict { flags.push("--strict-locals"); }
                if !bridge { flags.push("--strict-dynamic-code"); }
                let output = StrictFixture::new(&source).capture(&flags);
                let expected = if strict { "T|changedO|changedM|N|okA|T|" }
                    else { "changedW|changedO|changedM|W|okW|changedW|" };
                let tail = if bridge { if strict { "S|new|new|" } else { "W|new|new|" } } else { "" };
                assert_eq!(String::from_utf8_lossy(&output.stdout), format!("{expected}{tail}"),
                    "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
                assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                    "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
            }
        }
    }
}

/// Every literal assignment validates before later effects while earlier accepted writes remain visible.
#[test]
fn test_strict_literal_eval_checks_each_write_before_later_effects() {
    let source = include_str!("../fixtures/strict/literal_eval_eager_publication.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for strict in [false, true] {
            for bridge in [false, true] {
                let source = if bridge { format!("{source}\n$other = 1; eval('unset($other);');") }
                    else { source.to_string() };
                let mut flags = vec![mode, "--heap-debug"];
                if strict { flags.push("--strict-locals"); }
                if !bridge { flags.push("--strict-dynamic-code"); }
                let output = StrictFixture::new(&source).capture(&flags);
                assert!(output.status.success(), "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
                let expected = if strict { "Tinteger:1|SWinteger:2|ATinteger:2|OTinteger:1|ATarray:next|" }
                    else { "SWinteger:2|SWinteger:2|ASWstring:bad|OSWinteger:2|ASWinteger:3|" };
                assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{flags:?}: {}",
                    String::from_utf8_lossy(&output.stderr));
                assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                    "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
            }
        }
    }
}

/// Scope finalization consumes its loaded string copy after normal and rejected publication.
#[test]
fn test_eval_scope_finalizer_balances_loaded_string_owners() {
    let source = include_str!("../fixtures/strict/literal_eval_scope_string_owners.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for strict in [false, true] {
            let mut flags = vec![mode, "--heap-debug"];
            if strict { flags.push("--strict-locals"); }
            let output = StrictFixture::new(source).capture(&flags);
            assert!(output.status.success(), "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
            assert_eq!(String::from_utf8_lossy(&output.stdout), "after|after|after|after|");
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
        let rejected = r#"<?php $value = 1;
try { eval('$value="before"; $value="after";'); echo 'W'; }
catch (TypeError $error) { echo 'T'; }
echo gettype($value), ':', $value;
"#;
        let output = StrictFixture::new(rejected).capture(&[mode, "--strict-locals", "--heap-debug"]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "Tinteger:1", "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
            "{mode}: {}", String::from_utf8_lossy(&output.stderr));
    }
}

/// Native and bridge-backed literal eval retain scalar contracts, references and exact owners.
#[test]
fn test_strict_literal_eval_preserves_native_binding_contracts() {
    let source = include_str!("../fixtures/strict/literal_eval_binding_contracts.php");
    assert_eq!(StrictFixture::new("<?php $value = 1; eval('$value = 2.5;'); echo gettype($value), ':', $value;")
        .run(&["--strict-dynamic-code"]), "double:2.5");
    let expected = concat!("Tinteger:1|Tinteger:1:1|3:3|double:2.5|double:2.5|",
        "Tboolean:1:1|Tstring:kept:kept|Tdouble:2.5:2.5|TNULL::|");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for bridge in [false, true] {
            let source = if bridge { format!("{source}\n$value = 1; eval('unset($value); $value = 2.5;'); echo gettype($value), ':', $value, '|';") }
                else { source.to_string() };
            let mut flags = vec!["--strict-locals", "--heap-debug", mode];
            if !bridge { flags.push("--strict-dynamic-code"); }
            let output = StrictFixture::new(&source).capture(&flags);
            assert!(output.status.success(), "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
            let expected = if bridge { format!("{expected}double:2.5|") } else { expected.to_string() };
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

/// Mutating conversion validates the original binding, including aliases and named argument effects.
#[test]
fn test_strict_settype_preserves_local_contracts_and_owned_outputs() {
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        let output = StrictFixture::new(include_str!("../fixtures/strict/local_settype_contracts.php"))
            .capture(&["--strict-locals", "--heap-debug", mode]);
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), concat!(
            "Tboolean:1|Tinteger:1|Tboolean:1|Wboolean:1|Tboolean:1|Tboolean:1|Wboolean:1|",
            "STboolean:|integer:1|TNULL:|Tarray|"), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
            "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        for (source, expected) in [
            (include_str!("../fixtures/strict/local_settype_boolean.php"), "Winteger|1"),
            (include_str!("../fixtures/strict/local_settype_integer.php"), "Wstring|1"),
        ] {
            let output = StrictFixture::new(source).capture(&["--heap-debug", mode]);
            assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

/// Valid scalar, nominal and container replacements keep exact values and balanced ownership.
#[test]
fn test_strict_local_value_contracts_accept_valid_reference_and_union_members() {
    let source = include_str!("../fixtures/strict/local_binding_members.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        let output = StrictFixture::new(source).capture(&["--strict-locals", "--heap-debug", mode]);
        assert_eq!(String::from_utf8_lossy(&output.stdout),
            "NULL|after|boolean||Tafter|TNULL|LocalValueChild|DTLocalValueBase|TLocalValueChild|next|Tarray|next|boolean|TNULL|next|2|",
            "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
            "{mode}: {}", String::from_utf8_lossy(&output.stderr));
    }
}

/// Original value kinds and nominal contracts survive dynamic stores and reference calls.
#[test]
fn test_strict_local_value_contracts_guard_original_kinds() {
    let source = include_str!("../fixtures/strict/scalar_binding_contracts.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for (strict, expected) in [
            (false, "Winteger|2|Winteger|2|Winteger|2|WOtherScalarBinding|Winteger|array|next|Winteger|2|integer|2|"),
            (true, "Tstring|before|Tboolean|1|TNULL||TOriginalScalarBinding|Tarray|array|next|Tboolean|1|integer|2|"),
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

/// Opaque eval validates an original native numeric binding before replacing its shared child.
#[test]
fn test_strict_numeric_locals_native_contract_opaque_eval_bundle() {
    let source = include_str!("../fixtures/strict/native_eval_local_contract.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for (strict, expected) in [
            (false, "Wdouble|2.5|NNULL||Iinteger|9007199254740993|Rdouble|2.5|Fdouble|2.5|UWstring|bad|Odouble|2.5|"),
            (true, "Tinteger|1|NNULL||Iinteger|9007199254740993|Rdouble|2.5|Fdouble|2.5|UTdouble|2.5|Odouble|2.5|"),
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

/// Reference contracts survive calls and unwind while ending at unset and preserving explicit kinds.
#[test]
fn test_strict_numeric_reference_contract_lifetimes_and_explicit_types() {
    let source = include_str!("../fixtures/strict/numeric_reference_lifetimes.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for (strict, expected) in [
            (false, "3.5|Wdouble|2.5|WE|2.5|Wstring|bad|2.5|Wdouble|2.5|WNULL||Wdouble|2.5|"),
            (true, "3.5|Tinteger|1|TE|2.5|Tdouble|2.5|2.5|Tinteger|1|WNULL||Tinteger|1|"),
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

/// Untyped reference parameters validate the caller's original numeric contract before writing.
#[test]
fn test_strict_numeric_locals_keep_caller_contract_in_untyped_reference() {
    let source = include_str!("../fixtures/strict/native_untyped_local_contract.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for (strict, expected) in [
            (false, "Wdouble|2.5|Winteger|3|Winteger|9007199254740993|"),
            (true, "Tinteger|1|Winteger|3|Winteger|9007199254740993|"),
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
        "Quantity requires an integer or null\nQuantity: 2\nQuantity: 3\nQuantity: not tracked\nThe integer binding kept its value\nArithmetic result: double\nFresh measurement: 2.5\nThe referenced quantity kept its integer value\nThe label kept its string value\nThe conversion kept the label value\nThe literal eval kept the label value\nThe eval-created quantity kept its integer type\nThe runtime-created quantity kept its integer type\nRuntime measurement: 2.5\nThe conditional quantity kept its integer type\nFresh conditional quantity: 2.5\nRuntime quantity: 3.5\nThe runtime quantity kept its float value\nRuntime quantity: 2.5\nThe copied quantity kept its float value\nCopied quantity: 2.5\nThe callback kept its callable contract\nQuantity: 3\nThe stream kept its resource contract\nThe selected binding kept its original kind\nSelected value: 1\nSelected value: ready\nCurrent value: reset\nCurrent value: 1\nRecreated reference: ready\nReference argument: ready\nReference label: Quantity\nThe snapshot kept its original kind\nSnapshot original: 1\nSnapshot value: ready\nSnapshot original: pending\nGlobal snapshot: 1\nGlobal quantity: 2\nStored quantity: not tracked\nStored quantity: 1\nDetached quantity: not tracked\nThe global quantity kept its integer type\nRetained global quantity: 2\n");
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

/// Ordinary source diagnostics survive constant guards consumed during loader discovery.
#[test]
fn test_strict_locals_cli_rejects_discarded_top_level_sources() {
    for tail in [
        "if(false) {$value='bad';}",
        "if(true) {} else {$value='bad';}",
        "if(true) {spl_autoload_register(function($name) {require $name;});} else {$value='bad';}",
        "if(false) {spl_autoload_register(function($name) {require $name;}); $value='bad';}",
    ] {
        let fixture = StrictFixture::new(&format!("<?php declare(strict_types=0); $value=1; {tail}"));
        for optimize in ["--ir-opt=off", "--ir-opt=on"] {
            fixture.assert_error(&["--strict-locals", optimize], "--strict-locals");
        }
    }
}

/// Selected loader setup still discovers classes without executing inactive ordinary source.
#[test]
fn test_strict_locals_cli_keeps_guarded_autoload_execution() {
    let fixture = StrictFixture::new(r#"<?php
if(false) {
    spl_autoload_register(function($name) { require __DIR__ . '/missing/' . $name . '.php'; });
    echo 'dead';
} elseif(true) {
    spl_autoload_register(function($name) { require __DIR__ . '/lib/' . $name . '.php'; });
    echo 'live|';
} else { echo 'dead'; }
$value = new SourceGuarded();
echo $value->tag();
"#);
    fixture.file("lib/SourceGuarded.php", "<?php class SourceGuarded {public function tag(): string {return 'loaded';}}");
    for optimize in ["--ir-opt=off", "--ir-opt=on"] {
        let output = fixture.capture(&["--strict-locals", optimize, "--heap-debug"]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "live|loaded");
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"));
    }
}
