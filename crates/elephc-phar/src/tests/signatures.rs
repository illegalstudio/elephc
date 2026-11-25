//! Purpose:
//! Tests hash and OpenSSL signatures for native, tar, and ZIP PHAR archives.
//!
//! Called from:
//! - `cargo test -p elephc-phar` through Rust's test harness.
//!
//! Key details:
//! - Tests verify both signature metadata and the exact signed byte ranges.

use super::*;

pub(super) const TEST_RSA_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIICdgIBADANBgkqhkiG9w0BAQEFAASCAmAwggJcAgEAAoGBAOuAP7xZaVfhwn9l\n\
BaMgxKPU1ODBpuT7Ybu6Fav03TJp1BKc1wUMiXnUPraUUI2R2JxoattDe7R/LcGk\n\
jVoPiBGGPoxxTaByd5LJZJk6MJAiGBhzQT7bkK3OMDHLQqhziefqDFfnDLt/TN7+\n\
umuMCPtLmuF6UUXiebMzyH21x7jvAgMBAAECgYBBhL+2rgVxzrxm5vsnhEFQ9zB2\n\
i0ncYNey+7V1zr0PfoPi3cGwhOlmfJcqAp9ak534/c/kyqSK9esL+bTdvn5zIQqC\n\
Swt2znffaW9nC6lM/pkZcvGLETt2m0L71n6pZVkMewsGBm9YrBQFA1krC7BV674U\n\
mlOmmYpM3LPgzmRLwQJBAPm/G7O4Stmzu5xV5qtvYX1dNZ2gydkVyfK/AwCYpfbK\n\
8ZXntKeWCt1BER1hNBSMPacHKb0LotK3j3LNNteLHCECQQDxZdNsXNLTHylWKA/X\n\
dyM3SH9mM6ESZP07cU7Ifq6t9zJdTfGdiyxsAjaaXxDmShL+bAjU16iwaTAGcYTB\n\
NrMPAkEAoUGwVV7Nlbvji5I7mr4UKKoikGDdc/oJp1+GRMBLiQqI6s3ta7gJ08rL\n\
jjjRM+NJe6u4W4RD4eL8EJhIrOv5gQJAK4Tm+8c0PtmEU0L/sCGLWMEaLquqIy3P\n\
tXK0+FJWXYiOLOILaBKaHJK9k1EGM+4wxGtnoC+M+tjLzq2SeF7LIwJAPdLUn2Qq\n\
eGMK12chOVcx41RxYctqsOlEKCIt011yGsV2/Mdm9ljTXeyXvNXCVOVcnHaf1v5w\n\
rNiobfy8sSb6iw==\n\
-----END PRIVATE KEY-----\n";

/// Derives the public half of the static test RSA key.
fn test_rsa_public_key() -> rsa::RsaPublicKey {
    use rsa::pkcs8::DecodePrivateKey;

    let private = rsa::RsaPrivateKey::from_pkcs8_pem(TEST_RSA_KEY_PEM).unwrap();
    rsa::RsaPublicKey::from(&private)
}

/// Writes the static test public key to PHP's `<archive>.pubkey` sidecar path.
pub(super) fn write_test_public_key(path: &std::path::Path) {
    use rsa::pkcs8::{EncodePublicKey, LineEnding};

    let pem = test_rsa_public_key()
        .to_public_key_pem(LineEnding::LF)
        .unwrap();
    std::fs::write(archive_public_key_path(path), pem).unwrap();
}

/// Removes an archive fixture and its optional public-key sidecar.
pub(super) fn remove_signed_fixture(path: &std::path::Path) {
    std::fs::remove_file(path).ok();
    std::fs::remove_file(archive_public_key_path(path)).ok();
}

