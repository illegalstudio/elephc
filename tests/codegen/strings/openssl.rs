//! Purpose:
//! End-to-end AOT tests for OpenSSL constants, helpers, and AES cipher modes.
//!
//! Called from:
//! - `cargo test --test codegen_tests openssl` through the Rust test harness.
//!
//! Key details:
//! - Ciphertext expectations come from the checked-in PHP 8.4/OpenSSL 3.6 fixtures.
//! - GCM cases pin by-reference tags, AAD authentication, non-default IVs, and failures.

use super::*;

/// Verifies raw CBC, CTR, and ECB encryption matches the locked PHP ciphertexts.
#[test]
fn test_openssl_encrypt_non_aead_php_goldens() {
    if skip_without_openssl_native("test_openssl_encrypt_non_aead_php_goldens") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$pt = "The quick brown fox jumps over the lazy dog";
$iv = str_repeat("i", 16);
echo bin2hex(openssl_encrypt($pt, "aes-128-cbc", str_repeat("k", 16), OPENSSL_RAW_DATA, $iv)), "|";
echo bin2hex(openssl_encrypt($pt, "aes-256-ctr", str_repeat("k", 32), OPENSSL_RAW_DATA, $iv)), "|";
echo bin2hex(openssl_encrypt($pt, "aes-192-ecb", str_repeat("k", 24), OPENSSL_RAW_DATA)), "|";
echo bin2hex(openssl_encrypt($pt, "aes-256-cbc", str_repeat("k", 32), OPENSSL_RAW_DATA, "")), "|";
echo bin2hex(openssl_encrypt($pt, "aes-256-ctr", str_repeat("k", 32), OPENSSL_RAW_DATA, ""));
"#,
    );
    assert_eq!(
        out,
        concat!(
            "835efdd4ef3b4970b9f3db30d9127055f882c14e350482e2dbb2d280a3a88f6d",
            "b7268c07781eaef8c5bb68f682c1ccfe|",
            "4ed947c17245c2403b7f3bbc62aa3ea7c495c675da80902be67c78d801659339c",
            "0c7d00344365121b8fbf0|",
            "d3040c8b0ca3c82b183939dcc25b1556acf172ad850be4639d2e78c1902697284",
            "ee72046aa5ca058b5c0dc44ef422d1f|",
            "e17e9c1f4520d16e0ec019104b5ea547f0f3dfcfa25a8b813c3abf951dd8506e",
            "462b38a791415c8fcc3f56e357502a55|",
            "8aa92548f4b92874dbd7c62cc707e5c01ba5680094fb75f3a91357dd6a4d0e1a",
            "880bd0c5a6d56d1a4b7175",
        )
    );
}

/// Verifies default base64 output and raw/default decrypt round trips.
#[test]
fn test_openssl_non_aead_base64_and_raw_roundtrip() {
    if skip_without_openssl_native("test_openssl_non_aead_base64_and_raw_roundtrip") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$pt = "The quick brown fox jumps over the lazy dog";
$key = str_repeat("k", 32);
$iv = str_repeat("i", 16);
$encoded = openssl_encrypt($pt, "AES-256-CBC", $key, 0, $iv);
$raw = openssl_encrypt($pt, "aes-256-ctr", $key, OPENSSL_RAW_DATA, $iv);
echo $encoded, "|";
echo openssl_decrypt($encoded, "aes-256-cbc", $key, 0, $iv), "|";
echo openssl_decrypt($raw, "AES-256-CTR", $key, OPENSSL_RAW_DATA, $iv);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "vjHwei9me/NHVOjsRbsmgr8x9DDcswOrCJp60TzpOCYLVV1y+KcUFF5Dr3y1CiHh|",
            "The quick brown fox jumps over the lazy dog|",
            "The quick brown fox jumps over the lazy dog",
        )
    );
}

