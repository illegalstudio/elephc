//! Purpose:
//! Integration tests for the `Random\Engine\*` implementations, ported from php-src
//! `ext/random/tests/02_engine`: reference streams, state serialization, seeding and
//! constructor/jump validation.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Reference values are bit-exact for a fixed seed, so an off-by-one in tempering,
//!   the PCG rotation, or the xoshiro state transition fails here first.
//! - `__debugInfo()` is queried directly instead of through `var_dump()` so the
//!   assertions do not depend on object-handle numbering.

use crate::support::*;

/// `Mt19937(1234)` produces php-src's tenth-thousand-and-first 32-bit draw.
#[test]
fn test_random_engine_mt19937_reference_value() {
    let out = compile_and_run(
        r#"<?php
$engine = new Random\Engine\Mt19937(1234);
for ($i = 0; $i < 10_000; $i++) {
    $engine->generate();
}
var_dump(bin2hex($engine->generate()));
"#,
    );
    assert_eq!(out, "string(8) \"60fe95d9\"\n");
}

/// `PcgOneseq128XslRr64(1234)` matches php-src after 10 000 draws and a jump.
#[test]
fn test_random_engine_pcg_reference_value() {
    let out = compile_and_run(
        r#"<?php
$engine = new Random\Engine\PcgOneseq128XslRr64(1234);
for ($i = 0; $i < 10_000; $i++) {
    $engine->generate();
}
$engine->jump(1234567);
var_dump(bin2hex($engine->generate()));
"#,
    );
    assert_eq!(out, "string(16) \"b88e2a0f23720a1d\"\n");
}

/// `Xoshiro256StarStar(1234)` matches php-src after 10 000 draws, `jump()` and `jumpLong()`.
#[test]
fn test_random_engine_xoshiro_reference_value() {
    let out = compile_and_run(
        r#"<?php
$engine = new Random\Engine\Xoshiro256StarStar(1234);
for ($i = 0; $i < 10_000; $i++) {
    $engine->generate();
}
$engine->jump();
$engine->jumpLong();
var_dump(bin2hex($engine->generate()));
"#,
    );
    assert_eq!(out, "string(16) \"1f197e9ca7969123\"\n");
}

/// PCG state serialization is byte-stable and little-endian per 64-bit half.
#[test]
fn test_random_engine_pcg_serialize_shape() {
    let out = compile_and_run(
        r#"<?php
$engine = new Random\Engine\PcgOneseq128XslRr64(1234);
echo serialize($engine), "\n";
echo implode(",", $engine->__debugInfo()["__states"]), "\n";
"#,
    );
    assert_eq!(
        out,
        "O:33:\"Random\\Engine\\PcgOneseq128XslRr64\":2:{i:0;a:0:{}i:1;a:2:{i:0;s:16:\"c6d571c37c41a8d1\";i:1;s:16:\"345e7e82265d6e27\";}}\n\
c6d571c37c41a8d1,345e7e82265d6e27\n"
    );
}

/// Xoshiro state serialization is byte-stable for the four 64-bit words.
#[test]
fn test_random_engine_xoshiro_serialize_shape() {
    let out = compile_and_run(
        r#"<?php
$engine = new Random\Engine\Xoshiro256StarStar(1234);
echo serialize($engine), "\n";
echo implode(",", $engine->__debugInfo()["__states"]), "\n";
"#,
    );
    assert_eq!(
        out,
        "O:32:\"Random\\Engine\\Xoshiro256StarStar\":2:{i:0;a:0:{}i:1;a:4:{i:0;s:16:\"db1c182f1bf60cbb\";i:1;s:16:\"2465f04d36a1c797\";i:2;s:16:\"da25c09be4fabe33\";i:3;s:16:\"33a0d052f241624e\";}}\n\
db1c182f1bf60cbb,2465f04d36a1c797,da25c09be4fabe33,33a0d052f241624e\n"
    );
}

/// The MT19937 serialized envelope carries the twisted 624-word state plus count/mode.
#[test]
fn test_random_engine_mt19937_serialize_envelope() {
    let out = compile_and_run(
        r#"<?php
$serialized = serialize(new Random\Engine\Mt19937(1234));
echo strlen($serialized), "\n";
echo substr($serialized, 0, 52), "\n";
echo substr($serialized, -40), "\n";
$restored = unserialize($serialized);
var_dump(bin2hex($restored->generate()));
"#,
    );
    assert_eq!(
        out,
        "13069\nO:21:\"Random\\Engine\\Mt19937\":2:{i:0;a:0:{}i:1;a:626:\n\
23;s:8:\"dd86eeb1\";i:624;i:0;i:625;i:0;}}\n\
string(8) \"2f6b0731\"\n"
    );
}

/// A valid 16-byte PCG seed reproduces php-src's reference stream.
#[test]
fn test_random_engine_pcg_string_seed_reference() {
    let out = compile_and_run(
        r#"<?php
$engine = new Random\Engine\PcgOneseq128XslRr64(str_repeat("\x01\x02\x03\x04\x05\x06\x07\x08", 2));
for ($i = 0; $i < 1000; $i++) {
    $engine->generate();
}
var_dump(bin2hex($engine->generate()));
"#,
    );
    assert_eq!(out, "string(16) \"c42016cd9005ef2e\"\n");
}