/// OpenSSL signing replaces the native PHAR's SHA1 trailer with an RSA-SHA1
/// signature trailer, the signature is deterministic and verifies against the
/// derived public key, and the signature metadata reads back as OpenSSL.
#[test]
pub(super) fn native_phar_openssl_signature_round_trip() {
    use rsa::pkcs8::DecodePrivateKey;
    use rsa::{Pkcs1v15Sign, RsaPrivateKey, RsaPublicKey};
    use sha1::{Digest, Sha1};

    let path = std::env::temp_dir().join(format!("elephc_phar_sig_{}.phar", std::process::id()));
    let pb = path.to_string_lossy();
    assert_eq!(put_entry_bytes(pb.as_bytes(), b"a.txt", b"alpha"), Some(5));
    assert_eq!(
        sign_archive_openssl(pb.as_bytes(), TEST_RSA_KEY_PEM.as_bytes()),
        Some(())
    );

    let signed = std::fs::read(&path).unwrap();
    assert!(
        parse_archive(&signed).is_none(),
        "byte-only parsing must reject an OpenSSL signature without a key"
    );
    assert!(
        read_signature_info(pb.as_bytes()).is_none(),
        "path reads must reject an OpenSSL signature without its sidecar key"
    );
    write_test_public_key(&path);
    let n = signed.len();
    assert_eq!(&signed[n - 4..], b"GBMB");
    assert_eq!(
        u32::from_le_bytes(signed[n - 8..n - 4].try_into().unwrap()),
        PHAR_OPENSSL_SIGNATURE_TYPE
    );

    // The signature reads back as OpenSSL with a 1024-bit (128-byte) RSA signature.
    let (flags, sig) = read_signature_info(pb.as_bytes()).unwrap();
    assert_eq!(flags, PHAR_OPENSSL_SIGNATURE_TYPE);
    assert_eq!(sig.len(), 128, "1024-bit RSA signature is 128 bytes");
    assert_eq!(
        u32::from_le_bytes(signed[n - 12..n - 8].try_into().unwrap()) as usize,
        sig.len()
    );
    assert_eq!(signature_type_name(pb.as_bytes()).as_deref(), Some(&b"OpenSSL"[..]));

    // Re-signing is deterministic (PKCS#1 v1.5).
    assert_eq!(
        sign_archive_openssl(pb.as_bytes(), TEST_RSA_KEY_PEM.as_bytes()),
        Some(())
    );
    let (_, sig2) = read_signature_info(pb.as_bytes()).unwrap();
    assert_eq!(sig, sig2, "PKCS#1 v1.5 signature is deterministic");

    // The signature verifies against the public key over the signed data.
    let key = RsaPrivateKey::from_pkcs8_pem(TEST_RSA_KEY_PEM).unwrap();
    let pubkey = RsaPublicKey::from(&key);
    let data = strip_signature_trailer(&std::fs::read(&path).unwrap()).to_vec();
    let hashed = Sha1::digest(&data);
    pubkey
        .verify(Pkcs1v15Sign::new::<Sha1>(), &hashed, &sig)
        .expect("signature verifies");
    remove_signed_fixture(&path);
}

/// Hash-based signing rewrites the native PHAR trailer with the requested digest
/// algorithm, readable back via the signature metadata.
#[test]
pub(super) fn native_phar_hash_signature_round_trip() {
    let path = std::env::temp_dir().join(format!("elephc_phar_hsig_{}.phar", std::process::id()));
    let pb = path.to_string_lossy();
    assert_eq!(put_entry_bytes(pb.as_bytes(), b"a.txt", b"alpha"), Some(5));
    // SHA-256 (algo 3): 32-byte digest, type "SHA-256".
    assert_eq!(sign_archive_hash(pb.as_bytes(), 3), Some(()));
    let (flags, digest) = read_signature_info(pb.as_bytes()).unwrap();
    assert_eq!(flags, 3);
    assert_eq!(digest.len(), 32);
    assert_eq!(signature_type_name(pb.as_bytes()).as_deref(), Some(&b"SHA-256"[..]));
    std::fs::remove_file(&path).ok();
}

