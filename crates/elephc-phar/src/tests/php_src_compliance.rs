//! Purpose:
//! Rust ports of the php-src PHAR signature and `phar.require_hash` PHPTs.
//!
//! Called from:
//! - `cargo test -p elephc-phar` through Rust's test harness.
//!
//! Key details:
//! - Each test names the php-src fixture it mirrors and asserts the same
//!   observable outcomes: unsigned executable phars rejected by default,
//!   unsigned data archives accepted, SHA-256 as the default signature, and the
//!   hash algorithms reporting PHP's `hash_type` names.
//! - Paths are isolated per process and test. `RequireHashGuard` restores the
//!   thread's policy so parallel tests never leak a relaxed `phar.require_hash`.

use super::signatures::{remove_signed_fixture, write_test_public_key, TEST_RSA_KEY_PEM};
use super::*;
use std::path::PathBuf;

/// Serializes test archive names across parallel tests in this module.
static TEST_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Returns an isolated temp path for one fixture.
fn temp_path(stem: &str, extension: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "elephc_phar_php_src_{}_{}_{}.{}",
        std::process::id(),
        stem,
        id,
        extension
    ))
}

/// Builds one uncompressed archive entry.
fn entry(name: &[u8], payload: &[u8]) -> ArchiveEntry {
    ArchiveEntry {
        name: name.to_vec(),
        payload: payload.to_vec(),
        compression: PharCompression::None,
        metadata: Vec::new(),
    }
}

/// Builds an unsigned executable tar phar with a `.phar/stub.php` entry.
fn executable_tar() -> Vec<u8> {
    build_tar_archive(
        &[
            entry(b"tar_004.php", b"<?php var_dump(__FILE__);"),
            entry(b"internal/file/here", b"hi there!\n"),
        ],
        &[],
        b"__HALT_COMPILER();",
    )
    .expect("tar fixture")
}

/// Builds an unsigned executable zip phar with a `.phar/stub.php` entry.
fn executable_zip() -> Vec<u8> {
    build_zip_archive(
        &[
            entry(b"zip_001.php", b"<?php var_dump(__FILE__);"),
            entry(b"internal/file/here", b"hi there!\n"),
        ],
        &[],
        b"__HALT_COMPILER();",
    )
    .expect("zip fixture")
}

/// Builds an unsigned tar data archive without a `.phar/stub.php` entry.
fn data_tar() -> Vec<u8> {
    build_tar_archive(&[entry(b"doc.txt", b"plain data tar\n")], &[], b"")
        .expect("tar data fixture")
}

/// Mirrors `ext/phar/tests/tar/require_hash.phpt`: an unsigned executable tar
/// phar is rejected while `phar.require_hash=1`, then MD5 signing makes it
/// readable.
#[test]
pub(super) fn php_src_tar_require_hash_rejects_unsigned_executable_phar() {
    let path = temp_path("tar_require_hash", "tar");
    std::fs::write(&path, executable_tar()).unwrap();
    let path_bytes = path.to_string_lossy();
    assert!(
        parse_archive_path(&path).is_none(),
        "tar phar without a signature must be rejected by default"
    );

    {
        let _policy = RequireHashGuard::relaxed();
        assert!(
            parse_archive_path(&path).is_some(),
            "phar.require_hash=0 must accept the unsigned tar phar"
        );
        assert_eq!(sign_archive_hash(path_bytes.as_bytes(), 1), Some(()));
    }

    let (flag, digest) = read_signature_info(path_bytes.as_bytes()).expect("MD5 signature");
    assert_eq!(flag, 1);
    assert_eq!(digest.len(), 16);
    assert_eq!(
        signature_type_name(path_bytes.as_bytes()).as_deref(),
        Some(&b"MD5"[..])
    );
    assert!(
        parse_archive_path(&path).is_some(),
        "signed tar phar must be accepted under the default policy"
    );
    remove_signed_fixture(&path);
}

/// The OOP validity probe honors the caller's data/executable distinction and the
/// require-hash policy: `PharData` accepts unsigned archives, `Phar` does not.
#[test]
pub(super) fn archive_validity_honors_is_data_and_require_hash() {
    let path = temp_path("archive_validity", "phar");
    std::fs::write(&path, build_native_phar(&[("a.txt", b"alpha")])).unwrap();
    let path_bytes = path.to_string_lossy();
    assert!(
        !archive_is_valid(path_bytes.as_bytes(), false),
        "an unsigned executable archive must fail the Phar probe"
    );
    assert!(
        archive_is_valid(path_bytes.as_bytes(), true),
        "PharData must accept the same unsigned archive"
    );
    {
        let _policy = RequireHashGuard::relaxed();
        assert!(
            archive_is_valid(path_bytes.as_bytes(), false),
            "the relaxed policy must accept the unsigned archive"
        );
        assert_eq!(sign_archive_hash(path_bytes.as_bytes(), 3), Some(()));
    }
    assert!(
        archive_is_valid(path_bytes.as_bytes(), false),
        "a signed archive must pass the Phar probe"
    );
    assert!(archive_is_valid(path_bytes.as_bytes(), true));
    remove_signed_fixture(&path);
}

