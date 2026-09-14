//! Purpose:
//! Exercises mbstring array returns, binary string elements, and encoding metadata.
//!
//! Called from:
//! - The codegen test binary's string module.
//!
//! Key details:
//! - Native and opaque eval fixtures use the same PHP-verified expected results.
//! - Array copies and callable dispatch must preserve independent ownership.

use crate::support::*;

const ARRAY_PROGRAM: &str = r#"$parts = mb_str_split("Aé猫B", 2);
foreach ($parts as $index => $part) { echo $index, "=", bin2hex((string)$part), ";"; }
echo "\n";
$copy = $parts;
$copy[0] = "changed";
echo (string)$parts[0], ":", (string)$copy[0], ":", count($parts), ":";
echo count(mb_str_split("")), ":", count(mb_encoding_aliases("UTF-32BE")), "\n";
$binary = chr(0) . chr(255) . chr(128);
foreach (mb_str_split($binary, 2, "8bit") as $part) { echo bin2hex((string)$part), ":"; }
echo "\n";
$aliases = mb_encoding_aliases("ASCII");
echo count($aliases), ":", (string)$aliases[0], ":", (string)$aliases[10], ":";
$utf = Mb_EnCoDiNg_AlIaSeS("UTF-8" . chr(0) . "ignored");
echo (string)$utf[0], ":";
var_dump(mb_preferred_mime_name("utf8"));
$split = mb_str_split(...);
foreach ($split(...["encoding" => null, "length" => 2, "string" => "猫éB"]) as $part) { echo (string)$part, ":"; }
$alias = mb_encoding_aliases(...);
echo count($alias("ASCII")), ":";
$mime = mb_preferred_mime_name(...);
var_dump($mime("ASCII"));
"#;
const ARRAY_OUTPUT: &str = r#"0=41c3a9;1=e78cab42;
Aé:changed:2:0:0
00ff:80:
11:ANSI_X3.4-1968:csASCII:utf8:string(5) "UTF-8"
猫é:B:11:string(8) "US-ASCII"
"#;

/// Verifies native arrays preserve binary elements, COW copies, and named/spread callable arguments.
#[test]
fn test_mbstring_array_native_operations() {
    assert_eq!(compile_and_run(&format!("<?php namespace ArrayText; {ARRAY_PROGRAM}")), ARRAY_OUTPUT);
}

/// Verifies boxed eval array results support iteration, indexing, and ordinary copy-on-write.
#[test]
fn test_mbstring_array_eval_operations() {
    assert_eq!(compile_and_run(&opaque_eval(ARRAY_PROGRAM)), ARRAY_OUTPUT);
}

/// Keeps eval source opaque without changing PHP string literals inside the test body.
fn opaque_eval(body: &str) -> String {
    let literal = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php $source = $argc > 0 ? '{literal}' : ''; eval($source);")
}