/// Verifies a signed native PHAR is rejected when its entry payload is
/// modified without recomputing the recorded SHA-256 signature.
#[test]
pub(super) fn native_phar_rejects_tampered_signed_payload() {
    let path = std::env::temp_dir().join(format!(
        "elephc_phar_tampered_signature_{}.phar",
        std::process::id()
    ));
    let pb = path.to_string_lossy();
    assert_eq!(put_entry_bytes(pb.as_bytes(), b"a.txt", b"alpha"), Some(5));
    assert_eq!(sign_archive_hash(pb.as_bytes(), 3), Some(()));

    let mut data = std::fs::read(&path).unwrap();
    let payload = data
        .windows(b"alpha".len())
        .position(|window| window == b"alpha")
        .expect("signed fixture contains its payload");
    data[payload] ^= 0x01;

    assert!(
        parse_archive(&data).is_none(),
        "a payload whose signature no longer verifies must be rejected"
    );
    std::fs::remove_file(&path).ok();
}

/// Verifies signed tar and ZIP PHARs authenticate their payload while being
/// opened, rather than merely exposing unverified signature metadata.
#[test]
pub(super) fn tar_and_zip_phars_reject_tampered_signed_payloads() {
    const PAYLOAD: &[u8] = b"authenticated archive payload";
    for extension in ["tar", "zip"] {
        let path = std::env::temp_dir().join(format!(
            "elephc-phar-tamper-{}-{extension}.{extension}",
            std::process::id()
        ));
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"payload.txt", PAYLOAD),
            Some(PAYLOAD.len())
        );
        assert_eq!(sign_archive_hash(path_bytes.as_bytes(), 3), Some(()));

        let mut archive = std::fs::read(&path).expect("read signed PHAR fixture");
        let offset = archive
            .windows(PAYLOAD.len())
            .position(|window| window == PAYLOAD)
            .expect("locate stored payload bytes");
        archive[offset] ^= 0x01;
        assert!(
            parse_archive(&archive).is_none(),
            "tampered signed {extension} PHAR must fail authentication"
        );
        std::fs::remove_file(&path).ok();
    }
}

/// Verifies a tar PHAR cannot authenticate a valid prefix and then expose an
/// unsigned entry appended after its signature control record.
#[test]
pub(super) fn tar_phar_rejects_entries_appended_after_signature() {
    let path = std::env::temp_dir().join(format!(
        "elephc-phar-appended-after-signature-{}.tar",
        std::process::id()
    ));
    let path_bytes = path.to_string_lossy();
    assert_eq!(
        put_entry_bytes(path_bytes.as_bytes(), b"signed.txt", b"signed"),
        Some(6)
    );
    assert_eq!(sign_archive_hash(path_bytes.as_bytes(), 3), Some(()));

    let mut archive = std::fs::read(&path).unwrap();
    archive.truncate(archive.len() - 1024);
    write_tar_entry(&mut archive, b"unsigned.txt", b"appended").unwrap();
    archive.extend_from_slice(&[0u8; 1024]);

    assert!(
        parse_archive(&archive).is_none(),
        "entries after the authenticated tar prefix must be rejected"
    );
    std::fs::remove_file(path).ok();
}

/// Reconstructs the byte range a tar/zip phar signature is computed over from a
/// parsed archive: the tar data records, or the zip locals + central + comment.
pub(super) fn tar_zip_signed_range(arch: &Archive) -> Vec<u8> {
    match arch.format {
        ArchiveFormat::Tar => {
            let mut body = Vec::new();
            write_tar_body(&mut body, &arch.entries, &arch.metadata, &arch.stub).unwrap();
            body
        }
        ArchiveFormat::Zip => {
            let mut out = Vec::new();
            let mut central = Vec::new();
            write_zip_body(&mut out, &mut central, &arch.entries, &arch.stub).unwrap();
            out.extend_from_slice(&central);
            out.extend_from_slice(&arch.metadata);
            out
        }
        ArchiveFormat::NativePhar => unreachable!("native phars sign with a trailer"),
    }
}

