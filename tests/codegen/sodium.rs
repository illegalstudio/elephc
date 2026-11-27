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
16:2001:db8::1\n16:c0000201:192.0.2.1\n\
SodiumException:sodium_crypto_xof_shake128(): Argument #1 ($length) must be greater than 0\n\
SodiumException:sodium_bin2ip(): Argument #1 ($bin) must be 16 bytes long\n\
SodiumException:sodium_ip2bin(): Argument #1 ($ip) must be a valid IP address\n"
    );
}

/// IPcrypt plain/PFX vectors, ND/NDX round-trips and argument diagnostics.
#[test]
fn test_sodium_ipcrypt_php86() {
    let out = compile_and_run(
        r#"<?php
$key = hex2bin("0123456789abcdeffedcba9876543210");
if (!is_string($key)) {
    exit(1);
}
echo sodium_crypto_ipcrypt_encrypt("0.0.0.0", $key), "\n";
$plain = sodium_crypto_ipcrypt_encrypt("192.0.2.1", $key);
echo sodium_crypto_ipcrypt_decrypt($plain, $key), "\n";

$pfxKey = hex2bin("0123456789abcdeffedcba98765432101032547698badcfeefcdab8967452301");
if (!is_string($pfxKey)) {
    exit(1);
}
$pfx = sodium_crypto_ipcrypt_pfx_encrypt("192.0.2.1", $pfxKey);
echo $pfx, ":", sodium_crypto_ipcrypt_pfx_decrypt($pfx, $pfxKey), "\n";

$ndKey = sodium_crypto_ipcrypt_nd_keygen();
$nd = sodium_crypto_ipcrypt_nd_encrypt("2001:db8::1", $ndKey);
echo strlen($nd), ":", sodium_crypto_ipcrypt_nd_decrypt($nd, $ndKey), "\n";

$ndxKey = sodium_crypto_ipcrypt_ndx_keygen();
$ndx = sodium_crypto_ipcrypt_ndx_encrypt("192.0.2.1", $ndxKey);
echo strlen($ndx), ":", sodium_crypto_ipcrypt_ndx_decrypt($ndx, $ndxKey), "\n";

try {
    sodium_crypto_ipcrypt_encrypt("192.0.2.1", "short");
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    sodium_crypto_ipcrypt_nd_decrypt("00", $ndKey);
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    sodium_crypto_ipcrypt_pfx_encrypt("not-an-ip", $pfxKey);
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "bde9:6789:d353:824c:d7c6:f58a:6bd2:26eb\n192.0.2.1\n\
100.115.72.131:192.0.2.1\n48:2001:db8::1\n64:192.0.2.1\n\
SodiumException:sodium_crypto_ipcrypt_encrypt(): Argument #2 ($key) must be SODIUM_CRYPTO_IPCRYPT_KEYBYTES bytes long\n\
SodiumException:sodium_crypto_ipcrypt_nd_decrypt(): Argument #1 ($ciphertext_hex) must be a valid hex-encoded ciphertext\n\
SodiumException:sodium_crypto_ipcrypt_pfx_encrypt(): Argument #1 ($ip) must be a valid IP address\n"
    );
}

/// PHP 8.6 TurboSHAKE one-shot vectors and streaming state classes.
#[test]
fn test_sodium_turboshake_php86() {
    let out = compile_and_run(
        r#"<?php
echo bin2hex(sodium_crypto_xof_turboshake128(32, ""))
    === "1e415f1c5983aff2169217277d17bb538cd945a397ddec541f1ce41af2c1b74c"
    ? "turbo128" : "bad", "\n";
echo bin2hex(sodium_crypto_xof_turboshake256(32, ""))
    === "367a329dafea871c7802ec67f905ae13c57695dc2c6663c61035f59a18f8e7db"
    ? "turbo256" : "bad", "\n";

$state = sodium_crypto_xof_turboshake128_init();
sodium_crypto_xof_turboshake128_update($state, "abc");
$first = sodium_crypto_xof_turboshake128_squeeze($state, 16);
$second = sodium_crypto_xof_turboshake128_squeeze($state, 16);
echo bin2hex($first . $second) === bin2hex(sodium_crypto_xof_turboshake128(32, "abc"))
    ? "stream" : "bad", "\n";

try {
    sodium_crypto_xof_turboshake128_init(0);
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "turbo128\nturbo256\nstream\n\
SodiumException:sodium_crypto_xof_turboshake128_init(): Argument #1 ($domain) must be between 0x01 and 0x7f\n"
    );
}

/// X-Wing and ML-KEM-768 keypair, encapsulation and decapsulation round-trips.
#[test]
fn test_sodium_kem_php86() {
    let out = compile_and_run(
        r#"<?php
$keypair = sodium_crypto_kem_keypair();
echo strlen($keypair), ":",
    strlen(sodium_crypto_kem_secretkey($keypair)), ":",
    strlen(sodium_crypto_kem_publickey($keypair)), "\n";

$seed = str_repeat("\x07", SODIUM_CRYPTO_KEM_SEEDBYTES);
$first = sodium_crypto_kem_seed_keypair($seed);
$second = sodium_crypto_kem_seed_keypair($seed);
echo $first === $second ? "deterministic" : "bad", "\n";

$pair = sodium_crypto_kem_enc(sodium_crypto_kem_publickey($first));
$ciphertext = $pair[0];
$shared = $pair[1];
echo strlen($ciphertext), ":", strlen($shared), ":",
    sodium_crypto_kem_dec($ciphertext, sodium_crypto_kem_secretkey($first)) === $shared
        ? "match" : "bad", "\n";

$mlkem = sodium_crypto_kem_mlkem768_keypair();
echo strlen($mlkem), ":",
    strlen(sodium_crypto_kem_mlkem768_secretkey($mlkem)), ":",
    strlen(sodium_crypto_kem_mlkem768_publickey($mlkem)), "\n";

$mlkem_seed = str_repeat("\x09",SODIUM_CRYPTO_KEM_MLKEM768_SEEDBYTES);
echo sodium_crypto_kem_mlkem768_seed_keypair($mlkem_seed)
    === sodium_crypto_kem_mlkem768_seed_keypair($mlkem_seed)
    ? "deterministic" : "bad", "\n";

$mlkem_pair = sodium_crypto_kem_mlkem768_enc(
    sodium_crypto_kem_mlkem768_publickey($mlkem)
);
$mlkem_ciphertext = $mlkem_pair[0];
$mlkem_shared = $mlkem_pair[1];
echo strlen($mlkem_ciphertext), ":", strlen($mlkem_shared), ":",
    sodium_crypto_kem_mlkem768_dec(
        $mlkem_ciphertext,
        sodium_crypto_kem_mlkem768_secretkey($mlkem)
    ) === $mlkem_shared ? "match" : "bad", "\n";

try {
    sodium_crypto_kem_seed_keypair("short");
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    sodium_crypto_kem_mlkem768_enc("short");
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "1248:32:1216\ndeterministic\n1120:32:match\n3584:2400:1184\ndeterministic\n\
1088:32:match\n\
SodiumException:sodium_crypto_kem_seed_keypair(): Argument #1 ($seed) must be SODIUM_CRYPTO_KEM_SEEDBYTES bytes long\n\
SodiumException:sodium_crypto_kem_mlkem768_enc(): Argument #1 ($public_key) must be SODIUM_CRYPTO_KEM_MLKEM768_PUBLICKEYBYTES bytes long\n"
    );
}

/// Argon2id raw vectors, PHC string verification and PHP 8.6 ValueError text.
#[test]
fn test_sodium_pwhash_php86() {
    let out = compile_and_run(
        r#"<?php
$password = hex2bin(
    "a347ae92bce9f80f6f595a4480fc9c2fe7e7d7148d371e9487d75f5c23008ffae0"
    . "65577a928febd9b1973a5a95073acdbeb6a030cfc0d79caa2dc5cd011cef02c08d"
    . "a232d76d52dfbca38ca8dcbd665b17d1665f7cf5fe59772ec909733b24de97d6f5"
    . "8d220b20c60d7c07ec1fd93c52c31020300c6c1facd77937a597c7a6"
);
$salt = hex2bin("5541fbc995d5c197ba290346d2c559de");
if (!is_string($password) || !is_string($salt)) {
    exit(1);
}
$hash = sodium_crypto_pwhash(155, $password, $salt, 5, 7256678,
    SODIUM_CRYPTO_PWHASH_ALG_ARGON2ID13);
echo bin2hex($hash)
    === "18acec5d6507739f203d1f5d9f1d862f7c2cdac4f19d2bdff64487e60d969e3c"
        . "ed615337b9eec6ac4461c6ca07f0939741e57c24d0005c7ea171a0ee1e734824"
        . "9d135b38f222e4dad7b9a033ed83f5ca27277393e316582033c74affe2566a2b"
        . "ea47f91f0fd9fe49ece7e1f79f3ad6e9b23e0277c8ecc4b313225748dd2a80f5"
        . "679534a0700e246a79a49b3f74eb89ec6205fe1eeb941c73b1fcf1"
    ? "vector" : "bad", "\n";

$phc = sodium_crypto_pwhash_str("password", 2, 67108864);
echo substr($phc, 0, 10), ":", strlen($phc) < SODIUM_CRYPTO_PWHASH_STRBYTES ? "len" : "bad", "\n";
var_dump(sodium_crypto_pwhash_str_verify($phc, "password"));
var_dump(sodium_crypto_pwhash_str_verify($phc, "wrong"));

$argon2i = sodium_crypto_pwhash_argon2i_str("password", 2, 8192);
echo substr($argon2i, 0, 9), ":",
    sodium_crypto_pwhash_argon2i_str_verify($argon2i, "password") ? "ok" : "bad", "\n";

try {
    sodium_crypto_pwhash(0, "p", $salt, 2, 67108864);
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    sodium_crypto_pwhash(32, "p", $salt, 0, 67108864);
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    sodium_crypto_pwhash(32, "p", $salt, 1, 8191);
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    sodium_crypto_pwhash_str("p", 0, 67108864);
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "vector\n$argon2id$:len\nbool(true)\nbool(false)\n$argon2i$:ok\n\
ValueError:sodium_crypto_pwhash(): Argument #1 ($length) must be greater than 0\n\
ValueError:sodium_crypto_pwhash(): Argument #4 ($opslimit) must be greater than 0\n\
ValueError:sodium_crypto_pwhash(): Argument #5 ($memlimit) must be greater than or equal to 8192\n\
ValueError:sodium_crypto_pwhash_str(): Argument #2 ($opslimit) must be greater than 0\n"
    );
}

/// scrypt-salsa20/poly1305 vector, `$7$` strings and PHP 8.6 ValueError text.
#[test]
fn test_sodium_scrypt_pwhash_php86() {
    let out = compile_and_run(
        r#"<?php
$salt = str_repeat("S", SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_SALTBYTES);
$hash = sodium_crypto_pwhash_scryptsalsa208sha256(
    32, "password", $salt, 524288, 16777216
);
echo bin2hex($hash)
    === "7dda42b29bdce385ff353534b97cead6504a5f9fc9216d30d6e4635867e07e2c"
    ? "vector" : "bad", "\n";

$str = sodium_crypto_pwhash_scryptsalsa208sha256_str("password", 524288, 16777216);
echo substr($str, 0, 3), ":", strlen($str), ":";
var_dump(sodium_crypto_pwhash_scryptsalsa208sha256_str_verify($str, "password"));
var_dump(sodium_crypto_pwhash_scryptsalsa208sha256_str_verify($str, "wrong"));

try {
    sodium_crypto_pwhash_scryptsalsa208sha256(
        32, "p", $salt, 524287, 16777216
    );
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    sodium_crypto_pwhash_scryptsalsa208sha256_str("p", 524288, 16777215);
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    sodium_crypto_pwhash_scryptsalsa208sha256(
        32, "p", str_repeat("S", 31), 524288, 16777216
    );
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "vector\n$7$:101:bool(true)\nbool(false)\n\
ValueError:sodium_crypto_pwhash_scryptsalsa208sha256(): Argument #4 ($opslimit) must be greater than or equal to 524288\n\
ValueError:sodium_crypto_pwhash_scryptsalsa208sha256_str(): Argument #3 ($memlimit) must be greater than or equal to 16777216\n\
ValueError:sodium_crypto_pwhash_scryptsalsa208sha256(): Argument #3 ($salt) must be SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_SALTBYTES bytes long\n"
    );
}
