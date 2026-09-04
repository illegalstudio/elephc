//! Purpose:
//! Integration tests for PHP's `Array to string conversion`: every site that renders an array as
//! a string prints the literal `Array` AND raises the warning, on the line the conversion is
//! written.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - elephc produced the right VALUE at every statically typed site and raised NONE of the nine
//!   warnings php raises. Through a boxed `Mixed` cell it was worse: `__rt_mixed_cast_string`
//!   and `__rt_mixed_write_stdout` had no array arm at all, so `mixed $v` holding an array
//!   rendered as the EMPTY STRING in a concatenation and printed NOTHING for `echo` — a silent
//!   wrong answer, not a missing diagnostic.
//! - Every expectation was measured on `php -n` 8.5.6, one conversion shape per line.
//! - The only array-meets-string case php leaves silent is a loose COMPARISON (`$a == "Array"`),
//!   which never converts. It is pinned here as a control so a future "warn everywhere" change
//!   cannot quietly start warning on it.
//! - The warning travels on the DIAGNOSTIC stream, which is php's stdout, so these assert
//!   `out.diagnostics`; the ` in FILE on line N` half is asserted through `located_diagnostics`,
//!   because the location comes from a separate mechanism (the line each instruction publishes)
//!   and a missing publication still prints a well-formed line with the wrong number.

use crate::support::*;

/// The warning php raises at every array-to-string conversion, without its location.
const WARNING: &str = "Warning: Array to string conversion\n";

/// Verifies the six statically typed conversion shapes warn once each, in source order.
#[test]
fn test_every_array_to_string_shape_warns_and_yields_the_literal() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [1, 2];
echo "concat: " . $a . "\n";
echo "interp: {$a}\n";
echo "cast: " . (string) $a . "\n";
echo "strval: " . strval($a) . "\n";
$s = "dotequals: ";
$s .= $a;
echo $s, "\n";
echo "echo: ";
echo $a;
echo "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "concat: Array\ninterp: Array\ncast: Array\nstrval: Array\ndotequals: Array\necho: Array\n"
    );
    assert_eq!(out.diagnostics, WARNING.repeat(6));
}

/// Verifies `print` warns like `echo` and names its own line.
///
/// `print` is a separate opcode from `echo` and refines its effects separately. Without its own
/// admission that it may warn it printed the warning with whatever line the PREVIOUS diagnostic
/// had published — measured as line 3 for a `print` on line 5.
#[test]
fn test_print_of_an_array_warns_and_names_its_own_line() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [1, 2];
echo $a;
echo "\n";
print $a;
echo "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "Array\nArray\n");
    assert_eq!(
        out.located_diagnostics,
        "Warning: Array to string conversion in test.php on line 3\n\
         Warning: Array to string conversion in test.php on line 5\n"
    );
}

/// Verifies an array inside a boxed `Mixed` renders like a statically typed one.
///
/// This is the half that was a WRONG VALUE rather than a missing warning: the runtime's boxed
/// string-cast and stdout helpers dispatched on the payload tag and had no arm for tags 4 and 5,
/// so an array fell through to the empty-string default.
#[test]
fn test_a_boxed_mixed_array_renders_as_array() {
    let out = compile_and_run_capture(
        r#"<?php
function show(mixed $v): void
{
    echo "concat: " . $v . "\n";
    echo "echo: ";
    echo $v;
    echo "\n";
    echo "cast: " . (string) $v . "\n";
}
show([1, 2]);
show(["k" => "v"]);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "concat: Array\necho: Array\ncast: Array\nconcat: Array\necho: Array\ncast: Array\n"
    );
    assert_eq!(out.diagnostics, WARNING.repeat(6));
}

/// Verifies a loose comparison against a string does NOT convert and does NOT warn.
///
/// php compares an array to a string by type, answering `false` without ever rendering the
/// array. A control: the fix must not reach this path.
#[test]
fn test_a_loose_comparison_against_a_string_stays_silent() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [1, 2];
echo ($a == "Array") ? "eq" : "ne", "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "ne\n");
    assert_eq!(out.diagnostics, "");
}

/// Verifies each warning names the line its own conversion is written on.
#[test]
fn test_the_conversion_warning_names_its_line() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [1, 2];
echo "first\n";
echo "x" . $a . "\n";
echo "middle\n";
echo (string) $a, "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "first\nxArray\nmiddle\nArray\n");
    assert_eq!(
        out.located_diagnostics,
        "Warning: Array to string conversion in test.php on line 4\n\
         Warning: Array to string conversion in test.php on line 6\n"
    );
}

/// Verifies an interpolation inside a HEREDOC names its own line, not the heredoc's opening line.
///
/// Every token an interpolated string produces used to carry the span of the string's first
/// character. That is harmless for a one-line double-quoted string and wrong for a heredoc body,
/// which spans many: a conversion on the fifth line of a template reported the line of the
/// `<<<LABEL` that opened it. php names the line the interpolation is written on.
#[test]
fn test_a_heredoc_interpolation_names_its_own_line() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [1, 2];
$s = <<<EOT
line one
{$a}
line three
EOT;
echo strlen($s), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "25\n");
    assert_eq!(
        out.located_diagnostics,
        "Warning: Array to string conversion in test.php on line 5\n"
    );
}

/// Verifies a plain multi-line double-quoted string tracks its lines too.
///
/// Same mechanism as the heredoc, reached by the other syntax — a double-quoted literal that
/// contains real newlines. php names line 5 for the second interpolation.
#[test]
fn test_a_multi_line_double_quoted_string_names_each_interpolation_line() {
    let out = compile_and_run_capture(
        "<?php\n$a = [1, 2];\n$s = \"one {$a}\ntwo\nthree {$a}\";\necho strlen($s), \"\\n\";\n",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "25\n");
    assert_eq!(
        out.located_diagnostics,
        "Warning: Array to string conversion in test.php on line 3\n\
         Warning: Array to string conversion in test.php on line 5\n"
    );
}