/// Hash signing a tar/zip phar writes a hidden `.phar/signature.bin` entry
/// (`LE32(flag) ++ LE32(len) ++ digest`) computed over the signed range, leaving
/// real entries readable and reporting the right algorithm.
pub(super) fn check_tar_zip_hash_signature(ext: &str) {
    let path =
        std::env::temp_dir().join(format!("elephc_phar_sig_{}_{ext}.{ext}", std::process::id()));
    let pb = path.to_string_lossy();
    assert_eq!(
        put_entry_bytes(pb.as_bytes(), b"doc.txt", b"bundled document\n"),
        Some(17)
    );
    // SHA-256 (algo 3).
    assert_eq!(sign_archive_hash(pb.as_bytes(), 3), Some(()));
    let data = std::fs::read(&path).unwrap();
    // The signature entry is hidden; the real entry still reads back.
    assert_eq!(
        extract_entry_bytes(&data, b"doc.txt").as_deref(),
        Some(&b"bundled document\n"[..])
    );
    let (flag, digest) = read_signature_info(pb.as_bytes()).unwrap();
    assert_eq!(flag, 3);
    assert_eq!(digest.len(), 32);
    assert_eq!(signature_type_name(pb.as_bytes()).as_deref(), Some(&b"SHA-256"[..]));
    // The digest covers exactly the reconstructed signed range.
    let arch = parse_archive(&data).unwrap();
    assert_eq!(digest, compute_signature(3, None, &tar_zip_signed_range(&arch)).unwrap());
    std::fs::remove_file(&path).ok();
}

/// SHA-256 signing a tar phar round-trips through `.phar/signature.bin`.
#[test]
pub(super) fn tar_phar_hash_signature_round_trip() {
    check_tar_zip_hash_signature("tar");
}

/// SHA-256 signing a zip phar round-trips through `.phar/signature.bin`.
#[test]
pub(super) fn zip_phar_hash_signature_round_trip() {
    check_tar_zip_hash_signature("zip");
}

/// CVE-2026-128090: an adjacent `<archive>.pubkey` means OpenSSL verification
/// is expected, so an archive that declares a keyless hash signature must be
/// rejected instead of trusted. Without the sidecar the hash signature keeps
/// PHP's existing behaviour.
#[test]
pub(super) fn hash_signed_phars_reject_pubkey_sidecar_downgrade() {
    const PAYLOAD: &[u8] = b"downgrade-guard payload";
    for extension in ["phar", "tar", "zip"] {
        let path = std::env::temp_dir().join(format!(
            "elephc-phar-sidecar-downgrade-{}-{extension}.{extension}",
            std::process::id()
        ));
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"payload.txt", PAYLOAD),
            Some(PAYLOAD.len())
        );
        assert_eq!(sign_archive_hash(path_bytes.as_bytes(), 3), Some(()));
        let url = format!("phar://{}/payload.txt", path.display());

        // No sidecar: the archive-declared SHA-256 digest is the trust decision.
        assert!(
            parse_archive_path(&path).is_some(),
            "{extension}: hash signature must remain usable without a sidecar"
        );
        assert_eq!(
            extract_url_bytes(url.as_bytes()).as_deref(),
            Some(PAYLOAD),
            "{extension}: entry must remain readable without a sidecar"
        );
        assert!(
            read_signature_info(path_bytes.as_bytes()).is_some(),
            "{extension}: signature metadata must remain readable without a sidecar"
        );

        // Trusted channel installs the public key: now only OpenSSL is accepted.
        write_test_public_key(&path);
        assert!(
            parse_archive_path(&path).is_none(),
            "{extension}: a hash signature must be rejected when <archive>.pubkey exists"
        );
        assert!(
            extract_url_bytes(url.as_bytes()).is_none(),
            "{extension}: entry extraction must reject the downgraded signature"
        );
        assert!(
            read_signature_info(path_bytes.as_bytes()).is_none(),
            "{extension}: signature metadata must reject the downgraded signature"
        );

        remove_signed_fixture(&path);
    }
}

