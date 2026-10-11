//! Purpose:
//! Native, tar, and ZIP PHAR signature generation and inspection.
//!
//! Called from:
//! - PHAR signing APIs and their C ABI wrappers.
//!
//! Key details:
//! - Hash and OpenSSL signatures cover the exact family-specific signed byte range.

use super::*;

/// Parses an RSA private key from PKCS#8 or PKCS#1 PEM.
fn rsa_private_key_from_pem(key_pem: &[u8]) -> Option<rsa::RsaPrivateKey> {
    use rsa::pkcs1::DecodeRsaPrivateKey;
    use rsa::pkcs8::DecodePrivateKey;

    let pem = std::str::from_utf8(key_pem).ok()?;
    rsa::RsaPrivateKey::from_pkcs8_pem(pem)
        .ok()
        .or_else(|| rsa::RsaPrivateKey::from_pkcs1_pem(pem).ok())
}

/// Parses an RSA public key from SubjectPublicKeyInfo or PKCS#1 PEM.
fn rsa_public_key_from_pem(key_pem: &[u8]) -> Option<rsa::RsaPublicKey> {
    use rsa::pkcs1::DecodeRsaPublicKey;
    use rsa::pkcs8::DecodePublicKey;

    let pem = std::str::from_utf8(key_pem).ok()?;
    rsa::RsaPublicKey::from_public_key_pem(pem)
        .ok()
        .or_else(|| rsa::RsaPublicKey::from_pkcs1_pem(pem).ok())
}

/// Returns PHP's conventional public-key sidecar path for an archive.
pub(super) fn archive_public_key_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut sidecar = path.as_os_str().to_os_string();
    sidecar.push(".pubkey");
    std::path::PathBuf::from(sidecar)
}

/// Loads and parses the RSA public key stored at `<archive>.pubkey`.
pub(super) fn read_archive_public_key(path: &std::path::Path) -> Option<rsa::RsaPublicKey> {
    rsa_public_key_from_pem(&std::fs::read(archive_public_key_path(path)).ok()?)
}

