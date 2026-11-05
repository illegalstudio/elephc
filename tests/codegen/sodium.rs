//! Purpose:
//! End-to-end coverage for PHP's `ext/sodium` surface: constants, utility functions,
//! every cryptographic family, by-reference state mutation, error text, and eval.
//!
//! Called from:
//! - `cargo test --test codegen_tests sodium` through Rust's test harness.
//!
//! Key details:
//! - Deterministic vectors are captured from the pinned PHP 8.5.10 + libsodium 1.0.22
//!   oracle, so a passing test proves compatibility with php-src rather than merely
//!   self-consistency.
//! - Randomised constructions (box seal, secretstream headers) assert round trips and
//!   structural invariants instead of fixed bytes.
//! - The fixture links the `elephc_sodium` bridge and the managed `libsodium` package;
//!   `skip_without_sodium_native` keeps a machine without the artifact honest.

use crate::support::*;

/// Utilities, integer arithmetic, padding and encoding vectors.
#[test]
fn test_sodium_utility_vectors() {
    let out = compile_and_run(
        r#"<?php
$s = "\x00\x01\xfe\xff";
echo bin2hex(sodium_bin2hex($s)), "\n";
echo bin2hex(sodium_hex2bin("00 01 fe ff", " ")), "\n";
$b = sodium_bin2base64($s, SODIUM_BASE64_VARIANT_URLSAFE_NO_PADDING);
echo $b, "\n";
echo bin2hex(sodium_base642bin($b, SODIUM_BASE64_VARIANT_URLSAFE_NO_PADDING)), "\n";
echo sodium_compare("abc", "abd"), ",", sodium_compare("abd", "abc"), ",", sodium_compare("abc", "abc"), "\n";
$n = hex2bin("00ff"); sodium_increment($n); echo bin2hex($n), "\n";
$a = hex2bin("ff01"); sodium_add($a, hex2bin("0101")); echo bin2hex($a), "\n";
$p = sodium_pad("hello", 8); echo bin2hex($p), "\n"; echo sodium_unpad($p, 8), "\n";
$z = "secret"; sodium_memzero($z); var_dump($z);
"#,
    );
    assert_eq!(
        out,
        "3030303166656666\n0001feff\nAAH-_w\n0001feff\n-1,1,0\n01ff\n0003\n68656c6c6f800000\nhello\nNULL\n"
    );
}

/// Secretbox, auth, shorthash, BLAKE2b and KDF vectors.
#[test]
fn test_sodium_symmetric_vectors() {
    let out = compile_and_run(
        r#"<?php
$h = fn($s) => bin2hex($s);
$key = hex2bin(str_repeat("01", 32));
$nonce = hex2bin(str_repeat("02", 24));
$msg = "attack at dawn";
echo $h(sodium_crypto_secretbox($msg, $nonce, $key)), "\n";
echo sodium_crypto_secretbox_open(sodium_crypto_secretbox($msg, $nonce, $key), $nonce, $key), "\n";
echo $h(sodium_crypto_auth($msg, $key)), "\n";
var_dump(sodium_crypto_auth_verify(sodium_crypto_auth($msg, $key), $msg, $key));
$shk = hex2bin(str_repeat("03", 16));
echo $h(sodium_crypto_shorthash($msg, $shk)), "\n";
$ghk = hex2bin(str_repeat("03", 32));
echo $h(sodium_crypto_generichash($msg, $ghk)), "\n";
echo $h(sodium_crypto_generichash($msg, "", 64)), "\n";
$state = sodium_crypto_generichash_init($ghk);
sodium_crypto_generichash_update($state, "attack ");
sodium_crypto_generichash_update($state, "at dawn");
echo $h(sodium_crypto_generichash_final($state)), "\n";
var_dump($state);
echo $h(sodium_crypto_kdf_derive_from_key(32, 7, "context!", $key)), "\n";
"#,
    );
    assert_eq!(
        out,
        "63ee8eb5bf46771459d8023e80c939809c11aa9c1d51f22c145dd39f2f28\n\
         attack at dawn\n\
         d25128067d831ede3b48faf4955fca46f03c2c34455cc666d4b5a4020a98fbd6\n\
         bool(true)\n\
         de5a4ee100f2a801\n\
         6d37295eb8c6cf7d92fdd294ab6f0cd3fec390aab042080f9b77a10e019224ec\n\
         0582a5a78e629316b4542ec5f6d0e97da7bca7feb4ec9542c39a881bb61ddfbc3016e2bbb00b81fa522437ad8398f9622ec2e37c57563eab1e421eff0b494028\n\
         6d37295eb8c6cf7d92fdd294ab6f0cd3fec390aab042080f9b77a10e019224ec\n\
         NULL\n\
         a849c33c235fa2f33b49782989a25fa93d3ebb0583a572abc037a495cd5ada20\n"
    );
}

