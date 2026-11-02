//! Purpose:
//! End-to-end AOT tests for the `sodium_crypto_box_*` sealed-box surface and `SodiumException`.
//!
//! Called from:
//! - `cargo test --test codegen_tests sodium` through the Rust test harness.
//!
//! Key details:
//! - The fixture keypair and sealed box were produced by php 8.5's libsodium, so opening them
//!   proves a compiled program reads a vault php wrote.
//! - Argument-length errors must be catchable `SodiumException`s with php's exact messages.

use super::*;

const LIBSODIUM_KEYPAIR: &str = "ed734e9903f45f221cb0ce1355425925d7bca73cdb62d60f507d4357d3abc649d2e6e33181889e912ae79956dfe2f7162d7fdbad3bafaee44ce3e764d731f170";
const LIBSODIUM_SEALED: &str = "293a9115f819046278ff63c323739c0c342cc7b525fbafafbb9141a87973e16971263e1ed24ee377530f27d38ba1dae483433b9c9c93ca343a471affbc3f723e8a13f4";

/// A keypair round-trips through seal/open, the public key is the keypair's second half, and a
/// box sealed by libsodium opens.
#[test]
fn test_sodium_crypto_box_seal_round_trip_and_libsodium_interop() {
    let out = compile_and_run(&format!(
        r#"<?php
$pair = sodium_crypto_box_keypair();
$public = sodium_crypto_box_publickey($pair);
echo strlen($pair), ":", strlen($public), ":", (substr($pair, 32) === $public ? "y" : "n"), "|";
$sealed = sodium_crypto_box_seal("probe-value", $public);
echo strlen($sealed), ":", sodium_crypto_box_seal_open($sealed, $pair), "|";
echo strlen(sodium_crypto_box_seal("", $public)), "|";
echo sodium_crypto_box_seal_open(hex2bin("{LIBSODIUM_SEALED}"), hex2bin("{LIBSODIUM_KEYPAIR}")), "|";
var_dump(sodium_crypto_box_seal_open(str_repeat("x", 60), $pair));
var_dump(sodium_crypto_box_seal_open("short", $pair));
"#
    ));
    assert_eq!(
        out,
        "64:32:y|59:probe-value|48|sealed by libsodium|bool(false)\nbool(false)\n"
    );
}

/// Wrong key lengths throw catchable `SodiumException`s with php's messages, from a namespaced,
/// case-folded call as well.
#[test]
fn test_sodium_crypto_box_length_errors_throw_sodium_exception() {
    let out = compile_and_run(
        r#"<?php
namespace App;
foreach ([
    fn () => sodium_crypto_box_publickey("x"),
    fn () => \SODIUM_CRYPTO_BOX_SEAL("m", "x"),
    fn () => sodium_crypto_box_seal_open("c", "x"),
] as $call) {
    try {
        $call();
    } catch (\SodiumException $e) {
        echo get_class($e), "|", get_parent_class($e), "|", $e->getMessage(), "\n";
    }
}
echo function_exists('sodium_crypto_box_seal') ? "exists" : "missing", "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "SodiumException|Exception|sodium_crypto_box_publickey(): Argument #1 ($key_pair) must be SODIUM_CRYPTO_BOX_KEYPAIRBYTES bytes long\n",
            "SodiumException|Exception|sodium_crypto_box_seal(): Argument #2 ($public_key) must be SODIUM_CRYPTO_BOX_PUBLICKEYBYTES bytes long\n",
            "SodiumException|Exception|sodium_crypto_box_seal_open(): Argument #2 ($key_pair) must be SODIUM_CRYPTO_BOX_KEYPAIRBYTES bytes long\n",
            "exists\n",
        )
    );
}

/// A program that only probes the surface by name still gets it.
#[test]
fn test_sodium_function_exists_probe_injects_the_surface() {
    let out = compile_and_run(
        r#"<?php
if (function_exists('sodium_crypto_box_seal')) {
    $f = 'sodium_crypto_box_keypair';
    echo strlen($f());
}
"#,
    );
    assert_eq!(out, "64");
}
