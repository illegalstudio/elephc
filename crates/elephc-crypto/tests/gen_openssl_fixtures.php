<?php
// Generates the PHP golden fixtures for the elephc-crypto openssl cipher
// engine (crates/elephc-crypto/tests/fixtures/openssl_php_fixtures.rs).
//
// Usage:
//   php crates/elephc-crypto/tests/gen_openssl_fixtures.php \
//       > crates/elephc-crypto/tests/fixtures/openssl_php_fixtures.rs
//
// The generated file locks the observed behavior of the local PHP build for
// the cipher matrix documented in docs/php/strings.md, Symmetric encryption.
// Regenerate only when intentionally re-baselining against a new PHP version;
// bump FIXTURE_PHP_VERSION notes in the output when doing so.

declare(strict_types=1);

error_reporting(E_ALL);
if (!extension_loaded('openssl')) {
    fwrite(STDERR, "openssl extension is required\n");
    exit(1);
}

const PT43 = 'The quick brown fox jumps over the lazy dog'; // 43 bytes: exercises PKCS#7
const PT16 = '0123456789abcdef';                            // one AES block
const PT32 = '0123456789abcdef0123456789abcdef';            // two AES blocks
const AAD  = 'fixture-aad';
const SHORT_KEY8 = 'shortkey';                              // 8 bytes: exercises key zero-pad

function k(int $n): string { return str_repeat('k', $n); }
function iv16(): string { return str_repeat('i', 16); }
function iv12(): string { return str_repeat('i', 12); }

// Encrypt with OPENSSL_RAW_DATA and return ciphertext (and tag for AEAD).
// Dies with context on unexpected failure so a broken baseline never ships.
// $quiet silences PHP warnings for vectors that intentionally exercise
// warning-producing edge cases (short/long/empty IV).
function raw_encrypt(
    string $data, string $cipher, string $key, int $options, string $iv,
    ?string $aad = null, int $tag_length = 16, ?string &$tag = null,
    bool $quiet = false
): string {
    $opts = $options | OPENSSL_RAW_DATA;
    if ($aad !== null) {
        $ct = $quiet
            ? @openssl_encrypt($data, $cipher, $key, $opts, $iv, $tag, $aad, $tag_length)
            : openssl_encrypt($data, $cipher, $key, $opts, $iv, $tag, $aad, $tag_length);
    } else {
        $ct = $quiet
            ? @openssl_encrypt($data, $cipher, $key, $opts, $iv)
            : openssl_encrypt($data, $cipher, $key, $opts, $iv);
    }
    if ($ct === false) {
        fwrite(STDERR, "fixture encrypt failed: $cipher\n");
        exit(1);
    }
    return $ct;
}

// Round-trip self-check for every generated vector (PHP vs itself).
function check_round_trip(
    string $ct, string $plain, string $cipher, string $key, int $options, string $iv,
    ?string $aad = null, ?string $tag = null, bool $quiet = false
): void {
    $opts = $options | OPENSSL_RAW_DATA;
    if ($aad !== null) {
        $pt = $quiet
            ? @openssl_decrypt($ct, $cipher, $key, $opts, $iv, $tag, $aad)
            : openssl_decrypt($ct, $cipher, $key, $opts, $iv, $tag, $aad);
    } else {
        $pt = $quiet
            ? @openssl_decrypt($ct, $cipher, $key, $opts, $iv)
            : openssl_decrypt($ct, $cipher, $key, $opts, $iv);
    }
    if ($pt !== $plain) {
        fwrite(STDERR, "fixture round-trip failed: $cipher\n");
        exit(1);
    }
}

function hex(string $s): string { return bin2hex($s); }

// Run one PHP call and capture at most one PHP-level warning. OpenSSL's provider
// error queue is deliberately drained and excluded because its wording is tied
// to the linked OpenSSL release rather than PHP's public surface.
function observe(string $case, callable $call): array {
    $warnings = [];
    set_error_handler(static function (int $_level, string $message) use (&$warnings): bool {
        $warnings[] = $message;
        return true;
    });

    try {
        $result = $call();
    } finally {
        restore_error_handler();
    }

    if (count($warnings) > 1) {
        fwrite(STDERR, "fixture call emitted multiple PHP warnings: $case\n");
        exit(1);
    }

    while (openssl_error_string() !== false) {}

    return [$result, $warnings[0] ?? ''];
}

