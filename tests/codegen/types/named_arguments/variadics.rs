//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of types, named arguments variadics, including named arguments unknown variadic named args keep string keys, named arguments variadic mixes positional and named extra args, and named arguments variadic after long spread keeps tail and named args.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies user-defined variadic functions accept unknown named args as string-keyed variadic entries;
/// `show(head: 1, extra: 2)` outputs "extra=2;" with "extra" as the string key in `...$rest`.
#[test]
fn test_named_arguments_unknown_variadic_named_args_keep_string_keys() {
    let out = compile_and_run(
        r#"<?php
function show($head, ...$rest) {
    foreach ($rest as $key => $value) {
        echo $key . "=" . $value . ";";
    }
}
show(head: 1, extra: 2);
"#,
    );
    assert_eq!(out, "extra=2;");
}

/// Verifies user-defined variadic functions mix positional and named extra args;
/// `show(1, 2, extra: 3)` outputs "0=2;extra=3;" (positional in `$rest` with numeric keys, named with string key).
#[test]
fn test_named_arguments_variadic_mixes_positional_and_named_extra_args() {
    let out = compile_and_run(
        r#"<?php
function show($head, ...$rest) {
    foreach ($rest as $key => $value) {
        echo $key . "=" . $value . ";";
    }
}
show(1, 2, extra: 3);
"#,
    );
    assert_eq!(out, "0=2;extra=3;");
}

/// Verifies a spread longer than the required params followed by named args fills the variadic tail correctly;
/// `show(...[1, 2, 3], extra: 4)` outputs "head=1;0=2;1=3;extra=4;".
#[test]
fn test_named_arguments_variadic_after_long_spread_keeps_tail_and_named_args() {
    let out = compile_and_run(
        r#"<?php
function show($head, ...$rest) {
    echo "head=" . $head . ";";
    foreach ($rest as $key => $value) {
        echo $key . "=" . $value . ";";
    }
}
show(...[1, 2, 3], extra: 4);
"#,
    );
    assert_eq!(out, "head=1;0=2;1=3;extra=4;");
}

/// Verifies a spread that exactly fills the variadic head followed by named args works correctly;
/// `show(...[1], extra: 4)` outputs "head=1;extra=4;".
#[test]
fn test_named_arguments_variadic_after_exact_spread_keeps_named_arg() {
    let out = compile_and_run(
        r#"<?php
function show($head, ...$rest) {
    echo "head=" . $head . ";";
    foreach ($rest as $key => $value) {
        echo $key . "=" . $value . ";";
    }
}
show(...[1], extra: 4);
"#,
    );
    assert_eq!(out, "head=1;extra=4;");
}

/// Verifies static assoc spread supplying named args before a positional spread that fills variadic tail;
/// `sum_variadic(...["head" => 10], ...[20, 30])` outputs "60".
#[test]
fn test_static_assoc_spread_named_then_positional_spread_variadic_tail() {
    let out = compile_and_run(
        r#"<?php
function stamp_triplet($a, $b, $c = 30) {
    echo $a;
    echo ",";
    echo $b;
    echo ",";
    echo $c;
    echo "\n";
}

function sum_variadic($head, ...$rest) {
    $total = $head;
    foreach ($rest as $v) {
        $total += $v;
    }
    echo $total;
    echo "\n";
}

stamp_triplet(...["b" => 2, "a" => 1], c: 3);
sum_variadic(...["head" => 10], ...[20, 30]);
"#,
    );
    assert_eq!(out, "1,2,3\n60\n");
}

/// Verifies multiple positional spreads continue into the variadic tail and fill it;
/// `f(...[10], ...[7, 30])` outputs "10:7:[30]" (a=10, b=7 from second spread, rest=[30]).
#[test]
fn test_multiple_positional_spreads_continue_into_variadic_tail() {
    let out = compile_and_run(
        r#"<?php
function f($a, $b = 20, ...$rest) {
    echo $a . ":" . $b . ":" . json_encode($rest);
}
f(...[10], ...[7, 30]);
"#,
    );
    assert_eq!(out, "10:7:[30]");
}

/// Verifies an assoc spread after a positional spread keeps extra string-keyed entries in the variadic;
/// `f(...[10], ...["b" => 7, "x" => 30])` outputs "10:7:{\"x\":30}".
#[test]
fn test_assoc_spread_extra_after_positional_spread_keeps_variadic_key() {
    let out = compile_and_run(
        r#"<?php
function f($a, $b = 20, ...$rest) {
    echo $a . ":" . $b . ":" . json_encode($rest);
}
f(...[10], ...["b" => 7, "x" => 30]);
"#,
    );
    assert_eq!(out, "10:7:{\"x\":30}");
}

/// Verifies multiple assoc spread extras after a positional spread are all kept in the variadic tail;
/// `f(...[10], ...["b" => 7, "x" => 30, "y" => 40])` outputs "10:7:{\"x\":30,\"y\":40}".
#[test]
fn test_assoc_spread_extras_after_positional_spread_keep_variadic_keys() {
    let out = compile_and_run(
        r#"<?php
function f($a, $b = 20, ...$rest) {
    echo $a . ":" . $b . ":" . json_encode($rest);
}
f(...[10], ...["b" => 7, "x" => 30, "y" => 40]);
"#,
    );
    assert_eq!(out, "10:7:{\"x\":30,\"y\":40}");
}

/// Verifies `collect(...$hash)` into a user variadic binds string keys as named extras in
/// `$args`, as PHP does: through a function, a relay variadic, an instance method and a static
/// method, with and without a leading positional argument, and for a hash returned by a call.
#[test]
fn test_spread_of_a_hash_into_a_user_variadic_keeps_the_string_keys() {
    let out = compile_and_run(
        r#"<?php
function collect(...$args) { return json_encode($args); }
function relay(...$args) { return collect(...$args); }
function mk(int $n) { $r = []; for ($i = 0; $i < $n; $i++) { $r["k$i"] = $i; } return $r; }
class Bag {
    public function take(...$args) { return json_encode($args); }
    public static function grab(...$args) { return json_encode($args); }
}
$p = ['view' => 'v', 'data' => [1]];
$list = [1, 2, 3];
$bag = new Bag();
echo collect(...$p), "\n";
echo collect(...$list), "\n";
echo collect(...mk(2)), "\n";
echo collect(0, ...$p), "\n";
echo relay(...$p), "\n";
echo $bag->take(...$p), "\n";
echo $bag->take(0, ...$p), "\n";
echo Bag::grab(...$p), "\n";
echo Bag::grab(7, ...$p), "\n";
$empty = [];
echo collect(...$empty), "\n";
"#,
    );
    assert_eq!(
        out,
        "{\"view\":\"v\",\"data\":[1]}\n[1,2,3]\n{\"k0\":0,\"k1\":1}\n{\"0\":0,\"view\":\"v\",\"data\":[1]}\n{\"view\":\"v\",\"data\":[1]}\n{\"view\":\"v\",\"data\":[1]}\n{\"0\":0,\"view\":\"v\",\"data\":[1]}\n{\"view\":\"v\",\"data\":[1]}\n{\"0\":7,\"view\":\"v\",\"data\":[1]}\n[]\n"
    );
}
