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

/// Generichash, auth, KDF and shorthash match libsodium's documented vectors.
#[test]
fn test_sodium_generichash_auth_kdf_shorthash() {
    let out = compile_and_run(
        r#"<?php
$key = str_repeat("k", 32);
$key16 = str_repeat("k", 16);
$message = "hello";
echo bin2hex(sodium_crypto_generichash($message)), "\n";
echo bin2hex(sodium_crypto_generichash($message, "", 16)), "\n";
echo bin2hex(sodium_crypto_generichash($message, $key16, 32)), "\n";
$state = sodium_crypto_generichash_init("", 32);
sodium_crypto_generichash_update($state, "he");
sodium_crypto_generichash_update($state, "llo");
echo bin2hex(sodium_crypto_generichash_final($state, 32)), "\n";
echo bin2hex(sodium_crypto_auth($message, $key)), "\n";
echo sodium_crypto_auth_verify(sodium_crypto_auth($message, $key), $message, $key) ? "auth_ok" : "auth_bad", "\n";
echo bin2hex(sodium_crypto_kdf_derive_from_key(32, 7, "context!", $key)), "\n";
echo bin2hex(sodium_crypto_shorthash($message, $key16)), "\n";
echo strlen(sodium_crypto_generichash_keygen()), "\n";
"#,
    );
    assert_eq!(
        out,
        "324dcf027dd4a30a932c441f365a25e86b173defa4b8e58948253471b81b72cf\n\
         46fb7408d4f285228f4af516ea25851b\n\
         94072b2b462d672f8d29d1675fcdce0408bd8a5d8345d0202faa908513c2b3ff\n\
         324dcf027dd4a30a932c441f365a25e86b173defa4b8e58948253471b81b72cf\n\
         d1d5b8a0f98474ab10a4966537473add27863dbc60446d3804e210cdadb752ad\n\
         auth_ok\n\
         896ec86a37c2bc1b068c0b945dbbc1838280277b0e2a2ff0fc2e16227044431d\n\
         fecf1a72463c4d9e\n\
         32\n"
    );
}

/// Ed25519 key derivation, detached/combined signatures and rejection paths.
#[test]
fn test_sodium_sign_family() {
    let out = compile_and_run(
        r#"<?php
$seed = str_repeat("s", 32);
$pair = sodium_crypto_sign_seed_keypair($seed);
$secret = sodium_crypto_sign_secretkey($pair);
$public = sodium_crypto_sign_publickey($pair);
echo bin2hex($public), "\n";
$signature = sodium_crypto_sign_detached("hello", $secret);
echo bin2hex($signature), "\n";
echo sodium_crypto_sign_verify_detached($signature, "hello", $public) ? "verify" : "bad", "\n";
echo sodium_crypto_sign_open(sodium_crypto_sign("hello", $secret), $public), "\n";
echo sodium_crypto_sign_verify_detached($signature, "tampered", $public) ? "bad" : "reject", "\n";
echo sodium_crypto_sign_publickey_from_secretkey($secret) === $public ? "derive" : "bad", "\n";
echo strlen(sodium_crypto_sign_keypair()), "\n";
"#,
    );
    assert_eq!(
        out,
        "fec1428673f6afe89264b3807344ae8b269d125a503855202687db5a7128579f\n\
         4c7f91db0f26f32ed8f04893c9b5861c9bab4f88537dad0741d167b858255de5\
         e1e3306e95c43ca6d5ed0bdf8ffe3c86cf410b370c89c7e6f69d01a45e9fbe08\n\
         verify\n\
         hello\n\
         reject\n\
         derive\n\
         96\n"
    );
}

/// Curve25519 box, sealed box and KX session keys agree across both sides.
#[test]
fn test_sodium_box_kx_family() {
    let out = compile_and_run(
        r#"<?php
$seed = str_repeat("s", 32);
$pair = sodium_crypto_box_seed_keypair($seed);
$nonce = str_repeat("n", 24);
$cipher = sodium_crypto_box("hello", $nonce, $pair);
echo bin2hex(sodium_crypto_box_publickey($pair)), "\n";
echo bin2hex($cipher), "\n";
echo sodium_crypto_box_open($cipher, $nonce, $pair), "\n";
$sealed = sodium_crypto_box_seal("secret", sodium_crypto_box_publickey($pair));
echo sodium_crypto_box_seal_open($sealed, $pair), ":", strlen($sealed), "\n";
$client = sodium_crypto_kx_seed_keypair(str_repeat("c", 32));
$server = sodium_crypto_kx_seed_keypair(str_repeat("s", 32));
$client_keys = sodium_crypto_kx_client_session_keys($client, sodium_crypto_kx_publickey($server));
$server_keys = sodium_crypto_kx_server_session_keys($server, sodium_crypto_kx_publickey($client));
echo bin2hex($client_keys[0]), "\n";
echo bin2hex($client_keys[1]), "\n";
echo ($client_keys[0] === $server_keys[1] && $client_keys[1] === $server_keys[0]) ? "session" : "bad", "\n";
"#,
    );
    assert_eq!(
        out,
        "c3c490800a4fa9abb5d55cc5578269e889fcb05fe2f1edbb9cd9fdc4a9c2014d\n\
         80c58585eb440793aa57279a0455240b9623aa47e0\n\
         hello\n\
         secret:54\n\
         0b61f01b012bdc704d5cb066d1978f8510a50ad387ec262c98c0820264acadf3\n\
         0209f91e8a56a959af6b140d153f7c74e1104fd6a93d124c1d3c20017087e540\n\
         session\n"
    );
}