/// OpenSSL signing a tar/zip phar writes an RSA-SHA1 `.phar/signature.bin` that
/// verifies against the derived public key over the archive's signed range.
pub(super) fn check_tar_zip_openssl_signature(ext: &str) {
    use rsa::pkcs8::DecodePrivateKey;
    use rsa::{Pkcs1v15Sign, RsaPrivateKey, RsaPublicKey};
    use sha1::{Digest, Sha1};

    let path =
        std::env::temp_dir().join(format!("elephc_phar_osig_{}_{ext}.{ext}", std::process::id()));
    let pb = path.to_string_lossy();
    assert_eq!(
        put_entry_bytes(pb.as_bytes(), b"doc.txt", b"bundled document\n"),
        Some(17)
    );
    assert_eq!(
        sign_archive_openssl(pb.as_bytes(), TEST_RSA_KEY_PEM.as_bytes()),
        Some(())
    );
    let data = std::fs::read(&path).unwrap();
    assert!(
        parse_archive(&data).is_none(),
        "byte-only parsing must reject an OpenSSL signature without a key"
    );
    assert!(
        read_signature_info(pb.as_bytes()).is_none(),
        "path reads must reject an OpenSSL signature without its sidecar key"
    );
    write_test_public_key(&path);
    let (flag, sig) = read_signature_info(pb.as_bytes()).unwrap();
    assert_eq!(flag, PHAR_OPENSSL_SIGNATURE_TYPE);
    assert_eq!(sig.len(), 128, "1024-bit RSA signature is 128 bytes");
    assert_eq!(signature_type_name(pb.as_bytes()).as_deref(), Some(&b"OpenSSL"[..]));
    // The signature verifies against the public key over the signed range.
    let arch = parse_archive_with_public_key(&data, Some(&test_rsa_public_key()), true, true).unwrap();
    let key = RsaPrivateKey::from_pkcs8_pem(TEST_RSA_KEY_PEM).unwrap();
    let pubkey = RsaPublicKey::from(&key);
    let hashed = Sha1::digest(tar_zip_signed_range(&arch));
    pubkey
        .verify(Pkcs1v15Sign::new::<Sha1>(), &hashed, &sig)
        .expect("tar/zip OpenSSL signature verifies");
    remove_signed_fixture(&path);
}

/// OpenSSL signing a tar phar verifies against the derived public key.
#[test]
pub(super) fn tar_phar_openssl_signature_round_trip() {
    check_tar_zip_openssl_signature("tar");
}

/// OpenSSL signing a zip phar verifies against the derived public key.
#[test]
pub(super) fn zip_phar_openssl_signature_round_trip() {
    check_tar_zip_openssl_signature("zip");
}

/// RSA-SHA256 and RSA-SHA512 signatures round-trip for native, tar, and ZIP phars.
#[test]
pub(super) fn openssl_sha256_and_sha512_signatures_round_trip_all_families() {
    use rsa::Pkcs1v15Sign;
    use sha2::{Digest, Sha256, Sha512};

    let pubkey = test_rsa_public_key();
    for ext in ["phar", "tar", "zip"] {
        for (flag, name) in [
            (PHAR_OPENSSL_SHA256_SIGNATURE_TYPE, &b"OpenSSL_SHA256"[..]),
            (PHAR_OPENSSL_SHA512_SIGNATURE_TYPE, &b"OpenSSL_SHA512"[..]),
        ] {
            let path = std::env::temp_dir().join(format!(
                "elephc_phar_osig_algo_{}_{ext}_{flag}.{ext}",
                std::process::id()
            ));
            let pb = path.to_string_lossy();
            assert_eq!(
                put_entry_bytes(pb.as_bytes(), b"doc.txt", b"bundled document\n"),
                Some(17)
            );
            assert_eq!(
                sign_archive_openssl_with_algorithm(
                    pb.as_bytes(),
                    TEST_RSA_KEY_PEM.as_bytes(),
                    flag
                ),
                Some(())
            );
            let data = std::fs::read(&path).unwrap();
            write_test_public_key(&path);
            let (read_flag, sig) = read_signature_info(pb.as_bytes()).unwrap();
            assert_eq!(read_flag, flag);
            assert_eq!(sig.len(), 128, "1024-bit RSA signature is 128 bytes");
            assert_eq!(signature_type_name(pb.as_bytes()).as_deref(), Some(name));
            let signed_range = if ext == "phar" {
                strip_signature_trailer(&data).to_vec()
            } else {
                let archive =
                    parse_archive_with_public_key(&data, Some(&pubkey), true, true).unwrap();
                tar_zip_signed_range(&archive)
            };
            match flag {
                PHAR_OPENSSL_SHA256_SIGNATURE_TYPE => {
                    let hashed = Sha256::digest(&signed_range);
                    pubkey
                        .verify(Pkcs1v15Sign::new::<Sha256>(), &hashed, &sig)
                        .expect("RSA-SHA256 signature verifies");
                }
                _ => {
                    let hashed = Sha512::digest(&signed_range);
                    pubkey
                        .verify(Pkcs1v15Sign::new::<Sha512>(), &hashed, &sig)
                        .expect("RSA-SHA512 signature verifies");
                }
            }
            remove_signed_fixture(&path);
        }
    }
}