// Run a PHP call that must return false and record its PHP-level warning.
function failure(
    string $case, string $operation, string $cipher, string $data, string $key,
    string $iv, int $options, string $aad, string $tag, int $tag_length,
    callable $call
): array {
    [$result, $warning] = observe($case, $call);
    if ($result !== false) {
        fwrite(STDERR, "fixture failure unexpectedly succeeded: $case\n");
        exit(1);
    }

    return [
        'case' => $case,
        'operation' => $operation,
        'cipher' => $cipher,
        'data' => $data,
        'key' => $key,
        'iv' => $iv,
        'options' => $options,
        'aad' => $aad,
        'tag' => $tag,
        'tag_length' => $tag_length,
        'warning' => $warning,
    ];
}

// ---------------------------------------------------------------------------
// 1. Matrix vectors: one encrypt vector per cipher (raw ciphertext).
// ---------------------------------------------------------------------------
$matrix = []; // [cipher, key, iv]
foreach ([128, 192, 256] as $bits) {
    $matrix[] = ["aes-$bits-cbc", k($bits >> 3), iv16()];
}
foreach ([128, 192, 256] as $bits) {
    $matrix[] = ["aes-$bits-ecb", k($bits >> 3), ''];
}
foreach ([128, 192, 256] as $bits) {
    $matrix[] = ["aes-$bits-ctr", k($bits >> 3), iv16()];
}

$vectors = []; // emitted Rust rows
foreach ($matrix as [$cipher, $key, $iv]) {
    $ct = raw_encrypt(PT43, $cipher, $key, 0, $iv);
    check_round_trip($ct, PT43, $cipher, $key, 0, $iv);
    $vectors[] = [
        'name' => $cipher, 'key' => $key, 'iv' => $iv, 'aad' => '',
        'ct' => $ct, 'tag' => '', 'tag_length' => 16,
    ];
}

// ---------------------------------------------------------------------------
// 2. GCM vectors: with AAD and with empty AAD (ciphertext is AAD-independent;
//    the tag is not).
// ---------------------------------------------------------------------------
$gcm = [];
foreach ([128, 192, 256] as $bits) {
    foreach ([AAD, ''] as $aad) {
        $tag = null;
        $cipher = "aes-$bits-gcm";
        $key = k($bits >> 3);
        $ct = raw_encrypt(PT43, $cipher, $key, 0, iv12(), $aad, 16, $tag);
        check_round_trip($ct, PT43, $cipher, $key, 0, iv12(), $aad, $tag);
        $gcm[] = [
            'name' => $cipher, 'key' => $key, 'iv' => iv12(), 'aad' => $aad,
            'ct' => $ct, 'tag' => (string)$tag, 'tag_length' => 16,
        ];
    }
}

// GCM tag truncation: ciphertext is unchanged; tag is the first N bytes.
$gcm_tag_lengths = [];
foreach ([1, 4, 12] as $tl) {
    $tag = null;
    $ct = raw_encrypt(PT43, 'aes-256-gcm', k(32), 0, iv12(), AAD, $tl, $tag);
    check_round_trip($ct, PT43, 'aes-256-gcm', k(32), 0, iv12(), AAD, $tag);
    $gcm_tag_lengths[] = ['tag_length' => $tl, 'ct' => $ct, 'tag' => (string)$tag];
}

// Non-default GCM IV lengths: PHP/OpenSSL accepts every non-empty length. The
// 12-byte default is already covered by the main matrix vectors.
$gcm_iv_lengths = [];
foreach ([1, 16, 20] as $iv_length) {
    $iv = str_repeat('i', $iv_length);
    $tag = null;
    $ct = raw_encrypt(PT43, 'aes-256-gcm', k(32), 0, $iv, AAD, 16, $tag);
    check_round_trip($ct, PT43, 'aes-256-gcm', k(32), 0, $iv, AAD, $tag);
    $gcm_iv_lengths[] = ['iv' => $iv, 'ct' => $ct, 'tag' => (string)$tag];
}