/// Stream ciphers, box, sealed box and detached/attached signatures.
#[test]
fn test_sodium_public_key_and_stream_vectors() {
    let out = compile_and_run(
        r#"<?php
$h = fn($s) => bin2hex($s);
$key = hex2bin(str_repeat("01", 32));
$nonce = hex2bin(str_repeat("02", 24));
$msg = "attack at dawn";
echo $h(sodium_crypto_stream(16, $nonce, $key)), "\n";
echo $h(sodium_crypto_stream_xor($msg, $nonce, $key)), "\n";
echo $h(sodium_crypto_stream_xchacha20(16, $nonce, $key)), "\n";
echo $h(sodium_crypto_stream_xchacha20_xor($msg, $nonce, $key)), "\n";
echo $h(sodium_crypto_stream_xchacha20_xor_ic($msg, $nonce, 5, $key)), "\n";
$boxsk = hex2bin(str_repeat("04", 32));
$boxpk = sodium_crypto_box_publickey_from_secretkey($boxsk);
$boxkp = $boxsk . $boxpk;
echo $h(sodium_crypto_box($msg, $nonce, $boxkp)), "\n";
echo sodium_crypto_box_open(sodium_crypto_box($msg, $nonce, $boxkp), $nonce, $boxkp), "\n";
$sealed = sodium_crypto_box_seal($msg, $boxpk);
var_dump(strlen($sealed));
echo sodium_crypto_box_seal_open($sealed, $boxkp), "\n";
$seed = hex2bin(str_repeat("05", 32));
$signkp = sodium_crypto_sign_seed_keypair($seed);
$signsk = sodium_crypto_sign_secretkey($signkp);
$signpk = sodium_crypto_sign_publickey($signkp);
echo $h(sodium_crypto_sign($msg, $signsk)), "\n";
echo sodium_crypto_sign_open(sodium_crypto_sign($msg, $signsk), $signpk), "\n";
echo $h(sodium_crypto_sign_detached($msg, $signsk)), "\n";
var_dump(sodium_crypto_sign_verify_detached(sodium_crypto_sign_detached($msg, $signsk), $msg, $signpk));
echo $h(sodium_crypto_sign_ed25519_pk_to_curve25519($signpk)), "\n";
echo $h(sodium_crypto_sign_ed25519_sk_to_curve25519($signsk)), "\n";
"#,
    );
    assert_eq!(
        out,
        "63aaaf184d07dbb96a64db8288ae3f3c\n\
         02dedb792e6cfbd81e44bfe3ffc0\n\
         b6bb0ee1dfec94dd5ebe93cf321db795\n\
         d7cf7a80bc87b4bc2a9ef7ae4573\n\
         04cd59026981f01477b74262993c\n\
         d873edf27f7fa841a9fb873714a4332d5a4ef4a0d04dec0356c523498cf9\n\
         attack at dawn\n\
         int(62)\n\
         attack at dawn\n\
         e6823e06bc7ba9f1a872db8217a3d67855bbc511395a683a473d7f1117df165866c554e0371e3a2a021d78a54b621617669a9881900d34e72ce32dfb659b690761747461636b206174206461776e\n\
         attack at dawn\n\
         e6823e06bc7ba9f1a872db8217a3d67855bbc511395a683a473d7f1117df165866c554e0371e3a2a021d78a54b621617669a9881900d34e72ce32dfb659b6907\n\
         bool(true)\n\
         c44e429251771ec76197c7a1f8ea289a18ca3dd7a7e102ba7cc84df6b55cbe1a\n\
         48370d6146de919cc1ce472897775d9a6c2834c509e08e14efcb2b52188f946e\n"
    );
}