/// Verifies zero padding succeeds for aligned plaintext and rejects partial blocks.
#[test]
fn test_openssl_zero_padding_non_aead() {
    if skip_without_openssl_native("test_openssl_zero_padding_non_aead") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$key = str_repeat("k", 32);
$iv = str_repeat("i", 16);
$aligned = "0123456789abcdef0123456789abcdef";
$ciphertext = openssl_encrypt($aligned, "aes-256-cbc", $key, OPENSSL_RAW_DATA | OPENSSL_ZERO_PADDING, $iv);
echo bin2hex($ciphertext), "|";
echo openssl_decrypt($ciphertext, "aes-256-cbc", $key, OPENSSL_RAW_DATA | OPENSSL_ZERO_PADDING, $iv), "|";
echo openssl_encrypt("short", "aes-256-cbc", $key, OPENSSL_RAW_DATA | OPENSSL_ZERO_PADDING, $iv) === false ? "false" : "bad";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "471e64cc510482e98305e3091b78fdbd91dbec72fc540174e38d251b11e15285|",
            "0123456789abcdef0123456789abcdef|false",
        )
    );
}

/// Verifies constants, IV lengths, unknown-cipher false, and PHP's 135-name inventory.
#[test]
fn test_openssl_constants_and_cipher_helpers() {
    if skip_without_openssl_native("test_openssl_constants_and_cipher_helpers") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
namespace CryptoFixture;
$methods = openssl_get_cipher_methods();
echo OPENSSL_RAW_DATA, ":", OPENSSL_ZERO_PADDING, ":", OPENSSL_DONT_ZERO_PAD_KEY, "|";
echo OpEnSsL_CiPhEr_Iv_LeNgTh("aes-128-cbc"), ":", openssl_cipher_iv_length("AES-256-GCM"), ":", openssl_cipher_iv_length("aes-192-ecb"), "|";
echo openssl_cipher_iv_length("not-a-cipher") === false ? "false" : "bad", "|";
echo count($methods), ":", in_array("aes-128-cbc", $methods) ? "yes" : "no", ":", in_array("aes-256-gcm", $methods) ? "yes" : "no", ":", in_array("aria-128-cbc", $methods) ? "yes" : "no", ":", in_array("aes-128-wrap", $methods) ? "yes" : "no";
"#,
    );
    assert_eq!(out, "1:2:4|16:12:0|false|135:yes:yes:yes:yes");
}

/// Verifies unknown ciphers and invalid padded ciphertext return false in both directions.
#[test]
fn test_openssl_non_aead_failures_return_false() {
    if skip_without_openssl_native("test_openssl_non_aead_failures_return_false") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$key = str_repeat("k", 32);
$iv = str_repeat("i", 16);
echo openssl_encrypt("data", "unknown", $key, OPENSSL_RAW_DATA, $iv) === false ? "e" : "bad";
echo openssl_decrypt("not-a-block", "aes-256-cbc", $key, OPENSSL_RAW_DATA, $iv) === false ? "d" : "bad";
echo openssl_encrypt("data", "aes-256-cbc", "short", OPENSSL_RAW_DATA | OPENSSL_DONT_ZERO_PAD_KEY, $iv) === false ? "k" : "bad";
echo openssl_decrypt("%%%", "aes-256-cbc", $key, 0, $iv) === false ? "b" : "bad";
"#,
    );
    assert_eq!(out, "edkb");
}