/// A valid 32-byte xoshiro seed reproduces php-src's reference stream.
#[test]
fn test_random_engine_xoshiro_string_seed_reference() {
    let out = compile_and_run(
        r#"<?php
$engine = new Random\Engine\Xoshiro256StarStar(str_repeat("\x01\x02\x03\x04\x05\x06\x07\x08", 4));
for ($i = 0; $i < 1000; $i++) {
    $engine->generate();
}
var_dump(bin2hex($engine->generate()));
"#,
    );
    assert_eq!(out, "string(16) \"90a025df9300cfd1\"\n");
}

/// `Mt19937` rejects a mode other than MT_RAND_MT19937/MT_RAND_PHP.
#[test]
fn test_random_engine_mt19937_rejects_unknown_mode() {
    let out = compile_and_run(
        r#"<?php
try {
    new Random\Engine\Mt19937(1234, 2);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "Random\\Engine\\Mt19937::__construct(): Argument #2 ($mode) must be either MT_RAND_MT19937 or MT_RAND_PHP\n"
    );
}

/// PCG rejects a seed string that is not exactly 16 bytes.
#[test]
fn test_random_engine_pcg_rejects_wrong_seed_length() {
    let out = compile_and_run(
        r#"<?php
try {
    new Random\Engine\PcgOneseq128XslRr64("foobar");
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "Random\\Engine\\PcgOneseq128XslRr64::__construct(): Argument #1 ($seed) must be a 16 byte (128 bit) string\n"
    );
}

/// Xoshiro rejects a seed string that is not exactly 32 bytes.
#[test]
fn test_random_engine_xoshiro_rejects_wrong_seed_length() {
    let out = compile_and_run(
        r#"<?php
try {
    new Random\Engine\Xoshiro256StarStar("foobar");
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "Random\\Engine\\Xoshiro256StarStar::__construct(): Argument #1 ($seed) must be a 32 byte (256 bit) string\n"
    );
}

/// Xoshiro rejects an all-NUL string seed.
#[test]
fn test_random_engine_xoshiro_rejects_nul_seed() {
    let out = compile_and_run(
        r#"<?php
try {
    new Random\Engine\Xoshiro256StarStar(str_repeat("\x00", 32));
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "Random\\Engine\\Xoshiro256StarStar::__construct(): Argument #1 ($seed) must not consist entirely of NUL bytes\n"
    );
}

/// A negative PCG jump is rejected without disturbing the engine state.
#[test]
fn test_random_engine_pcg_jump_rejects_negative_advance() {
    let out = compile_and_run(
        r#"<?php
$engine = new Random\Engine\PcgOneseq128XslRr64(1234);
$reference = new Random\Engine\PcgOneseq128XslRr64(1234);
try {
    $engine->jump(-1);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
for ($i = 0; $i < 10_000; $i++) {
    if ($engine->generate() !== $reference->generate()) {
        echo "failure";
        exit(1);
    }
}
echo "success";
"#,
    );
    assert_eq!(
        out,
        "Random\\Engine\\PcgOneseq128XslRr64::jump(): Argument #1 ($advance) must be greater than or equal to 0\nsuccess"
    );
}

/// Unserializing an all-zero xoshiro state is rejected (GH-21731).
#[test]
fn test_random_engine_xoshiro_rejects_zero_unserialized_state() {
    let out = compile_and_run(
        r#"<?php
try {
    var_dump(unserialize('O:32:"Random\Engine\Xoshiro256StarStar":2:{i:0;a:0:{}i:1;a:4:{i:0;s:16:"0000000000000000";i:1;s:16:"0000000000000000";i:2;s:16:"0000000000000000";i:3;s:16:"0000000000000000";}}'));
} catch (Exception $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "Invalid serialization data for Random\\Engine\\Xoshiro256StarStar object\n"
    );
}

/// Cloning an engine deep-copies its state.
#[test]
fn test_random_engine_clone_is_independent() {
    let out = compile_and_run(
        r#"<?php
$engine = new Random\Engine\Mt19937(1234);
$clone = clone $engine;
var_dump($engine->generate() === $clone->generate());
$engine->generate();
$engine->generate();
var_dump($engine->generate() === $clone->generate());
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\n");
}

/// `Secure::generate()` returns eight CSPRNG bytes and refuses serialization.
#[test]
fn test_random_engine_secure_generate_and_serialize() {
    let out = compile_and_run(
        r#"<?php
$engine = new Random\Engine\Secure();
var_dump(strlen($engine->generate()));
var_dump($engine instanceof Random\CryptoSafeEngine);
try {
    serialize($engine);
} catch (Exception $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "int(8)\nbool(true)\nSerialization of 'Random\\Engine\\Secure' is not allowed\n"
    );
}


/// Seeded engine constructors reject non-integer/string seeds with PHP's runtime `TypeError`.
#[test]
fn test_random_engine_seed_type_errors() {
    let out = compile_and_run(
        r#"<?php
try {
    new Random\Engine\PcgOneseq128XslRr64(1.0);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
try {
    new Random\Engine\Xoshiro256StarStar(1.0);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
try {
    new Random\Engine\Mt19937(1.0);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "Random\\Engine\\PcgOneseq128XslRr64::__construct(): Argument #1 ($seed) must be of type string|int|null, float given\n\
Random\\Engine\\Xoshiro256StarStar::__construct(): Argument #1 ($seed) must be of type string|int|null, float given\n\
Random\\Engine\\Mt19937::__construct(): Argument #1 ($seed) must be of type ?int, float given\n"
    );
}