/// Every AEAD family: exact ciphertext vector, round trip and tamper rejection.
#[test]
fn test_sodium_aead_vectors() {
    let out = compile_and_run(
        r#"<?php
$h = fn($s) => bin2hex($s);
$msg = "attack at dawn";
$ad = "aad";

$k = hex2bin(str_repeat("06", 16)); $n = hex2bin(str_repeat("07", 16));
$c = sodium_crypto_aead_aegis128l_encrypt($msg, $ad, $n, $k); echo $h($c), "\n";
echo sodium_crypto_aead_aegis128l_decrypt($c, $ad, $n, $k), "\n";
$c[0] = chr(ord($c[0]) ^ 1); var_dump(sodium_crypto_aead_aegis128l_decrypt($c, $ad, $n, $k));

$k = hex2bin(str_repeat("06", 32)); $n = hex2bin(str_repeat("07", 32));
$c = sodium_crypto_aead_aegis256_encrypt($msg, $ad, $n, $k); echo $h($c), "\n";
echo sodium_crypto_aead_aegis256_decrypt($c, $ad, $n, $k), "\n";
$c[0] = chr(ord($c[0]) ^ 1); var_dump(sodium_crypto_aead_aegis256_decrypt($c, $ad, $n, $k));

$k = hex2bin(str_repeat("06", 32)); $n = hex2bin(str_repeat("07", 12));
$c = sodium_crypto_aead_aes256gcm_encrypt($msg, $ad, $n, $k); echo $h($c), "\n";
echo sodium_crypto_aead_aes256gcm_decrypt($c, $ad, $n, $k), "\n";
$c[0] = chr(ord($c[0]) ^ 1); var_dump(sodium_crypto_aead_aes256gcm_decrypt($c, $ad, $n, $k));

$k = hex2bin(str_repeat("06", 32)); $n = hex2bin(str_repeat("07", 8));
$c = sodium_crypto_aead_chacha20poly1305_encrypt($msg, $ad, $n, $k); echo $h($c), "\n";
echo sodium_crypto_aead_chacha20poly1305_decrypt($c, $ad, $n, $k), "\n";
$c[0] = chr(ord($c[0]) ^ 1); var_dump(sodium_crypto_aead_chacha20poly1305_decrypt($c, $ad, $n, $k));

$n = hex2bin(str_repeat("07", 12));
$c = sodium_crypto_aead_chacha20poly1305_ietf_encrypt($msg, $ad, $n, $k); echo $h($c), "\n";
echo sodium_crypto_aead_chacha20poly1305_ietf_decrypt($c, $ad, $n, $k), "\n";
$c[0] = chr(ord($c[0]) ^ 1); var_dump(sodium_crypto_aead_chacha20poly1305_ietf_decrypt($c, $ad, $n, $k));

$n = hex2bin(str_repeat("07", 24));
$c = sodium_crypto_aead_xchacha20poly1305_ietf_encrypt($msg, $ad, $n, $k); echo $h($c), "\n";
echo sodium_crypto_aead_xchacha20poly1305_ietf_decrypt($c, $ad, $n, $k), "\n";
$c[0] = chr(ord($c[0]) ^ 1); var_dump(sodium_crypto_aead_xchacha20poly1305_ietf_decrypt($c, $ad, $n, $k));
"#,
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "372549fe356800fe400add980d3639dff5aca570be3d5725a56abd64ca2c08ca7747994424e89fb9b4038b6b9c99");
    assert_eq!(lines[1], "attack at dawn");
    assert_eq!(lines[2], "bool(false)");
    assert_eq!(lines[3], "d75fcbf0a6b6f1c59c8fa29b6688142487b021680b6dd613f775f58a9f0c8fbb9b77c87f7ad3ccc7364da3efeb7f");
    assert_eq!(lines[4], "attack at dawn");
    assert_eq!(lines[5], "bool(false)");
    assert_eq!(lines[6], "c38b8655e58cf36ecf99420c73796cddfb232c1af27ce2a9b1d68312bd1b");
    assert_eq!(lines[7], "attack at dawn");
    assert_eq!(lines[8], "bool(false)");
    assert_eq!(lines[9], "ed73cd0256d2358e3686ce54841ec897547252203db501618838bcbf903e");
    assert_eq!(lines[10], "attack at dawn");
    assert_eq!(lines[11], "bool(false)");
    assert_eq!(lines[12], "660a2d215b1a30d6d5ac1b84c63883814cabbf059c33579e6f8ee5ab74b5");
    assert_eq!(lines[13], "attack at dawn");
    assert_eq!(lines[14], "bool(false)");
    assert_eq!(lines[15], "aaf360059f2b6b8810c39539e38ba6f25b594f9dcba7ebd79ac35890cecd");
    assert_eq!(lines[16], "attack at dawn");
    assert_eq!(lines[17], "bool(false)");
}