/// Verifies every GCM key size and supported truncated-tag edge matches PHP end to end.
#[test]
fn test_openssl_gcm_php_goldens_and_tag_writeback() {
    if skip_without_openssl_native("test_openssl_gcm_php_goldens_and_tag_writeback") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$pt = "The quick brown fox jumps over the lazy dog";
$key = str_repeat("k", 32);
$iv = str_repeat("i", 12);
$ciphertext = openssl_encrypt($pt, "aes-256-gcm", $key, OPENSSL_RAW_DATA, $iv, $tag, "fixture-aad");
echo bin2hex($ciphertext), ":", bin2hex($tag), "|";
$short = openssl_encrypt(tag: $short_tag, data: $pt, cipher_algo: "AES-256-GCM", passphrase: $key, options: OPENSSL_RAW_DATA, iv: $iv, aad: "fixture-aad", tag_length: 12);
echo bin2hex($short), ":", bin2hex($short_tag), "|";
$key128 = str_repeat("k", 16);
$cipher128 = openssl_encrypt($pt, "aes-128-gcm", $key128, OPENSSL_RAW_DATA, $iv, $tag128, "fixture-aad");
$plain128 = openssl_decrypt($cipher128, "aes-128-gcm", $key128, OPENSSL_RAW_DATA, $iv, $tag128, "fixture-aad");
echo bin2hex($cipher128), ":", bin2hex($tag128), ":", ($plain128 === $pt ? "ok" : "bad"), "|";
$key192 = str_repeat("k", 24);
$cipher192 = openssl_encrypt($pt, "aes-192-gcm", $key192, OPENSSL_RAW_DATA, $iv, $tag192, "fixture-aad");
$plain192 = openssl_decrypt($cipher192, "aes-192-gcm", $key192, OPENSSL_RAW_DATA, $iv, $tag192, "fixture-aad");
echo bin2hex($cipher192), ":", bin2hex($tag192), ":", ($plain192 === $pt ? "ok" : "bad"), "|";
$cipher1 = openssl_encrypt($pt, "aes-256-gcm", $key, OPENSSL_RAW_DATA, $iv, $tag1, "fixture-aad", 1);
$plain1 = openssl_decrypt($cipher1, "aes-256-gcm", $key, OPENSSL_RAW_DATA, $iv, $tag1, "fixture-aad");
echo bin2hex($tag1), ":", ($plain1 === $pt ? "ok" : "bad"), "|";
$cipher4 = openssl_encrypt($pt, "aes-256-gcm", $key, OPENSSL_RAW_DATA, $iv, $tag4, "fixture-aad", 4);
$plain4 = openssl_decrypt($cipher4, "aes-256-gcm", $key, OPENSSL_RAW_DATA, $iv, $tag4, "fixture-aad");
echo bin2hex($tag4), ":", ($plain4 === $pt ? "ok" : "bad");
"#,
    );
    assert_eq!(
        out,
        concat!(
            "8eec9d54438dc8679074830af4604b47f4cb062a754edb3c561d21b76dbac62b",
            "6fd7b2c040ca67e3e5647a:9dc8b5e7b800bfed5c1be43fc4614f51|",
            "8eec9d54438dc8679074830af4604b47f4cb062a754edb3c561d21b76dbac62b",
            "6fd7b2c040ca67e3e5647a:9dc8b5e7b800bfed5c1be43f|",
            "498c432b6dc88827705706a39e646b6a0698aa89834fb084768c9c416f758018",
            "ccc5ae865bc43a1db8b91a:bec1dfa8e8781396a92dc401358ee08c:ok|",
            "4213f3f4708cef7d03ac167c8487ec43b14e740e0f0ea458b41ae4eca28cb9cb",
            "46d24ec873f24e40010cf5:f9ebd39bfa23886420d8a159d50f4dad:ok|",
            "9d:ok|9dc8b5e7:ok",
        )
    );
}