/// Verifies native, tar, and ZIP OpenSSL signatures fail closed after signed
/// payload bytes are modified, even when the expected public key is present.
#[test]
pub(super) fn openssl_signed_phars_reject_tampered_payloads() {
    const PAYLOAD: &[u8] = b"authenticated OpenSSL payload";
    for extension in ["phar", "tar", "zip"] {
        let path = std::env::temp_dir().join(format!(
            "elephc-phar-openssl-tamper-{}-{extension}.{extension}",
            std::process::id()
        ));
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"payload.txt", PAYLOAD),
            Some(PAYLOAD.len())
        );
        assert_eq!(
            sign_archive_openssl(path_bytes.as_bytes(), TEST_RSA_KEY_PEM.as_bytes()),
            Some(())
        );
        write_test_public_key(&path);

        let url = format!("phar://{}/payload.txt", path.display());
        assert_eq!(
            extract_url_bytes(url.as_bytes()).as_deref(),
            Some(PAYLOAD),
            "valid signed {extension} fixture must authenticate"
        );

        let mut archive = std::fs::read(&path).unwrap();
        let offset = archive
            .windows(PAYLOAD.len())
            .position(|window| window == PAYLOAD)
            .expect("locate signed payload bytes");
        archive[offset] ^= 0x01;
        std::fs::write(&path, archive).unwrap();

        assert!(
            extract_url_bytes(url.as_bytes()).is_none(),
            "tampered OpenSSL-signed {extension} PHAR must not expose payloads"
        );
        assert!(
            read_signature_info(path_bytes.as_bytes()).is_none(),
            "tampered OpenSSL-signed {extension} PHAR must not expose signature metadata"
        );
        remove_signed_fixture(&path);
    }
}

/// Removes a native PHAR's signature trailer and clears the manifest signature
/// flag, reproducing an archive whose signature was stripped entirely.
fn strip_native_signature(data: &[u8]) -> Vec<u8> {
    let halt = b"__HALT_COMPILER();";
    let halt_idx = find_subslice(data, halt).expect("native phar stub");
    let mut manifest_start = halt_idx + halt.len();
    for &ch in &[b' ', b'?', b'>', b'\r', b'\n'] {
        if data.get(manifest_start) == Some(&ch) {
            manifest_start += 1;
        }
    }
    let mut out = strip_signature_trailer(data).to_vec();
    let flags = le32(&out, manifest_start + 10).expect("global flags");
    let cleared = flags & !PHAR_HDR_SIGNATURE;
    out[manifest_start + 10..manifest_start + 14].copy_from_slice(&cleared.to_le_bytes());
    out
}

/// Rebuilds a tar PHAR without its `.phar/signature.bin` entry.
fn unsigned_tar(data: &[u8]) -> Vec<u8> {
    let archive = parse_archive(data).expect("parse signed tar phar");
    build_tar_archive(&archive.entries, &archive.metadata, &archive.stub)
        .expect("rebuild unsigned tar phar")
}