/// Password hashing, scalar multiplication, Ristretto255 and key exchange vectors.
#[test]
fn test_sodium_kdf_and_group_vectors() {
    let out = compile_and_run(
        r#"<?php
$h = fn($s) => bin2hex($s);
$salt = hex2bin(str_repeat("08", 16));
echo $h(sodium_crypto_pwhash(32, "password", $salt, SODIUM_CRYPTO_PWHASH_OPSLIMIT_INTERACTIVE, SODIUM_CRYPTO_PWHASH_MEMLIMIT_INTERACTIVE, SODIUM_CRYPTO_PWHASH_ALG_ARGON2ID13)), "\n";
$salt2 = hex2bin(str_repeat("09", 32));
echo $h(sodium_crypto_pwhash_scryptsalsa208sha256(32, "password", $salt2, SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_OPSLIMIT_INTERACTIVE, SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_MEMLIMIT_INTERACTIVE)), "\n";
$scalar = hex2bin(str_repeat("0a", 32));
echo $h(sodium_crypto_scalarmult_base($scalar)), "\n";
$point = sodium_crypto_scalarmult_base(hex2bin(str_repeat("0b", 32)));
echo $h(sodium_crypto_scalarmult($scalar, $point)), "\n";
$r = sodium_crypto_core_ristretto255_from_hash(hex2bin(str_repeat("0c", 64)));
echo $h($r), "\n";
var_dump(sodium_crypto_core_ristretto255_is_valid_point($r));
echo $h(sodium_crypto_core_ristretto255_add($r, $r)), "\n";
echo $h(sodium_crypto_core_ristretto255_sub(sodium_crypto_core_ristretto255_add($r, $r), $r)), "\n";
echo $h(sodium_crypto_core_ristretto255_scalar_add($scalar, $scalar)), "\n";
echo $h(sodium_crypto_core_ristretto255_scalar_complement($scalar)), "\n";
echo $h(sodium_crypto_core_ristretto255_scalar_invert($scalar)), "\n";
echo $h(sodium_crypto_core_ristretto255_scalar_mul($scalar, $scalar)), "\n";
echo $h(sodium_crypto_core_ristretto255_scalar_negate($scalar)), "\n";
echo $h(sodium_crypto_core_ristretto255_scalar_reduce(hex2bin(str_repeat("0d", 64)))), "\n";
$kxseed = hex2bin(str_repeat("0f", 32));
$client = sodium_crypto_kx_seed_keypair($kxseed);
$server = sodium_crypto_kx_seed_keypair(hex2bin(str_repeat("10", 32)));
$cs = sodium_crypto_kx_client_session_keys($client, sodium_crypto_kx_publickey($server));
$ss = sodium_crypto_kx_server_session_keys($server, sodium_crypto_kx_publickey($client));
echo $h($cs[0]), ",", $h($cs[1]), ";", $h($ss[0]), ",", $h($ss[1]), "\n";
var_dump($cs[0] === $ss[1] && $cs[1] === $ss[0]);
"#,
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "2389de4ae388ecc691c19b1301c9fbe6d6c7d8823bc8ea3bef616aa103969dc6");
    assert_eq!(lines[1], "a8719af16ac9fbd5147376e3d9c87dc9e6a3403103cfebf35fb3b7a0ea95ce0e");
    assert_eq!(lines[2], "f77ff4b10788bfdca62ca0bb160d427cf5762d85f2b5cad6807ec9c3febbde09");
    assert_eq!(lines[3], "c09d8a17f54f06a53f844eacbc6273017581b9bc53b5f31f3d338cc3ffd7b86b");
    assert_eq!(lines[4], "c406a2debedbbaf8383fe07b7f75e4032560c67e244f5d3ef61a3cf9834db14a");
    assert_eq!(lines[5], "bool(true)");
    assert_eq!(lines[6], "d6b4703e47312830e447868f3b48e3548cde8c5f540661a9e9c56b6ea88caa30");
    assert_eq!(lines[7], "c406a2debedbbaf8383fe07b7f75e4032560c67e244f5d3ef61a3cf9834db14a");
    assert_eq!(lines[8], "27401eb7f9b001bc3d771c71351a35ff13141414141414141414141414141404");
    assert_eq!(lines[9], "e4c9eb521059084ecc92ed98d4efd40af6f5f5f5f5f5f5f5f5f5f5f5f5f5f505");
    assert_eq!(lines[10], "3a849c0332da6e09fc3801b171c77fe1d5d83b4cfad455ff6542787434c27000");
    assert_eq!(lines[11], "f9b5148dd9ab5374991a9d62a30a7016ee2f700cbaa5ee0b4ed04e500e3b4507");
    assert_eq!(lines[12], "e3c9eb521059084ecc92ed98d4efd40af6f5f5f5f5f5f5f5f5f5f5f5f5f5f505");
    assert_eq!(lines[13], "4f2ea248fd2cc33062d4d469fd9ede5f2d8207d0244341bf9bc64e009b4c8405");
    assert_eq!(
        lines[14],
        "bcd9d550bb5d0f4f35cbfc5b79c6e9788c1a3f22c24470610b8c3c3b2f2a0485,bc0e2b427f12b4a5a9be902f96f02c141d01d74854631067ae2b1741a2a08abe;bc0e2b427f12b4a5a9be902f96f02c141d01d74854631067ae2b1741a2a08abe,bcd9d550bb5d0f4f35cbfc5b79c6e9788c1a3f22c24470610b8c3c3b2f2a0485"
    );
    assert_eq!(lines[15], "bool(true)");
}

