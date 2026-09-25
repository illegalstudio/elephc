//! Purpose:
//! Tests ZIP descriptors, ZIP64, ZipCrypto, and encrypted signature interoperability.
//!
//! Called from:
//! - `cargo test -p elephc-phar` through Rust's test harness.
//!
//! Key details:
//! - Specialized fixtures exercise every extended ZIP decoding and writing path.

use super::*;

/// Builds a single-entry ZIP whose local header uses a streaming data
/// descriptor (general-purpose flag bit 3): the local CRC/size fields are
/// zero, the real values live in a trailing data descriptor, and the central
/// directory carries the authoritative sizes.
pub(super) fn build_zip_with_data_descriptor(name: &str, content: &[u8], deflate: bool) -> Vec<u8> {
    let stored = if deflate {
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(content).unwrap();
        encoder.finish().unwrap()
    } else {
        content.to_vec()
    };
    let method = if deflate { ZIP_METHOD_DEFLATE } else { ZIP_METHOD_STORE };
    let crc = crc32(content);
    let comp = stored.len() as u32;
    let uncomp = content.len() as u32;
    let mut out = Vec::new();
    // -- local file header: zeroed sizes, data-descriptor flag set --
    out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes());
    out.extend_from_slice(&0x0008u16.to_le_bytes());
    out.extend_from_slice(&method.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(&stored);
    // -- trailing data descriptor carrying the real crc/sizes --
    out.extend_from_slice(&0x0807_4b50u32.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&comp.to_le_bytes());
    out.extend_from_slice(&uncomp.to_le_bytes());
    // -- central directory with authoritative sizes --
    let central_offset = out.len() as u32;
    let mut central = Vec::new();
    central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
    central.extend_from_slice(&20u16.to_le_bytes());
    central.extend_from_slice(&20u16.to_le_bytes());
    central.extend_from_slice(&0x0008u16.to_le_bytes());
    central.extend_from_slice(&method.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&crc.to_le_bytes());
    central.extend_from_slice(&comp.to_le_bytes());
    central.extend_from_slice(&uncomp.to_le_bytes());
    central.extend_from_slice(&(name.len() as u16).to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u32.to_le_bytes());
    central.extend_from_slice(&0u32.to_le_bytes());
    central.extend_from_slice(name.as_bytes());
    out.extend_from_slice(&central);
    // -- end of central directory --
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// Verifies a ZIP entry written with a streaming data descriptor (flag bit 3)
/// is read via the authoritative central-directory sizes instead of rejected,
/// for both stored and deflated payloads.
#[test]
pub(super) fn extracts_zip_entry_with_data_descriptor() {
    let stored = build_zip_with_data_descriptor("stream.txt", b"streamed payload", false);
    assert_eq!(
        extract_entry_bytes(&stored, b"stream.txt").as_deref(),
        Some(&b"streamed payload"[..])
    );
    let deflated =
        build_zip_with_data_descriptor("stream.txt", b"streamed deflated payload", true);
    assert_eq!(
        extract_entry_bytes(&deflated, b"stream.txt").as_deref(),
        Some(&b"streamed deflated payload"[..])
    );
}

/// The ZIP64 extra-field builders emit the tag, length, and only the requested
/// 64-bit fields in APPNOTE order.
#[test]
pub(super) fn builds_zip64_extra_fields() {
    // Local extra always carries both sizes (16-byte body).
    let local = zip64_local_extra(0x1_0000_0001, 0x2_0000_0002);
    assert_eq!(le16(&local, 0), Some(ZIP64_EXTRA_TAG));
    assert_eq!(le16(&local, 2), Some(16));
    assert_eq!(le64(&local, 4), Some(0x1_0000_0001));
    assert_eq!(le64(&local, 12), Some(0x2_0000_0002));
    // Central extra carries only the overflowed fields, in order.
    let central = zip64_central_extra(Some(7), None, Some(9));
    assert_eq!(le16(&central, 2), Some(16));
    assert_eq!(le64(&central, 4), Some(7));
    assert_eq!(le64(&central, 12), Some(9));
    assert!(zip64_central_extra(None, None, None).len() == 4);
}

/// Builds a single-entry ZIP that uses every ZIP64 read path: a central record
/// whose sizes and header offset are 0xFFFFFFFF sentinels resolved by a ZIP64
/// extra field, plus a ZIP64 EOCD record + locator behind a sentinel EOCD.
pub(super) fn build_zip64_sentinel_fixture(name: &str, content: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let local_offset = out.len() as u32;
    let crc = crc32(content);
    let len = content.len() as u32;
    // -- local header with real sizes (central drives sizes anyway) --
    out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
    out.extend_from_slice(&45u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&ZIP_METHOD_STORE.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(content);
    // -- central record: all three size/offset fields are sentinels --
    let central_offset = out.len();
    let extra = zip64_central_extra(Some(len as u64), Some(len as u64), Some(local_offset as u64));
    let mut central = Vec::new();
    central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
    central.extend_from_slice(&45u16.to_le_bytes());
    central.extend_from_slice(&45u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&ZIP_METHOD_STORE.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&crc.to_le_bytes());
    central.extend_from_slice(&ZIP32_SENTINEL.to_le_bytes());
    central.extend_from_slice(&ZIP32_SENTINEL.to_le_bytes());
    central.extend_from_slice(&(name.len() as u16).to_le_bytes());
    central.extend_from_slice(&(extra.len() as u16).to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u32.to_le_bytes());
    central.extend_from_slice(&ZIP32_SENTINEL.to_le_bytes());
    central.extend_from_slice(name.as_bytes());
    central.extend_from_slice(&extra);
    let central_len = central.len();
    out.extend_from_slice(&central);
    // -- ZIP64 EOCD record + locator --
    let eocd64_offset = out.len() as u64;
    out.extend_from_slice(&0x0606_4b50u32.to_le_bytes());
    out.extend_from_slice(&44u64.to_le_bytes());
    out.extend_from_slice(&45u16.to_le_bytes());
    out.extend_from_slice(&45u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&1u64.to_le_bytes());
    out.extend_from_slice(&1u64.to_le_bytes());
    out.extend_from_slice(&(central_len as u64).to_le_bytes());
    out.extend_from_slice(&(central_offset as u64).to_le_bytes());
    out.extend_from_slice(&0x0706_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&eocd64_offset.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    // -- regular EOCD with count/offset sentinels --
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&ZIP16_SENTINEL.to_le_bytes());
    out.extend_from_slice(&ZIP16_SENTINEL.to_le_bytes());
    out.extend_from_slice(&ZIP32_SENTINEL.to_le_bytes());
    out.extend_from_slice(&ZIP32_SENTINEL.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// A ZIP64 archive (sentinel central fields + extra field + EOCD64/locator) is
/// read by resolving the 64-bit values, not rejected.
#[test]
pub(super) fn reads_zip64_archive_with_sentinels() {
    let archive = build_zip64_sentinel_fixture("big.txt", b"zip64 payload body");
    assert_eq!(
        extract_entry_bytes(&archive, b"big.txt").as_deref(),
        Some(&b"zip64 payload body"[..])
    );
}

/// Writing more than 65535 entries triggers ZIP64 output (EOCD64 record +
/// locator), and the bridge reads its own ZIP64 archive back. Set
/// `ELEPHC_KEEP_ZIP64=<path>` to also dump the archive for an external check.
#[test]
pub(super) fn writes_and_reads_zip64_many_entries() {
    let count = 70_000usize;
    let entries: Vec<ArchiveEntry> = (0..count)
        .map(|i| ArchiveEntry {
            name: format!("f{i}.txt").into_bytes(),
            payload: b"x".to_vec(),
            compression: PharCompression::None,
            metadata: Vec::new(),
        })
        .collect();
    let archive = build_zip_archive(&entries, &[], &[]).unwrap();
    // The ZIP64 EOCD record and locator must be present.
    assert!(find_subslice(&archive, &0x0606_4b50u32.to_le_bytes()).is_some());
    assert!(find_subslice(&archive, &0x0706_4b50u32.to_le_bytes()).is_some());
    // The regular EOCD carries the count sentinel.
    let eocd = find_zip_eocd(&archive).unwrap();
    assert_eq!(le16(&archive, eocd + 10), Some(ZIP16_SENTINEL));
    // Round-trip: the bridge reads back all entries and a sampled payload.
    let parsed = parse_zip_archive(&archive).unwrap();
    assert_eq!(parsed.entries.len(), count);
    assert_eq!(
        extract_entry_bytes(&archive, b"f69999.txt").as_deref(),
        Some(&b"x"[..])
    );
    if let Some(path) = std::env::var_os("ELEPHC_KEEP_ZIP64") {
        std::fs::write(path, &archive).unwrap();
    }
}

/// Returns the general-purpose bit-flag field of the first local file header
/// whose name matches `name`, found by scanning for the local-header signature.
/// Used to assert that the writer set (or cleared) the ZipCrypto "encrypted"
/// flag bit on a given entry.
pub(super) fn zip_local_flag(archive: &[u8], name: &[u8]) -> Option<u16> {
    let sig = 0x0403_4b50u32.to_le_bytes();
    let mut i = 0;
    while i + 30 <= archive.len() {
        if archive[i..i + 4] == sig {
            let flag = u16::from_le_bytes([archive[i + 6], archive[i + 7]]);
            let name_len = u16::from_le_bytes([archive[i + 26], archive[i + 27]]) as usize;
            let name_start = i + 30;
            if archive.get(name_start..name_start + name_len) == Some(name) {
                return Some(flag);
            }
        }
        i += 1;
    }
    None
}

/// Builds a single-entry ZIP whose stored entry is traditional-PKWARE
/// (ZipCrypto) encrypted with `password`: a 12-byte encryption header (last
/// byte = the CRC's high byte check) plus the encrypted payload.
pub(super) fn build_zipcrypto_zip(name: &str, content: &[u8], password: &[u8]) -> Vec<u8> {
    let crc = crc32(content);
    // Reuse the production encryptor so the test fixture and the writer share a
    // single cipher direction (check byte = the CRC's high byte, no descriptor).
    let enc = zipcrypto_encrypt(password, content, (crc >> 24) as u8);
    let csz = enc.len() as u32;
    let usz = content.len() as u32;
    let mut out = Vec::new();
    // -- local header with the encrypted flag set --
    out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes());
    out.extend_from_slice(&ZIP_FLAG_ENCRYPTED.to_le_bytes());
    out.extend_from_slice(&ZIP_METHOD_STORE.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&csz.to_le_bytes());
    out.extend_from_slice(&usz.to_le_bytes());
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(&enc);
    // -- central record --
    let central_offset = out.len() as u32;
    let mut central = Vec::new();
    central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
    central.extend_from_slice(&20u16.to_le_bytes());
    central.extend_from_slice(&20u16.to_le_bytes());
    central.extend_from_slice(&ZIP_FLAG_ENCRYPTED.to_le_bytes());
    central.extend_from_slice(&ZIP_METHOD_STORE.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&crc.to_le_bytes());
    central.extend_from_slice(&csz.to_le_bytes());
    central.extend_from_slice(&usz.to_le_bytes());
    central.extend_from_slice(&(name.len() as u16).to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u32.to_le_bytes());
    central.extend_from_slice(&0u32.to_le_bytes()); // local header offset
    central.extend_from_slice(name.as_bytes());
    out.extend_from_slice(&central);
    // -- end of central directory --
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// A ZipCrypto-encrypted ZIP entry decrypts only with the correct password set
/// via `set_zip_password`; a missing or wrong password yields no payload.
#[test]
pub(super) fn reads_zipcrypto_encrypted_entry() {
    let content = b"secret zipcrypto payload\n";
    let archive = build_zipcrypto_zip("zc.txt", content, b"hunter2");
    // No password set: the encrypted entry is unreadable.
    set_zip_password(b"");
    assert_eq!(extract_entry_bytes(&archive, b"zc.txt"), None);
    // Wrong password is rejected by the header check byte.
    set_zip_password(b"wrong-password");
    assert_eq!(extract_entry_bytes(&archive, b"zc.txt"), None);
    // Correct password decrypts the entry.
    set_zip_password(b"hunter2");
    assert_eq!(
        extract_entry_bytes(&archive, b"zc.txt").as_deref(),
        Some(&content[..])
    );
    set_zip_password(b"");
}

/// A wrong ZipCrypto password remains rejected when its one-byte header check
/// collides. The CRC from the central directory is the definitive integrity check.
#[test]
pub(super) fn rejects_zipcrypto_password_after_header_check_collision() {
    let content = b"zipcrypto crc verification\n";
    let password = b"hunter2";
    let wrong_password = b"wrong-password";
    let mut archive = build_zipcrypto_zip("zc-collision.txt", content, password);
    let central = find_zip_eocd(&archive).expect("fixture has central directory");
    let central_offset = le32(&archive, central + 16).expect("central offset") as usize;
    let check_byte = (le32(&archive, central_offset + 16).expect("central crc") >> 24) as u8;
    let local_name_len = le16(&archive, 26).expect("local name length") as usize;
    let payload_start = 30 + local_name_len;

    // Mutate the final encrypted header byte through all values. Exactly one makes
    // the wrong password pass ZipCrypto's one-byte header check, without relying
    // on the nonce used by the production fixture builder.
    let original = archive[payload_start + 11];
    let collision_found = (0..=u8::MAX).any(|byte| {
        archive[payload_start + 11] = byte;
        zipcrypto_decrypt(wrong_password, &archive[payload_start..], check_byte).is_some()
    });
    if !collision_found {
        archive[payload_start + 11] = original;
    }
    assert!(collision_found, "fixture must force a header-check collision");
    set_zip_password(wrong_password);
    assert_eq!(extract_entry_bytes(&archive, b"zc-collision.txt"), None);
    set_zip_password(b"");
}

/// With a zip password set, `build_zip_archive` encrypts every file entry — the
/// stub included — so entries read back only with the correct password; a wrong
/// or cleared password fails, and an archive built with no password stays plain.
#[test]
pub(super) fn writes_then_reads_zipcrypto_entry() {
    let stored = b"plain stored payload".to_vec();
    // Repetitive bytes so the deflate path actually compresses.
    let deflated = b"compress me ".repeat(64);
    let entries = vec![
        ArchiveEntry {
            name: b"a.txt".to_vec(),
            payload: stored.clone(),
            compression: PharCompression::None,
            metadata: Vec::new(),
        },
        ArchiveEntry {
            name: b"b.txt".to_vec(),
            payload: deflated.clone(),
            compression: PharCompression::Gzip,
            metadata: Vec::new(),
        },
    ];
    let stub = PHAR_DEFAULT_STUB.to_vec();

    set_zip_password(b"hunter2");
    let archive = build_zip_archive(&entries, &[], &stub).unwrap();

    // The correct password decrypts both the stored and the deflated entry.
    assert_eq!(
        extract_entry_bytes(&archive, b"a.txt").as_deref(),
        Some(&stored[..])
    );
    assert_eq!(
        extract_entry_bytes(&archive, b"b.txt").as_deref(),
        Some(&deflated[..])
    );

    // The encrypted flag is set on a regular entry and on the stub (chosen scope).
    assert_eq!(zip_local_flag(&archive, b"a.txt"), Some(ZIP_FLAG_ENCRYPTED));
    assert_eq!(zip_local_flag(&archive, PHAR_STUB_ENTRY), Some(ZIP_FLAG_ENCRYPTED));

    // A wrong then cleared password cannot decrypt the entry.
    set_zip_password(b"nope");
    assert_eq!(extract_entry_bytes(&archive, b"a.txt"), None);
    set_zip_password(b"");
    assert_eq!(extract_entry_bytes(&archive, b"a.txt"), None);

    // Built with no password the archive is plain and reads with none set.
    let plain = build_zip_archive(&entries, &[], &stub).unwrap();
    assert_eq!(
        extract_entry_bytes(&plain, b"a.txt").as_deref(),
        Some(&stored[..])
    );
    assert_eq!(zip_local_flag(&plain, b"a.txt"), Some(0));
}

/// Signing a zip phar whose entries are encrypted still produces a readable
/// `.phar/signature.bin`: the signed range covers the encrypted bytes, the entry
/// decrypts with the password, the signature reports SHA-256, and the signature
/// entry itself stays in the clear (no encrypted flag).
#[test]
pub(super) fn signed_encrypted_zip_still_verifies() {
    let path =
        std::env::temp_dir().join(format!("elephc_phar_encsig_{}.zip", std::process::id()));
    let pb = path.to_string_lossy();

    set_zip_password(b"hunter2");
    // Write an encrypted entry, then SHA-256 (algo 3) sign the archive.
    assert_eq!(
        put_entry_bytes(pb.as_bytes(), b"doc.txt", b"top secret\n"),
        Some(11)
    );
    assert_eq!(sign_archive_hash(pb.as_bytes(), 3), Some(()));
    let data = std::fs::read(&path).unwrap();

    // The entry still decrypts; the signature reports SHA-256 with a 32-byte digest.
    assert_eq!(
        extract_entry_bytes(&data, b"doc.txt").as_deref(),
        Some(&b"top secret\n"[..])
    );
    assert_eq!(signature_type_name(pb.as_bytes()).as_deref(), Some(&b"SHA-256"[..]));
    let (flag, digest) = read_signature_info(pb.as_bytes()).unwrap();
    assert_eq!(flag, 3);
    assert_eq!(digest.len(), 32);

    // The entry is encrypted but the signature entry stays in the clear.
    assert_eq!(zip_local_flag(&data, b"doc.txt"), Some(ZIP_FLAG_ENCRYPTED));
    assert_eq!(zip_local_flag(&data, PHAR_SIGNATURE_ENTRY), Some(0));

    // Without the password the encrypted entry is unreadable.
    set_zip_password(b"");
    assert_eq!(extract_entry_bytes(&data, b"doc.txt"), None);
    std::fs::remove_file(&path).ok();
}