/// Rebuilds a ZIP PHAR without its `.phar/signature.bin` entry, including the
/// end-of-central-directory record so the result is a readable archive.
fn unsigned_zip(data: &[u8]) -> Vec<u8> {
    let archive = parse_archive(data).expect("parse signed zip phar");
    build_zip_archive(&archive.entries, &archive.metadata, &archive.stub)
        .expect("rebuild unsigned zip phar")
}

/// A trusted `<archive>.pubkey` sidecar requires an OpenSSL signature to be
/// present: stripping it from a native, tar or ZIP archive must not turn the
/// archive into an accepted unsigned one.
#[test]
pub(super) fn stripped_signature_with_pubkey_sidecar_is_rejected() {
    let _policy = RequireHashGuard::relaxed();
    for extension in ["phar", "tar", "zip"] {
        let path = std::env::temp_dir().join(format!(
            "elephc-phar-stripped-signature-{}-{extension}.{extension}",
            std::process::id()
        ));
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"payload.txt", b"stripped payload"),
            Some(16)
        );
        let signed = std::fs::read(&path).unwrap();
        let unsigned = match extension {
            "phar" => strip_native_signature(&signed),
            "tar" => unsigned_tar(&signed),
            "zip" => unsigned_zip(&signed),
            _ => unreachable!(),
        };
        std::fs::write(&path, unsigned).unwrap();
        assert!(
            parse_archive_path(&path).is_some(),
            "{extension}: the unsigned fixture must be a valid readable archive before adding the sidecar"
        );
        write_test_public_key(&path);

        assert!(
            parse_archive_path(&path).is_none(),
            "{extension}: an unsigned archive must be rejected when <archive>.pubkey exists"
        );
        let url = format!("phar://{}/payload.txt", path.display());
        assert!(
            extract_url_bytes(url.as_bytes()).is_none(),
            "{extension}: entry extraction must reject the stripped signature"
        );
        assert!(
            read_signature_info(path_bytes.as_bytes()).is_none(),
            "{extension}: signature metadata must reject the stripped signature"
        );
        remove_signed_fixture(&path);
    }
}

/// Implicit archive edits (entry add/delete, compression, metadata, stub) would
/// replace an OpenSSL signature with the writer's SHA1 trailer while leaving the
/// `<archive>.pubkey` sidecar in place. They must fail before touching the file.
#[test]
pub(super) fn implicit_edits_of_openssl_signed_archives_are_rejected() {
    for extension in ["phar", "tar", "zip"] {
        let path = std::env::temp_dir().join(format!(
            "elephc-phar-implicit-edit-{}-{extension}.{extension}",
            std::process::id()
        ));
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"a.txt", b"alpha"),
            Some(5)
        );
        assert_eq!(
            sign_archive_openssl(path_bytes.as_bytes(), TEST_RSA_KEY_PEM.as_bytes()),
            Some(())
        );
        write_test_public_key(&path);
        let before = std::fs::read(&path).unwrap();

        let add_url = format!("phar://{}/b.txt", path.display());
        let delete_url = format!("phar://{}/a.txt", path.display());
        assert!(
            put_entry_bytes(path_bytes.as_bytes(), b"b.txt", b"beta").is_none(),
            "{extension}: adding an entry must not silently downgrade the signature"
        );
        assert!(
            put_url_bytes(add_url.as_bytes(), b"beta").is_none(),
            "{extension}: adding an entry through a URL must be rejected"
        );
        assert!(
            delete_entry_bytes(path_bytes.as_bytes(), b"a.txt").is_none(),
            "{extension}: deleting an entry must not silently downgrade the signature"
        );
        assert!(
            delete_url_bytes(delete_url.as_bytes()).is_none(),
            "{extension}: deleting an entry through a URL must be rejected"
        );
        assert!(
            set_archive_compression(path_bytes.as_bytes(), 4096).is_none(),
            "{extension}: changing entry compression must be rejected"
        );
        assert!(
            set_metadata_bytes(path_bytes.as_bytes(), b"meta").is_none(),
            "{extension}: changing global metadata must be rejected"
        );
        assert!(
            set_stub_bytes(path_bytes.as_bytes(), b"<?php __HALT_COMPILER();").is_none(),
            "{extension}: changing the stub must be rejected"
        );
        assert!(
            set_file_metadata_bytes(path_bytes.as_bytes(), b"a.txt", b"meta").is_none(),
            "{extension}: changing per-file metadata must be rejected"
        );

        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "{extension}: rejected edits must leave the archive byte-identical"
        );
        remove_signed_fixture(&path);
    }
}