/// AEAD families and stream helpers match libsodium's wire format.
#[test]
fn test_sodium_aead_stream_family() {
    let out = compile_and_run(
        r#"<?php
$key = str_repeat("k", 32);
$nonce8 = str_repeat("n", 8);
$nonce12 = str_repeat("n", 12);
$nonce24 = str_repeat("n", 24);
$legacy = sodium_crypto_aead_chacha20poly1305_encrypt("hello", "ad", $nonce8, $key);
echo bin2hex($legacy), ":", sodium_crypto_aead_chacha20poly1305_decrypt($legacy, "ad", $nonce8, $key), "\n";
$aead = sodium_crypto_aead_chacha20poly1305_ietf_encrypt("hello", "ad", $nonce12, $key);
echo bin2hex($aead), ":", sodium_crypto_aead_chacha20poly1305_ietf_decrypt($aead, "ad", $nonce12, $key), "\n";
$xchacha = sodium_crypto_aead_xchacha20poly1305_ietf_encrypt("hello", "ad", $nonce24, $key);
echo bin2hex($xchacha), ":", sodium_crypto_aead_xchacha20poly1305_ietf_decrypt($xchacha, "ad", $nonce24, $key), "\n";
$gcm = sodium_crypto_aead_aes256gcm_encrypt("hello", "ad", $nonce12, $key);
echo bin2hex($gcm), ":", sodium_crypto_aead_aes256gcm_decrypt($gcm, "ad", $nonce12, $key), "\n";
$tampered = substr($gcm, 0, -1) . "X";
var_dump(sodium_crypto_aead_aes256gcm_decrypt($tampered, "ad", $nonce12, $key));
echo bin2hex(sodium_crypto_stream(16, $nonce24, $key)), "\n";
echo bin2hex(sodium_crypto_stream_xor("hello", $nonce24, $key)), "\n";
echo bin2hex(sodium_crypto_stream_xchacha20(16, $nonce24, $key)), "\n";
echo bin2hex(sodium_crypto_stream_xchacha20_xor("hello", $nonce24, $key)), "\n";
echo bin2hex(sodium_crypto_stream_xchacha20_xor_ic("hello", $nonce24, 3, $key)), "\n";
"#,
    );
    assert_eq!(
        out,
        "3d0105d90104c96e9096f13b73817242e77c8c43b3:hello\n\
         e69a523ead3e9e4afd85b28a17fd7b7b438d46cddd:hello\n\
         9d9da4d9a984c3672f08ec06b0492c584f5ef15477:hello\n\
         f05e8bdc7e6253125933b5b14bbe5b1474032744d4:hello\n\
         bool(false)\n\
         91172bd584998fa76d97a543eccf9eaf\n\
         f97247b9eb\n\
         93a68aa3fb23e8c037e4a89be4ae63d1\n\
         fbc3e6cf94\n\
         f45f9e6486\n"
    );
}

/// Secretstream pulls a libsodium fixture and round-trips fresh streams.
#[test]
fn test_sodium_secretstream_family() {
    let out = compile_and_run(
        r#"<?php
$key = str_repeat("k", 32);
$header = hex2bin("aed3f1ba74be5f265a1b0191f31edd27c3562569c53e90a3");
$cipher = hex2bin("ee6221186394aaaafee93cf16d06b24496bf15186db1");
$state = sodium_crypto_secretstream_xchacha20poly1305_init_pull($header, $key);
$result = sodium_crypto_secretstream_xchacha20poly1305_pull($state, $cipher, "");
echo $result[0], ":", $result[1], "\n";
$push = sodium_crypto_secretstream_xchacha20poly1305_init_push($key);
$state = $push[0];
$cipher = sodium_crypto_secretstream_xchacha20poly1305_push($state, "hello", "", SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_MESSAGE);
echo strlen($cipher), "\n";
$pull = sodium_crypto_secretstream_xchacha20poly1305_init_pull($push[1], $key);
$result = sodium_crypto_secretstream_xchacha20poly1305_pull($pull, $cipher, "");
echo $result[0], ":", $result[1], "\n";
$tampered = substr($cipher, 0, -1) . "X";
var_dump(sodium_crypto_secretstream_xchacha20poly1305_pull($pull, $tampered, ""));
"#,
    );
    assert_eq!(out, "hello:0\n22\nhello:0\nbool(false)\n");
}