/// Secretstream push/pull through the by-reference state parameter.
#[test]
fn test_sodium_secretstream_round_trip() {
    let out = compile_and_run(
        r#"<?php
$key = hex2bin(str_repeat("0e", 32));
$init = sodium_crypto_secretstream_xchacha20poly1305_init_push($key);
var_dump(count($init), strlen($init[0]), strlen($init[1]));
$state = $init[0];
$header = $init[1];
$c1 = sodium_crypto_secretstream_xchacha20poly1305_push($state, "first", "", SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_MESSAGE);
$c2 = sodium_crypto_secretstream_xchacha20poly1305_push($state, "second", "", SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_FINAL);
$pull = sodium_crypto_secretstream_xchacha20poly1305_init_pull($header, $key);
$r1 = sodium_crypto_secretstream_xchacha20poly1305_pull($pull, $c1, "");
$r2 = sodium_crypto_secretstream_xchacha20poly1305_pull($pull, $c2, "");
echo $r1[0], " ", $r1[1], "\n";
echo $r2[0], " ", $r2[1], "\n";
var_dump(sodium_crypto_secretstream_xchacha20poly1305_pull($pull, $c2, ""));
"#,
    );
    assert_eq!(
        out,
        "int(2)\nint(52)\nint(24)\nfirst 0\nsecond 3\nbool(false)\n"
    );
}

