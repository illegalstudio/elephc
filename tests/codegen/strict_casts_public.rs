//! Purpose:
//! Verifies checked explicit conversions through public compiler and runtime entry points.
//!
//! Called from:
//! - Casts admission checks after native reference and Reflection publication parity.
//!
//! Key details:
//! - Native and runtime-selected source share exact decimal and scalar-domain expectations.
//! - Explicit casts remain independent of implicit coercion and arithmetic policies.

use crate::support::*;

/// Public native and opaque casts preserve exact results, contained failures and owned sources.
#[test]
fn test_strict_casts_public_native_and_eval_domains_bundle() {
    let source = include_str!("../fixtures/strict/cast_values.php");
    let opaque = source.trim_start_matches("<?php")
        .replace("checkedCast", "opaqueCast").replace("CheckedCastObject", "OpaqueCastObject")
        .replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("{source}\n$fragment = $argc > 0 ? '{opaque}' : ''; eval($fragment);");
    let expected = "9223372036854775807|-9223372036854775808|9223372036854775807|0|0|-7|10|FIN|V.V.V.V.V.V.RRRRAO|7|-7|";
    for optimization in ["--ir-opt=off", "--ir-opt=on"] {
        let output = StrictFixture::new(&source).capture(&["--strict-casts", optimization, "--heap-debug"]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected.repeat(2), "{optimization}: {}",
            String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
            "{optimization}: {}", String::from_utf8_lossy(&output.stderr));
    }
}

/// Proven invalid casts fail before folding; independent explicit and implicit controls remain valid.
#[test]
fn test_strict_casts_public_source_evidence_and_independence() {
    for source in ["<?php echo (int)'9223372036854775808';", "<?php echo (float)'12tail';",
        "<?php if (false) { echo (int)INF; }", "<?php echo (int)[1];"] {
        StrictFixture::new(source).assert_error(&["--strict-casts"], "strict-casts");
    }
    assert_eq!(StrictFixture::new("<?php function widened(float $value): float { return $value; } \
        echo widened(1), '|', is_int(6 / 2) ? 'I' : 'F', '|', (int)'7.9', '|', strval(7);")
        .run(&["--strict-casts"]), "1|I|7|7");
    assert_eq!(StrictFixture::new("<?php echo (int)'12tail';").run(&["--no-strict-casts"]), "12");
}

/// Runtime string-to-float casts retain the sign bit of an integer-shaped negative zero.
#[test]
fn test_strict_casts_public_runtime_string_float_preserves_negative_zero() {
    let source = r#"<?php
function checkedRuntimeFloat(mixed $value): float { return (float)$value; }
$value = $argc > 0 ? '-0' : '0';
echo json_encode(checkedRuntimeFloat($value));
gc_collect_cycles();
"#;
    for optimization in ["--ir-opt=off", "--ir-opt=on"] {
        let output = StrictFixture::new(source).capture(&[
            "--strict-casts", optimization, "--heap-debug",
        ]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "-0", "{optimization}: {}",
            String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
            "{optimization}: {}", String::from_utf8_lossy(&output.stderr));
    }
}

/// Runtime string-to-float casts reject bytes after an embedded NUL instead of truncating them.
#[test]
fn test_strict_casts_public_runtime_string_float_rejects_embedded_nul() {
    let source = r#"<?php
function checkedRuntimeFloatWithNul(mixed $value): float { return (float)$value; }
$value = $argc > 0 ? "12\0junk" : '0';
try {
    echo checkedRuntimeFloatWithNul($value);
} catch (ValueError $error) {
    echo 'V';
}
gc_collect_cycles();
"#;
    for optimization in ["--ir-opt=off", "--ir-opt=on"] {
        let output = StrictFixture::new(source).capture(&[
            "--strict-casts", optimization, "--heap-debug",
        ]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "V", "{optimization}: {}",
            String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
            "{optimization}: {}", String::from_utf8_lossy(&output.stderr));
    }
}

/// Public checked casts reach descriptors, Reflection, callback arrays and native/eval re-entry.
#[test]
fn test_strict_casts_public_callable_hosts_bundle() {
    let source = r#"<?php
function nativeHostCast(mixed $value): int { return (int)$value; }
$cast = nativeHostCast(...); $name = $argc > 0 ? 'nativeHostCast' : 'nativeHostCast';
try { $cast('junk'); } catch (ValueError $error) { echo 'A'; unset($error); }
try { $name('junk'); } catch (ValueError $error) { echo 'B'; unset($error); }
try { (new ReflectionFunction('nativeHostCast'))->invoke('junk'); } catch (ValueError $error) { echo 'C'; unset($error); }
try { (new ReflectionFunction('nativeHostCast'))->invokeArgs(['junk']); } catch (ValueError $error) { echo 'D'; unset($error); }
try { call_user_func('nativeHostCast', 'junk'); } catch (ValueError $error) { echo 'E'; unset($error); }
try { call_user_func_array('nativeHostCast', ['junk']); } catch (ValueError $error) { echo 'F'; unset($error); }
try { array_map('nativeHostCast', ['junk']); } catch (ValueError $error) { echo 'G'; unset($error); }
echo '|';
$fragment = $argc > 0 ? '
function evalHostCast(mixed $value): int { return (int)$value; }
$evalCast = evalHostCast(...);
try { nativeHostCast("junk"); } catch (ValueError $error) { echo "N"; unset($error); }
try { evalHostCast("junk"); } catch (ValueError $error) { echo "E"; unset($error); }
try { $cast("junk"); } catch (ValueError $error) { echo "C"; unset($error); }
try { $evalCast("junk"); } catch (ValueError $error) { echo "L"; unset($error); }
try { (new ReflectionFunction("nativeHostCast"))->invoke("junk"); } catch (ValueError $error) { echo "R"; unset($error); }
try { call_user_func("evalHostCast", "junk"); } catch (ValueError $error) { echo "U"; unset($error); }
echo "|", $cast("7.9"), ":", $evalCast("-7.9");
' : '';
eval($fragment);
gc_collect_cycles();
"#;
    for optimization in ["--ir-opt=off", "--ir-opt=on"] {
        let output = StrictFixture::new(source).capture(&["--strict-casts", optimization, "--heap-debug"]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "ABCDEFG|NECLRU|7:-7", "{optimization}: {}",
            String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
            "{optimization}: {}", String::from_utf8_lossy(&output.stderr));
    }
}

/// The public scalar contract emits through every supported standalone-library ABI.
#[test]
fn test_strict_casts_public_library_emission_all_targets() {
    let fixture = StrictFixture::new(include_str!("../fixtures/strict/cast_values.php"));
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        fixture.assert_compiles(&["--strict-casts", "--emit-asm", "--emit", "staticlib", "--target", name]);
    }
}

/// The example demonstrates checked domains without enabling an implicit-conversion restriction.
#[test]
fn test_strict_casts_public_example_values_and_domain_error() {
    let source = include_str!("../../examples/strict-casts/main.php");
    let output = StrictFixture::new(source).capture(&["--strict-casts", "--heap-debug"]);
    assert_eq!(String::from_utf8_lossy(&output.stdout), concat!(
        "Count: 42\nExact maximum: 9223372036854775807\n",
        "Count requires a numeric string\nAmount: 125\n"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
        "{}", String::from_utf8_lossy(&output.stderr));
}
