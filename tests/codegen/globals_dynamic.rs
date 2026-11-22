//! Purpose:
//! End-to-end tests for computed-key `$GLOBALS[$name]` reads and writes (php 8.1+).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Expected output was produced by `php -n` on the same fixture.

use crate::support::*;

/// Reads, assignments, compound operators, `??=`, `++`/`--`, `isset`, `unset`, and nested
/// writes through computed `$GLOBALS` keys behave like php's.
#[test]
fn test_computed_globals_key_reads_and_writes_match_php() {
    let out = compile_and_run(
        r#"
<?php
$a = 1;
$s = 'x';
$arr = [1, 2];
$n = 5;
$u = 'gone';
function writes(string $name, string $other) {
    $GLOBALS[$name] = 10;
    $GLOBALS[$other] .= 'y';
    $GLOBALS['n'] = $GLOBALS['n'] + 1;
    $GLOBALS[$name] += 5;
    $GLOBALS[$name]++;
    ++$GLOBALS[$name];
    $GLOBALS[$name]--;
    echo $GLOBALS[$name], "\n";
    echo isset($GLOBALS[$name]) ? "set" : "unset", "\n";
    echo isset($GLOBALS['nope_' . $name]) ? "set" : "unset", "\n";
    $GLOBALS[$name . '_fresh'] = 'created';
    echo $GLOBALS[$name . '_fresh'], "\n";
    $GLOBALS['arr'][] = 3;
    $GLOBALS[$name . 'arr'] = [1];
    $v = $GLOBALS[$name] = 77;
    echo $v, "\n";
    $GLOBALS[$other] ??= 'never';
    $GLOBALS[$name . 'l'] ??= 'dflt'; echo $GLOBALS['al'], "\n";
    unset($GLOBALS['u']);
    $k = 'a';
    unset($GLOBALS[$k]);
    echo isset($GLOBALS[$k]) ? "a set" : "a unset", "\n";
}
writes('a', 's');
var_dump($a ?? 'a-null', $s, $n, $late ?? 'no-late');
function nested(string $k) {
    $GLOBALS[$k][] = 9;
    $GLOBALS[$k]['x'] = 8;
    echo json_encode($GLOBALS[$k]), "\n";
    echo $GLOBALS[$k]['x'], "\n";
    $GLOBALS[$k . 'new']['deep'] = 1;
    echo json_encode($GLOBALS[$k . 'new']), "\n";
}
$list = [1];
nested('list');
echo json_encode($list), "\n";
echo $arr[2], count($arr), "\n";
"#,
    );
    assert_eq!(
        out,
        "16\nset\nunset\ncreated\n77\ndflt\na unset\nstring(6) \"a-null\"\nstring(2) \"xy\"\nint(6)\nstring(7) \"no-late\"\n{\"0\":1,\"1\":9,\"x\":8}\n8\n{\"deep\":1}\n{\"0\":1,\"1\":9,\"x\":8}\n33\n"
    );
}
