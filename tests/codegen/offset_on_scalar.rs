//! Purpose:
//! Integration tests for php's `Trying to access array offset on <type>`: indexing a scalar
//! warns and answers NULL, and the program keeps running.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - elephc REFUSED these programs outright — `error: Cannot index non-array` — which is the
//!   worst possible response to what php treats as a warning about a bug the program survives.
//!   The `null` base was already tolerated; `false`, `true`, `int` and `float` were not.
//! - php names the VALUE, not the static type: one site produces `on true` or `on false`
//!   depending on what the variable holds, so the `bool` case is decided at run time. A literal
//!   `true` types as plain `bool` here, so that is the ordinary spelling and not an exotic one.
//! - A STRING base is excluded: `"abc"[1]` is a legal read and must stay one.
//! - The probe constructs stay SILENT — `isset($f['k'])` is `false` and `$f['k'] ?? $d` is `$d`,
//!   with no diagnostic — which is the same rule the null base already followed. Indexing the
//!   scalar anyway answered `isset()` TRUE and then crashed.
//! - Every expectation was measured on `php -n` 8.5.6.

use crate::support::*;

/// Verifies each scalar base warns with php's own word and answers null.
#[test]
fn test_every_scalar_base_warns_and_answers_null() {
    let out = compile_and_run_capture(
        r#"<?php
$f = false;
var_dump($f['k']);
$t = true;
var_dump($t[0]);
$i = 42;
var_dump($i[0]);
$fl = 1.5;
var_dump($fl[0]);
$n = null;
var_dump($n[0]);
echo "done\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "NULL\nNULL\nNULL\nNULL\nNULL\ndone\n");
    assert_eq!(
        out.diagnostics,
        "Warning: Trying to access array offset on false\n\
         Warning: Trying to access array offset on true\n\
         Warning: Trying to access array offset on int\n\
         Warning: Trying to access array offset on float\n\
         Warning: Trying to access array offset on null\n"
    );
}

/// Verifies the word follows the bool's VALUE, decided at run time.
///
/// The static type is plain `bool` on both sides here, so a single message would be wrong for
/// one of them. php answers `on true` for the first and `on false` for the second.
#[test]
fn test_the_bool_word_follows_the_value() {
    let out = compile_and_run_capture(
        r#"<?php
function yes(): bool { return strlen("ab") === 2; }
function no(): bool { return strlen("ab") === 5; }
$t = yes();
var_dump($t["any"]);
$f = no();
var_dump($f["any"]);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "NULL\nNULL\n");
    assert_eq!(
        out.diagnostics,
        "Warning: Trying to access array offset on true\n\
         Warning: Trying to access array offset on false\n"
    );
}

/// Verifies the probe constructs stay silent and answer what php answers.
///
/// `isset()` and `??` exist to name storage that may not be there, so php raises nothing through
/// them. This is the same rule the null base already followed.
#[test]
fn test_the_null_probes_say_nothing_about_a_scalar_base() {
    let out = compile_and_run_capture(
        r#"<?php
$f = false;
$i = 42;
var_dump(isset($f['k']));
var_dump($f['k'] ?? 'default');
var_dump(empty($i[0]));
var_dump(isset($i[0]));
echo "done\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\nstring(7) \"default\"\nbool(true)\nbool(false)\ndone\n"
    );
    assert_eq!(out.diagnostics, "");
}

/// Verifies a CHAIN warns once per level, with the word each level deserves.
///
/// `false[1][2]` reads an offset on `false`, which answers null, and then an offset on that
/// null — two warnings from one expression, in php's order.
#[test]
fn test_a_chained_offset_warns_at_every_level() {
    let out = compile_and_run_capture(
        r#"<?php
$chain = false;
var_dump($chain[1][2]);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "NULL\n");
    assert_eq!(
        out.diagnostics,
        "Warning: Trying to access array offset on false\n\
         Warning: Trying to access array offset on null\n"
    );
}

/// Verifies each warning names its own line.
#[test]
fn test_the_offset_warning_names_its_line() {
    let out = compile_and_run_capture(
        r#"<?php
$f = false;
echo "first\n";
var_dump($f['k']);
echo "middle\n";
$i = 3;
var_dump($i[0]);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "first\nNULL\nmiddle\nNULL\n");
    assert_eq!(
        out.located_diagnostics,
        "Warning: Trying to access array offset on false in test.php on line 4\n\
         Warning: Trying to access array offset on int in test.php on line 7\n"
    );
}

/// Verifies a STRING base still indexes, and says nothing.
///
/// The control: `"abc"[1]` is a legal read, and the scalar rule must not reach it.
#[test]
fn test_a_string_base_still_indexes() {
    let out = compile_and_run_capture(
        r#"<?php
$s = "abc";
var_dump($s[1], $s[-1]);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "string(1) \"b\"\nstring(1) \"c\"\n");
    assert_eq!(out.diagnostics, "");
}

/// Verifies a BOXED scalar warns with the same word, decided at run time.
///
/// The receiver's type is not known while lowering here: a `mixed` parameter carries its payload
/// tag, and php names what that tag holds. A boxed STRING and a boxed ARRAY are legal reads that
/// answer a value, and must stay silent.
#[test]
fn test_a_boxed_scalar_warns_with_phps_word() {
    let out = compile_and_run_capture(
        r#"<?php
function probe(mixed $v): void
{
    var_dump($v[0]);
}
probe(false);
probe(true);
probe(7);
probe(1.5);
probe(null);
probe("abc");
probe([9, 8]);
$h = fopen("php://memory", "w+");
probe($h);
fclose($h);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "NULL\nNULL\nNULL\nNULL\nNULL\nstring(1) \"a\"\nint(9)\nNULL\n"
    );
    assert_eq!(
        out.diagnostics,
        "Warning: Trying to access array offset on false\n\
         Warning: Trying to access array offset on true\n\
         Warning: Trying to access array offset on int\n\
         Warning: Trying to access array offset on float\n\
         Warning: Trying to access array offset on null\n\
         Warning: Trying to access array offset on resource\n"
    );
}

/// Verifies a base whose type is a UNION resolved at RUN TIME warns on its scalar arm.
///
/// `stat()` answers `array|false`, so nothing decides the word before the program runs. The
/// VALUE was already null here — this was a missing diagnostic, not a wrong answer — and it is
/// the shape the auditor corpus named.
#[test]
fn test_a_runtime_union_warns_on_its_false_arm() {
    let out = compile_and_run_capture(
        r#"<?php
$s = @stat("no-such-file-at-all.txt");
var_dump($s);
var_dump($s[0]);
var_dump($s['dev']);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\nNULL\nNULL\n");
    assert_eq!(
        out.diagnostics,
        "Warning: Trying to access array offset on false\n\
         Warning: Trying to access array offset on false\n"
    );
}

/// Verifies the probes stay silent for a boxed scalar too.
#[test]
fn test_the_null_probes_say_nothing_about_a_boxed_scalar() {
    let out = compile_and_run_capture(
        r#"<?php
function probe(mixed $v): void
{
    var_dump(isset($v[0]), $v[0] ?? "dflt", empty($v[0]));
}
probe(false);
probe(7);
probe([1, 2]);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\nstring(4) \"dflt\"\nbool(true)\n\
         bool(false)\nstring(4) \"dflt\"\nbool(true)\n\
         bool(true)\nint(1)\nbool(false)\n"
    );
    assert_eq!(out.diagnostics, "");
}