/// Comparison, mutation, padding, base64 and hex helpers match PHP.
#[test]
fn test_sodium_helpers() {
    let out = compile_and_run(
        r#"<?php
echo sodium_compare("a", "b"), sodium_compare("b", "a"), sodium_compare("a", "a"), "\n";
echo sodium_memcmp("abc", "abc"), sodium_memcmp("abc", "abd"), "\n";
$add = "\xff\xff";
sodium_add($add, "\x01\x00");
echo bin2hex($add), "\n";
$increment = "\xff\xff";
sodium_increment($increment);
echo bin2hex($increment), "\n";
$padded = sodium_pad("hello", 8);
echo bin2hex($padded), ":", sodium_unpad($padded, 8), "\n";
echo sodium_bin2base64("hello\xff", SODIUM_BASE64_VARIANT_ORIGINAL), ":",
    sodium_bin2base64("hello\xff", SODIUM_BASE64_VARIANT_URLSAFE_NO_PADDING), "\n";
echo bin2hex(sodium_base642bin("aGVsbG8=", SODIUM_BASE64_VARIANT_ORIGINAL)), "\n";
echo bin2hex(sodium_hex2bin("aa bb", " ")), "\n";
try {
    sodium_hex2bin("zz");
} catch (Throwable $error) {
    echo $error->getMessage(), "\n";
}
try {
    sodium_memcmp("a", "bb");
} catch (Throwable $error) {
    echo $error->getMessage(), "\n";
}
$hash = sodium_crypto_pwhash_str("pw", 2, 67108864);
var_dump(sodium_crypto_pwhash_str_needs_rehash($hash, 2, 67108864));
var_dump(sodium_crypto_pwhash_str_needs_rehash($hash, 3, 67108864));
"#,
    );
    assert_eq!(
        out,
        "-110\n0-1\n0000\n0000\n68656c6c6f800000:hello\n\
         aGVsbG//:aGVsbG__\n68656c6c6f\naabb\n\
         sodium_hex2bin(): Argument #1 ($string) must be a valid hexadecimal string\n\
         sodium_memcmp(): Argument #1 ($string1) and argument #2 ($string_2) must have the same length\n\
         bool(false)\nbool(true)\n"
    );
}

/// Ristretto255 points, scalars and base multiplication match libsodium.
#[test]
fn test_sodium_ristretto255_family() {
    let out = compile_and_run(
        r#"<?php
$p = sodium_crypto_core_ristretto255_from_hash(str_repeat("h", 64));
$q = sodium_crypto_core_ristretto255_from_hash(str_repeat("i", 64));
echo bin2hex($p), "\n";
echo bin2hex($q), "\n";
echo bin2hex(sodium_crypto_core_ristretto255_add($p, $q)), "\n";
echo bin2hex(sodium_crypto_core_ristretto255_sub($p, $q)), "\n";
$two = "\x02" . str_repeat("\x00", 31);
$three = "\x03" . str_repeat("\x00", 31);
echo bin2hex(sodium_crypto_scalarmult_ristretto255($two, $p)), "\n";
echo bin2hex(sodium_crypto_scalarmult_ristretto255_base($two)), "\n";
echo bin2hex(sodium_crypto_core_ristretto255_scalar_add($two, $three)), "\n";
echo bin2hex(sodium_crypto_core_ristretto255_scalar_sub($two, $three)), "\n";
echo bin2hex(sodium_crypto_core_ristretto255_scalar_mul($two, $three)), "\n";
echo bin2hex(sodium_crypto_core_ristretto255_scalar_negate($two)), "\n";
echo bin2hex(sodium_crypto_core_ristretto255_scalar_complement($two)), "\n";
echo bin2hex(sodium_crypto_core_ristretto255_scalar_invert($two)), "\n";
echo bin2hex(sodium_crypto_core_ristretto255_scalar_reduce(str_repeat("\xff", 64))), "\n";
var_dump(sodium_crypto_core_ristretto255_is_valid_point(str_repeat("\x00", 32)));
var_dump(sodium_crypto_core_ristretto255_is_valid_point($p));
try {
    sodium_crypto_scalarmult_ristretto255(str_repeat("\x00", 32), $p);
} catch (Throwable $error) {
    echo $error->getMessage(), "\n";
}
try {
    sodium_crypto_core_ristretto255_scalar_invert(str_repeat("\x00", 32));
} catch (Throwable $error) {
    echo $error->getMessage(), "\n";
}
echo strlen(sodium_crypto_core_ristretto255_random()), ":",
    strlen(sodium_crypto_core_ristretto255_scalar_random()), "\n";
"#,
    );
    assert_eq!(
        out,
        "b2b46977c1b18ebd857e5b576d15dec8c0b351bc2be386a157a379917e460474\n\
         ec0ccf947db85b60dda70d473180e84cbe7281dc945be3e6496a246bcb87da07\n\
         c042147358a9d6005f909c6275934a5f0bc7806082f680111fbab12ae6045f5f\n\
         907783cee074d1029e80b4c685accfd1a94f588ee1ca0d260f67bc9c96293226\n\
         0418e8bb63134089fbbb74c7e2c33191b439fa180dce00518b2223fb4bbfb926\n\
         6a493210f7499cd17fecb510ae0cea23a110e8d5b901f8acadd3095c73a3b919\n\
         0500000000000000000000000000000000000000000000000000000000000000\n\
         ecd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010\n\
         0600000000000000000000000000000000000000000000000000000000000000\n\
         ebd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010\n\
         ecd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010\n\
         f7e97a2e8d31092c6bce7b51ef7c6f0a00000000000000000000000000000008\n\
         000f9c44e31106a447938568a71b0ed065bef517d273ecce3d9a307c1b419903\n\
         bool(true)\n\
         bool(true)\n\
         Result is identity element\n\
         internal error\n\
         32:32\n"
    );
}

