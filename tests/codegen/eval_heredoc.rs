//! Purpose:
//! End-to-end value tests for the two constructs the interpreter's scanner used to refuse:
//! a heredoc whose body interpolates, and an identifier holding a byte at or above `0x80`.
//! Both reach the interpreter's own lexer, either through a runtime `include` of a file
//! whose path is only known at run time or through `eval()`.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_heredoc` through Rust's test harness.
//!
//! Key details:
//! - A sweep of the Symfony vendor tree found fourteen files refused for the heredoc shape
//!   and one (`symfony/cache` ValueWrapper) for the identifier shape, all accepted by
//!   `php -n -l`. These fixtures are their reduced forms.
//! - Every expected string is the output of `php -n` 8.5.6 on the same fixture, so a
//!   regression shows up as a value difference, not merely as a compile failure.
//! - The `include` path is deliberately dynamic: a literal include is resolved and compiled
//!   ahead of time and would never reach the interpreter's scanner.

use crate::support::*;

/// The main file that forces a run-time include through a variable path.
const RUNTIME_INCLUDE_MAIN: &str =
    "<?php function load($path) { include $path; } load('piece.php');";

/// Verifies a heredoc interpolates every simple and complex form the interpreter meets.
///
/// `php -n` 8.5.6 prints `v=X b=KV c=PROP d=KV e=PROP` for this body. The scanner used to
/// answer `UnsupportedConstruct` for any heredoc whose body held a `$` at all, which is what
/// refused fourteen Symfony vendor files.
#[test]
fn test_runtime_include_heredoc_interpolates_every_form() {
    let out = compile_and_run_files(
        &[
            ("main.php", RUNTIME_INCLUDE_MAIN),
            (
                "piece.php",
                r#"<?php
$a = 'X';
$arr = ['k' => 'KV'];
$o = new stdClass();
$o->p = 'PROP';
$s = <<<EOT
    v=$a b={$arr['k']} c={$o->p} d=$arr[k] e=$o->p
    EOT;
echo $s;
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "v=X b=KV c=PROP d=KV e=PROP");
}

/// Verifies the flexible-heredoc reducer of the refused vendor files prints PHP's value.
///
/// `$a = "X"; $s = <<<EOT\n    v=$a\n    EOT;` prints `v=X`: the closing marker's four
/// spaces are stripped from the body line before the body is interpolated.
#[test]
fn test_runtime_include_flexible_heredoc_strips_indentation_then_interpolates() {
    let out = compile_and_run_files(
        &[
            ("main.php", RUNTIME_INCLUDE_MAIN),
            (
                "piece.php",
                "<?php\n$a = 'X';\n$s = <<<EOT\n    v=$a\n    EOT;\necho $s;\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "v=X");
}

/// Verifies a nowdoc keeps its dollars and braces as text now that a heredoc expands.
///
/// `php -n` 8.5.6 prints `raw $a {$arr['k']}` verbatim: a nowdoc body is a single-quoted
/// body, so it neither interpolates nor expands escapes.
#[test]
fn test_runtime_include_nowdoc_keeps_its_dollars_literal() {
    let out = compile_and_run_files(
        &[
            ("main.php", RUNTIME_INCLUDE_MAIN),
            (
                "piece.php",
                r#"<?php
$a = 'X';
$arr = ['k' => 'KV'];
$n = <<<'EOT'
    raw $a {$arr['k']}
    EOT;
echo $n;
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "raw $a {$arr['k']}");
}

/// Verifies a heredoc body expands escapes but keeps `\"`, which a `"…"` literal unescapes.
///
/// `php -n` 8.5.6 prints a real tab, then `q:\"`, then one backslash, then `$a` for the body
/// `tab:\t q:\" bs:\\ d:\$a`. The kept backslash is the single deliberate divergence between
/// the two bodies: a heredoc has no quote to escape.
#[test]
fn test_runtime_include_heredoc_expands_escapes_but_keeps_the_quote() {
    let out = compile_and_run_files(
        &[
            ("main.php", RUNTIME_INCLUDE_MAIN),
            (
                "piece.php",
                "<?php\n$a = 'X';\necho <<<EOT\ntab:\\t q:\\\" bs:\\\\ d:\\$a\nEOT;\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "tab:\t q:\\\" bs:\\ d:$a");
}

/// Verifies an identifier written in UTF-8 above the ASCII range declares and resolves.
///
/// PHP's identifier rule is a byte rule: `class Café { public const V = 11; }` is legal and
/// `Café::V` prints `11`.
#[test]
fn test_runtime_include_non_ascii_class_name_resolves() {
    let out = compile_and_run_files(
        &[
            ("main.php", RUNTIME_INCLUDE_MAIN),
            (
                "piece.php",
                "<?php\nclass Café { public const V = 11; }\necho Café::V;\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "11");
}

/// Verifies a heredoc inside an `eval()` fragment interpolates and prints PHP's value.
///
/// The outer nowdoc hands the inner heredoc to `eval()` verbatim, so the interpreter's own
/// scanner is the one that reads it. `php -n` 8.5.6 prints `v=X`.
#[test]
fn test_eval_fragment_heredoc_interpolates() {
    let out = compile_and_run(
        r#"<?php
$code = <<<'PHPCODE'
$a = "X";
$s = <<<EOT
    v=$a
    EOT;
echo $s;
PHPCODE;
eval($code);
"#,
    );
    assert_eq!(out, "v=X");
}

/// Verifies a class named by a raw byte at or above `0x80` declares and resolves.
///
/// This is the reduced form of the refused `symfony/cache` file. `php -n` 8.5.6 runs
/// `class \xA9 { public const V = 5; } echo \xA9::V;` and prints `5`. The byte is built with
/// `chr()` so the fragment carries it while the surrounding file stays valid UTF-8, which is
/// exactly how the parser meets it: as bytes that are not valid UTF-8.
#[test]
fn test_eval_fragment_high_byte_class_name_resolves() {
    let out = compile_and_run(
        r#"<?php
$name = chr(169);
eval("class {$name} { public const V = 5; } echo {$name}::V;");
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies a variable named by a raw byte at or above `0x80` stores and reads back.
///
/// `php -n` 8.5.6 prints `7`. Before high bytes outside a literal became markers, the
/// fragment failed as invalid UTF-8 without ever reaching the grammar.
#[test]
fn test_eval_fragment_high_byte_variable_name_round_trips() {
    let out = compile_and_run(
        r#"<?php
eval("\$" . chr(169) . " = 7; echo \$" . chr(169) . ";");
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies `\x`, octal and `\u{}` escapes produce PHP's exact bytes inside a fragment.
///
/// `bin2hex("\xFF\377\u{41}")` is `ffff41` under `php -n` 8.5.6. The first two name the same
/// byte, which is not valid UTF-8 on its own, so the literal has to reach the interpreter as
/// bytes rather than as text.
#[test]
fn test_eval_fragment_numeric_escapes_produce_php_bytes() {
    let out = compile_and_run(
        r#"<?php
eval('echo bin2hex("\xFF\377\u{41}");');
"#,
    );
    assert_eq!(out, "ffff41");
}

/// Verifies the same escapes inside a heredoc body produce the same bytes.
///
/// PHP defines a heredoc body as a double-quoted body, so `bin2hex()` of a heredoc holding
/// `\xFF\377\u{41}` is `ffff41` too.
#[test]
fn test_runtime_include_heredoc_numeric_escapes_produce_php_bytes() {
    let out = compile_and_run_files(
        &[
            ("main.php", RUNTIME_INCLUDE_MAIN),
            (
                "piece.php",
                "<?php\necho bin2hex(<<<EOT\n\\xFF\\377\\u{41}\nEOT);\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ffff41");
}
