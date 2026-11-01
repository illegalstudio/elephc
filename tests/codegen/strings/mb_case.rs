//! Purpose:
//! End-to-end tests for mbstring's `mb_strtoupper()` and `mb_strtolower()` on the native
//! compilation path.
//!
//! Called from:
//! - `cargo test --test codegen_tests mb_strto` through Rust's test harness.
//!
//! Key details:
//! - Every expected value was captured from php 8.5.10 with mbstring.
//! - Results are asserted through `bin2hex()` wherever invalid bytes or a non-UTF-8
//!   encoding are involved, so the assertions do not depend on the harness's encoding.
//! - The conversion runs in the `elephc-iconv` bridge; these fixtures prove the call
//!   staging, the string-returning runtime entry, and the dynamic `ValueError`.

use crate::support::*;

/// Verifies ASCII text maps letters only and leaves everything else alone.
#[test]
fn test_mb_strtoupper_and_mb_strtolower_ascii() {
    let out = compile_and_run(
        "<?php echo mb_strtoupper('Hello, World! 123'), '|', mb_strtolower('Hello, World! 123'), '|', mb_strtoupper(''), '|';",
    );
    assert_eq!(out, "HELLO, WORLD! 123|hello, world! 123||");
}

/// Verifies full (SpecialCasing) mappings, including expansions and digraphs.
#[test]
fn test_mb_strtoupper_full_case_mapping() {
    let out = compile_and_run(
        "<?php echo mb_strtoupper('straße éà ǆ ﬁ'), '|', mb_strtolower('İSTANBUL'), '|', mb_strtoupper('ŉ');",
    );
    assert_eq!(out, "STRASSE ÉÀ Ǆ FI|i̇stanbul|ʼN");
}

/// Verifies mbstring's final-sigma rule when lowercasing Greek.
#[test]
fn test_mb_strtolower_final_sigma() {
    let out = compile_and_run(
        "<?php echo mb_strtolower('ΣΑΣ ΑΣ.Α Σ'), '|', mb_strtolower(\"Α'Σ\");",
    );
    assert_eq!(out, "σας ασ.α σ|α'ς");
}

/// Verifies each maximal invalid UTF-8 subpart becomes one `?`.
#[test]
fn test_mb_strtoupper_invalid_utf8() {
    let out = compile_and_run(
        r#"<?php echo mb_strtoupper("a\xffb"), '|', bin2hex(mb_strtoupper("\xed\xa0\x80z\xe2\x82"));"#,
    );
    assert_eq!(out, "A?B|3f3f3f5a3f");
}

/// Verifies explicit UTF-8 aliases, a null encoding, and a runtime-null encoding.
#[test]
fn test_mb_strtoupper_utf8_aliases_and_null_encoding() {
    let out = compile_and_run(
        r#"<?php
echo mb_strtoupper("héllo", "UTF-8"), "|";
echo mb_strtoupper("héllo", "utf8"), "|";
echo mb_strtoupper("héllo", null), "|";
echo mb_strtolower(string: "HÉLLO", encoding: null), "|";
$encoding = $argc > 5 ? "UTF-8" : null;
echo mb_strtolower("ÀB", $encoding), "|";
function upper(string $text, ?string $encoding = null): string {
    return mb_strtoupper($text, $encoding);
}
echo upper("ça"), "|", upper("ça", "UTF-8");"#,
    );
    assert_eq!(out, "HÉLLO|HÉLLO|HÉLLO|héllo|àb|ÇA|ÇA");
}

/// Verifies a single-byte encoding decodes, maps, and re-encodes through mbstring's table.
#[test]
fn test_mb_strtoupper_iso_8859_1_round_trip() {
    let out = compile_and_run(
        r#"<?php
echo bin2hex(mb_strtoupper("\xe9t\xe9", "ISO-8859-1")), "|";
echo bin2hex(mb_strtoupper("\xff\xdf", "latin1")), "|";
echo bin2hex(mb_strtolower("I\xdd", "ISO-8859-9"));"#,
    );
    assert_eq!(out, "c954c9|3f5353|fd69");
}

/// Verifies an unknown encoding throws a catchable `ValueError` quoting the caller's name.
#[test]
fn test_mb_strtoupper_unknown_encoding_throws_value_error() {
    let out = compile_and_run(
        r#"<?php
try {
    mb_strtoupper("a", "nope");
} catch (ValueError $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
$name = $argc > 5 ? "UTF-8" : "";
try {
    mb_strtolower("a", $name);
} catch (\ValueError $e) {
    echo $e->getMessage(), "\n";
}
try {
    echo mb_strtolower("a", "UTF_8");
} catch (Throwable $e) {
    echo get_class($e), "|", $e->getCode(), "\n";
}
echo "done";"#,
    );
    assert_eq!(
        out,
        "ValueError: mb_strtoupper(): Argument #2 ($encoding) must be a valid encoding, \"nope\" given\n\
mb_strtolower(): Argument #2 ($encoding) must be a valid encoding, \"\" given\n\
ValueError|0\n\
done"
    );
}

/// Verifies both builtins are visible to `function_exists()`, case-insensitively and namespaced.
#[test]
fn test_mb_strtoupper_function_exists_and_call_forms() {
    let out = compile_and_run(
        r#"<?php
namespace App;
var_dump(function_exists('mb_strtoupper'), function_exists('MB_STRTOLOWER'));
echo MB_STRTOUPPER("é"), \mb_strtolower("É"), mb_strtoupper("ü");"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\nÉéÜ");
}

/// Verifies gradual (mixed) subjects are coerced like PHP scalar strings.
#[test]
fn test_mb_strtoupper_gradual_subject() {
    let out = compile_and_run(
        r#"<?php
$values = ["é", 12, 1.5, true];
foreach ($values as $value) {
    echo mb_strtoupper($value), ",";
}"#,
    );
    assert_eq!(out, "É,12,1.5,1,");
}