/// AEGIS-128L and AEGIS-256 ciphertexts match libsodium's PHP vectors.
#[test]
fn test_sodium_aegis_family() {
    let out = compile_and_run(
        r#"<?php
$key16 = str_repeat("k", 16);
$nonce16 = str_repeat("n", 16);
$key32 = str_repeat("k", 32);
$nonce32 = str_repeat("n", 32);
$cipher128 = sodium_crypto_aead_aegis128l_encrypt("hello", "ad", $nonce16, $key16);
echo bin2hex($cipher128), ":", sodium_crypto_aead_aegis128l_decrypt($cipher128, "ad", $nonce16, $key16), "\n";
$cipher256 = sodium_crypto_aead_aegis256_encrypt("hello", "ad", $nonce32, $key32);
echo bin2hex($cipher256), ":", sodium_crypto_aead_aegis256_decrypt($cipher256, "ad", $nonce32, $key32), "\n";
$tampered = substr($cipher128, 0, -1) . "X";
var_dump(sodium_crypto_aead_aegis128l_decrypt($tampered, "ad", $nonce16, $key16));
echo strlen(sodium_crypto_aead_aegis128l_keygen()), ":",
    strlen(sodium_crypto_aead_aegis256_keygen()), "\n";
"#,
    );
    assert_eq!(
        out,
        "607ccb15b0bb84ac2e6c2859c4b9b678862ea42abff37a175d1800257481360d23654aa412:hello\n\
         304098fab477291f6f684d90c9d21ef2eea34480837241d9b0dec3d92bd4f93e459f5b0c88:hello\n\
         bool(false)\n\
         16:32\n"
    );
}

/// Ed25519 public-key conversion rejects invalid, small-order and off-subgroup keys.
#[test]
fn test_sodium_ed25519_pk_conversion_errors() {
    let out = compile_and_run(
        r#"<?php
foreach ([str_repeat("\x00", 32), str_repeat("\xff", 32), str_repeat("\x01", 32)] as $public) {
    try {
        sodium_crypto_sign_ed25519_pk_to_curve25519($public);
        echo "accepted\n";
    } catch (Throwable $error) {
        echo get_class($error), ":", $error->getMessage(), "\n";
    }
}
$keypair = sodium_crypto_sign_keypair();
$converted = sodium_crypto_sign_ed25519_pk_to_curve25519(sodium_crypto_sign_publickey($keypair));
echo strlen($converted), ":", bin2hex($converted) === bin2hex(sodium_crypto_scalarmult_base(sodium_crypto_sign_ed25519_sk_to_curve25519(sodium_crypto_sign_secretkey($keypair)))) ? "match" : "mismatch", "\n";
"#,
    );
    assert_eq!(
        out,
        "SodiumException:conversion failed\nSodiumException:conversion failed\n\
         SodiumException:conversion failed\n32:match\n"
    );
}