/// Mirrors php-src's native rule: an unsigned executable native phar is
/// rejected while `phar.require_hash=1`, then MD5 signing makes it readable.
#[test]
pub(super) fn php_src_native_require_hash_rejects_unsigned_phar() {
    let path = temp_path("native_require_hash", "phar");
    std::fs::write(&path, build_native_phar(&[("a.txt", b"alpha")])).unwrap();
    let path_bytes = path.to_string_lossy();
    assert!(
        parse_archive_path(&path).is_none(),
        "native phar without a signature must be rejected by default"
    );

    {
        let _policy = RequireHashGuard::relaxed();
        assert!(
            parse_archive_path(&path).is_some(),
            "phar.require_hash=0 must accept the unsigned native phar"
        );
        assert_eq!(sign_archive_hash(path_bytes.as_bytes(), 1), Some(()));
    }

    assert!(
        parse_archive_path(&path).is_some(),
        "signed native phar must be accepted under the default policy"
    );
    remove_signed_fixture(&path);
}

/// Mirrors `ext/phar/tests/zip/require_hash.phpt`: an unsigned executable zip
/// phar is rejected while `phar.require_hash=1`, then MD5 signing makes it
/// readable.
#[test]
pub(super) fn php_src_zip_require_hash_rejects_unsigned_executable_phar() {
    let path = temp_path("zip_require_hash", "zip");
    std::fs::write(&path, executable_zip()).unwrap();
    let path_bytes = path.to_string_lossy();
    assert!(
        parse_archive_path(&path).is_none(),
        "zip phar without a signature must be rejected by default"
    );

    {
        let _policy = RequireHashGuard::relaxed();
        assert!(
            parse_archive_path(&path).is_some(),
            "phar.require_hash=0 must accept the unsigned zip phar"
        );
        assert_eq!(sign_archive_hash(path_bytes.as_bytes(), 1), Some(()));
    }

    let (flag, _) = read_signature_info(path_bytes.as_bytes()).expect("MD5 signature");
    assert_eq!(flag, 1);
    assert_eq!(
        signature_type_name(path_bytes.as_bytes()).as_deref(),
        Some(&b"MD5"[..])
    );
    assert!(parse_archive_path(&path).is_some());
    remove_signed_fixture(&path);
}

/// Mirrors `ext/phar/tests/tar/tar_nohash.phpt`: a plain tar data archive
/// without a stub is a `PharData` container, not an executable phar, so it
/// stays readable even with `phar.require_hash=1`.
#[test]
pub(super) fn php_src_tar_nohash_accepts_unsigned_data_archive() {
    let path = temp_path("tar_nohash", "tar");
    std::fs::write(&path, data_tar()).unwrap();
    assert!(
        parse_archive_path(&path).is_some(),
        "an unsigned tar data archive must remain readable"
    );
    let url = format!("phar://{}/doc.txt", path.display());
    assert_eq!(
        extract_url_bytes(url.as_bytes()).as_deref(),
        Some(&b"plain data tar\n"[..])
    );
    remove_signed_fixture(&path);
}

/// Mirrors the first `getSignature()` assertion in
/// `ext/phar/tests/phar_setsignaturealgo2.phpt` (and its tar/zip variants):
/// newly created executable phars default to SHA-256.
#[test]
pub(super) fn php_src_new_phars_default_to_sha256() {
    for extension in ["phar", "tar", "zip"] {
        let path = temp_path("default_sha256", extension);
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            set_stub_bytes(path_bytes.as_bytes(), b"<?php __HALT_COMPILER(); ?>"),
            Some(())
        );
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"file1.txt", b"hi"),
            Some(2)
        );
        let (flag, digest) = read_signature_info(path_bytes.as_bytes()).expect("default signature");
        assert_eq!(flag, 3, "{extension}: default signature must be SHA-256");
        assert_eq!(digest.len(), 32);
        assert_eq!(
            signature_type_name(path_bytes.as_bytes()).as_deref(),
            Some(&b"SHA-256"[..])
        );
        assert!(parse_archive_path(&path).is_some());
        remove_signed_fixture(&path);
    }
}

/// `PharData` containers stay unsigned unless a signature was requested explicitly.
#[test]
pub(super) fn php_src_data_containers_stay_unsigned_by_default() {
    for extension in ["tar", "zip"] {
        let path = temp_path("data_unsigned", extension);
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"file1.txt", b"hi"),
            Some(2)
        );
        assert_eq!(
            read_signature_info(path_bytes.as_bytes()),
            None,
            "{extension}: data archives must not be signed by default"
        );
        assert_eq!(
            signature_type_name(path_bytes.as_bytes()),
            None,
            "{extension}: data archives must not report a signature type"
        );
        assert!(parse_archive_path(&path).is_some());
        remove_signed_fixture(&path);
    }
}

