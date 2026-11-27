//! Purpose:
//! End-to-end coverage for the first `sodium_*` slice and its pure-Rust
//! `elephc-sodium` bridge.
//!
//! Called from:
//! - `cargo test --test codegen_tests_ext sodium`.
//!
//! Key details:
//! - The vector is NaCl's `tests/secretbox` fixture, also used by libsodium,
//!   so a matching ciphertext proves byte compatibility with PHP's ext/sodium.
//! - The bridge is linked from the prelude's `extern "elephc_sodium"` block;
//!   compiling and running these programs is the bridge/link test.

use crate::support::*;

/// NaCl's XSalsa20-Poly1305 vector matches `sodium_crypto_secretbox` byte-for-byte.
#[test]
fn test_sodium_secretbox_nacl_vector_and_round_trip() {
    let out = compile_and_run(
        r#"<?php
$key = hex2bin("1b27556473e985d462cd51197a9a46c76009549eac6474f206c4ee0844f68389");
$nonce = hex2bin("69696ee955b62b73cd62bda875fc73d68219e0036b7a0b37");
$plain = hex2bin(
    "be075fc53c81f2d5cf141316ebeb0c7b5228c52a4c62cbd44b66849b64244ffce5ecbaaf33bd751a"
    . "1ac728d45e6c61296cdc3c01233561f41db66cce314adb310e3be8250c46f06dceea3a7fa1348057"
    . "e2f6556ad6b1318a024a838f21af1fde048977eb48f59ffd4924ca1c60902e52f0a089bc76897040"
    . "e082f937763848645e0705"
);
if (!is_string($key) || !is_string($nonce) || !is_string($plain)) {
    exit(1);
}
$expected = "f3ffc7703f9400e52a7dfb4b3d3305d98e993b9f48681273c29650ba32fc76ce48332ea7164d96a4"
    . "476fb8c531a1186ac0dfc17c98dce87b4da7f011ec48c97271d2c20f9b928fe2270d6fb863d51738"
    . "b48eeee314a7cc8ab932164548e526ae90224368517acfeabd6bb3732bc0e9da99832b61ca01b6de"
    . "56244a9e88d5f9b37973f622a43d14a6599b1f654cb45a74e355a5";
$cipher = sodium_crypto_secretbox($plain, $nonce, $key);
echo bin2hex($cipher) === $expected ? "vector" : "mismatch", "\n";
echo sodium_crypto_secretbox_open($cipher, $nonce, $key) === $plain ? "roundtrip" : "bad", "\n";

$hello = sodium_crypto_secretbox("hello", $nonce, $key);
echo sodium_crypto_secretbox_open($hello, $nonce, $key), "\n";
$tampered = "X" . substr($hello, 1);
var_dump(sodium_crypto_secretbox_open($tampered, $nonce, $key));

$empty = sodium_crypto_secretbox("", $nonce, $key);
echo strlen($empty), ":";
var_dump(sodium_crypto_secretbox_open($empty, $nonce, $key));

echo strlen(sodium_crypto_secretbox_keygen()), ":";
echo SODIUM_CRYPTO_SECRETBOX_KEYBYTES, ":", SODIUM_CRYPTO_SECRETBOX_NONCEBYTES, ":",
    SODIUM_CRYPTO_SECRETBOX_MACBYTES, "\n";
"#,
    );
    assert_eq!(
        out,
        "vector\nroundtrip\nhello\nbool(false)\n16:string(0) \"\"\n32:32:24:16\n"
    );
}

/// Invalid nonce/key lengths raise PHP's exact `SodiumException` diagnostics.
#[test]
fn test_sodium_secretbox_argument_errors() {
    let out = compile_and_run(
        r#"<?php
$key = str_repeat("k", 32);
$nonce = str_repeat("n", 24);
try {
    sodium_crypto_secretbox("m", str_repeat("n", 23), $key);
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    sodium_crypto_secretbox("m", $nonce, str_repeat("k", 31));
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    sodium_crypto_secretbox_open("short", str_repeat("n", 23), $key);
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
var_dump(sodium_crypto_secretbox_open("short", $nonce, $key));
"#,
    );
    assert_eq!(
        out,
        "SodiumException:sodium_crypto_secretbox(): Argument #2 ($nonce) must be SODIUM_CRYPTO_SECRETBOX_NONCEBYTES bytes long\n\
SodiumException:sodium_crypto_secretbox(): Argument #3 ($key) must be SODIUM_CRYPTO_SECRETBOX_KEYBYTES bytes long\n\
SodiumException:sodium_crypto_secretbox_open(): Argument #2 ($nonce) must be SODIUM_CRYPTO_SECRETBOX_NONCEBYTES bytes long\n\
bool(false)\n"
    );
}

/// PHP 8.6 SHAKE one-shot vectors and the `sodium_bin2ip`/`ip2bin` bridge.
#[test]
fn test_sodium_xof_and_ip_php86() {
    let out = compile_and_run(
        r#"<?php
echo bin2hex(sodium_crypto_xof_shake128(32, ""))
    === "7f9c2ba4e88f827d616045507605853ed73b8093f6efbc88eb1a6eacfa66ef26"
    ? "shake128" : "bad", "\n";
echo bin2hex(sodium_crypto_xof_shake256(64, ""))
    === "46b9dd2b0ba88d13233b3feb743eeb243fcd52ea62b81b82b50c27646ed5762f"
        . "d75dc4ddd8c0f200cb05019d67b592f6fc821c49479ab48640292eacb3b7c4be"
    ? "shake256" : "bad", "\n";

$state = sodium_crypto_xof_shake128_init();
sodium_crypto_xof_shake128_update($state, "the quick ");
sodium_crypto_xof_shake128_update($state, "brown fox");
$first = sodium_crypto_xof_shake128_squeeze($state, 20);
$second = sodium_crypto_xof_shake128_squeeze($state, 28);
echo bin2hex($first . $second) === bin2hex(sodium_crypto_xof_shake128(48, "the quick brown fox"))
    ? "stream" : "bad", "\n";
try {
    sodium_crypto_xof_shake128_update($state, "late");
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    sodium_crypto_xof_shake128_init(0);
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    sodium_crypto_xof_shake128_squeeze($state, 0);
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}

$binary = sodium_ip2bin("2001:db8::1");
echo strlen($binary), ":", sodium_bin2ip($binary), "\n";
$mapped = sodium_ip2bin("192.0.2.1");
echo strlen($mapped), ":", bin2hex(substr($mapped, 12)), ":", sodium_bin2ip($mapped), "\n";

try {
    sodium_crypto_xof_shake128(0, "");
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    sodium_bin2ip("short");
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    sodium_ip2bin("not-an-ip");
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "shake128\nshake256\nstream\n\
SodiumException:sodium_crypto_xof_shake128_update(): internal error\n\
SodiumException:sodium_crypto_xof_shake128_init(): Argument #1 ($domain) must be between 0x01 and 0x7f\n\
SodiumException:sodium_crypto_xof_shake128_squeeze(): Argument #2 ($length) must be greater than 0\n\
16:2001:db8::1\n16:c0000201:::ffff:192.0.2.1\n\
SodiumException:sodium_crypto_xof_shake128(): Argument #1 ($length) must be greater than 0\n\
SodiumException:sodium_bin2ip(): Argument #1 ($bin) must be 16 bytes long\n\
SodiumException:sodium_ip2bin(): Argument #1 ($ip) must be a valid IP address\n"
    );
}