// ---------------------------------------------------------------------------
// 3. Empty plaintext edge (CBC pads to a full block; CTR/GCM emit nothing).
// ---------------------------------------------------------------------------
$empty_cbc = raw_encrypt('', 'aes-256-cbc', k(32), 0, iv16());
check_round_trip($empty_cbc, '', 'aes-256-cbc', k(32), 0, iv16());
$empty_ctr = raw_encrypt('', 'aes-256-ctr', k(32), 0, iv16());
if ($empty_ctr !== '') { fwrite(STDERR, "CTR empty plaintext not empty\n"); exit(1); }
check_round_trip($empty_ctr, '', 'aes-256-ctr', k(32), 0, iv16());
$empty_gcm_tag = null;
$empty_gcm = raw_encrypt('', 'aes-256-gcm', k(32), 0, iv12(), AAD, 16, $empty_gcm_tag);
if ($empty_gcm !== '') { fwrite(STDERR, "GCM empty plaintext not empty\n"); exit(1); }
check_round_trip(
    $empty_gcm, '', 'aes-256-gcm', k(32), 0, iv12(), AAD, $empty_gcm_tag
);

// ---------------------------------------------------------------------------
// 4. Key normalization edges (locked PHP behavior: short key zero-padded,
//    long key truncated).
// ---------------------------------------------------------------------------
$short_key_ct = raw_encrypt(PT43, 'aes-256-cbc', SHORT_KEY8, 0, iv16());
check_round_trip($short_key_ct, PT43, 'aes-256-cbc', SHORT_KEY8, 0, iv16());
$long_key_ct = raw_encrypt(PT43, 'aes-128-cbc', k(32), 0, iv16()); // truncated to 16
check_round_trip($long_key_ct, PT43, 'aes-128-cbc', k(32), 0, iv16());

// ---------------------------------------------------------------------------
// 5. CBC/CTR IV normalization and warnings on successful calls. Each row
//    records the ciphertext plus encrypt/decrypt warnings for one round-trip.
// ---------------------------------------------------------------------------
$iv_normalization = [];
$iv_ciphertexts = [];
$iv_cases = [
    'empty' => '',
    'short' => 'abc',
    'long' => str_repeat('a', 20),
];
foreach (['aes-256-cbc', 'aes-256-ctr'] as $cipher) {
    $mode = str_ends_with($cipher, '-cbc') ? 'cbc' : 'ctr';
    foreach ($iv_cases as $case => $iv) {
        [$ct, $encrypt_warning] = observe(
            "{$mode}_{$case}_iv_encrypt",
            fn() => openssl_encrypt(PT43, $cipher, k(32), OPENSSL_RAW_DATA, $iv)
        );
        if (!is_string($ct)) {
            fwrite(STDERR, "IV normalization encrypt failed: $cipher/$case\n");
            exit(1);
        }
        [$plain, $decrypt_warning] = observe(
            "{$mode}_{$case}_iv_decrypt",
            fn() => openssl_decrypt($ct, $cipher, k(32), OPENSSL_RAW_DATA, $iv)
        );
        if ($plain !== PT43) {
            fwrite(STDERR, "IV normalization round-trip failed: $cipher/$case\n");
            exit(1);
        }
        $encrypt_warns = str_starts_with($encrypt_warning, 'openssl_encrypt():');
        $decrypt_warning_is_expected = $case === 'empty'
            ? $decrypt_warning === ''
            : str_starts_with($decrypt_warning, 'openssl_decrypt():');
        if (!$encrypt_warns || !$decrypt_warning_is_expected) {
            fwrite(STDERR, "IV normalization warning mismatch: $cipher/$case\n");
            exit(1);
        }
        $iv_normalization[] = [
            'case' => $case,
            'cipher' => $cipher,
            'key' => k(32),
            'iv' => $iv,
            'plain' => PT43,
            'ct' => $ct,
            'encrypt_warning' => $encrypt_warning,
            'decrypt_warning' => $decrypt_warning,
        ];
        $iv_ciphertexts[$cipher][$case] = $ct;
    }
}