/// Mirrors the hash-algorithm portion of `ext/phar/tests/phar_setsignaturealgo2.phpt`
/// and `test_signaturealgos.phpt`: every supported hash flag is written,
/// verified, and reported with PHP's `hash_type` spelling.
#[test]
pub(super) fn php_src_hash_signature_algorithms_round_trip() {
    for extension in ["phar", "tar", "zip"] {
        for (flag, name) in [
            (1, &b"MD5"[..]),
            (2, b"SHA-1"),
            (3, b"SHA-256"),
            (4, b"SHA-512"),
        ] {
            let path = temp_path("hash_algo", extension);
            let path_bytes = path.to_string_lossy();
            assert_eq!(
                put_entry_bytes(path_bytes.as_bytes(), b"file1.txt", b"hi"),
                Some(2)
            );
            assert_eq!(sign_archive_hash(path_bytes.as_bytes(), flag), Some(()));
            let (actual, digest) =
                read_signature_info(path_bytes.as_bytes()).expect("hash signature");
            assert_eq!(actual, flag, "{extension}: flag {flag} must round-trip");
            assert_eq!(
                digest.len(),
                signature_digest_len(flag).expect("hash digest length")
            );
            assert_eq!(
                signature_type_name(path_bytes.as_bytes()).as_deref(),
                Some(name),
                "{extension}: flag {flag} type name"
            );
            assert!(
                parse_archive_path(&path).is_some(),
                "{extension}: flag {flag} must verify"
            );
            remove_signed_fixture(&path);
        }
    }
}

/// Mirrors the `OPENSSL_SHA256`/`OPENSSL_SHA512` entries of `test_signaturealgos.phpt`:
/// native, tar, and zip signatures verify against the sidecar under the default
/// `phar.require_hash=1` policy and report PHP's matching hash type.
#[test]
pub(super) fn php_src_openssl_sha256_and_sha512_signatures_round_trip() {
    for extension in ["phar", "tar", "zip"] {
        for (flag, name) in [
            (PHAR_OPENSSL_SHA256_SIGNATURE_TYPE, &b"OpenSSL_SHA256"[..]),
            (PHAR_OPENSSL_SHA512_SIGNATURE_TYPE, &b"OpenSSL_SHA512"[..]),
        ] {
            let path = temp_path("openssl_sha2", extension);
            let path_bytes = path.to_string_lossy();
            assert_eq!(
                put_entry_bytes(path_bytes.as_bytes(), b"file1.txt", b"hi"),
                Some(2)
            );
            assert_eq!(
                sign_archive_openssl_with_algorithm(
                    path_bytes.as_bytes(),
                    TEST_RSA_KEY_PEM.as_bytes(),
                    flag
                ),
                Some(())
            );
            write_test_public_key(&path);
            let (read_flag, _) =
                read_signature_info(path_bytes.as_bytes()).expect("OpenSSL SHA-2 signature");
            assert_eq!(read_flag, flag);
            assert_eq!(
                signature_type_name(path_bytes.as_bytes()).as_deref(),
                Some(name),
                "{extension}: OpenSSL SHA-2 type name"
            );
            assert!(
                parse_archive_path(&path).is_some(),
                "{extension}: OpenSSL SHA-2 signature must verify under require_hash=1"
            );
            remove_signed_fixture(&path);
        }
    }
}

/// Mirrors the OpenSSL entries of `test_signaturealgos.phpt` and
/// `ext/phar/tests/tar/tar_openssl_hash.phpt` for the supported RSA-SHA1 form:
/// native, tar, and zip signatures verify against the sidecar under the default
/// `phar.require_hash=1` policy and report PHP's `OpenSSL` hash type.
#[test]
pub(super) fn php_src_openssl_sha1_signatures_round_trip() {
    for extension in ["phar", "tar", "zip"] {
        let path = temp_path("openssl_sha1", extension);
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"file1.txt", b"hi"),
            Some(2)
        );
        assert_eq!(
            sign_archive_openssl(path_bytes.as_bytes(), TEST_RSA_KEY_PEM.as_bytes()),
            Some(())
        );
        write_test_public_key(&path);
        let (flag, _) = read_signature_info(path_bytes.as_bytes()).expect("OpenSSL signature");
        assert_eq!(flag, PHAR_OPENSSL_SIGNATURE_TYPE);
        assert_eq!(
            signature_type_name(path_bytes.as_bytes()).as_deref(),
            Some(&b"OpenSSL"[..]),
            "{extension}: OpenSSL type name"
        );
        assert!(
            parse_archive_path(&path).is_some(),
            "{extension}: OpenSSL signature must verify under require_hash=1"
        );
        remove_signed_fixture(&path);
    }
}
