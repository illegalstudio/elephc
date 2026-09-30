//! Purpose:
//! End-to-end AOT tests for PHP's `sscanf()` / `fscanf()` scanner, which elephc implements
//! through the injected `crate::scanf_prelude` engine rather than per-target assembly.
//!
//! Called from:
//! - `cargo test --test codegen_tests scanf` through the strings integration module.
//!
//! Key details:
//! - `tests/fixtures/scanf_cases.json` is the corpus, one case per rule, and every `expected`
//!   in it was CAPTURED from `php -n` 8.5.6 rather than written by hand — the `why` field says
//!   which rule the case exists for. Subject and format are hex-encoded so control bytes,
//!   whitespace and `%` survive the round trip into PHP source.
//! - The corpus runs through ONE `sscanf()` call site inside a loop, so the compiled program
//!   stays small and the test exercises the same lowering a real program gets.
//! - The named tests below pin what a corpus of VALUES cannot: the `ValueError` wordings and
//!   `fscanf()`'s per-line stream behaviour.

use crate::support::*;
use serde_json::Value;

/// Runs every php-captured scanf case through the native `sscanf()` path.
///
/// The previous `__rt_sscanf` assembly passed exactly one shape of this corpus — a `%s`
/// against a word — and failed the rest: `%d` came back as a STRING, an unmatched conversion
/// as `""` instead of `NULL`, and widths, suppression, character classes, `%i`/`%u`/`%x`/`%o`/
/// `%c`/`%n` and the end-of-input NULL result were absent outright.
#[test]
fn test_sscanf_matches_php_across_the_captured_corpus() {
    let cases: Value = serde_json::from_str(include_str!("../../fixtures/scanf_cases.json"))
        .expect("scanf fixture JSON must parse");
    let cases = cases.as_array().expect("fixture root must be an array");
    let mut subjects = Vec::new();
    let mut formats = Vec::new();
    let mut expected = Vec::new();
    for case in cases {
        subjects.push(format!(
            "hex2bin(\"{}\")",
            case["subject"].as_str().expect("subject must be a string")
        ));
        formats.push(format!(
            "hex2bin(\"{}\")",
            case["format"].as_str().expect("format must be a string")
        ));
        expected.push(
            case["expected"]
                .as_str()
                .expect("expected must be a string")
                .to_string(),
        );
    }
    let source = format!(
        r#"<?php
$subjects = [{}];
$formats = [{}];
$count = count($subjects);
for ($i = 0; $i < $count; $i++) {{
    $r = sscanf($subjects[$i], $formats[$i]);
    if ($r === null) {{
        echo "NULL\n";
        continue;
    }}
    $line = "";
    foreach ($r as $v) {{
        if ($line !== "") {{ $line = $line . ","; }}
        if (is_int($v)) {{ $line = $line . "i:" . $v; }}
        elseif (is_float($v)) {{ $line = $line . "f:" . var_export($v, true); }}
        elseif (is_string($v)) {{ $line = $line . "s:" . $v; }}
        elseif ($v === null) {{ $line = $line . "NULL"; }}
        else {{ $line = $line . "?"; }}
    }}
    echo "[", $line, "]\n";
}}
"#,
        subjects.join(", "),
        formats.join(", "),
    );
    let actual = compile_and_run(&source);
    let actual: Vec<&str> = actual.lines().collect();
    assert_eq!(actual.len(), expected.len(), "one output line per case");
    for (index, case) in cases.iter().enumerate() {
        assert_eq!(
            actual[index], expected[index],
            "case {index} ({}): {}",
            case["why"].as_str().unwrap_or(""),
            case["subject"].as_str().unwrap_or(""),
        );
    }
}

/// Verifies `%d` produces an INT, the divergence that made the old scanner silently wrong.
///
/// `sscanf('77 xx', '%d %d')` answered `['77', '']` before: two strings where php answers an
/// int and a `NULL`. `is_int()`/`===`/`var_dump()` all lied about the result of a builtin whose
/// whole purpose is to type its input.
#[test]
fn test_sscanf_numeric_conversions_carry_their_php_type() {
    let out = compile_and_run(
        r#"<?php
$r = sscanf("77 xx", "%d %d");
var_dump($r);
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [0]=>\n  int(77)\n  [1]=>\n  NULL\n}\n"
    );
}