// ---------------------------------------------------------------------------
// 6. OPENSSL_ZERO_PADDING (block-aligned input only; nothing stripped on
//    decrypt).
// ---------------------------------------------------------------------------
$zp_cbc = raw_encrypt(PT32, 'aes-256-cbc', k(32), OPENSSL_ZERO_PADDING, iv16());
check_round_trip($zp_cbc, PT32, 'aes-256-cbc', k(32), OPENSSL_ZERO_PADDING, iv16());
$zp_ecb = raw_encrypt(PT32, 'aes-256-ecb', k(32), OPENSSL_ZERO_PADDING, '');
check_round_trip($zp_ecb, PT32, 'aes-256-ecb', k(32), OPENSSL_ZERO_PADDING, '');

// ---------------------------------------------------------------------------
// 7. Default (base64) mode output for the glue layer.
// ---------------------------------------------------------------------------
$b64 = openssl_encrypt(PT43, 'aes-256-cbc', k(32), 0, iv16());
if ($b64 === false || base64_decode($b64, true) === false) {
    fwrite(STDERR, "base64 fixture failed\n");
    exit(1);
}
$b64_round = openssl_decrypt($b64, 'aes-256-cbc', k(32), 0, iv16());
if ($b64_round !== PT43) { fwrite(STDERR, "base64 round-trip failed\n"); exit(1); }

// ---------------------------------------------------------------------------
// 8. iv_length table + case-insensitive lookup sanity.
// ---------------------------------------------------------------------------
$iv_lengths = [];
foreach (['aes-128-cbc', 'aes-192-cbc', 'aes-256-cbc',
          'aes-128-ecb', 'aes-192-ecb', 'aes-256-ecb',
          'aes-128-ctr', 'aes-192-ctr', 'aes-256-ctr',
          'aes-128-gcm', 'aes-192-gcm', 'aes-256-gcm'] as $cipher) {
    $n = openssl_cipher_iv_length($cipher);
    if ($n === false) { fwrite(STDERR, "iv_length failed: $cipher\n"); exit(1); }
    $iv_lengths[] = [$cipher, $n];
}
$upper = openssl_encrypt(PT43, 'AES-256-CBC', k(32), OPENSSL_RAW_DATA, iv16());
if ($upper === false) { fwrite(STDERR, "uppercase cipher name failed\n"); exit(1); }

