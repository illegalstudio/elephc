//! Purpose:
//! End-to-end tests for the `$flags` / `$offset` parameters of `preg_match()` and the
//! `$offset` parameter of `preg_match_all()`.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through `tests/codegen/mod.rs`.
//!
//! Key details:
//! - Expected output is PHP 8.5's; `ELEPHC_PHP_CHECK=1` re-runs each fixture under `php`.
//! - Offsets keep the whole subject visible, so anchors and lookbehind are exercised.
//! - The eval fixture builds its source at runtime so it runs through Magician.

use crate::support::*;

/// `PREG_OFFSET_CAPTURE` and `PREG_UNMATCHED_AS_NULL` shape `preg_match()` captures like PHP.
#[test]
fn test_preg_match_capture_flags() {
    let out = compile_and_run(r#"<?php
echo preg_match('/(b)(x)?(c)?/', 'abcd', $m, PREG_OFFSET_CAPTURE), json_encode($m), "\n";
echo preg_match('/(b)(x)?(z)?/', 'abcd', $m, PREG_UNMATCHED_AS_NULL), json_encode($m), "\n";
echo preg_match('/(b)(x)?/', 'abcd', $m, PREG_UNMATCHED_AS_NULL | PREG_OFFSET_CAPTURE), json_encode($m), "\n";
echo preg_match('/(b)(x)?(z)?/', 'abcd', $m, 0), json_encode($m), "\n";
echo preg_match('/q/', 'abcd', $m, PREG_OFFSET_CAPTURE), json_encode($m), "\n";
"#);
    assert_eq!(
        out,
        "1[[\"bc\",1],[\"b\",1],[\"\",-1],[\"c\",2]]\n\
         1[\"b\",\"b\",null,null]\n\
         1[[\"b\",1],[\"b\",1],[null,-1]]\n\
         1[\"b\",\"b\"]\n\
         0[]\n"
    );
}

/// `$offset` starts the search without hiding the subject from anchors and lookbehind.
#[test]
fn test_preg_match_offset() {
    let out = compile_and_run(r#"<?php
echo preg_match('/a/', 'abca', $m, PREG_OFFSET_CAPTURE, 1), json_encode($m), "\n";
echo preg_match('/a/', 'abca', $m, PREG_OFFSET_CAPTURE, -1), json_encode($m), "\n";
echo preg_match('/a/', 'abca', $m, PREG_OFFSET_CAPTURE, -10), json_encode($m), "\n";
echo preg_match('/^b/', 'abc', $m, 0, 1), preg_match('/(?<=a)b/', 'abc', $m, 0, 1), "\n";
echo preg_match('/\Gb/', 'abc', $m, 0, 1), preg_match('/c$/', 'abc', $m, 0, 3), "\n";
echo \PrEg_MaTcH('/a/', 'xa', offset: 1), preg_match(subject: 'xa', pattern: '/a/', offset: 2), "\n";
"#);
    assert_eq!(out, "1[[\"a\",3]]\n1[[\"a\",3]]\n1[[\"a\",0]]\n01\n10\n10\n");
}

/// Exact key and binding policies preserve named regex options expanded from static spreads.
#[test]
fn test_preg_match_named_spread_options_with_strict_profiles() {
    let source = r#"<?php
$offset = 2;
preg_match('/a/', 'baab', $matches, ...['flags' => PREG_OFFSET_CAPTURE, 'offset' => $offset]);
echo $matches[0][0], ':', $matches[0][1], '|';
echo preg_match_all('/a/', 'baab', ...['offset' => $offset]), '|';
preg_match(subject: 'baab', pattern: '/a/', matches: $legacy);
echo $legacy[0];
"#;
    let output = compile_cli_file_and_run_with_managed_pcre2_and_flags(
        source, &["--strict-array-keys", "--strict-coercions"],
    );
    assert_eq!(output, "a:2|1|a");
}

/// An offset past the end returns false and still writes the empty `$matches` array.
#[test]
fn test_preg_match_offset_past_end() {
    let out = compile_and_run(r#"<?php
$m = 'keep';
var_dump(preg_match('/a/', 'abc', $m, 0, 4));
echo json_encode($m), "\n";
$all = 'keep';
var_dump(preg_match_all('/a/', 'abc', $all, 0, 4));
echo json_encode($all), "\n";
var_dump(preg_match('/a/', 'abc', $m, 0, 3));
"#);
    assert_eq!(out, "bool(false)\n[]\nbool(false)\n[]\nint(0)\n");
}

/// Order flags are rejected for `preg_match()` with PHP's `ValueError`; other bits are ignored.
#[test]
fn test_preg_match_invalid_flags() {
    let out = compile_and_run(r#"<?php
foreach ([PREG_PATTERN_ORDER, PREG_SET_ORDER, 255] as $flag) {
    try {
        preg_match('/a/', 'a', $m, $flag);
    } catch (ValueError $e) {
        echo $e->getMessage(), "\n";
    }
}
echo preg_match('/a/', 'a', $m, 1024), json_encode($m), "\n";
"#);
    assert_eq!(
        out,
        "preg_match(): Argument #4 ($flags) must be a PREG_* constant\n\
         preg_match(): Argument #4 ($flags) must be a PREG_* constant\n\
         preg_match(): Argument #4 ($flags) must be a PREG_* constant\n\
         1[\"a\"]\n"
    );
}

/// `preg_match_all()` honours `$offset` with and without a `$matches` destination.
#[test]
fn test_preg_match_all_offset() {
    let out = compile_and_run(r#"<?php
echo preg_match_all('/a/', 'abab', $m, PREG_OFFSET_CAPTURE, 1), json_encode($m), "\n";
echo preg_match_all('/a/', 'abab', $m, PREG_PATTERN_ORDER, -2), json_encode($m), "\n";
echo preg_match_all('/\Ga/', 'aab', $m, 0, 1), json_encode($m), "\n";
echo preg_match_all('/a/', 'abab', offset: 1), preg_match_all('/a/', 'abab', offset: -10), "\n";
"#);
    assert_eq!(
        out,
        "1[[[\"a\",2]]]\n1[[\"a\"]]\n1[[\"a\"]]\n12\n"
    );
}

/// `PREG_SET_ORDER` rows drop trailing unmatched groups unless they become nulls.
#[test]
fn test_preg_match_all_set_order_trims_trailing_groups() {
    let out = compile_and_run(r#"<?php
preg_match_all('/(a)(x)?/', 'baba', $m, PREG_SET_ORDER);
echo json_encode($m), "\n";
preg_match_all('/(a)(x)?/', 'baba', $m, PREG_SET_ORDER | PREG_UNMATCHED_AS_NULL);
echo json_encode($m), "\n";
preg_match_all('/(x)?(a)/', 'ba', $m, PREG_SET_ORDER | PREG_OFFSET_CAPTURE);
echo json_encode($m), "\n";
"#);
    assert_eq!(
        out,
        "[[\"a\",\"a\"],[\"a\",\"a\"]]\n\
         [[\"a\",\"a\",null],[\"a\",\"a\",null]]\n\
         [[[\"a\",1],[\"\",-1],[\"a\",1]]]\n"
    );
}

/// Boxed `$flags` / `$offset` values are cast to integers, so a capture can drive the next search.
#[test]
fn test_preg_match_boxed_offset_and_flags() {
    let out = compile_and_run(r#"<?php
$subject = 'hello preg world';
$offset = 0;
$options = ['flags' => PREG_OFFSET_CAPTURE, 'name' => 'tokens'];
$flags = $options['flags'];
while (preg_match('/\w+/', $subject, $token, $flags, $offset) === 1) {
    echo $token[0][0], '@', $token[0][1], ' ';
    $offset = $token[0][1] + strlen($token[0][0]);
}
echo preg_match_all('/o/', $subject, $all, $flags, $options['flags'] - 250), json_encode($all), "\n";
"#);
    assert_eq!(out, "hello@0 preg@6 world@11 1[[[\"o\",12]]]\n");
}

/// Offsets stay byte offsets with `/u`, and the single row keeps interior unmatched groups.
#[test]
fn test_preg_match_offset_capture_utf8_bytes() {
    let out = compile_and_run(r#"<?php
echo preg_match('/(é)(x)/u', 'aéx', $m, PREG_OFFSET_CAPTURE), json_encode($m), "\n";
echo preg_match('/./u', "aéb", $m, PREG_OFFSET_CAPTURE, 3), json_encode($m), "\n";
echo preg_match('/(a)(b)?(c)/', 'xac', $m, PREG_OFFSET_CAPTURE), json_encode($m), "\n";
"#);
    assert_eq!(
        out,
        "1[[\"\\u00e9x\",1],[\"\\u00e9\",1],[\"x\",3]]\n\
         1[[\"b\",3]]\n\
         1[[\"ac\",1],[\"a\",1],[\"\",-1],[\"c\",2]]\n"
    );
}

/// Repeated window calls release every single-match row, discarded array and bad-offset result.
#[test]
fn test_preg_match_window_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$s = 'hello preg world';
$total = 0;
for ($i = 0; $i < 50; $i++) {
    $offset = 0;
    while (preg_match('/(\w)(\w+)?(z)?/', $s, $t, PREG_OFFSET_CAPTURE, $offset) === 1) {
        $offset = $t[0][1] + strlen($t[0][0]);
        $total++;
    }
    preg_match('/(o)(x)?/', $s, $u, PREG_UNMATCHED_AS_NULL | PREG_OFFSET_CAPTURE, -5);
    preg_match('/a/', $s, $w, 0, 999);
    $total += preg_match('/o/', $s, flags: PREG_OFFSET_CAPTURE, offset: 2);
    $total += preg_match_all('/o/', $s, offset: 4);
    preg_match_all('/o/', $s, $all, 0, 999);
    try { preg_match('/a/', $s, $v, PREG_SET_ORDER); } catch (ValueError $e) { $total++; }
}
$subject = 'xab';
echo $total, ':', preg_match('/([a-z])/', $subject, $subject, PREG_OFFSET_CAPTURE, 1), json_encode($subject);
unset($subject);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "350:1[[\"a\",1],[\"a\",1]]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Fixture: each `$flags` / `$offset` form takes part in the `preg_last_error()` protocol.
/// A success resets the state, malformed UTF-8 under `/u` and an `$offset` inside a
/// multibyte character fail with their own codes, and an `$offset` past the end fails with
/// `PREG_INTERNAL_ERROR`, with and without `$matches`.
const WINDOW_LAST_ERROR: &str = r#"function window_state_SFX() { return preg_last_error() . ':' . preg_last_error_msg(); }
$r = [];
$r[] = [preg_match('/b/', 'abc', $m, 0, 1), window_state_SFX()];
$r[] = [preg_match('/./u', "a\xff", $m, 0, 0), window_state_SFX()];
$r[] = [preg_match('/a/', 'abc', $m, 0, 9), window_state_SFX(), $m];
$r[] = [preg_match('/./u', "\xc3\xa9", $m, PREG_OFFSET_CAPTURE, 1), window_state_SFX()];
$r[] = [preg_match('/b/', 'abc', $m, 0, 1), window_state_SFX()];
$r[] = [preg_match_all('/b/', 'abcb', $m, 0, 2), window_state_SFX()];
$r[] = [preg_match_all('/./u', "a\xff", $m, 0, 0), window_state_SFX()];
$r[] = [preg_match_all('/a/', 'abc', $m, 0, 9), window_state_SFX(), $m];
$r[] = [preg_match_all('/a/', 'abc', $m, 0, 1), window_state_SFX()];
$r[] = [preg_match_all('/b/', 'abcb', offset: 2), window_state_SFX()];
$r[] = [preg_match_all('/./u', "a\xff", offset: 0), window_state_SFX()];
$r[] = [preg_match_all('/a/', 'abc', offset: 9), window_state_SFX()];
$r[] = [preg_match_all('/./u', "\xc3\xa9", offset: 1), window_state_SFX()];
$r[] = [preg_match('/b/', 'abc', offset: 1), window_state_SFX()];
$r[] = [preg_match('/a/', 'abc', offset: 9), window_state_SFX()];
$r[] = [preg_match('/./u', "\xc3\xa9", offset: 1), window_state_SFX()];
$r[] = [preg_match('/./u', "a\xff", offset: 0), window_state_SFX()];
$r[] = [preg_match('/b/', 'abc'), window_state_SFX()];
echo json_encode($r), "\n";
"#;

/// Fixture: an invalid pattern in each `$flags` / `$offset` form records
/// `PREG_INTERNAL_ERROR` and the next success clears it. Compiled only: eval treats an
/// invalid `preg_match()` pattern as fatal on main too (#2388).
const WINDOW_INVALID_PATTERN: &str = r#"function window_invalid_state() { return preg_last_error() . ':' . preg_last_error_msg(); }
$r = [];
$r[] = [preg_match('/(/', 'abc', $m, 0, 0), window_invalid_state()];
$r[] = [preg_match_all('/(/', 'abc', $m, 0, 1), window_invalid_state()];
$r[] = [preg_match_all('/(/', 'abc', offset: 1), window_invalid_state()];
$r[] = [preg_match('/(/', 'abc', offset: 0), window_invalid_state()];
$r[] = [preg_match('/b/', 'abc', offset: 1), window_invalid_state()];
echo json_encode($r), "\n";
"#;

/// Fixture: a destructor's nested `preg_*` error while `$matches` is overwritten does not
/// replace the outer window call's own outcome.
const WINDOW_MATCHES_DESTRUCTOR: &str = r#"class WindowNoisy_SFX { public function __destruct() { var_dump(preg_match('/./u', "\xff")); } }
$one = new WindowNoisy_SFX();
var_dump(preg_match('/a/', 'a', $one, 0, 0), preg_last_error());
$two = new WindowNoisy_SFX();
var_dump(preg_match('/a/', 'a', $two, 0, 5), preg_last_error());
$all = new WindowNoisy_SFX();
var_dump(preg_match_all('/a/', 'aa', $all, 0, 1), preg_last_error());
"#;

/// Fixture: eval runs the same `$flags` / `$offset` semantics through Magician.
const WINDOW_EVAL_PARITY: &str = r#"echo preg_match("/(b)(x)?/", "abcb", $parity_m, PREG_OFFSET_CAPTURE | PREG_UNMATCHED_AS_NULL, 2), json_encode($parity_m), "|";
echo preg_match_all("/(a)(x)?/", "baba", $parity_all, PREG_SET_ORDER, 2), json_encode($parity_all), "|";
var_dump(preg_match("/a/", "a", $parity_m, 0, 5));
try { preg_match("/a/", "a", $parity_m, PREG_SET_ORDER); } catch (ValueError $e) { echo $e->getMessage(); }
"#;

/// Fixture: named `offset:` / `flags:` that skip `$matches` bind through callables, without
/// a by-reference warning for the omitted `$matches`. The handler echoes any warning and is
/// restored so later bundled cases keep the default one.
const WINDOW_NAMED_CALLABLES: &str = r#"set_error_handler(function ($no, $msg) { echo "[", $msg, "]"; return true; });
echo call_user_func_array('preg_match', ['pattern' => '/a/', 'subject' => 'ba', 'offset' => 1]), call_user_func_array('preg_match', ['/a/', 'ba', 'offset' => 2]), "|";
$match_SFX = preg_match(...);
echo $match_SFX('/a/', 'ba', offset: 2), $match_SFX(pattern: '/a/', subject: 'bab', offset: 2, flags: 0), "|";
$match_all_SFX = preg_match_all(...);
echo call_user_func_array('preg_match_all', ['pattern' => '/a/', 'subject' => 'baa', 'offset' => 2]), $match_all_SFX('/a/', 'baaa', offset: 1), "|";
var_dump(call_user_func_array('preg_match', ['pattern' => '/a/', 'subject' => 'ba', 'offset' => 5]));
echo preg_last_error(), "|";
try { call_user_func_array('preg_match', ['pattern' => '/a/', 'subject' => 'ba', 'flags' => 99]); } catch (ValueError $e) { echo $e->getMessage(), "|"; }
try { $match_all_SFX('/a/', 'ba', flags: 99); } catch (ValueError $e) { echo $e->getMessage(); }
restore_error_handler();
"#;

/// The `$flags` / `$offset` forms record PHP's `preg_last_error()` codes, run in eval, and
/// bind named arguments through callables, in compiled code and through a runtime-built
/// `eval()`, sharing one compile. `SFX` keeps the bundled declarations unique.
#[test]
fn test_preg_match_window_bundle() {
    let internal = "1:Internal error";
    let utf8 = "4:Malformed UTF-8 characters, possibly incorrectly encoded";
    let offset = "5:The offset did not correspond to the beginning of a valid UTF-8 code point";
    let ok = "0:No error";
    let errors_expected = format!(
        "[[1,\"{ok}\"],[false,\"{utf8}\"],[false,\"{internal}\",[]],\
         [false,\"{offset}\"],[1,\"{ok}\"],[1,\"{ok}\"],[false,\"{utf8}\"],\
         [false,\"{internal}\",[]],[0,\"{ok}\"],[1,\"{ok}\"],[false,\"{utf8}\"],\
         [false,\"{internal}\"],[false,\"{offset}\"],[1,\"{ok}\"],[false,\"{internal}\"],\
         [false,\"{offset}\"],[false,\"{utf8}\"],[1,\"{ok}\"]]\n"
    );
    let invalid_expected = format!(
        "[[false,\"{internal}\"],[false,\"{internal}\"],[false,\"{internal}\"],\
         [false,\"{internal}\"],[1,\"{ok}\"]]\n"
    );
    let destructor_expected = "bool(false)\nint(1)\nint(0)\nbool(false)\nbool(false)\nint(1)\n\
                               bool(false)\nint(1)\nint(0)\n";
    let parity_expected = "1[[\"b\",3],[\"b\",3],[null,-1]]|1[[\"a\",\"a\"]]|bool(false)\n\
                           preg_match(): Argument #4 ($flags) must be a PREG_* constant";
    let named_expected = "10|00|13|bool(false)\n1|preg_match(): Argument #4 ($flags) must be a PREG_* constant|\
                          preg_match_all(): Argument #4 ($flags) must be a PREG_* constant";
    let aot = |body: &str| format!("<?php\n{}", body.replace("SFX", "aot"));
    let eval = |body: &str| opaque_eval(&body.replace("SFX", "eval"));
    let sources = [
        aot(WINDOW_LAST_ERROR),
        eval(WINDOW_LAST_ERROR),
        aot(WINDOW_MATCHES_DESTRUCTOR),
        eval(WINDOW_MATCHES_DESTRUCTOR),
        aot(WINDOW_INVALID_PATTERN),
        eval(WINDOW_EVAL_PARITY),
        aot(WINDOW_NAMED_CALLABLES),
        eval(WINDOW_NAMED_CALLABLES),
    ];
    assert_bundle_with(
        &[
            BundleCase { name: "window_last_error_aot", source: &sources[0], expected: &errors_expected },
            BundleCase { name: "window_last_error_eval", source: &sources[1], expected: &errors_expected },
            BundleCase { name: "window_destructor_aot", source: &sources[2], expected: destructor_expected },
            BundleCase { name: "window_destructor_eval", source: &sources[3], expected: destructor_expected },
            BundleCase { name: "window_invalid_pattern_aot", source: &sources[4], expected: &invalid_expected },
            BundleCase { name: "window_eval_parity", source: &sources[5], expected: parity_expected },
            BundleCase { name: "window_named_callables_aot", source: &sources[6], expected: named_expected },
            BundleCase { name: "window_named_callables_eval", source: &sources[7], expected: named_expected },
        ],
        compile_and_run_with_regex,
    );
}

/// Wraps a fixture body in an `eval()` whose source is only known at runtime, so it runs
/// through Magician rather than being compiled inline.
fn opaque_eval(body: &str) -> String {
    let quoted = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php\neval('{quoted}' . ' // ' . $argc);\n")
}
