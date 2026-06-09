//! Purpose:
//! Integration tests validating elephc-crypto digests against published test
//! vectors and PHP golden values.
//!
//! Called from:
//! - `cargo test -p elephc-crypto` through Rust's test harness.
//!
//! Key details:
//! - Calls the C ABI functions directly (the crate links as an rlib in tests).
//! - Vectors are NIST/RFC published values; non-crypto checksums are cross-checked
//!   against `php -r 'echo hash(...);'`.

use elephc_crypto::*;

/// Convenience: run the one-shot C ABI and return the lowercase hex digest, or
/// `None` when the algorithm is unknown (ABI returns -1).
fn hash_hex(algo: &str, data: &[u8]) -> Option<String> {
    let mut out = [0u8; 64];
    let n = unsafe {
        elephc_crypto_hash(
            algo.as_ptr(),
            algo.len(),
            data.as_ptr(),
            data.len(),
            out.as_mut_ptr(),
        )
    };
    if n < 0 {
        return None;
    }
    Some(out[..n as usize].iter().map(|b| format!("{:02x}", b)).collect())
}

/// Verifies one-shot crypto digests against published NIST/RFC known-answer vectors.
#[test]
fn crypto_one_shot_known_vectors() {
    assert_eq!(hash_hex("md5", b"").unwrap(), "d41d8cd98f00b204e9800998ecf8427e");
    assert_eq!(hash_hex("md5", b"abc").unwrap(), "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(hash_hex("sha1", b"abc").unwrap(), "a9993e364706816aba3e25717850c26c9cd0d89d");
    assert_eq!(
        hash_hex("sha256", b"abc").unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hash_hex("sha512", b"abc").unwrap(),
        "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
    );
    assert_eq!(
        hash_hex("sha3-256", b"abc").unwrap(),
        "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"
    );
    assert_eq!(hash_hex("ripemd160", b"abc").unwrap(), "8eb208f7e05d987a9b044a8e98c6b087f15a0bfc");
}

/// Verifies that an unrecognized algorithm name causes the ABI to return -1 (PHP ValueError path).
#[test]
fn unknown_algorithm_returns_negative() {
    assert!(hash_hex("tiger", b"abc").is_none());
    assert!(hash_hex("not-a-hash", b"abc").is_none());
}

/// Verifies non-crypto checksum digests byte-for-byte against PHP 8.4 golden values (input "abc").
#[test]
fn non_crypto_checksum_vectors_match_php() {
    // Golden values produced by `php -r 'echo hash($algo, "abc");'` (PHP 8.4).
    assert_eq!(hash_hex("crc32", b"abc").unwrap(), "73bb8c64");
    assert_eq!(hash_hex("crc32b", b"abc").unwrap(), "352441c2");
    assert_eq!(hash_hex("crc32c", b"abc").unwrap(), "364b3fb7");
    assert_eq!(hash_hex("adler32", b"abc").unwrap(), "024d0127");
    assert_eq!(hash_hex("fnv132", b"abc").unwrap(), "439c2f4b");
    assert_eq!(hash_hex("fnv1a32", b"abc").unwrap(), "1a47e90b");
    assert_eq!(hash_hex("fnv164", b"abc").unwrap(), "d8dcca186bafadcb");
    assert_eq!(hash_hex("fnv1a64", b"abc").unwrap(), "e71fa2190541574b");
    assert_eq!(hash_hex("joaat", b"abc").unwrap(), "ed131f5b");
    // Standard CRC-32C check value: hash("crc32c", "123456789") == "e3069283" in PHP 8.4.
    assert_eq!(hash_hex("crc32c", b"123456789").unwrap(), "e3069283");
}

/// Verifies non-crypto checksum digests for the empty string against PHP 8.4 golden values.
#[test]
fn non_crypto_checksum_empty_string_vectors_match_php() {
    // PHP 8.4 `hash($algo, "")` golden values.
    assert_eq!(hash_hex("crc32", b"").unwrap(), "00000000");
    assert_eq!(hash_hex("crc32b", b"").unwrap(), "00000000");
    assert_eq!(hash_hex("crc32c", b"").unwrap(), "00000000");
    assert_eq!(hash_hex("adler32", b"").unwrap(), "00000001");
    assert_eq!(hash_hex("fnv132", b"").unwrap(), "811c9dc5");
    assert_eq!(hash_hex("fnv1a32", b"").unwrap(), "811c9dc5");
    assert_eq!(hash_hex("fnv164", b"").unwrap(), "cbf29ce484222325");
    assert_eq!(hash_hex("fnv1a64", b"").unwrap(), "cbf29ce484222325");
    assert_eq!(hash_hex("joaat", b"").unwrap(), "00000000");
}

/// Verifies that every supported algorithm produces a raw digest of the documented byte length.
#[test]
fn all_algorithms_produce_correct_digest_length() {
    // (algorithm name, raw digest size in bytes)
    let cases: &[(&str, usize)] = &[
        ("md2", 16), ("md4", 16), ("md5", 16), ("sha1", 20),
        ("sha224", 28), ("sha256", 32), ("sha384", 48), ("sha512", 64),
        ("sha512/224", 28), ("sha512/256", 32),
        ("sha3-224", 28), ("sha3-256", 32), ("sha3-384", 48), ("sha3-512", 64),
        ("ripemd128", 16), ("ripemd160", 20), ("ripemd256", 32), ("ripemd320", 40),
        ("whirlpool", 64),
        ("crc32", 4), ("crc32b", 4), ("crc32c", 4), ("adler32", 4),
        ("fnv132", 4), ("fnv1a32", 4), ("fnv164", 8), ("fnv1a64", 8), ("joaat", 4),
    ];
    for (algo, len) in cases {
        let hex = hash_hex(algo, b"the quick brown fox")
            .unwrap_or_else(|| panic!("algorithm {algo} returned unknown (-1)"));
        assert_eq!(hex.len(), len * 2, "wrong digest length for {algo}");
    }
}

/// Runs the one-shot HMAC C ABI; returns the lowercase hex digest or None (-1).
/// Arg order mirrors the ABI: (algo, key, data). PHP's hash_hmac is ($algo,$data,$key).
fn hmac_hex(algo: &str, key: &[u8], data: &[u8]) -> Option<String> {
    let mut out = [0u8; 64];
    let n = unsafe {
        elephc_crypto_hmac(
            algo.as_ptr(), algo.len(),
            key.as_ptr(), key.len(),
            data.as_ptr(), data.len(),
            out.as_mut_ptr(),
        )
    };
    if n < 0 { return None; }
    Some(out[..n as usize].iter().map(|b| format!("{:02x}", b)).collect())
}

/// Verifies one-shot HMAC digests against PHP 8.4 golden values and RFC 4231 vectors.
#[test]
fn hmac_matches_php_golden() {
    // PHP hash_hmac($algo,$data,$key); helper takes (algo, key, data).
    assert_eq!(
        hmac_hex("sha256", b"Jefe", b"what do ya want for nothing?").unwrap(),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
    assert_eq!(
        hmac_hex("sha1", b"key", b"abc").unwrap(),
        "4fd0b215276ef12f2b3e4c8ecac2811498b656fc"
    );
    assert_eq!(
        hmac_hex("md5", b"secret", b"The quick brown fox").unwrap(),
        "313f4de51b3a46edb724f38a8520c61e"
    );
    // 200-byte key exceeds sha256's 64-byte block, exercising the hash-down path.
    let long_key = vec![b'k'; 200];
    assert_eq!(
        hmac_hex("sha256", &long_key, b"abc").unwrap(),
        "ce632aa86d6a3fd3c79f06217c0a506599d055cd38eb385b16a2939f2488f686"
    );
    // sha512 has a 128-byte block (vs 64 for sha1/sha256), exercising that path.
    assert_eq!(
        hmac_hex("sha512", b"key", b"abc").unwrap(),
        "3926a207c8c42b0c41792cbd3e1a1aaaf5f7a25704f62dfc939c4987dd7ce060\
009c5bb1c2447355b3216f10b537e9afa7b64a4e5391b0d631172d07939e087a"
    );
}

/// Verifies that HMAC rejects non-crypto checksums (block_size==0) and unknown algorithms.
#[test]
fn hmac_rejects_checksums_and_unknown() {
    // crc32b and adler32 are rejected because they are non-crypto checksums
    // (block_size == 0), matching PHP's ValueError for hash_hmac over checksums.
    assert!(hmac_hex("crc32b", b"key", b"abc").is_none());
    assert!(hmac_hex("adler32", b"key", b"abc").is_none());
    // tiger is rejected because it is an unknown algorithm (make() returns None).
    assert!(hmac_hex("tiger", b"key", b"abc").is_none());
}

use std::os::raw::c_void;

/// Finalizes a context via the C ABI and returns the lowercase hex digest.
fn finalize_hex(ctx: *mut c_void) -> String {
    let mut out = [0u8; 64];
    let n = unsafe { elephc_crypto_final(ctx, out.as_mut_ptr()) };
    assert!(n >= 0);
    out[..n as usize].iter().map(|b| format!("{:02x}", b)).collect()
}

/// Verifies that feeding data incrementally via update() produces the same digest as one-shot hash().
#[test]
fn incremental_matches_one_shot() {
    let ctx = unsafe { elephc_crypto_init(b"sha256".as_ptr(), 6) };
    assert!(!ctx.is_null());
    unsafe {
        elephc_crypto_update(ctx, b"ab".as_ptr(), 2);
        elephc_crypto_update(ctx, b"c".as_ptr(), 1);
    }
    assert_eq!(finalize_hex(ctx), hash_hex("sha256", b"abc").unwrap());
}

/// Verifies that cloning a context mid-stream produces a fully independent copy that diverges correctly.
#[test]
fn clone_produces_independent_state() {
    let ctx = unsafe { elephc_crypto_init(b"sha256".as_ptr(), 6) };
    unsafe { elephc_crypto_update(ctx, b"a".as_ptr(), 1); }
    let ctx2 = unsafe { elephc_crypto_clone(ctx) };
    assert!(!ctx2.is_null());
    // Diverge after cloning: ctx -> "abc", ctx2 -> "aXY".
    unsafe {
        elephc_crypto_update(ctx, b"bc".as_ptr(), 2);
        elephc_crypto_update(ctx2, b"XY".as_ptr(), 2);
    }
    let h1 = finalize_hex(ctx);
    let h2 = finalize_hex(ctx2);
    assert_eq!(h1, hash_hex("sha256", b"abc").unwrap());
    assert_eq!(h2, hash_hex("sha256", b"aXY").unwrap());
    assert_ne!(h1, h2, "clone must be independent of the original");
}

/// Verifies that incremental HMAC streaming via init_hmac/update/final matches the one-shot HMAC ABI.
#[test]
fn incremental_hmac_matches_one_shot() {
    let key = b"Jefe";
    let ctx = unsafe { elephc_crypto_init_hmac(b"sha256".as_ptr(), 6, key.as_ptr(), key.len()) };
    assert!(!ctx.is_null());
    unsafe {
        elephc_crypto_update(ctx, b"what do ya ".as_ptr(), 11);
        elephc_crypto_update(ctx, b"want for nothing?".as_ptr(), 17);
    }
    assert_eq!(
        finalize_hex(ctx),
        hmac_hex("sha256", key, b"what do ya want for nothing?").unwrap()
    );
}

/// Verifies that init() returns null for an unrecognized algorithm name.
#[test]
fn init_unknown_algorithm_returns_null() {
    let ctx = unsafe { elephc_crypto_init(b"tiger".as_ptr(), 5) };
    assert!(ctx.is_null());
}

/// Verifies that init_hmac() returns null for non-crypto checksums (PHP rejects HMAC over checksums).
#[test]
fn init_hmac_rejects_checksum_returns_null() {
    // PHP rejects hash_init(..., HASH_HMAC) over non-crypto checksums.
    let key = b"key";
    let ctx = unsafe { elephc_crypto_init_hmac(b"crc32b".as_ptr(), 6, key.as_ptr(), key.len()) };
    assert!(ctx.is_null());
}

/// Verifies that free() on an unfinalized context does not leak or double-free (Miri/valgrind-clean).
#[test]
fn free_releases_unfinalized_context() {
    let ctx = unsafe { elephc_crypto_init(b"sha256".as_ptr(), 6) };
    unsafe { elephc_crypto_update(ctx, b"abc".as_ptr(), 3); }
    unsafe { elephc_crypto_free(ctx) }; // must not leak/double-free
}

/// Verifies that final() on a null context handle returns -1 without crashing.
#[test]
fn final_on_null_context_returns_negative() {
    let mut out = [0u8; 64];
    let n = unsafe { elephc_crypto_final(std::ptr::null_mut(), out.as_mut_ptr()) };
    assert_eq!(n, -1);
}

/// Verifies that cloning an HMAC streaming context produces an independent copy that diverges correctly.
#[test]
fn hmac_clone_produces_independent_state() {
    let key = b"Jefe";
    let ctx = unsafe { elephc_crypto_init_hmac(b"sha256".as_ptr(), 6, key.as_ptr(), key.len()) };
    unsafe { elephc_crypto_update(ctx, b"what do ya ".as_ptr(), 11); }
    let ctx2 = unsafe { elephc_crypto_clone(ctx) };
    assert!(!ctx2.is_null());
    unsafe {
        elephc_crypto_update(ctx, b"want for nothing?".as_ptr(), 17);
        elephc_crypto_update(ctx2, b"DIFFERENT".as_ptr(), 9);
    }
    let h1 = finalize_hex(ctx);
    let h2 = finalize_hex(ctx2);
    assert_eq!(h1, hmac_hex("sha256", key, b"what do ya want for nothing?").unwrap());
    assert_eq!(h2, hmac_hex("sha256", key, b"what do ya DIFFERENT").unwrap());
    assert_ne!(h1, h2);
}