/// Copies an archive's public-key sidecar to a derived archive path when present.
pub(super) fn copy_archive_public_key(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> Option<()> {
    let source = archive_public_key_path(source);
    if !source.exists() {
        return Some(());
    }
    std::fs::copy(source, archive_public_key_path(destination))
        .ok()
        .map(|_| ())
}

/// Appends PHP's raw-SHA1 PHAR signature trailer to `archive`.
pub(super) fn append_sha1_signature(archive: &mut Vec<u8>) {
    use sha1::{Digest, Sha1};

    let digest = Sha1::digest(&archive);
    archive.extend_from_slice(&digest);
    archive.extend_from_slice(&PHAR_SHA1_SIGNATURE_TYPE.to_le_bytes());
    archive.extend_from_slice(b"GBMB");
}

/// Returns the raw digest length for a PHP hash-based PHAR signature flag
/// (MD5=1, SHA1=2, SHA256=3, SHA512=4); `None` for non-hash flags.
pub(super) fn signature_digest_len(flags: u32) -> Option<usize> {
    match flags {
        1 => Some(16),
        2 => Some(20),
        3 => Some(32),
        4 => Some(64),
        _ => None,
    }
}

/// Returns the archive bytes with any trailing PHP signature trailer removed
/// (native PHAR `digest ++ LE32(flag) ++ "GBMB"`, or the OpenSSL variant
/// `sig ++ LE32(sig_len) ++ LE32(0x10) ++ "GBMB"`). Returns the input unchanged
/// when no recognized trailer is present.
pub(super) fn strip_signature_trailer(archive: &[u8]) -> &[u8] {
    let n = archive.len();
    if n < 8 || &archive[n - 4..] != b"GBMB" {
        return archive;
    }
    let flags = u32::from_le_bytes(archive[n - 8..n - 4].try_into().unwrap());
    if flags == PHAR_OPENSSL_SIGNATURE_TYPE {
        if n >= 12 {
            let sig_len = u32::from_le_bytes(archive[n - 12..n - 8].try_into().unwrap()) as usize;
            if let Some(total) = sig_len.checked_add(12) {
                if n >= total {
                    return &archive[..n - total];
                }
            }
        }
    } else if let Some(dlen) = signature_digest_len(flags) {
        let total = dlen + 8;
        if n >= total {
            return &archive[..n - total];
        }
    }
    archive
}

/// Computes the PKCS#1 v1.5 RSA-SHA1 signature of `data` with a PEM private key
/// (PKCS#8 or PKCS#1), matching PHP's `openssl_sign(..., OPENSSL_ALGO_SHA1)`.
pub(super) fn rsa_sha1_sign(data: &[u8], key_pem: &[u8]) -> Option<Vec<u8>> {
    use rsa::Pkcs1v15Sign;
    use sha1::{Digest, Sha1};

    let key = rsa_private_key_from_pem(key_pem)?;
    let hashed = Sha1::digest(data);
    key.sign(Pkcs1v15Sign::new::<Sha1>(), &hashed).ok()
}

/// Verifies a PKCS#1 v1.5 RSA-SHA1 signature against the exact signed bytes.
fn rsa_sha1_verify(data: &[u8], signature: &[u8], key: &rsa::RsaPublicKey) -> Option<()> {
    use rsa::Pkcs1v15Sign;
    use sha1::{Digest, Sha1};

    let hashed = Sha1::digest(data);
    key.verify(Pkcs1v15Sign::new::<Sha1>(), &hashed, signature)
        .ok()
}

/// Computes a PHP-compatible signature over `data` for a signature `flag`: a raw
/// MD5/SHA1/SHA256/SHA512 digest (flags 1..=4) or an RSA-SHA1 OpenSSL signature
/// (flag 0x10, requiring the PEM `key`). Returns `None` for an unknown flag or a
/// missing/invalid key.
pub(super) fn compute_signature(flag: u32, key: Option<&[u8]>, data: &[u8]) -> Option<Vec<u8>> {
    use md5::Md5;
    use sha1::{Digest, Sha1};
    use sha2::{Sha256, Sha512};

    match flag {
        1 => Some(Md5::digest(data).to_vec()),
        2 => Some(Sha1::digest(data).to_vec()),
        3 => Some(Sha256::digest(data).to_vec()),
        4 => Some(Sha512::digest(data).to_vec()),
        PHAR_OPENSSL_SIGNATURE_TYPE => rsa_sha1_sign(data, key?),
        _ => None,
    }
}

/// Authenticates the signature required by a native PHAR manifest before payloads
/// are decoded. OpenSSL signatures require a verified external public key.
pub(super) fn verify_native_phar_signature(
    data: &[u8],
    signature_required: bool,
    public_key: Option<&rsa::RsaPublicKey>,
) -> Option<()> {
    if !signature_required {
        if data.ends_with(b"GBMB") {
            let flags = le32(data, data.len().checked_sub(8)?)?;
            if flags == PHAR_OPENSSL_SIGNATURE_TYPE || signature_digest_len(flags).is_some() {
                return None;
            }
        }
        return Some(());
    }
    if !data.ends_with(b"GBMB") {
        return None;
    }

    let flags = le32(data, data.len().checked_sub(8)?)?;
    if flags == PHAR_OPENSSL_SIGNATURE_TYPE {
        let signature_len = le32(data, data.len().checked_sub(12)?)? as usize;
        let trailer_len = signature_len.checked_add(12)?;
        let signed_len = data.len().checked_sub(trailer_len)?;
        let signature = data.get(signed_len..signed_len.checked_add(signature_len)?)?;
        return rsa_sha1_verify(data.get(..signed_len)?, signature, public_key?);
    }

    let digest_len = signature_digest_len(flags)?;
    let trailer_len = digest_len.checked_add(8)?;
    let signed_len = data.len().checked_sub(trailer_len)?;
    let expected = data.get(signed_len..signed_len.checked_add(digest_len)?)?;
    let actual = compute_signature(flags, None, data.get(..signed_len)?)?;
    (actual.as_slice() == expected).then_some(())
}

/// Builds the `.phar/signature.bin` payload for a tar/zip phar:
/// `LE32(sig_flag) ++ LE32(sig_len) ++ signature`.
pub(super) fn signature_bin_payload(flag: u32, sig: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(8 + sig.len());
    out.extend_from_slice(&flag.to_le_bytes());
    out.extend_from_slice(&u32::try_from(sig.len()).ok()?.to_le_bytes());
    out.extend_from_slice(sig);
    Some(out)
}

/// Detects the archive family of `data` for signature operations: zip (PK magic),
/// tar (ustar magic at offset 257), or native PHAR (default). Returns `None` for a
/// gzip/bzip2-wrapped archive, where signature rewriting is not supported.
pub(super) fn signing_format(data: &[u8]) -> Option<ArchiveFormat> {
    if data.starts_with(&[0x50, 0x4b, 0x03, 0x04]) || data.starts_with(&[0x50, 0x4b, 0x05, 0x06]) {
        Some(ArchiveFormat::Zip)
    } else if data.get(257..262) == Some(b"ustar") {
        Some(ArchiveFormat::Tar)
    } else if data.starts_with(&[0x1f, 0x8b]) || data.starts_with(b"BZh") {
        None
    } else {
        Some(ArchiveFormat::NativePhar)
    }
}

/// Re-signs the phar at `path` with an OpenSSL (RSA-SHA1) signature. Native PHARs
/// gain a `sig ++ LE32(sig_len) ++ LE32(0x10) ++ "GBMB"` trailer; tar/zip phars
/// gain a `.phar/signature.bin` entry. The matching public key must be written
/// separately to `<archive>.pubkey`; this operation does not create the sidecar.
pub(super) fn sign_archive_openssl(path: &[u8], key_pem: &[u8]) -> Option<()> {
    let data = read_path(path)?;
    let private_key = rsa_private_key_from_pem(key_pem)?;
    let signing_public_key = rsa::RsaPublicKey::from(&private_key);
    let fs_path = std::path::Path::new(std::str::from_utf8(path).ok()?);
    let sidecar_public_key = read_archive_public_key(fs_path);
    let verification_key = sidecar_public_key
        .as_ref()
        .unwrap_or(&signing_public_key);
    match signing_format(&data)? {
        ArchiveFormat::Zip => {
            let archive = parse_zip_archive_with_public_key(&data, Some(verification_key))?;
            let signed = sign_zip_archive(
                &archive,
                PHAR_OPENSSL_SIGNATURE_TYPE,
                Some(key_pem),
            )?;
            write_path(path, &signed)
        }
        ArchiveFormat::Tar => {
            let archive = parse_tar_archive_with_public_key(&data, Some(verification_key))?;
            let signed = sign_tar_archive(
                &archive,
                PHAR_OPENSSL_SIGNATURE_TYPE,
                Some(key_pem),
            )?;
            write_path(path, &signed)
        }
        ArchiveFormat::NativePhar => {
            parse_native_phar_archive_with_public_key(&data, Some(verification_key))?;
            let mut out = strip_signature_trailer(&data).to_vec();
            let sig = rsa_sha1_sign(&out, key_pem)?;
            out.extend_from_slice(&sig);
            out.extend_from_slice(&u32::try_from(sig.len()).ok()?.to_le_bytes());
            out.extend_from_slice(&PHAR_OPENSSL_SIGNATURE_TYPE.to_le_bytes());
            out.extend_from_slice(b"GBMB");
            write_path(path, &out)
        }
    }
}

/// Re-signs the phar at `path` with a hash-based signature (MD5/SHA1/SHA256/SHA512
/// per `algo` 1..=4). Native PHARs append `digest ++ LE32(algo) ++ "GBMB"`; tar/zip
/// phars gain a `.phar/signature.bin` entry.
pub(super) fn sign_archive_hash(path: &[u8], algo: u32) -> Option<()> {
    let fs_path = std::path::Path::new(std::str::from_utf8(path).ok()?);
    let (data, archive) = read_verified_archive(fs_path)?;
    match signing_format(&data)? {
        ArchiveFormat::Zip => {
            let signed = sign_zip_archive(&archive, algo, None)?;
            write_path(path, &signed)
        }
        ArchiveFormat::Tar => {
            let signed = sign_tar_archive(&archive, algo, None)?;
            write_path(path, &signed)
        }
        ArchiveFormat::NativePhar => {
            let mut out = strip_signature_trailer(&data).to_vec();
            let digest = compute_signature(algo, None, &out)?;
            out.extend_from_slice(&digest);
            out.extend_from_slice(&algo.to_le_bytes());
            out.extend_from_slice(b"GBMB");
            write_path(path, &out)
        }
    }
}

/// Decodes a tar/zip `.phar/signature.bin` payload into its flag and signature
/// bytes (`LE32(flag) ++ LE32(len) ++ signature`).
pub(super) fn parse_signature_bin(payload: &[u8]) -> Option<(u32, Vec<u8>)> {
    let flag = le32(payload, 0)?;
    let len = le32(payload, 4)? as usize;
    let end = 8usize.checked_add(len)?;
    (end == payload.len()).then_some((flag, payload.get(8..end)?.to_vec()))
}

/// Returns the raw `.phar/signature.bin` payload from a tar phar, if present.
pub(super) fn read_tar_signature(data: &[u8]) -> Option<Vec<u8>> {
    let mut p = 0usize;
    while p.checked_add(512)? <= data.len() {
        let header = &data[p..p + 512];
        if header.iter().all(|&b| b == 0) {
            break;
        }
        let size = parse_tar_octal(&header[124..136])?;
        let payload_start = p.checked_add(512)?;
        let typeflag = header[156];
        if (typeflag == 0 || typeflag == b'0') && tar_entry_name(header)? == PHAR_SIGNATURE_ENTRY {
            return data
                .get(payload_start..payload_start.checked_add(size)?)
                .map(<[u8]>::to_vec);
        }
        p = payload_start.checked_add(round_up_to_512(size)?)?;
    }
    None
}

/// Returns the raw `.phar/signature.bin` payload from a zip phar, if present.
pub(super) fn read_zip_signature(data: &[u8]) -> Option<Vec<u8>> {
    let (entry_count, central_dir_offset) = zip_eocd_info(data)?;
    let mut p = central_dir_offset;
    for _ in 0..entry_count {
        if le32(data, p)? != 0x0201_4b50 {
            return None;
        }
        let method = le16(data, p + 10)?;
        let crc = le32(data, p + 16)?;
        let mut compressed_size = le32(data, p + 20)? as usize;
        let mut uncompressed_size = le32(data, p + 24)? as usize;
        let name_len = le16(data, p + 28)? as usize;
        let extra_len = le16(data, p + 30)? as usize;
        let comment_len = le16(data, p + 32)? as usize;
        let mut local_offset = le32(data, p + 42)? as usize;
        let name_start = p + 46;
        let name = data.get(name_start..name_start.checked_add(name_len)?)?;
        if name == PHAR_SIGNATURE_ENTRY {
            apply_zip64_central_extra(
                data,
                name_start.checked_add(name_len)?,
                extra_len,
                &mut uncompressed_size,
                &mut compressed_size,
                &mut local_offset,
            )?;
            // The reserved signature entry is never encrypted.
            return decode_zip_local_entry(
                data,
                local_offset,
                method,
                compressed_size,
                uncompressed_size,
                false,
                0,
                crc,
            );
        }
        p = name_start
            .checked_add(name_len)?
            .checked_add(extra_len)?
            .checked_add(comment_len)?;
    }
    None
}

/// Authenticates a tar PHAR signature before any entry is exposed.
///
/// Tar signatures cover every byte before the signature entry's header. OpenSSL
/// signatures require the archive's external public key.
pub(super) fn verify_tar_phar_signature(
    data: &[u8],
    public_key: Option<&rsa::RsaPublicKey>,
) -> Option<()> {
    let mut p = 0usize;
    while p.checked_add(512)? <= data.len() {
        let header = &data[p..p + 512];
        if header.iter().all(|&byte| byte == 0) {
            return Some(());
        }
        let size = parse_tar_octal(&header[124..136])?;
        let payload_start = p.checked_add(512)?;
        if (header[156] == 0 || header[156] == b'0')
            && tar_entry_name(header)? == PHAR_SIGNATURE_ENTRY
        {
            let payload = data.get(payload_start..payload_start.checked_add(size)?)?;
            let (flag, expected) = parse_signature_bin(payload)?;
            let trailing = payload_start.checked_add(round_up_to_512(size)?)?;
            if data.get(trailing..)?.iter().any(|&byte| byte != 0) {
                return None;
            }
            if flag == PHAR_OPENSSL_SIGNATURE_TYPE {
                return rsa_sha1_verify(data.get(..p)?, &expected, public_key?);
            }
            signature_digest_len(flag)?;
            let actual = compute_signature(flag, None, data.get(..p)?)?;
            return (actual == expected).then_some(());
        }
        p = payload_start.checked_add(round_up_to_512(size)?)?;
    }
    Some(())
}

/// Authenticates a ZIP PHAR signature before any entry is exposed.
///
/// The digest covers local records, central-directory records, and the ZIP
/// comment while excluding the reserved signature entry and EOCD record. OpenSSL
/// signatures require the archive's external public key.
pub(super) fn verify_zip_phar_signature(
    data: &[u8],
    public_key: Option<&rsa::RsaPublicKey>,
) -> Option<()> {
    let eocd = find_zip_eocd(data)?;
    let (entry_count, central_dir_offset) = zip_eocd_info(data)?;
    let comment_len = le16(data, eocd.checked_add(20)?)? as usize;
    let comment_start = eocd.checked_add(22)?;
    let comment = data.get(comment_start..comment_start.checked_add(comment_len)?)?;
    let mut p = central_dir_offset;
    let mut signature: Option<(usize, usize, usize, u32, Vec<u8>)> = None;
    for _ in 0..entry_count {
        if le32(data, p)? != 0x0201_4b50 {
            return None;
        }
        let method = le16(data, p + 10)?;
        let crc = le32(data, p + 16)?;
        let mut compressed_size = le32(data, p + 20)? as usize;
        let mut uncompressed_size = le32(data, p + 24)? as usize;
        let name_len = le16(data, p + 28)? as usize;
        let extra_len = le16(data, p + 30)? as usize;
        let entry_comment_len = le16(data, p + 32)? as usize;
        let mut local_offset = le32(data, p + 42)? as usize;
        let name_start = p.checked_add(46)?;
        let name = data.get(name_start..name_start.checked_add(name_len)?)?;
        apply_zip64_central_extra(
            data,
            name_start.checked_add(name_len)?,
            extra_len,
            &mut uncompressed_size,
            &mut compressed_size,
            &mut local_offset,
        )?;
        let central_end = name_start
            .checked_add(name_len)?
            .checked_add(extra_len)?
            .checked_add(entry_comment_len)?;
        if name == PHAR_SIGNATURE_ENTRY {
            if method != ZIP_METHOD_STORE || signature.is_some() {
                return None;
            }
            let payload = decode_zip_local_entry(
                data,
                local_offset,
                method,
                compressed_size,
                uncompressed_size,
                false,
                0,
                crc,
            )?;
            let (flag, expected) = parse_signature_bin(&payload)?;
            if flag != PHAR_OPENSSL_SIGNATURE_TYPE {
                signature_digest_len(flag)?;
            }
            let local_name_len = le16(data, local_offset.checked_add(26)?)? as usize;
            let local_extra_len = le16(data, local_offset.checked_add(28)?)? as usize;
            let local_end = local_offset
                .checked_add(30)?
                .checked_add(local_name_len)?
                .checked_add(local_extra_len)?
                .checked_add(compressed_size)?;
            signature = Some((local_offset, local_end, p, flag, expected));
        }
        p = central_end;
    }
    let Some((local_start, local_end, central_start, flag, expected)) = signature else {
        return Some(());
    };
    let central_end = p;
    if local_end > central_dir_offset || central_end > eocd {
        return None;
    }
    let mut signed = Vec::with_capacity(data.len().saturating_sub(local_end - local_start));
    signed.extend_from_slice(data.get(..local_start)?);
    signed.extend_from_slice(data.get(local_end..central_dir_offset)?);
    signed.extend_from_slice(data.get(central_dir_offset..central_start)?);
    let signature_central_end = {
        let name_len = le16(data, central_start.checked_add(28)?)? as usize;
        let extra_len = le16(data, central_start.checked_add(30)?)? as usize;
        let comment_len = le16(data, central_start.checked_add(32)?)? as usize;
        central_start
            .checked_add(46)?
            .checked_add(name_len)?
            .checked_add(extra_len)?
            .checked_add(comment_len)?
    };
    signed.extend_from_slice(data.get(signature_central_end..central_end)?);
    signed.extend_from_slice(comment);
    if flag == PHAR_OPENSSL_SIGNATURE_TYPE {
        rsa_sha1_verify(&signed, &expected, public_key?)
    } else {
        (compute_signature(flag, None, &signed)? == expected).then_some(())
    }
}

/// Reads the signature of the phar at `path`, returning the flag and the raw
/// signature/digest bytes. Native PHARs use the `GBMB` trailer; tar/zip phars use
/// the `.phar/signature.bin` entry.
pub(super) fn read_signature_info(path: &[u8]) -> Option<(u32, Vec<u8>)> {
    let fs_path = std::path::Path::new(std::str::from_utf8(path).ok()?);
    let (data, _) = read_verified_archive(fs_path)?;
    match signing_format(&data)? {
        ArchiveFormat::Zip => parse_signature_bin(&read_zip_signature(&data)?),
        ArchiveFormat::Tar => parse_signature_bin(&read_tar_signature(&data)?),
        ArchiveFormat::NativePhar => {
            let n = data.len();
            if n < 8 || &data[n - 4..] != b"GBMB" {
                return None;
            }
            let flags = u32::from_le_bytes(data[n - 8..n - 4].try_into().unwrap());
            if flags == PHAR_OPENSSL_SIGNATURE_TYPE {
                let sig_len =
                    u32::from_le_bytes(data.get(n - 12..n - 8)?.try_into().unwrap()) as usize;
                let start = n.checked_sub(12)?.checked_sub(sig_len)?;
                Some((flags, data.get(start..n - 12)?.to_vec()))
            } else {
                let dlen = signature_digest_len(flags)?;
                let start = n.checked_sub(8)?.checked_sub(dlen)?;
                Some((flags, data.get(start..n - 8)?.to_vec()))
            }
        }
    }
}

/// Returns the uppercase hex of the PHAR's signature/digest bytes (PHP
/// `Phar::getSignature()['hash']`).
pub(super) fn signature_hash_hex(path: &[u8]) -> Option<Vec<u8>> {
    let (_, bytes) = read_signature_info(path)?;
    let mut hex = Vec::with_capacity(bytes.len() * 2);
    for byte in bytes {
        hex.extend_from_slice(format!("{byte:02X}").as_bytes());
    }
    Some(hex)
}

/// Returns the PHP signature type name for the PHAR (`getSignature()['hash_type']`).
pub(super) fn signature_type_name(path: &[u8]) -> Option<Vec<u8>> {
    let (flags, _) = read_signature_info(path)?;
    let name: &[u8] = match flags {
        1 => b"MD5",
        2 => b"SHA-1",
        3 => b"SHA-256",
        4 => b"SHA-512",
        PHAR_OPENSSL_SIGNATURE_TYPE => b"OpenSSL",
        _ => return None,
    };
    Some(name.to_vec())
}
