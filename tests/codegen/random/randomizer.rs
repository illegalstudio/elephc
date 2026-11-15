//! Purpose:
//! Integration tests for `Random\Randomizer`: engine adoption, integer/float/byte
//! generation, collection shuffling and key picking, ported from php-src
//! `ext/random/tests/03_randomizer`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Tests use fixed-seed native engines wherever php-src pins an exact value, and
//!   small deterministic user engines for the accumulation/rejection paths.
//! - `pack()`-based php-src fixtures are re-expressed as plain integer assertions.

use crate::support::*;

/// `getInt()` on a fixed-seed MT19937 reproduces php-src's GH-9415 values.
#[test]
fn test_random_randomizer_get_int_gh9415() {
    let out = compile_and_run(
        r#"<?php
$r = new Random\Randomizer(new Random\Engine\Mt19937(1234));
var_dump($r->getInt(-2147483648, 2147483647));
$r = new Random\Randomizer(new Random\Engine\Mt19937(4321));
var_dump($r->getInt(-2147483648, 2147483647));
"#,
    );
    assert_eq!(out, "int(-1324913873)\nint(-1843387587)\n");
}

/// `getInt()` is always inside its inclusive range and accepts the full int range.
#[test]
fn test_random_randomizer_get_int_ranges() {
    let out = compile_and_run(
        r#"<?php
$r = new Random\Randomizer(new Random\Engine\PcgOneseq128XslRr64(1234));
$ok = true;
for ($i = 0; $i < 2000; $i++) {
    $v = $r->getInt(-$i, $i);
    if ($v < -$i || $v > $i) {
        $ok = false;
    }
}
for ($i = 0; $i < 2000; $i++) {
    $r->getInt(PHP_INT_MIN, PHP_INT_MAX);
}
var_dump($ok);
"#,
    );
    assert_eq!(out, "bool(true)\n");
}

/// The legacy `getInt()` scaling keeps integer precision on large bounds.
#[test]
fn test_random_randomizer_get_int_legacy_integer_bounds() {
    let out = compile_and_run(
        r#"<?php
$r = new Random\Randomizer(new Random\Engine\Mt19937(0, 1));
var_dump($r->getInt(9007199254740993, 9007199254740993));
var_dump($r->getInt(-9007199254740993, -9007199254740993));
var_dump($r->getInt(PHP_INT_MAX, PHP_INT_MAX));
var_dump($r->getInt(PHP_INT_MIN, PHP_INT_MIN));
var_dump($r->getInt(PHP_INT_MIN, PHP_INT_MIN + 10));
var_dump($r->getInt(0, 100));
"#,
    );
    assert_eq!(
        out,
        "int(9007199254740993)\n\
int(-9007199254740993)\n\
int(9223372036854775807)\n\
int(-9223372036854775808)\n\
int(-9223372036854775808)\n\
int(86)\n"
    );
}

/// A one-byte user engine is accumulated little-endian for 32-bit ranges.
#[test]
fn test_random_randomizer_get_int_one_byte_engine_32() {
    let out = compile_and_run(
        r#"<?php
final class ByteEngine implements Random\Engine {
    public $count = 0;
    public function generate(): string {
        return "\x01\x02\x03\x04\x05\x06\x07\x08"[$this->count++];
    }
}
$r = new Random\Randomizer(new ByteEngine());
var_dump($r->getInt(0, 0x00FF_FFFF));
"#,
    );
    assert_eq!(out, "int(197121)\n");
}

/// A one-byte user engine is accumulated little-endian for 64-bit ranges.
#[test]
fn test_random_randomizer_get_int_one_byte_engine_64() {
    let out = compile_and_run(
        r#"<?php
final class ByteEngine2 implements Random\Engine {
    public $count = 0;
    public function generate(): string {
        return "\x01\x02\x03\x04\x05\x06\x07\x08"[$this->count++];
    }
}
$r = new Random\Randomizer(new ByteEngine2());
var_dump($r->getInt(0, 0x00FF_FFFF_FFFF_FFFF));
"#,
    );
    assert_eq!(out, "int(1976943448883713)\n");
}

/// `nextInt()` stays non-negative on a 64-bit host.
#[test]
fn test_random_randomizer_next_int() {
    let out = compile_and_run(
        r#"<?php
$r = new Random\Randomizer(new Random\Engine\Xoshiro256StarStar(1234));
$ok = true;
for ($i = 0; $i < 2000; $i++) {
    if ($r->nextInt() < 0) {
        $ok = false;
    }
}
var_dump($ok);
"#,
    );
    assert_eq!(out, "bool(true)\n");
}

