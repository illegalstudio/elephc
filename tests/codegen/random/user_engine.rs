//! Purpose:
//! Integration tests for user-defined `Random\Engine` implementations consumed by
//! `Random\Randomizer`, ported from php-src `ext/random/tests/02_engine` and
//! `03_randomizer/engine_unsafe_*`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - These fixtures pin the adapter rules: eight-byte cap, little-endian accumulation,
//!   by-reference returns, empty-string rejection and the 50-attempt guard.

use crate::support::*;

/// A by-reference user engine is dereferenced and consumed byte-at-a-time.
#[test]
fn test_random_user_engine_reference_return() {
    let out = compile_and_run(
        r#"<?php
final class ReferenceEngine implements Random\Engine {
    private $field = "abcdef";
    public function &generate(): string {
        return $this->field;
    }
}
$r = new Random\Randomizer(new ReferenceEngine());
var_dump($r->getBytes(64));
"#,
    );
    assert_eq!(
        out,
        "string(64) \"abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd\"
"
    );
}

/// An empty engine string is rejected with php-src's exact exception.
#[test]
fn test_random_user_engine_rejects_empty_string() {
    let out = compile_and_run(
        r#"<?php
final class EmptyEngine implements Random\Engine {
    public function generate(): string {
        return "";
    }
}
try {
    (new Random\Randomizer(new EmptyEngine()))->nextInt();
} catch (Random\BrokenRandomEngineError $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(out, "A random engine must return a non-empty string\n");
}

/// A constant all-0xFF engine is rejected by bounded ranges after 50 attempts.
#[test]
fn test_random_user_engine_biased_rejects_bounded_range() {
    let out = compile_and_run(
        r#"<?php
final class BiasedEngine implements Random\Engine {
    public function generate(): string {
        return str_repeat("\xff", 8);
    }
}
try {
    (new Random\Randomizer(new BiasedEngine()))->getInt(0, 1234);
} catch (Random\BrokenRandomEngineError $e) {
    echo $e->getMessage(), "\n";
}
$r = new Random\Randomizer(new BiasedEngine());
var_dump($r->nextInt());
"#,
    );
    assert_eq!(
        out,
        "Failed to generate an acceptable random number in 50 attempts\n\
int(9223372036854775807)\n"
    );
}

/// All-NUL engine output is valid and yields zero draws.
#[test]
fn test_random_user_engine_nul_output() {
    let out = compile_and_run(
        r#"<?php
final class NulEngine implements Random\Engine {
    public function generate(): string {
        return str_repeat("\x00", 8);
    }
}
$r = new Random\Randomizer(new NulEngine());
var_dump($r->getInt(0, 1234));
$r = new Random\Randomizer(new NulEngine());
var_dump($r->nextInt());
$r = new Random\Randomizer(new NulEngine());
var_dump(bin2hex($r->getBytes(16)));
"#,
    );
    assert_eq!(out, "int(0)\nint(0)\nstring(32) \"00000000000000000000000000000000\"\n");
}

/// A wrapper user engine reproduces the wrapped native engine's `getInt()` stream.
#[test]
fn test_random_user_engine_wrapper_compatibility() {
    let out = compile_and_run(
        r#"<?php
final class WrapperEngine implements Random\Engine {
    public function __construct(private Random\Engine $inner) {}
    public function generate(): string {
        return $this->inner->generate();
    }
}
$engine = new Random\Engine\Xoshiro256StarStar(1234);
$native = new Random\Randomizer(clone $engine);
$wrapper = new Random\Randomizer(new WrapperEngine(clone $engine));
$ok = true;
for ($i = 0; $i < 2000; $i++) {
    if ($native->getInt(0, $i) !== $wrapper->getInt(0, $i)) {
        $ok = false;
    }
}
var_dump($ok);
"#,
    );
    assert_eq!(out, "bool(true)\n");
}

/// Engine output longer than eight bytes is capped to its first eight bytes.
#[test]
fn test_random_user_engine_output_is_capped_at_eight_bytes() {
    let out = compile_and_run(
        r#"<?php
final class LongEngine implements Random\Engine {
    public $count = 0;
    public function generate(): string {
        $this->count++;
        return "\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c";
    }
}
$r = new Random\Randomizer(new LongEngine());
var_dump($r->getInt(0, 0x00FF_FFFF_FFFF_FFFF));
$engine = new LongEngine();
$r = new Random\Randomizer($engine);
$r->getBytes(8);
var_dump($engine->count);
"#,
    );
    assert_eq!(out, "int(1976943448883713)\nint(1)\n");
}

/// The Randomizer's default engine is `Random\Engine\Secure`.
#[test]
fn test_random_randomizer_default_engine_is_secure() {
    let out = compile_and_run(
        r#"<?php
$r = new Random\Randomizer();
var_dump($r->engine instanceof Random\Engine\Secure);
var_dump($r->getInt(0, 10) >= 0);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\n");
}