/// Constants, extension reporting and the `SodiumException` hierarchy.
#[test]
fn test_sodium_constants_and_exception() {
    let out = compile_and_run(
        r#"<?php
var_dump(SODIUM_LIBRARY_VERSION);
var_dump(SODIUM_LIBRARY_MAJOR_VERSION, SODIUM_LIBRARY_MINOR_VERSION);
var_dump(SODIUM_CRYPTO_SECRETBOX_KEYBYTES, SODIUM_CRYPTO_AEAD_AEGIS256_KEYBYTES);
var_dump(extension_loaded("sodium"));
$e = new SodiumException("boom");
var_dump($e instanceof Exception, $e->getMessage());
try {
    sodium_crypto_secretbox("x", "short", str_repeat("k", 32));
} catch (SodiumException $e) {
    echo $e->getMessage(), "\n";
}
try {
    sodium_crypto_generichash("x", "", 10);
} catch (SodiumException $e) {
    echo $e->getMessage(), "\n";
}
try {
    sodium_unpad("bad", 8);
} catch (SodiumException $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "string(6) \"1.0.22\"\nint(26)\nint(4)\nint(32)\nint(32)\nbool(true)\nbool(true)\nstring(4) \"boom\"\n\
         sodium_crypto_secretbox(): Argument #2 ($nonce) must be SODIUM_CRYPTO_SECRETBOX_NONCEBYTES bytes long\n\
         unsupported output length\n\
         sodium_unpad(): Argument #1 ($string) must be at least as long as the block size\n"
    );
}

/// `--with-sodium` and eval forwarding share the compiled prelude helpers.
#[test]
fn test_sodium_eval_forwarding() {
    let out = compile_and_run(
        r#"<?php
$code = 'echo bin2hex(sodium_bin2hex("abc")), "\n";';
eval($code);
$hash = sodium_crypto_generichash("abc");
$code = 'echo strlen(sodium_crypto_generichash("abc")), "\n";';
eval($code);
var_dump(strlen($hash));
"#,
    );
    assert_eq!(out, "363136323633\n32\nint(32)\n");
}

/// Key-generation/extraction families plus extension metadata.
#[test]
fn test_sodium_key_surface_and_extension_funcs() {
    let out = compile_and_run(
        r#"<?php
$box = sodium_crypto_box_keypair();
var_dump(strlen($box));
var_dump(strlen(sodium_crypto_box_secretkey($box)), strlen(sodium_crypto_box_publickey($box)));
var_dump(sodium_crypto_box_keypair_from_secretkey_and_publickey(sodium_crypto_box_secretkey($box), sodium_crypto_box_publickey($box)) === $box);
var_dump(sodium_crypto_box_publickey_from_secretkey(sodium_crypto_box_secretkey($box)) === sodium_crypto_box_publickey($box));
$seeded = sodium_crypto_box_seed_keypair(str_repeat("s", 32));
var_dump($seeded === sodium_crypto_box_seed_keypair(str_repeat("s", 32)));
$kx = sodium_crypto_kx_keypair();
var_dump(strlen($kx), strlen(sodium_crypto_kx_secretkey($kx)), strlen(sodium_crypto_kx_publickey($kx)));
var_dump(sodium_crypto_kx_seed_keypair(str_repeat("k", 32)) === sodium_crypto_kx_seed_keypair(str_repeat("k", 32)));
$sign = sodium_crypto_sign_keypair();
var_dump(strlen($sign), strlen(sodium_crypto_sign_secretkey($sign)), strlen(sodium_crypto_sign_publickey($sign)));
var_dump(sodium_crypto_sign_publickey_from_secretkey(sodium_crypto_sign_secretkey($sign)) === sodium_crypto_sign_publickey($sign));
var_dump(sodium_crypto_sign_seed_keypair(str_repeat("z", 32)) === sodium_crypto_sign_seed_keypair(str_repeat("z", 32)));
var_dump(sodium_crypto_aead_aes256gcm_is_available());
var_dump(strlen(sodium_crypto_aead_aegis128l_keygen()), strlen(sodium_crypto_aead_aegis256_keygen()));
var_dump(strlen(sodium_crypto_core_ristretto255_random()), strlen(sodium_crypto_core_ristretto255_scalar_random()));
var_dump(strlen(sodium_crypto_scalarmult_ristretto255_base(str_repeat("r", 32))));
"#,
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "int(64)");
    assert_eq!(lines[1], "int(32)");
    assert_eq!(lines[2], "int(32)");
    assert_eq!(lines[3], "bool(true)");
    assert_eq!(lines[4], "bool(true)");
    assert_eq!(lines[5], "bool(true)");
    assert_eq!(lines[6], "int(64)");
    assert_eq!(lines[7], "int(32)");
    assert_eq!(lines[8], "int(32)");
    assert_eq!(lines[9], "bool(true)");
    assert_eq!(lines[10], "int(96)");
    assert_eq!(lines[11], "int(64)");
    assert_eq!(lines[12], "int(32)");
    assert_eq!(lines[13], "bool(true)");
    assert_eq!(lines[14], "bool(true)");
    assert_eq!(lines[15], "bool(true)");
    assert_eq!(lines[16], "int(16)");
    assert_eq!(lines[17], "int(32)");
    assert_eq!(lines[18], "int(32)");
    assert_eq!(lines[19], "int(32)");
}

/// `get_extension_funcs('sodium')` reports the complete shared catalog surface.
#[test]
fn test_sodium_extension_funcs() {
    let out = compile_and_run(
        r#"<?php
$functions = get_extension_funcs('sodium');
var_dump(is_array($functions));
if (is_array($functions)) {
    echo count($functions), "\n";
    var_dump(in_array('sodium_add', $functions, true), in_array('sodium_crypto_pwhash_str', $functions, true));
}
"#,
    );
    assert_eq!(out, "bool(true)\n110\nbool(true)\nbool(true)\n");
}

/// The core password provider's Argon2i/Argon2id surface built on the sodium bridge.
#[test]
fn test_password_argon2_provider() {
    let out = compile_and_run(
        r#"<?php
$hash = password_hash('secret', PASSWORD_ARGON2ID, ['memory_cost' => 8191, 'time_cost' => 2, 'threads' => 1]);
var_dump(substr($hash, 0, 10));
var_dump(password_verify('secret', $hash), password_verify('wrong', $hash));
$info = password_get_info($hash);
var_dump($info['algo'], $info['algoName']);
var_dump($info['options']['memory_cost'], $info['options']['time_cost'], $info['options']['threads']);
var_dump(in_array('argon2i', password_algos(), true), in_array('argon2id', password_algos(), true));
var_dump(PASSWORD_ARGON2_PROVIDER);
$i = password_hash('secret', PASSWORD_ARGON2I, ['memory_cost' => 8191, 'time_cost' => 4]);
var_dump(substr($i, 0, 9), password_verify('secret', $i));
var_dump(password_needs_rehash($hash, PASSWORD_ARGON2ID, ['memory_cost' => 8191, 'time_cost' => 2]));
try { password_hash('x', PASSWORD_ARGON2ID, ['memory_cost' => 1]); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { password_hash('x', PASSWORD_ARGON2ID, ['time_cost' => 0]); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { password_hash('x', PASSWORD_ARGON2ID, ['threads' => 2]); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { password_hash('x', 'not-an-algorithm'); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
var_dump(password_needs_rehash($hash, PASSWORD_ARGON2ID, ['memory_cost' => 1]));
var_dump(password_needs_rehash($hash, 'not-an-algorithm'));
"#,
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "string(10) \"$argon2id$\"");
    assert_eq!(lines[1], "bool(true)");
    assert_eq!(lines[2], "bool(false)");
    assert_eq!(lines[3], "string(8) \"argon2id\"");
    assert_eq!(lines[4], "string(8) \"argon2id\"");
    assert_eq!(lines[5], "int(8191)");
    assert_eq!(lines[6], "int(2)");
    assert_eq!(lines[7], "int(1)");
    assert_eq!(lines[8], "bool(true)");
    assert_eq!(lines[9], "bool(true)");
    assert_eq!(lines[10], "string(8) \"standard\"");
    assert_eq!(lines[11], "string(9) \"$argon2i$\"");
    assert_eq!(lines[12], "bool(true)");
    assert_eq!(lines[13], "bool(false)");
    assert_eq!(lines[14], "Memory cost is outside of allowed memory range");
    assert_eq!(lines[15], "Time cost is outside of allowed time range");
    assert_eq!(lines[16], "A thread value other than 1 is not supported by this implementation");
    assert_eq!(
        lines[17],
        "password_hash(): Argument #2 ($algo) must be a valid password hashing algorithm"
    );
    assert_eq!(lines[18], "bool(true)");
    assert_eq!(lines[19], "bool(false)");
}

/// Eval's by-reference sodium mutators run through hidden PHP wrappers.
#[test]
fn test_sodium_eval_by_reference_mutators() {
    let out = compile_and_run(
        r#"<?php
eval('$a = "abc"; sodium_increment($a); echo $a, "\n";');
eval('$b = hex2bin("ff01"); sodium_add($b, hex2bin("0101")); echo bin2hex($b), "\n";');
eval('$z = "secret"; sodium_memzero($z); var_dump($z);');
eval('$k = str_repeat("k", 32); $s = sodium_crypto_generichash_init($k); sodium_crypto_generichash_update($s, "abc"); echo bin2hex(sodium_crypto_generichash_final($s)), "\n"; var_dump($s);');
eval('$key = str_repeat("k", 32); $init = sodium_crypto_secretstream_xchacha20poly1305_init_push($key); $state = $init[0]; $header = $init[1]; $c = sodium_crypto_secretstream_xchacha20poly1305_push($state, "hi", "", 0); $pull = sodium_crypto_secretstream_xchacha20poly1305_init_pull($header, $key); $r = sodium_crypto_secretstream_xchacha20poly1305_pull($pull, $c, ""); echo $r[0], " ", $r[1], " ", strlen($pull), "\n"; sodium_crypto_secretstream_xchacha20poly1305_rekey($pull); echo strlen($pull), "\n";');
"#,
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "bbc");
    assert_eq!(lines[1], "0003");
    assert_eq!(lines[2], "NULL");
    assert_eq!(lines[3], "0a9b1f4f633832a535f3e4539d906998c3814fbcd4cc136e57e8ea742ac36b61");
    assert_eq!(lines[4], "NULL");
    assert_eq!(lines[5], "hi 0 52");
    assert_eq!(lines[6], "52");
}