/// Editing a hash-signed archive must preserve its declared digest algorithm.
/// Before this, native rebuilds always wrote SHA1 and tar/zip rebuilds wrote no
/// signature at all, silently weakening SHA-256/512 archives.
#[test]
pub(super) fn implicit_edits_preserve_a_hash_signature() {
    for extension in ["phar", "tar", "zip"] {
        let path = std::env::temp_dir().join(format!(
            "elephc-phar-preserve-signature-{}-{extension}.{extension}",
            std::process::id()
        ));
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"a.txt", b"alpha"),
            Some(5)
        );
        assert_eq!(sign_archive_hash(path_bytes.as_bytes(), 3), Some(()));

        // Add an entry: the implicit rebuild must re-sign with SHA-256.
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"b.txt", b"beta"),
            Some(4)
        );
        let (flag, digest) =
            read_signature_info(path_bytes.as_bytes()).expect("signature survives the edit");
        assert_eq!(flag, 3, "{extension}: SHA-256 must not be downgraded");
        assert_eq!(digest.len(), 32);
        assert_eq!(
            signature_type_name(path_bytes.as_bytes()).as_deref(),
            Some(&b"SHA-256"[..])
        );
        assert!(
            parse_archive_path(&path).is_some(),
            "{extension}: edited archive must still authenticate"
        );

        // Delete an entry: still SHA-256.
        assert_eq!(delete_entry_bytes(path_bytes.as_bytes(), b"a.txt"), Some(()));
        let (flag, _) =
            read_signature_info(path_bytes.as_bytes()).expect("signature survives deletion");
        assert_eq!(flag, 3, "{extension}: deletion must not downgrade SHA-256");
        remove_signed_fixture(&path);
    }
}

/// Editing an unsigned archive must leave it unsigned; an implicit rebuild may
/// not invent a SHA1 signature that the source never had.
#[test]
pub(super) fn implicit_edits_keep_unsigned_archives_unsigned() {
    let _policy = RequireHashGuard::relaxed();
    for extension in ["phar", "tar", "zip"] {
        let path = std::env::temp_dir().join(format!(
            "elephc-phar-keep-unsigned-{}-{extension}.{extension}",
            std::process::id()
        ));
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"a.txt", b"alpha"),
            Some(5)
        );
        if extension == "phar" {
            // New native phars start SHA-256-signed; strip it to exercise the unsigned path.
            let signed = std::fs::read(&path).unwrap();
            std::fs::write(&path, strip_native_signature(&signed)).unwrap();
        } else {
            // New tar/zip phars start SHA-256-signed; rebuild them without a signature.
            let signed = std::fs::read(&path).unwrap();
            let unsigned = if extension == "tar" {
                unsigned_tar(&signed)
            } else {
                unsigned_zip(&signed)
            };
            std::fs::write(&path, unsigned).unwrap();
        }
        assert!(
            read_signature_info(path_bytes.as_bytes()).is_none(),
            "{extension}: fixture must start unsigned"
        );
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"b.txt", b"beta"),
            Some(4)
        );
        assert!(
            read_signature_info(path_bytes.as_bytes()).is_none(),
            "{extension}: an implicit edit must not add a signature"
        );
        assert!(
            parse_archive_path(&path).is_some(),
            "{extension}: unsigned archive stays readable"
        );
        remove_signed_fixture(&path);
    }
}