/// Verifies an unsupported conversion character raises php's `ValueError`, wording included.
///
/// php validates the WHOLE format, so the error still fires after scanning has already
/// stopped: `sscanf('x', '%d%q')` throws even though `%d` failed first.
#[test]
fn test_sscanf_bad_conversion_character_throws_php_value_error() {
    let out = compile_and_run(
        r#"<?php
try { sscanf("101", "%b"); } catch (\ValueError $e) { echo $e->getMessage(), "\n"; }
try { sscanf("x", "%d%q"); } catch (\ValueError $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "Bad scan conversion character \"b\"\nBad scan conversion character \"q\"\n"
    );
}

/// Verifies an unterminated `%[` set raises php's `ValueError`.
///
/// `%[^]` is the trap: the `]` closes nothing because a `]` in first position is a member of
/// the set, so php reports the bracket as unmatched rather than accepting an empty class.
#[test]
fn test_sscanf_unterminated_character_class_throws_php_value_error() {
    let out = compile_and_run(
        r#"<?php
foreach (["%[", "%[a", "%[^]"] as $format) {
    try { sscanf("abc", $format); } catch (\ValueError $e) { echo $e->getMessage(), "\n"; }
}
"#,
    );
    assert_eq!(
        out,
        "Unmatched [ in format string\nUnmatched [ in format string\nUnmatched [ in format string\n"
    );
}

/// Verifies `fscanf()` consumes exactly ONE LINE per call and distinguishes its three results.
///
/// Measured with `php -n` (8.5.6) on `"1 2\n3 4\nlast"`: `%d %d` takes the whole first line,
/// a following `%d` takes the whole SECOND one (the unread `4` is dropped with its line), `%s`
/// reads the unterminated tail, and the next call returns `false`. An EMPTY line is `null`,
/// not `false` — scanning `"\n"` reaches end of input without assigning.
#[test]
fn test_fscanf_reads_one_line_per_call() {
    let out = compile_and_run(
        r#"<?php
$path = sys_get_temp_dir() . "/elephc_fscanf_lines.txt";
file_put_contents($path, "1 2\n3 4\nlast");
$handle = fopen($path, "r");
var_dump(fscanf($handle, "%d %d"));
var_dump(fscanf($handle, "%d"));
var_dump(fscanf($handle, "%s"));
var_dump(fscanf($handle, "%s"));
fclose($handle);
file_put_contents($path, "a\n\nb\n");
$handle = fopen($path, "r");
var_dump(fscanf($handle, "%s"));
var_dump(fscanf($handle, "%s"));
var_dump(fscanf($handle, "%s"));
var_dump(fscanf($handle, "%s"));
fclose($handle);
unlink($path);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "array(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n}\n",
            "array(1) {\n  [0]=>\n  int(3)\n}\n",
            "array(1) {\n  [0]=>\n  string(4) \"last\"\n}\n",
            "bool(false)\n",
            "array(1) {\n  [0]=>\n  string(1) \"a\"\n}\n",
            "NULL\n",
            "array(1) {\n  [0]=>\n  string(1) \"b\"\n}\n",
            "bool(false)\n",
        )
    );
}

/// Verifies `fscanf()`'s line KEEPS its newline, which only a class conversion can observe.
///
/// php reads the line through `php_stream_get_line` without trimming, so `%[^z]` on `"a\n"`
/// returns `"a\n"`. A `%s`-only test cannot see this — it stops at whitespace either way.
#[test]
fn test_fscanf_line_keeps_its_newline() {
    let out = compile_and_run(
        r#"<?php
$path = sys_get_temp_dir() . "/elephc_fscanf_newline.txt";
file_put_contents($path, "a\nb\n");
$handle = fopen($path, "r");
var_dump(fscanf($handle, "%[^z]"));
fclose($handle);
unlink($path);
"#,
    );
    assert_eq!(out, "array(1) {\n  [0]=>\n  string(2) \"a\n\"\n}\n");
}

/// Verifies a format with no conversion still consumes its line and yields an empty array.
#[test]
fn test_fscanf_conversionless_format_consumes_the_line() {
    let out = compile_and_run(
        r#"<?php
$path = sys_get_temp_dir() . "/elephc_fscanf_literal.txt";
file_put_contents($path, "hello\n");
$handle = fopen($path, "r");
var_dump(fscanf($handle, "hello"));
var_dump(fscanf($handle, "hello"));
fclose($handle);
unlink($path);
"#,
    );
    assert_eq!(out, "array(0) {\n}\nbool(false)\n");
}

