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

/// `Mt19937` coerces convertible seeds in weak mode, like `Z_PARAM_LONG_OR_NULL`.
#[test]
fn test_random_mt19937_weak_seed_coercion() {
    let output = compile_and_run_capture(
        r#"<?php
foreach ([[1.0, 1], ["1", 1], [true, 1], [1.5, 1], ["1.5", 1], [false, 0], ["0", 0], [0.0, 0]] as [$seed, $expected]) {
    $actual = new Random\Engine\Mt19937($seed);
    $reference = new Random\Engine\Mt19937($expected);
    echo $actual->generate() === $reference->generate() ? "same\n" : "diff\n";
}
foreach (["abc", []] as $seed) {
    try {
        new Random\Engine\Mt19937($seed);
        echo "no error\n";
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
}
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(
        output.stdout,
        "same\nsame\nsame\nsame\nsame\nsame\nsame\nsame\n\
Random\\Engine\\Mt19937::__construct(): Argument #1 ($seed) must be of type ?int, string given\n\
Random\\Engine\\Mt19937::__construct(): Argument #1 ($seed) must be of type ?int, array given\n"
    );
    // Coercion is observable: the two lossy seeds must keep PHP's deprecations.
    assert_eq!(
        output
            .stderr
            .matches("loses precision")
            .count(),
        2,
        "{}",
        output.stderr
    );
    assert!(
        output
            .stderr
            .contains("Implicit conversion from float 1.5 to int loses precision"),
        "{}",
        output.stderr
    );
    assert!(
        output
            .stderr
            .contains("Implicit conversion from float-string \"1.5\" to int loses precision"),
        "{}",
        output.stderr
    );
}

/// Seed values outside PHP's integer binding rules raise the constructor's `TypeError`.
#[test]
fn test_random_mt19937_seed_out_of_range_values() {
    let out = compile_and_run(
        r#"<?php
foreach (["9223372036854775808", "999999999999999999999999", 1e20, NAN, INF] as $seed) {
    try {
        new Random\Engine\Mt19937($seed);
        echo "no error\n";
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
}
try {
    new Random\Engine\Mt19937(new stdClass());
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "Random\\Engine\\Mt19937::__construct(): Argument #1 ($seed) must be of type ?int, string given\n\
Random\\Engine\\Mt19937::__construct(): Argument #1 ($seed) must be of type ?int, string given\n\
Random\\Engine\\Mt19937::__construct(): Argument #1 ($seed) must be of type ?int, float given\n\
Random\\Engine\\Mt19937::__construct(): Argument #1 ($seed) must be of type ?int, float given\n\
Random\\Engine\\Mt19937::__construct(): Argument #1 ($seed) must be of type ?int, float given\n\
Random\\Engine\\Mt19937::__construct(): Argument #1 ($seed) must be of type ?int, stdClass given\n"
    );
}

/// Integer strings bind through `zend_strtol`, saturating at `PHP_INT_MIN` on negative overflow.
#[test]
fn test_random_mt19937_seed_integer_string_forms() {
    let out = compile_and_run(
        r#"<?php
foreach (["-9223372036854775809", "-9223372036854775808", "0009223372036854775807", "+1", " 1 "] as $seed) {
    $engine = new Random\Engine\Mt19937($seed);
    $expected = $seed === "0009223372036854775807"
        ? new Random\Engine\Mt19937(PHP_INT_MAX)
        : ($seed === "+1" || $seed === " 1 " ? new Random\Engine\Mt19937(1) : new Random\Engine\Mt19937(PHP_INT_MIN));
    echo $engine->generate() === $expected->generate() ? "same\n" : "diff\n";
}
"#,
    );
    assert_eq!(out, "same\nsame\nsame\nsame\nsame\n");
}

/// The seed binds by position or by name, including after the mode argument.
#[test]
fn test_random_mt19937_seed_argument_forms() {
    let out = compile_and_run(
        r#"<?php
$positional = new Random\Engine\Mt19937(1);
$named = new Random\Engine\Mt19937(mode: 0, seed: 1);
$spread = new Random\Engine\Mt19937(...[1]);
$expected = $positional->generate();
echo $named->generate() === $expected ? "named\n" : "diff\n";
echo $spread->generate() === $expected ? "spread\n" : "diff\n";
"#,
    );
    assert_eq!(out, "named\nspread\n");
}

/// A nested weak-file construction during argument evaluation cannot leak its strictness.
#[test]
fn test_random_mt19937_cross_file_seed_strictness() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\ndeclare(strict_types=1);\nrequire __DIR__ . '/weak.php';\ntry {\n    new Random\\Engine\\Mt19937(weakSeedHelper());\n    echo \"coerced\\n\";\n} catch (TypeError $e) {\n    echo $e->getMessage(), \"\\n\";\n}\n$weak = weakConstruct();\nvar_dump($weak->generate() === (new Random\\Engine\\Mt19937(1))->generate());\n$rc = new ReflectionClass(Random\\Engine\\Mt19937::class);\ntry {\n    $reflected = $rc->newInstance(1.5);\n    var_dump($reflected->generate() === (new Random\\Engine\\Mt19937(1))->generate());\n} catch (TypeError $e) {\n    echo $e->getMessage(), \"\\n\";\n}\n",
            ),
            (
                "weak.php",
                "<?php\nfunction weakSeedHelper(): float {\n    $class = \"stdClass\";\n    new $class();\n    return 1.5;\n}\nfunction weakConstruct(): Random\\Engine\\Mt19937 {\n    return new Random\\Engine\\Mt19937(1.5);\n}\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(
        out,
        "Random\\Engine\\Mt19937::__construct(): Argument #1 ($seed) must be of type ?int, float given\nbool(true)\nbool(true)\n"
    );
}

/// Dynamically constructed and spread seeds bind exactly like direct arguments.
#[test]
fn test_random_mt19937_dynamic_and_spread_seed_binding() {
    let out = compile_and_run(
        r#"<?php
$class = "Random\\Engine\\Mt19937";
$a = new $class(1.5);
$b = new Random\Engine\Mt19937(1);
echo $a->generate() === $b->generate() ? "dynamic\n" : "diff\n";
$c = new Random\Engine\Mt19937(...[1.5]);
echo $c->generate() === (new Random\Engine\Mt19937(1))->generate() ? "spread\n" : "diff\n";
$d = new $class(...["1"]);
echo $d->generate() === (new Random\Engine\Mt19937(1))->generate() ? "dynamic spread\n" : "diff\n";
"#,
    );
    assert_eq!(out, "dynamic\nspread\ndynamic spread\n");
}

/// A strict file rejects coercible seeds on dynamic and spread construction too.
#[test]
fn test_random_mt19937_dynamic_and_spread_seed_strict_rejection() {
    let out = compile_and_run(
        r#"<?php
declare(strict_types=1);
$class = "Random\\Engine\\Mt19937";
foreach ([fn() => new $class(1.5), fn() => new Random\Engine\Mt19937(...[1.5])] as $case) {
    try {
        $case();
        echo "no error\n";
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        "Random\\Engine\\Mt19937::__construct(): Argument #1 ($seed) must be of type ?int, float given\n\
Random\\Engine\\Mt19937::__construct(): Argument #1 ($seed) must be of type ?int, float given\n"
    );
}

/// Lossy seed deprecations use php-src's shortest float text at E_DEPRECATED.
#[test]
fn test_random_mt19937_seed_deprecation_text_and_level() {
    let output = compile_and_run_capture(
        r#"<?php
set_error_handler(function (int $level, string $message): bool {
    echo $level, " ", $message, "\n";
    return true;
});
new Random\Engine\Mt19937(0.1 + 0.2);
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(
        output.stdout,
        "8192 Implicit conversion from float 0.30000000000000004 to int loses precision\n"
    );
    assert_eq!(output.stderr, "");
}

/// Strict types keep `Mt19937`'s seed at `?int`, with `null` still random-seeding.
#[test]
fn test_random_mt19937_strict_seed_rejection() {
    let out = compile_and_run(
        r#"<?php
declare(strict_types=1);
foreach ([1.0, "1", true] as $seed) {
    try {
        new Random\Engine\Mt19937($seed);
        echo "no error\n";
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
}
var_dump((new Random\Engine\Mt19937(1))->generate() === (new Random\Engine\Mt19937(1))->generate());
"#,
    );
    assert_eq!(
        out,
        "Random\\Engine\\Mt19937::__construct(): Argument #1 ($seed) must be of type ?int, float given\n\
Random\\Engine\\Mt19937::__construct(): Argument #1 ($seed) must be of type ?int, string given\n\
Random\\Engine\\Mt19937::__construct(): Argument #1 ($seed) must be of type ?int, true given\n\
bool(true)\n"
    );
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
    new Random\Engine\Mt19937([]);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "Random\\Engine\\PcgOneseq128XslRr64::__construct(): Argument #1 ($seed) must be of type string|int|null, float given\n\
Random\\Engine\\Xoshiro256StarStar::__construct(): Argument #1 ($seed) must be of type string|int|null, float given\n\
Random\\Engine\\Mt19937::__construct(): Argument #1 ($seed) must be of type ?int, array given\n"
    );
}