// ---------------------------------------------------------------------------
// 9. Failure modes. All cases return false; `warning` records only a warning
//    emitted through PHP's error handler, never OpenSSL's provider error queue.
// ---------------------------------------------------------------------------
$failures = [];
$failures[] = failure(
    'unknown_cipher_encrypt', 'encrypt', 'not-a-cipher', PT43, k(32), iv16(),
    OPENSSL_RAW_DATA, '', '', 16,
    fn() => openssl_encrypt(PT43, 'not-a-cipher', k(32), OPENSSL_RAW_DATA, iv16())
);
$failures[] = failure(
    'unknown_cipher_decrypt', 'decrypt', 'not-a-cipher', 'ciphertext', k(32), iv16(),
    OPENSSL_RAW_DATA, '', '', 16,
    fn() => openssl_decrypt('ciphertext', 'not-a-cipher', k(32), OPENSSL_RAW_DATA, iv16())
);
$failures[] = failure(
    'unknown_cipher_iv_length', 'iv_length', 'not-a-cipher', '', '', '', 0, '', '', 0,
    fn() => openssl_cipher_iv_length('not-a-cipher')
);
$failures[] = failure(
    'short_key_dont_zero_pad_encrypt', 'encrypt', 'aes-256-cbc', PT43, SHORT_KEY8,
    iv16(), OPENSSL_RAW_DATA | OPENSSL_DONT_ZERO_PAD_KEY, '', '', 16,
    fn() => openssl_encrypt(
        PT43, 'aes-256-cbc', SHORT_KEY8,
        OPENSSL_RAW_DATA | OPENSSL_DONT_ZERO_PAD_KEY, iv16()
    )
);
$failures[] = failure(
    'zero_padding_unaligned_encrypt', 'encrypt', 'aes-256-cbc', PT43, k(32),
    iv16(), OPENSSL_RAW_DATA | OPENSSL_ZERO_PADDING, '', '', 16,
    fn() => openssl_encrypt(
        PT43, 'aes-256-cbc', k(32),
        OPENSSL_RAW_DATA | OPENSSL_ZERO_PADDING, iv16()
    )
);
$failures[] = failure(
    'zero_padding_misaligned_decrypt', 'decrypt', 'aes-256-cbc', 'abc', k(32),
    iv16(), OPENSSL_RAW_DATA | OPENSSL_ZERO_PADDING, '', '', 16,
    fn() => openssl_decrypt(
        'abc', 'aes-256-cbc', k(32),
        OPENSSL_RAW_DATA | OPENSSL_ZERO_PADDING, iv16()
    )
);
$failures[] = failure(
    'invalid_pkcs7_decrypt', 'decrypt', 'aes-256-cbc', str_repeat('x', 16), k(32),
    iv16(), OPENSSL_RAW_DATA, '', '', 16,
    fn() => openssl_decrypt(
        str_repeat('x', 16), 'aes-256-cbc', k(32), OPENSSL_RAW_DATA, iv16()
    )
);
$failures[] = failure(
    'gcm_empty_iv_encrypt', 'encrypt', 'aes-256-gcm', PT43, k(32), '',
    OPENSSL_RAW_DATA, AAD, '', 16,
    function (): string|false {
        $tag = null;
        return openssl_encrypt(
            PT43, 'aes-256-gcm', k(32), OPENSSL_RAW_DATA, '', $tag, AAD, 16
        );
    }
);
$good_gcm_tag = null;
$good_gcm_ct = raw_encrypt(
    PT43, 'aes-256-gcm', k(32), 0, iv12(), AAD, 16, $good_gcm_tag
);
$failures[] = failure(
    'gcm_wrong_tag_decrypt', 'decrypt', 'aes-256-gcm', $good_gcm_ct, k(32), iv12(),
    OPENSSL_RAW_DATA, AAD, str_repeat('x', 16), 16,
    fn() => openssl_decrypt(
        $good_gcm_ct, 'aes-256-gcm', k(32), OPENSSL_RAW_DATA, iv12(),
        str_repeat('x', 16), AAD
    )
);
$failures[] = failure(
    'gcm_missing_tag_decrypt', 'decrypt', 'aes-256-gcm', $good_gcm_ct, k(32), iv12(),
    OPENSSL_RAW_DATA, AAD, '', 0,
    fn() => openssl_decrypt(
        $good_gcm_ct, 'aes-256-gcm', k(32), OPENSSL_RAW_DATA, iv12(), '', AAD
    )
);
foreach ([0, 17] as $bad_tag_length) {
    $failures[] = failure(
        "gcm_tag_length_{$bad_tag_length}_encrypt", 'encrypt', 'aes-256-gcm', PT43,
        k(32), iv12(), OPENSSL_RAW_DATA, AAD, '', $bad_tag_length,
        function () use ($bad_tag_length): string|false {
            $tag = null;
            return openssl_encrypt(
                PT43, 'aes-256-gcm', k(32), OPENSSL_RAW_DATA, iv12(),
                $tag, AAD, $bad_tag_length
            );
        }
    );
}
$failures[] = failure(
    'invalid_base64_decrypt', 'decrypt', 'aes-256-cbc', '%%%', k(32), iv16(),
    0, '', '', 16,
    fn() => openssl_decrypt('%%%', 'aes-256-cbc', k(32), 0, iv16())
);

// ===========================================================================
// Emit Rust.
// ===========================================================================
function rust_hex(string $s): string { return hex($s) === '' ? '' : hex($s); }

function rust_string(string $s): string {
    return str_replace(
        ["\\", '"', "\n", "\r", "\t", "\0"],
        ["\\\\", '\\"', "\\n", "\\r", "\\t", "\\0"],
        $s
    );
}

function vector_row(array $v): string {
    return sprintf(
        "    CipherVector {\n" .
        "        cipher: \"%s\",\n" .
        "        key_hex: \"%s\",\n" .
        "        iv_hex: \"%s\",\n" .
        "        aad_hex: \"%s\",\n" .
        "        ciphertext_hex: \"%s\",\n" .
        "        tag_hex: \"%s\",\n" .
        "        tag_length: %d,\n" .
        "    },",
        $v['name'], rust_hex($v['key']), rust_hex($v['iv']), rust_hex($v['aad']),
        rust_hex($v['ct']), rust_hex($v['tag']), $v['tag_length']
    );
}

function gcm_iv_row(array $v): string {
    return sprintf(
        "    GcmIvVector { iv_hex: \"%s\", ciphertext_hex: \"%s\", tag_hex: \"%s\" },",
        rust_hex($v['iv']), rust_hex($v['ct']), rust_hex($v['tag'])
    );
}