/// Verifies GCM decrypt authenticates AAD and accepts a non-default 16-byte IV.
#[test]
fn test_openssl_gcm_roundtrip_aad_and_non_default_iv() {
    if skip_without_openssl_native("test_openssl_gcm_roundtrip_aad_and_non_default_iv") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$pt = "The quick brown fox jumps over the lazy dog";
$key = str_repeat("k", 32);
$iv = str_repeat("i", 16);
$ciphertext = openssl_encrypt($pt, "aes-256-gcm", $key, OPENSSL_RAW_DATA, $iv, $tag, "fixture-aad");
echo bin2hex($ciphertext), ":", bin2hex($tag), "|";
echo openssl_decrypt($ciphertext, "aes-256-gcm", $key, OPENSSL_RAW_DATA, $iv, $tag, "fixture-aad"), "|";
echo openssl_decrypt($ciphertext, "aes-256-gcm", $key, OPENSSL_RAW_DATA, $iv, $tag, "wrong-aad") === false ? "false" : "bad", "|";
$default_iv = str_repeat("i", 12);
$encoded = openssl_encrypt($pt, "aes-256-gcm", $key, 0, $default_iv, $encoded_tag, "fixture-aad");
echo $encoded, ":", bin2hex($encoded_tag), ":";
echo openssl_decrypt($encoded, "aes-256-gcm", $key, 0, $default_iv, $encoded_tag, "fixture-aad");
"#,
    );
    assert_eq!(
        out,
        concat!(
            "058c1967af117fc4142b6c851c3ca083c65b71f3c28d9041a8247fc5f3070afb",
            "513839a2043457948c44b6:69490efb2851c5a47142b09f074b143e|",
            "The quick brown fox jumps over the lazy dog|false|",
            "juydVEONyGeQdIMK9GBLR/TLBip1Tts8Vh0ht226xitv17LAQMpn4+Vkeg==:",
            "9dc8b5e7b800bfed5c1be43fc4614f51:The quick brown fox jumps over the lazy dog",
        )
    );
}

/// Verifies GCM rejects missing/wrong tags, empty IVs, and invalid requested tag lengths.
#[test]
fn test_openssl_gcm_failure_paths_return_false() {
    if skip_without_openssl_native("test_openssl_gcm_failure_paths_return_false") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$pt = "The quick brown fox jumps over the lazy dog";
$key = str_repeat("k", 32);
$iv = str_repeat("i", 12);
$ciphertext = openssl_encrypt($pt, "aes-256-gcm", $key, OPENSSL_RAW_DATA, $iv, $tag, "fixture-aad");
echo openssl_decrypt($ciphertext, "aes-256-gcm", $key, OPENSSL_RAW_DATA, $iv, str_repeat("x", 16), "fixture-aad") === false ? "w" : "bad";
echo openssl_decrypt($ciphertext, "aes-256-gcm", $key, OPENSSL_RAW_DATA, $iv) === false ? "m" : "bad";
echo openssl_encrypt($pt, "aes-256-gcm", $key, OPENSSL_RAW_DATA, "", $empty_iv_tag, "fixture-aad") === false ? "i" : "bad";
echo openssl_encrypt($pt, "aes-256-gcm", $key, OPENSSL_RAW_DATA, $iv, $zero_tag, "fixture-aad", 0) === false ? "z" : "bad";
echo openssl_encrypt($pt, "aes-256-gcm", $key, OPENSSL_RAW_DATA, $iv, $long_tag, "fixture-aad", 17) === false ? "l" : "bad";
echo openssl_encrypt($pt, "aes-256-gcm", $key, OPENSSL_RAW_DATA, $iv) === false ? "t" : "bad";
"#,
    );
    assert_eq!(out, "wmizlt");
}

/// Verifies empty GCM plaintext still writes and authenticates the PHP-golden tag.
#[test]
fn test_openssl_gcm_empty_plaintext_tag() {
    if skip_without_openssl_native("test_openssl_gcm_empty_plaintext_tag") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$key = str_repeat("k", 32);
$iv = str_repeat("i", 12);
$ciphertext = openssl_encrypt("", "aes-256-gcm", $key, OPENSSL_RAW_DATA, $iv, $tag, "fixture-aad");
echo strlen($ciphertext), ":", bin2hex($tag), ":";
$plain = openssl_decrypt($ciphertext, "aes-256-gcm", $key, OPENSSL_RAW_DATA, $iv, $tag, "fixture-aad");
echo $plain === false ? "false" : strlen($plain);
"#,
    );
    assert_eq!(out, "0:76a0e5a2ff64c6f1c8ee0f2c0f066c91:0");
}