/// `getBytes()` returns exactly the requested length for fast-path and byte-at-a-time engines.
#[test]
fn test_random_randomizer_get_bytes_lengths() {
    let out = compile_and_run(
        r#"<?php
foreach ([new Random\Engine\Mt19937(1234), new Random\Engine\PcgOneseq128XslRr64(1234)] as $engine) {
    $r = new Random\Randomizer($engine);
    $ok = true;
    for ($i = 1; $i < 200; $i++) {
        if (strlen($r->getBytes($i)) !== $i) {
            $ok = false;
        }
    }
    var_dump($ok);
}
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\n");
}

/// `getBytesFromString()` emits only source bytes and validates both arguments.
#[test]
fn test_random_randomizer_get_bytes_from_string() {
    let out = compile_and_run(
        r#"<?php
$r = new Random\Randomizer(new Random\Engine\Xoshiro256StarStar(1234));
var_dump($r->getBytesFromString("a", 10));
$source = "";
for ($i = 0; $i < 256; $i++) {
    $source .= chr($i);
}
$out = $r->getBytesFromString($source, 20000);
$seen = [];
for ($i = 0; $i < strlen($out); $i++) {
    $seen[ord($out[$i])] = true;
}
var_dump(count($seen));
try {
    $r->getBytesFromString("", 1);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
try {
    $r->getBytesFromString("abc", 0);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "string(10) \"aaaaaaaaaa\"\nint(256)\n\
Random\\Randomizer::getBytesFromString(): Argument #1 ($string) must not be empty\n\
Random\\Randomizer::getBytesFromString(): Argument #2 ($length) must be greater than 0\n"
    );
}

/// `shuffleArray()` returns a permutation of the input values.
#[test]
fn test_random_randomizer_shuffle_array_permutation() {
    let out = compile_and_run(
        r#"<?php
$r = new Random\Randomizer(new Random\Engine\Mt19937(1234));
$ok = true;
foreach ([1, 2, 3, 10, 49, 300] as $n) {
    $input = range(1, $n);
    $result = $r->shuffleArray($input);
    sort($result);
    if ($result !== $input) {
        $ok = false;
    }
}
var_dump($ok);
"#,
    );
    assert_eq!(out, "bool(true)\n");
}

/// `shuffleBytes()` returns a permutation of the input bytes.
#[test]
fn test_random_randomizer_shuffle_bytes_permutation() {
    let out = compile_and_run(
        r#"<?php
$r = new Random\Randomizer(new Random\Engine\PcgOneseq128XslRr64(1234));
$input = "abcdefghij";
$result = $r->shuffleBytes($input);
$chars = [];
for ($i = 0; $i < strlen($result); $i++) {
    $chars[] = $result[$i];
}
sort($chars);
$expected = [];
for ($i = 0; $i < strlen($input); $i++) {
    $expected[] = $input[$i];
}
sort($expected);
var_dump($chars === $expected);
var_dump($r->shuffleBytes("") === "");
var_dump($r->shuffleBytes("x") === "x");
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\nbool(true)\n");
}

/// `pickArrayKeys()` returns unique keys of the source array.
#[test]
fn test_random_randomizer_pick_array_keys() {
    let out = compile_and_run(
        r#"<?php
$r = new Random\Randomizer(new Random\Engine\Xoshiro256StarStar(1234));
$list = range(1, 200);
$assoc = [];
for ($i = 0; $i < 200; $i++) {
    $assoc["k" . $i] = $i;
}
function pickCheck(Random\Randomizer $r, array $array): bool {
    $keys = array_keys($array);
    foreach ([1, 2, 7, 199] as $num) {
        $picked = $r->pickArrayKeys($array, $num);
        if (count($picked) !== $num) {
            return false;
        }
        $seen = [];
        foreach ($picked as $key) {
            if (!in_array($key, $keys, true)) {
                return false;
            }
            if (in_array($key, $seen, true)) {
                return false;
            }
            $seen[] = $key;
        }
    }
    return true;
}
var_dump(pickCheck($r, $list));
var_dump(pickCheck($r, $assoc));
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\n");
}

/// `pickArrayKeys()` rejects empty arrays and out-of-range counts.
#[test]
fn test_random_randomizer_pick_array_keys_errors() {
    let out = compile_and_run(
        r#"<?php
$r = new Random\Randomizer(new Random\Engine\Mt19937(1234));
try {
    $empty = array_fill(0, 0, 0);
    $r->pickArrayKeys($empty, 1);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
try {
    $r->pickArrayKeys([1, 2, 3], 0);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
try {
    $r->pickArrayKeys([1, 2, 3], 4);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "Random\\Randomizer::pickArrayKeys(): Argument #1 ($array) must not be empty\n\
Random\\Randomizer::pickArrayKeys(): Argument #2 ($num) must be between 1 and the number of elements in argument #1 ($array)\n\
Random\\Randomizer::pickArrayKeys(): Argument #2 ($num) must be between 1 and the number of elements in argument #1 ($array)\n"
    );
}

/// `getFloat()` stays in the interval and is monotonically sampled.
#[test]
fn test_random_randomizer_get_float_range() {
    let out = compile_and_run(
        r#"<?php
$r = new Random\Randomizer(new Random\Engine\Xoshiro256StarStar(1234));
$ok = true;
for ($i = 0.0; $i < 2000.0; $i += 1.2345) {
    $v = $r->getFloat(-$i, $i, Random\IntervalBoundary::ClosedClosed);
    if ($v > $i || $v < -$i) {
        $ok = false;
    }
}
for ($i = 0; $i < 1000; $i++) {
    $v = $r->nextFloat();
    if ($v < 0.0 || $v >= 1.0) {
        $ok = false;
    }
}
var_dump($ok);
"#,
    );
    assert_eq!(out, "bool(true)\n");
}

/// `getFloat()` validation order and messages match php-src for every boundary.
#[test]
fn test_random_randomizer_get_float_errors() {
    let out = compile_and_run(
        r#"<?php
foreach ([
    Random\IntervalBoundary::ClosedClosed,
    Random\IntervalBoundary::ClosedOpen,
    Random\IntervalBoundary::OpenClosed,
    Random\IntervalBoundary::OpenOpen,
] as $boundary) {
    echo $boundary->name, "\n";
    foreach ([
        [NAN, 0.0], [INF, 0.0], [-INF, 0.0],
        [0.0, NAN], [0.0, INF], [0.0, -INF],
        [0.0, -0.1], [0.0, 0.0],
        [100_000_000_000_000_000.0, 100_000_000_000_000_000.1],
        [1.0, 1.0 + PHP_FLOAT_EPSILON],
    ] as [$min, $max]) {
        try {
            $r = new Random\Randomizer(new Random\Engine\Mt19937(1234));
            $r->getFloat($min, $max, $boundary);
        } catch (ValueError $e) {
            echo $e->getMessage(), "\n";
        }
    }
}
"#,
    );
    assert_eq!(
        out,
        "ClosedClosed\n\
Random\\Randomizer::getFloat(): Argument #1 ($min) must be finite\n\
Random\\Randomizer::getFloat(): Argument #1 ($min) must be finite\n\
Random\\Randomizer::getFloat(): Argument #1 ($min) must be finite\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be finite\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be finite\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be finite\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be greater than or equal to argument #1 ($min)\n\
ClosedOpen\n\
Random\\Randomizer::getFloat(): Argument #1 ($min) must be finite\n\
Random\\Randomizer::getFloat(): Argument #1 ($min) must be finite\n\
Random\\Randomizer::getFloat(): Argument #1 ($min) must be finite\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be finite\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be finite\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be finite\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be greater than argument #1 ($min)\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be greater than argument #1 ($min)\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be greater than argument #1 ($min)\n\
OpenClosed\n\
Random\\Randomizer::getFloat(): Argument #1 ($min) must be finite\n\
Random\\Randomizer::getFloat(): Argument #1 ($min) must be finite\n\
Random\\Randomizer::getFloat(): Argument #1 ($min) must be finite\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be finite\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be finite\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be finite\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be greater than argument #1 ($min)\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be greater than argument #1 ($min)\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be greater than argument #1 ($min)\n\
OpenOpen\n\
Random\\Randomizer::getFloat(): Argument #1 ($min) must be finite\n\
Random\\Randomizer::getFloat(): Argument #1 ($min) must be finite\n\
Random\\Randomizer::getFloat(): Argument #1 ($min) must be finite\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be finite\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be finite\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be finite\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be greater than argument #1 ($min)\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be greater than argument #1 ($min)\n\
Random\\Randomizer::getFloat(): Argument #2 ($max) must be greater than argument #1 ($min)\n\
The given interval is empty, there are no floats between argument #1 ($min) and argument #2 ($max)\n"
    );
}

/// A second `__construct()` call cannot replace the readonly engine.
#[test]
fn test_random_randomizer_construct_twice() {
    let out = compile_and_run(
        r#"<?php
try {
    (new Random\Randomizer())->__construct();
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
$r = new Random\Randomizer(new Random\Engine\Xoshiro256StarStar(1234));
try {
    $r->__construct(new Random\Engine\PcgOneseq128XslRr64(1234));
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
var_dump($r->engine::class);
"#,
    );
    assert_eq!(
        out,
        "Cannot modify readonly property Random\\Randomizer::$engine\n\
Cannot modify readonly property Random\\Randomizer::$engine\n\
string(32) \"Random\\Engine\\Xoshiro256StarStar\"\n"
    );
}

/// Serializing a Randomizer round-trips its engine state.
#[test]
fn test_random_randomizer_serialize_round_trip() {
    let out = compile_and_run(
        r#"<?php
$r = new Random\Randomizer(new Random\Engine\Xoshiro256StarStar(1234));
$restored = unserialize(serialize($r));
$ok = true;
for ($i = 0; $i < 1000; $i++) {
    if ($r->getInt(0, $i) !== $restored->getInt(0, $i)) {
        $ok = false;
    }
}
var_dump($ok);
var_dump($restored->engine instanceof Random\Engine\Xoshiro256StarStar);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\n");
}

/// A `Secure` engine cannot be serialized through a Randomizer.
#[test]
fn test_random_randomizer_serialize_secure_is_rejected() {
    let out = compile_and_run(
        r#"<?php
try {
    serialize(new Random\Randomizer(new Random\Engine\Secure()));
} catch (Exception $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "Serialization of 'Random\\Engine\\Secure' is not allowed\n"
    );
}