/// Verifies split-length prechecks and metadata errors use catchable PHP diagnostics in both paths.
#[test]
fn test_mbstring_array_validation_order() {
    let body = r#"
try { mb_str_split("a", 0, "invalid"); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { mb_str_split("a", 1073741824, "invalid"); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { mb_str_split("", 1, "invalid"); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { mb_encoding_aliases("invalid"); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { mb_preferred_mime_name("invalid"); } catch (ValueError $e) { echo $e->getMessage(); }
"#;
    let expected = concat!(
        "mb_str_split(): Argument #2 ($length) must be greater than 0\n",
        "mb_str_split(): Argument #2 ($length) is too large\n",
        "mb_str_split(): Argument #3 ($encoding) must be a valid encoding, \"invalid\" given\n",
        "mb_encoding_aliases(): Argument #1 ($encoding) must be a valid encoding, \"invalid\" given\n",
        "mb_preferred_mime_name(): Argument #1 ($encoding) must be a valid encoding, \"invalid\" given"
    );
    assert_eq!(compile_and_run(&format!("<?php {body}")), expected);
    assert_eq!(compile_and_run(&opaque_eval(body)), expected);
}

/// Verifies MIME warnings bypass, and alias lookups share, the AOT/eval deprecation cache.
#[test]
fn test_mbstring_array_metadata_cache_shared_with_eval() {
    let out = compile_and_run_capture(r#"<?php
mb_encoding_aliases("BASE64");
$source = $argc > 0 ? '
mb_preferred_mime_name("UTF-8");
mb_encoding_aliases("BASE64");
var_dump(mb_preferred_mime_name("UTF7-IMAP"));
' : '';
eval($source);
echo mb_strlen("YQ==", "BASE64"), ":";
mb_encoding_aliases("ASCII");
mb_encoding_aliases("BASE64");
"#);
    assert_eq!(out.stdout, "bool(false)\n1:");
    assert_eq!(out.stderr.matches("Handling Base64 via mbstring is deprecated").count(), 2);
    assert_eq!(out.stderr.matches("No MIME preferred name corresponding to \"UTF7-IMAP\"").count(), 1);
}

/// Verifies direct array copies keep independent cells while PHP references retain shared writes.
#[test]
fn test_mbstring_array_eval_copy_lifetimes() {
    let body = r#"
$parts = mb_str_split("猫犬");
$reference =& $parts;
$copy = $parts;
$copy[0] = "a";
$reference[0] = "b";
echo $parts[0], ":", $reference[0], ":", $copy[0], ":";
unset($parts, $reference);
$copy = $copy;
$copy[] = "c";
echo count($copy), ":", $copy[1], ":", $copy[2], ":";
$aliases = mb_encoding_aliases("ASCII");
$other = $aliases;
unset($aliases);
$other[0] = "custom";
echo $other[0], ":", count($other);
"#;
    assert_eq!(compile_and_run(&opaque_eval(body)), "b:b:a:3:犬:c:custom:11");
}

const CHECK_PROGRAM: &str = r#"
class CheckedObject {}
$valid = ["猫", "a" . chr(0) . "é"];
$invalid = ["good", chr(255)];
$nested = ["names" => $valid, "other" => ["A", "B"]];
var_dump(mb_check_encoding($valid), mb_check_encoding($invalid), mb_check_encoding($nested));
var_dump(mb_check_encoding([1, 2, 3]), mb_check_encoding([1.0, -0.0]), mb_check_encoding([true, false]));
var_dump(mb_check_encoding([null, 1]), mb_check_encoding([]));
var_dump(mb_check_encoding([chr(255) => "good"]), mb_check_encoding($invalid, "8bit"));
$copy = $valid; $copy[0] = chr(255);
var_dump(mb_check_encoding($valid), mb_check_encoding($copy));
var_dump(mb_check_encoding([new CheckedObject()]));
$check = mb_check_encoding(...);
var_dump($check(...["encoding" => "UTF-8", "value" => $nested]));
var_dump(Mb_ChEcK_EnCoDiNg(value: "é"), mb_check_encoding(chr(255)));
try { mb_check_encoding($valid, "bad"); } catch (\ValueError $e) { echo $e->getMessage(), "\n"; }
"#;
const CHECK_OUTPUT: &str = "bool(true)\nbool(false)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(false)\nbool(true)\nbool(true)\nbool(false)\nbool(false)\nbool(true)\nbool(true)\nbool(false)\nmb_check_encoding(): Argument #2 ($encoding) must be a valid encoding, \"bad\" given\n";

/// Verifies native encoding checks traverse all array storage shapes without mutating copies.
#[test]
fn test_mbstring_check_native_arrays() {
    assert_eq!(compile_and_run(&format!("<?php namespace CheckedText; {CHECK_PROGRAM}")), CHECK_OUTPUT);
}

/// Verifies opaque eval uses the same recursive engine and array/string argument contract.
#[test]
fn test_mbstring_check_eval_arrays() {
    assert_eq!(compile_and_run(&opaque_eval(CHECK_PROGRAM)), CHECK_OUTPUT);
}

/// Verifies array/string/null unions survive native function boundaries and omitted defaults.
#[test]
fn test_mbstring_check_union_and_request_state() {
    let source = r#"<?php
function check_value(array|string|null $value): bool { return mb_check_encoding($value); }
var_dump(check_value(["猫"]), check_value("é"), check_value(null));
var_dump(mb_check_encoding(chr(255)), mb_check_encoding());
mb_scrub(chr(255));
var_dump(mb_check_encoding(), mb_check_encoding(value: null, encoding: "UTF-8"));
try { mb_check_encoding(null, "bad"); } catch (ValueError) { echo "invalid\n"; }
"#;
    let out = compile_and_run_capture(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nbool(true)\nbool(true)\nbool(false)\nbool(true)\nbool(false)\nbool(false)\ninvalid\n");
    assert_eq!(out.stderr.matches("Calling mb_check_encoding() without argument is deprecated").count(), 4);
}

/// Verifies native and eval share the same conversion-error count for deprecated null checks.
#[test]
fn test_mbstring_check_request_state_shared_with_eval() {
    let body = "var_dump(mb_check_encoding()); mb_scrub(chr(255));";
    let source = format!("<?php var_dump(mb_check_encoding()); $source = $argc > 0 ? '{body}' : ''; eval($source); var_dump(mb_check_encoding());");
    let out = compile_and_run_capture(&source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nbool(true)\nbool(false)\n");
    assert_eq!(out.stderr.matches("Calling mb_check_encoding() without argument is deprecated").count(), 3);
}
