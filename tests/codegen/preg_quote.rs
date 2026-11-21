//! Purpose:
//! End-to-end tests for `preg_quote()`, the injected PCRE prelude function.
//!
//! Called from:
//! - `cargo test --test codegen_tests_ext preg_quote`.
//!
//! Key details:
//! - Expected output is PHP 8.5's.
//! - The eval fixture builds its source at runtime so it runs through Magician.

use crate::support::*;

/// The PHP calls every fixture runs, compiled and through eval.
const QUOTE_CALLS: &str = r#"var_dump(preg_quote("Hello.World?(1+1=2)[a]{b}^$|\\:-#/<>!*"));
var_dump(preg_quote("a/b#c", "/"));
var_dump(preg_quote("x\0y", ""));
var_dump(PREG_QUOTE("abc", "bz"));
var_dump(preg_quote("q", null));
"#;

/// PHP 8.5's output for [`QUOTE_CALLS`].
const QUOTE_OUTPUT: &str = r#"string(58) "Hello\.World\?\(1\+1\=2\)\[a\]\{b\}\^\$\|\\\:\-\#/\<\>\!\*"
string(7) "a\/b\#c"
string(6) "x\000y"
string(4) "a\bc"
string(1) "q"
"#;

/// Escapes metacharacters, the delimiter's first byte and NUL; `/` only as the delimiter.
#[test]
fn test_preg_quote_matches_php() {
    let out = compile_and_run(&format!("<?php\n{QUOTE_CALLS}"));
    assert_eq!(out, QUOTE_OUTPUT);
}

/// A quoted literal matches itself as a pattern.
#[test]
fn test_preg_quote_builds_a_literal_pattern() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
$s = "www.example.com/a+b?(c)";
echo preg_match("/^" . preg_quote($s, "/") . "$/", $s), preg_match("/^" . preg_quote($s, "/") . "$/", "wwwXexample.com/a+b?(c)"), "\n";
"#,
    );
    assert_eq!(out.stdout, "10\n");
}

/// A program's own global `preg_quote()` replaces the prelude's, and `preg_grep()` is still
/// injected next to it.
#[test]
fn test_user_preg_quote_keeps_preg_grep() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
function preg_quote(string $str, ?string $delimiter = null): string { return "user"; }
echo preg_quote("a.b"), "|", count(preg_grep("/a/", ["a", "b"])), "\n";
"#,
    );
    assert_eq!(out.stdout, "user|1\n");
}

/// Eval binds `preg_quote()` with the same results.
#[test]
fn test_preg_quote_in_eval() {
    let quoted = QUOTE_CALLS.replace('\\', "\\\\").replace('\'', "\\'");
    let out = compile_and_run_capture_with_regex(&format!(
        "<?php\neval('{quoted}' . ' // ' . $argc);\n"
    ));
    assert_eq!(out.stdout, QUOTE_OUTPUT);
}