function iv_normalization_row(array $v): string {
    return sprintf(
        "    IvNormalizationVector {\n" .
        "        case: \"%s\",\n" .
        "        cipher: \"%s\",\n" .
        "        key_hex: \"%s\",\n" .
        "        iv_hex: \"%s\",\n" .
        "        plaintext_hex: \"%s\",\n" .
        "        ciphertext_hex: \"%s\",\n" .
        "        encrypt_warning: \"%s\",\n" .
        "        decrypt_warning: \"%s\",\n" .
        "    },",
        rust_string($v['case']), rust_string($v['cipher']), rust_hex($v['key']),
        rust_hex($v['iv']), rust_hex($v['plain']), rust_hex($v['ct']),
        rust_string($v['encrypt_warning']), rust_string($v['decrypt_warning'])
    );
}

function failure_row(array $v): string {
    return sprintf(
        "    FailureVector {\n" .
        "        case: \"%s\",\n" .
        "        operation: \"%s\",\n" .
        "        cipher: \"%s\",\n" .
        "        data_hex: \"%s\",\n" .
        "        key_hex: \"%s\",\n" .
        "        iv_hex: \"%s\",\n" .
        "        options: %d,\n" .
        "        aad_hex: \"%s\",\n" .
        "        tag_hex: \"%s\",\n" .
        "        tag_length: %d,\n" .
        "        warning: \"%s\",\n" .
        "    },",
        rust_string($v['case']), rust_string($v['operation']), rust_string($v['cipher']),
        rust_hex($v['data']), rust_hex($v['key']), rust_hex($v['iv']), $v['options'],
        rust_hex($v['aad']), rust_hex($v['tag']), $v['tag_length'], rust_string($v['warning'])
    );
}

$php_version = PHP_MAJOR_VERSION . '.' . PHP_MINOR_VERSION . '.' . PHP_RELEASE_VERSION;
$openssl_version = OPENSSL_VERSION_TEXT;

$empty_cbc_hex = hex($empty_cbc);
$empty_ctr_hex = hex($empty_ctr);
$empty_gcm_hex = hex($empty_gcm);
$empty_gcm_tag_hex = hex((string)$empty_gcm_tag);
$short_key_ct_hex = hex($short_key_ct);
$long_key_ct_hex = hex($long_key_ct);
$empty_iv_ct_hex = hex($iv_ciphertexts['aes-256-cbc']['empty']);
$short_iv_ct_hex = hex($iv_ciphertexts['aes-256-cbc']['short']);
$long_iv_ct_hex = hex($iv_ciphertexts['aes-256-cbc']['long']);
$zp_cbc_hex = hex($zp_cbc);
$zp_ecb_hex = hex($zp_ecb);
$upper_hex = hex($upper);

$rows = [];
foreach ($vectors as $v) { $rows[] = vector_row($v); }
foreach ($gcm as $v) { $rows[] = vector_row($v); }
$vectors_rust = implode("\n", $rows);

$iv_rows = [];
foreach ($iv_lengths as [$name, $n]) { $iv_rows[] = "    (\"$name\", $n),"; }
$iv_rust = implode("\n", $iv_rows);

$tag_len_rows = [];
foreach ($gcm_tag_lengths as $t) {
    $tag_len_rows[] = sprintf(
        "    TagLengthVector { tag_length: %d, ciphertext_hex: \"%s\", tag_hex: \"%s\" },",
        $t['tag_length'], rust_hex($t['ct']), rust_hex($t['tag'])
    );
}
$tag_len_rust = implode("\n", $tag_len_rows);

$gcm_iv_rows = [];
foreach ($gcm_iv_lengths as $gcm_iv) { $gcm_iv_rows[] = gcm_iv_row($gcm_iv); }
$gcm_iv_rust = implode("\n", $gcm_iv_rows);

$iv_normalization_rows = [];
foreach ($iv_normalization as $iv_vector) {
    $iv_normalization_rows[] = iv_normalization_row($iv_vector);
}
$iv_normalization_rust = implode("\n", $iv_normalization_rows);