/// Verifies a program that never scans carries none of the prelude.
///
/// The engine is roughly 400 lines of PHP; injecting it unconditionally would put a scanner
/// into every binary that calls `strlen()`. `crate::scanf_prelude::detect` gates it, and the
/// neighbouring `sprintf`/`fgets` names must not trip that gate.
#[test]
fn test_scanf_prelude_is_pay_for_use() {
    assert!(
        user_asm(r#"<?php print_r(sscanf("1", "%d"));"#).contains("__elephc_scanf"),
        "a scanning program must carry the engine"
    );
    assert!(
        !user_asm(
            r#"<?php $h = fopen("/dev/null", "r"); echo sprintf("%d", strlen("abc")); fgets($h);"#
        )
        .contains("__elephc_scanf"),
        "sprintf/strlen/fgets must not pull the scanf engine in"
    );
}

/// Compiles PHP source and returns only the user-code assembly.
fn user_asm(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_scanf_asm");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    let _ = fs::remove_dir_all(&dir);
    user_asm
}

/// Verifies php's by-reference `$vars` form of `sscanf()` and `fscanf()`.
///
/// `sscanf("alice 30", "%s %d", $name, $age)` fills both variables and answers `int(2)`. elephc
/// refused the call outright, so the manual's own idiom did not compile — and neither variable
/// has to exist beforehand, which is the second half of what had to be delivered: the contract's
/// `variadic_writes` marks the tail as written so the checker treats those arguments as
/// definitions rather than reads.
///
/// The COUNT is the subtle part and is pinned by the `%*d` row: php counts every conversion that
/// consumed input, SUPPRESSED ones included, so `"%d %*d %d"` over `"1 2 3"` answers 3 while
/// filling two variables. `count($values)` would have answered 2.
///
/// The two exhaustion answers differ from the array form and from each other: `sscanf("", "%d",
/// $e)` answers `-1` where the array form answers `null`, while `fscanf()` on a stream already at
/// end of file answers `false` — and does so BEFORE the variable-count check, which is why the
/// last row can pass nine variables without raising.
#[test]
fn test_scanf_assigns_through_its_by_reference_vars() {
    let out = compile_and_run(
        r#"<?php
var_dump(sscanf("1 2", "%d %d", $a, $b), $a, $b);
var_dump(sscanf("1 x", "%d %d", $c, $d), $c, $d);
var_dump(sscanf("", "%d", $e), $e);
var_dump(sscanf("a", "%d %d", $f, $g), $f, $g);
var_dump(sscanf("1 2 3", "%d %*d %d", $h, $i), $h, $i);
var_dump(sscanf("age=42", "age=%d", $j), $j);
var_dump(sscanf("bob 7 1.5", "%s %d %f", $k, $l, $m), $k, $l, $m);
$s = fopen("php://memory", "w+");
fwrite($s, "age:42 name:bob\nage:7\n");
rewind($s);
var_dump(fscanf($s, "age:%d name:%s", $q, $r), $q, $r);
var_dump(fscanf($s, "age:%d name:%s", $t, $u), $t, $u);
var_dump(fscanf($s, "age:%d", $v), $v);
fclose($s);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "int(2)\nint(1)\nint(2)\n",
            "int(1)\nint(1)\nNULL\n",
            "int(-1)\nNULL\n",
            "int(0)\nNULL\nNULL\n",
            "int(3)\nint(1)\nint(3)\n",
            "int(1)\nint(42)\n",
            "int(3)\nstring(3) \"bob\"\nint(7)\nfloat(1.5)\n",
            "int(2)\nint(42)\nstring(3) \"bob\"\n",
            "int(1)\nint(7)\nNULL\n",
            "bool(false)\nNULL\n",
        )
    );
}

/// Verifies php's two `ValueError`s for a `$vars` count the format does not match.
///
/// php picks the wording by DIRECTION, which is easy to get backwards: more variables than
/// conversions is `Variable is not assigned by any conversion specifiers`, fewer is
/// `Different numbers of variable names and field specifiers`. Both measured on `php -n` 8.5.6.
#[test]
fn test_scanf_vars_count_must_match_the_conversions() {
    let out = compile_and_run(
        r#"<?php
try { sscanf("7", "%d", $a, $b); } catch (ValueError $e) { echo "1:", $e->getMessage(), "\n"; }
try { sscanf("1 2 3", "%d %d %d", $c); } catch (ValueError $e) { echo "2:", $e->getMessage(), "\n"; }
$h = fopen("php://memory", "w+");
fwrite($h, "1 2\n");
rewind($h);
try { fscanf($h, "%d", $d, $e); } catch (ValueError $x) { echo "3:", $x->getMessage(), "\n"; }
fclose($h);
// A suppressed conversion is not a variable, so this pair matches.
var_dump(sscanf("1 2 3", "%d %*d %d", $f, $g));
"#,
    );
    assert_eq!(
        out,
        concat!(
            "1:Variable is not assigned by any conversion specifiers\n",
            "2:Different numbers of variable names and field specifiers\n",
            "3:Variable is not assigned by any conversion specifiers\n",
            "int(3)\n",
        )
    );
}