/// Verifies GCM replaces tag storage while non-AEAD encryption leaves it unchanged.
#[test]
fn test_openssl_gcm_tag_overwrites_existing_and_ref_parameter_storage() {
    if skip_without_openssl_native("test_openssl_gcm_tag_overwrites_existing_and_ref_parameter_storage") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
function encrypt_with_tag(string $data, string $key, string $iv, &$tag) {
    return openssl_encrypt($data, "aes-256-gcm", $key, OPENSSL_RAW_DATA, $iv, $tag, "fixture-aad");
}
$pt = "The quick brown fox jumps over the lazy dog";
$key = str_repeat("k", 32);
$iv = str_repeat("i", 12);
$scalar_tag = 123;
$scalar_ciphertext = openssl_encrypt($pt, "aes-256-gcm", $key, OPENSSL_RAW_DATA, $iv, $scalar_tag, "fixture-aad");
echo bin2hex($scalar_ciphertext), ":", bin2hex($scalar_tag), "|";
$tag = "old-tag";
$ciphertext = encrypt_with_tag($pt, $key, $iv, $tag);
echo bin2hex($ciphertext), ":", bin2hex($tag), "|";
$cbc_tag = "keep";
openssl_encrypt($pt, "aes-256-cbc", $key, OPENSSL_RAW_DATA, $iv, $cbc_tag);
echo $cbc_tag;
"#,
    );
    assert_eq!(
        out,
        concat!(
            "8eec9d54438dc8679074830af4604b47f4cb062a754edb3c561d21b76dbac62b",
            "6fd7b2c040ca67e3e5647a:9dc8b5e7b800bfed5c1be43fc4614f51|",
            "8eec9d54438dc8679074830af4604b47f4cb062a754edb3c561d21b76dbac62b",
            "6fd7b2c040ca67e3e5647a:9dc8b5e7b800bfed5c1be43fc4614f51|keep",
        )
    );
}

/// Verifies the literal AES-CBC length folds and the raw-data fallback.
#[test]
fn test_openssl_cbc_length_folds() {
    if skip_without_openssl_native("test_openssl_cbc_length_folds") {
        return;
    }
    let out = compile_and_run_with_heap_debug(r#"<?php
$plain = "hello"; $iv = "1234567890123456";
echo strlen(openssl_encrypt($plain, "aes-128-cbc", "key", 0, $iv)), "|";
$cipher = openssl_encrypt($plain, "aes-128-cbc", "key", 0, $iv);
echo strlen(openssl_decrypt($cipher, "aes-128-cbc", "key", 0, $iv)), "|";
echo strlen(openssl_encrypt("", "aes-256-cbc", "", 0, $iv)), "|";
$raw = openssl_encrypt($plain, "aes-128-cbc", "key", OPENSSL_RAW_DATA, $iv);
echo strlen($raw), "|";
echo "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "24|5|24|16|\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Verifies the pinned literal IV-length fold and the dynamic/alias runtime fallback.
#[test]
fn test_openssl_cipher_iv_length_literal_fold_and_fallback() {
    if skip_without_openssl_native("test_openssl_cipher_iv_length_literal_fold_and_fallback") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$cipher = $argc > 0 ? "aes-128-cbc" : "aes-256-gcm";
echo openssl_cipher_iv_length($cipher), ":";
echo openssl_cipher_iv_length("AES-256-GCM"), ":";
echo openssl_cipher_iv_length("aes-128-wrap"), ":";
echo openssl_cipher_iv_length("aes128"), ":";
echo openssl_cipher_iv_length("not-a-cipher") === false ? "false" : "bad";
"#,
    );
    assert_eq!(out, "16:12:8:16:false");
}