$failure_rows = [];
foreach ($failures as $failure_vector) { $failure_rows[] = failure_row($failure_vector); }
$failures_rust = implode("\n", $failure_rows);

$pt16_hex = hex(PT16);

echo <<<RUST
//! Purpose:
//! PHP golden fixtures for the elephc-crypto openssl cipher engine
//! (`openssl_encrypt` / `openssl_decrypt` parity). Generated by
//! `crates/elephc-crypto/tests/gen_openssl_fixtures.php` — do not edit by hand.
//!
//! Called from:
//! - Included by `crates/elephc-crypto/tests/openssl_php_fixtures.rs` and the
//!   phase 1 cipher ABI tests.
//!
//! Key details:
//! - Baselined against PHP $php_version with $openssl_version (local CLI).
//!   Repository CI does not pin or provision PHP; checked-in values are the CI
//!   baseline, and regeneration is an explicit version-stamped operation.
//! - All vectors use OPENSSL_RAW_DATA semantics: ciphertext/tag bytes are raw
//!   (hex-encoded here). Base64 handling lives in the PHP glue layer, and the
//!   one base64-mode golden string below pins it.
//! - Locked PHP quirks (observed on PHP $php_version):
//!   - empty or short CBC/CTR IV is zero-padded to the cipher IV length; long
//!     IV is truncated. Encrypt warns for all three cases; decrypt warns for
//!     short/long IV but not an empty IV. Both results and warning strings are
//!     captured in `IV_NORMALIZATION_VECTORS`;
//!   - short key is zero-padded to the key length unless
//!     OPENSSL_DONT_ZERO_PAD_KEY (then the call fails); long key is truncated;
//!   - GCM accepts any non-empty IV length (it must match between encrypt and
//!     decrypt); empty GCM IV fails;
//!   - GCM encrypt tag_length range is 1..=16; decrypt compares only the first
//!     `tag.len()` bytes of the expected tag;
//!   - OPENSSL_ZERO_PADDING requires block-aligned plaintext on encrypt and
//!     strips nothing on decrypt (CBC/ECB only; ignored by CTR);
//!   - cipher names are matched case-insensitively (uppercase vector below).
//! - Plaintexts: PT43 = "The quick brown fox jumps over the lazy dog" (43
//!   bytes, non-block-aligned), PT16/PT32 = "0123456789abcdef" repeated.

/// PHP version the fixtures were baselined against.
pub const FIXTURE_PHP_VERSION: &str = "$php_version";

/// OpenSSL implementation the fixture-generating PHP executable used.
pub const FIXTURE_OPENSSL_VERSION: &str = "$openssl_version";

/// 43-byte non-block-aligned plaintext used across the matrix vectors.
pub const PT43: &str = "The quick brown fox jumps over the lazy dog";

/// One-block plaintext used by zero-padding decrypt checks.
pub const PT16_HEX: &str = "$pt16_hex";

/// One encrypt vector against the PHP golden output for a fixed key/IV/AAD.
pub struct CipherVector {
    pub cipher: &'static str,
    pub key_hex: &'static str,
    pub iv_hex: &'static str,
    pub aad_hex: &'static str,
    pub ciphertext_hex: &'static str,
    pub tag_hex: &'static str,
    pub tag_length: usize,
}

/// One vector per supported non-AEAD cipher plus GCM with and without AAD
/// (GCM ciphertext is AAD-independent; the tag is not).
pub static CIPHER_VECTORS: &[CipherVector] = &[
$vectors_rust
];

/// GCM encrypt with truncated tags: ciphertext equals the tag_length=16
/// vector; the tag is its first `tag_length` bytes.
pub struct TagLengthVector {
    pub tag_length: usize,
    pub ciphertext_hex: &'static str,
    pub tag_hex: &'static str,
}

/// aes-256-gcm encrypt of PT43 (key/IV/AAD as the matching CIPHER_VECTORS
/// entry) with non-default tag lengths.
pub static GCM_TAG_LENGTH_VECTORS: &[TagLengthVector] = &[
$tag_len_rust
];

/// AES-256-GCM encrypt result for a non-default, non-empty IV length.
pub struct GcmIvVector {
    pub iv_hex: &'static str,
    pub ciphertext_hex: &'static str,
    pub tag_hex: &'static str,
}

