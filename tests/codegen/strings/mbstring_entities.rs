//! Purpose:
//! Exercises public numeric-entity builtins through native EIR and opaque eval calls.
//!
//! Called from:
//! - The focused codegen string integration suite.
//!
//! Key details:
//! - Tests preserve binary encodings, map order, PHP warnings, references, and owned inputs.
//! - Map integer casts remain weak even when the outer native caller is strict.

use crate::support::*;

/// Wraps one body for native strict/weak execution or an opaque eval bridge call.
fn program(body: &str, eval: bool, strict: bool) -> String {
    if !eval { return format!("<?php declare(strict_types={}); {body}", usize::from(strict)); }
    let escaped = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php $source = $argc > 0 ? '{escaped}' : ''; eval($source);")
}

/// Preserves map ranges and exposes named, namespaced, case-insensitive, and callable forms.
#[test]
fn test_mbstring_entities_public_calls() {
    let body = r#"
namespace NumericEntities;
$map = [128, 1114111, 0, 4294967295];
echo Mb_EnCoDe_NuMeRiCeNtItY(map: $map, string: "Aé猫😀"), "\n";
$encode = mb_encode_numericentity(...);
$encoded = $encode("Aé猫😀", $map, "UTF-8", true);
echo $encoded, "\n";
echo call_user_func("mb_decode_numericentity", $encoded, $map), "\n";
echo mb_decode_numericentity("&#233 &#x732B; &#X732B;", $map), "\n";
echo mb_encode_numericentity("AB", [65, 65, 1, 255, 65, 66, 2, 255]), "\n";
echo mb_decode_numericentity("&#66;&#68;", [65, 65, 1, 0, 65, 66, 2, 0]), "\n";
echo mb_encode_numericentity("unchanged", []), "\n";
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval, false)),
            "A&#233;&#29483;&#128512;\nA&#xE9;&#x732B;&#x1F600;\nAé猫😀\né 猫 &#X732B;\n&#66;&#68;\nAB\nunchanged\n");
    }
}

/// Converts UTF-16 bytes and embedded NULs without truncating input or output strings.
#[test]
fn test_mbstring_entities_binary_and_substitution() {
    let body = r#"
$map = [0, 1114111, 0, 4294967295];
$binary = "A" . chr(0) . chr(233) . chr(0) . chr(43) . chr(115);
$encoded = mb_encode_numericentity($binary, $map, "UTF-16LE");
echo bin2hex(mb_decode_numericentity($encoded, $map, "UTF-16LE")), "\n";
echo mb_encode_numericentity("A\0B", $map), "\n";
mb_substitute_character(33);
echo mb_encode_numericentity(chr(255) . "é", [128, 1114111, 0, 4294967295]), "\n";
echo mb_decode_numericentity("&#x1F600;&#x110000;", [0, 4294967295, 0, 4294967295], "ASCII"), "\n";
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval, false)), "4100e9002b73\n&#65;&#0;&#66;\n!&#233;\n!!\n");
    }
}

/// Validates the encoding before map size and rejects unsupported elements without Stringable calls.
#[test]
fn test_mbstring_entities_validation_order() {
    let body = r#"
class EntityRejected { public function __toString(): string { echo "unexpected\n"; return "1"; } }
try { mb_encode_numericentity("", [1], "bad"); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { mb_encode_numericentity("", [new EntityRejected()]); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { mb_decode_numericentity("", [1, 2, 3]); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { mb_encode_numericentity("A", [0, 100, new EntityRejected(), 255]); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { mb_decode_numericentity("&#65;", [0, 100, "bad", 255]); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { mb_encode_numericentity("A", [0, 100, [], 255]); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
"#;
    let expected = "mb_encode_numericentity(): Argument #3 ($encoding) must be a valid encoding, \"bad\" given\nmb_encode_numericentity(): Argument #2 ($map) must have a multiple of 4 elements\nmb_decode_numericentity(): Argument #2 ($map) must have a multiple of 4 elements\nmb_encode_numericentity(): Argument #2 ($map) must only be composed of values of type int\nmb_decode_numericentity(): Argument #2 ($map) must only be composed of values of type int\nmb_encode_numericentity(): Argument #2 ($map) must only be composed of values of type int\n";
    for eval in [false, true] { assert_eq!(compile_and_run(&program(body, eval, false)), expected); }
}

/// Uses map integer-conversion warnings in strict native callers and weak eval callers alike.
#[test]
fn test_mbstring_entities_map_warnings() {
    let body = r#"
echo mb_encode_numericentity("A", [null, "100", true, -1]), "\n";
echo mb_encode_numericentity("A", [0, 100, "12bad", 255]), "\n";
echo mb_encode_numericentity("A", [0, 100, 1.5, 255]), "\n";
echo mb_encode_numericentity("A", [0, 100, "1.5bad", 255]), "\n";
echo mb_encode_numericentity("A", [0, 100, 100000000000000000000.0, 4294967295]), "\n";
"#;
    for eval in [false, true] {
        let output = compile_and_run_capture(&program(body, eval, true));
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "&#66;\n&#77;\n&#66;\n&#66;\n&#1661993025;\n");
        assert_eq!(output.stderr, "Warning: A non-numeric value encountered\nDeprecated: Implicit conversion from float 1.5 to int loses precision\nWarning: A non-numeric value encountered\nDeprecated: Implicit conversion from float-string \"1.5bad\" to int loses precision\nWarning: The float 1.0E+20 is not representable as an int, cast occurred\n");
    }
}

/// Reads map references after the encoding argument's Stringable callback updates their storage.
#[test]
fn test_mbstring_entities_outer_reference_callback() {
    for eval in [false, true] {
        let binding = if eval { "$offset = 0; $map = [0, 100, &$offset, 255];" }
            else { "$map = [0, 100, 0, 255]; $offset =& $map[2];" };
        let body = format!(r#"
class EntityEncoding {{
    public ?Closure $change = null;
    public function __toString(): string {{ $change = $this->change; if ($change !== null) {{ $change(); }} return "UTF-8"; }}
}}
{binding}
$encoding = new EntityEncoding();
$encoding->change = function () use (&$offset): void {{ $offset = 1; }};
echo mb_encode_numericentity("A", $map, $encoding), "\n";
echo mb_decode_numericentity("&#66;", $map, $encoding), "\n";
"#);
        assert_eq!(compile_and_run(&program(&body, eval, false)), "&#66;\nA\n");
    }
}

/// Balances map snapshots and copied elements across repeated successful and rejected calls.
#[test]
fn test_mbstring_entities_map_ownership() {
    for eval in [false, true] {
        let source = |count| {
            let repeated = "mb_encode_numericentity($text, $valid); try { mb_decode_numericentity($text, $invalid); } catch (ValueError) {}\n".repeat(count);
            program(&format!("$text = str_repeat(\"A\", 64); $valid = [0, 100, 0, 255]; $invalid = [0, 100, [], 255]; {repeated}"), eval, false)
        };
        let mut residual = Vec::new();
        for count in [1, 24] {
            let output = compile_and_run_with_gc_stats(&source(count));
            assert!(output.success, "{}", output.stderr);
            let (allocated, freed) = parse_gc_stats(&output.stderr);
            residual.push(allocated as i64 - freed as i64);
        }
        assert_eq!(residual[0], residual[1], "entity map ownership grew; eval={eval}");
    }
}