/// PHP goldens proving that GCM accepts short and long non-empty IVs.
pub static GCM_NON_DEFAULT_IV_VECTORS: &[GcmIvVector] = &[
$gcm_iv_rust
];

/// One successful CBC/CTR round-trip with a normalized IV and captured PHP
/// warnings for both encrypt and decrypt.
pub struct IvNormalizationVector {
    pub case: &'static str,
    pub cipher: &'static str,
    pub key_hex: &'static str,
    pub iv_hex: &'static str,
    pub plaintext_hex: &'static str,
    pub ciphertext_hex: &'static str,
    pub encrypt_warning: &'static str,
    pub decrypt_warning: &'static str,
}

/// Empty, short, and long IV behavior for both CBC and CTR.
pub static IV_NORMALIZATION_VECTORS: &[IvNormalizationVector] = &[
$iv_normalization_rust
];

/// A PHP call that is expected to return `false`, with raw byte inputs encoded
/// as hex. `warning` is empty when PHP emits no user-facing warning.
pub struct FailureVector {
    pub case: &'static str,
    pub operation: &'static str,
    pub cipher: &'static str,
    pub data_hex: &'static str,
    pub key_hex: &'static str,
    pub iv_hex: &'static str,
    pub options: u32,
    pub aad_hex: &'static str,
    pub tag_hex: &'static str,
    pub tag_length: usize,
    pub warning: &'static str,
}

/// Failure behavior observed through PHP's public return/warning surface.
pub static FAILURE_VECTORS: &[FailureVector] = &[
$failures_rust
];

/// `openssl_cipher_iv_length` results for the full matrix.
pub static IV_LENGTHS: &[(&str, usize)] = &[
$iv_rust
];

/// Empty plaintext: AES-256-CBC pads to one full block.
pub const EMPTY_PT_CBC_CIPHERTEXT_HEX: &str = "$empty_cbc_hex";

/// Empty plaintext: AES-256-CTR ciphertext remains empty.
pub const EMPTY_PT_CTR_CIPHERTEXT_HEX: &str = "$empty_ctr_hex";

/// Empty plaintext: AES-256-GCM ciphertext remains empty.
pub const EMPTY_PT_GCM_CIPHERTEXT_HEX: &str = "$empty_gcm_hex";

/// Empty plaintext: AES-256-GCM still produces an authentication tag.
pub const EMPTY_PT_GCM_TAG_HEX: &str = "$empty_gcm_tag_hex";

/// AES-256-CBC with the 8-byte key "shortkey": PHP zero-pads the key to 32.
pub const SHORT_KEY_CIPHERTEXT_HEX: &str = "$short_key_ct_hex";

/// AES-128-CBC with a 32-byte key: PHP truncates the key to 16.
pub const LONG_KEY_CIPHERTEXT_HEX: &str = "$long_key_ct_hex";

/// AES-256-CBC with an empty IV: PHP zero-fills the IV to 16 bytes.
pub const EMPTY_IV_CIPHERTEXT_HEX: &str = "$empty_iv_ct_hex";

/// AES-256-CBC with a 3-byte IV "abc": PHP zero-pads the IV to 16 bytes.
pub const SHORT_IV_CIPHERTEXT_HEX: &str = "$short_iv_ct_hex";

/// AES-256-CBC with a 20-byte IV: PHP truncates the IV to 16 bytes.
pub const LONG_IV_CIPHERTEXT_HEX: &str = "$long_iv_ct_hex";

/// AES-256-CBC, OPENSSL_ZERO_PADDING, two-block plaintext.
pub const ZERO_PAD_CBC_CIPHERTEXT_HEX: &str = "$zp_cbc_hex";

/// AES-256-ECB, OPENSSL_ZERO_PADDING, two-block plaintext.
pub const ZERO_PAD_ECB_CIPHERTEXT_HEX: &str = "$zp_ecb_hex";

/// AES-256-CBC default mode (no OPENSSL_RAW_DATA): base64 of the ciphertext.
pub const CBC_BASE64: &str = "$b64";

/// AES-256-CBC ciphertext for the name "AES-256-CBC" — must equal the
/// lowercase-name vector, pinning case-insensitive cipher lookup.
pub const UPPERCASE_NAME_CIPHERTEXT_HEX: &str = "$upper_hex";

RUST;
