//! Purpose:
//! Whole-archive compression decoding, encoding, and filesystem path operations.
//!
//! Called from:
//! - Archive parsing, compression APIs, and C ABI wrappers.
//!
//! Key details:
//! - Gzip and bzip2 wrappers are decoded before container-family dispatch.

use super::*;

/// Parses archive bytes into a full [`Archive`] (entries plus global metadata/stub).
///
/// Dispatch is by container signature rather than try-each-and-fallback: tar/zip-based
/// phars embed a `.phar/stub.php` containing `__HALT_COMPILER();`, so a native-first
/// scan would mistake them for native PHARs. ZIP starts with `PK\x03\x04` (or
/// `PK\x05\x06` when empty); TAR carries the ustar magic at offset 257; everything
/// else (a `<?php` stub) is a native PHAR.
#[cfg(test)]
pub(super) fn parse_archive(data: &[u8]) -> Option<Archive> {
    parse_archive_with_public_key(data, None, false, false)
}

/// Parses archive bytes while authenticating an OpenSSL signature with `public_key`.
///
/// The key is threaded through whole-archive compression so a `.tar.gz` or
/// `.tar.bz2` archive authenticates the inner PHAR before exposing entries.
/// `openssl_expected` reports an adjacent `<archive>.pubkey` sidecar: when set,
/// a keyless hash signature is rejected instead of trusted (CVE-2026-128090).
pub(super) fn parse_archive_with_public_key(
    data: &[u8],
    public_key: Option<&rsa::RsaPublicKey>,
    openssl_expected: bool,
    require_hash: bool,
) -> Option<Archive> {
    // A whole-archive gzip/bzip2 wrapper (e.g. `.tar.gz` / `.tar.bz2`) is decoded
    // transparently, then the inner archive is parsed normally.
    if data.starts_with(b"\x1f\x8b") {
        return parse_archive_with_public_key(
            &decompress_gzip_stream(data)?,
            public_key,
            openssl_expected,
            require_hash,
        );
    }
    if data.starts_with(b"BZh") {
        return parse_archive_with_public_key(
            &decompress_bzip2_stream(data)?,
            public_key,
            openssl_expected,
            require_hash,
        );
    }
    if data.starts_with(b"PK\x03\x04") || data.starts_with(b"PK\x05\x06") {
        parse_zip_archive_with_public_key(data, public_key, openssl_expected, require_hash)
    } else if data.get(257..262) == Some(b"ustar") {
        parse_tar_archive_with_public_key(data, public_key, openssl_expected, require_hash)
    } else {
        parse_native_phar_archive_with_public_key(data, public_key, openssl_expected, require_hash)
    }
}

/// Reads an archive from `path` and authenticates OpenSSL signatures with the
/// PEM public key stored at PHP's conventional `<archive>.pubkey` sidecar path.
pub(super) fn parse_archive_path(path: &std::path::Path) -> Option<Archive> {
    read_verified_archive(path).map(|(_, archive)| archive)
}

/// Reads and authenticates one filesystem snapshot of an archive.
///
/// Returning the verified bytes with the decoded archive prevents callers that
/// need both forms from validating one file snapshot and then consuming another.
pub(super) fn read_verified_archive(path: &std::path::Path) -> Option<(Vec<u8>, Archive)> {
    read_verified_archive_with_policy(path, require_hash())
}

/// Reads and authenticates one archive snapshot with an explicit require-hash
/// policy. The signing APIs use `false` so an unsigned archive can be opened and
/// then signed; normal path reads use the thread's `phar.require_hash` policy.
pub(super) fn read_verified_archive_with_policy(
    path: &std::path::Path,
    require_hash: bool,
) -> Option<(Vec<u8>, Archive)> {
    let data = std::fs::read(path).ok()?;
    // PHP checks for the sidecar's existence, not whether it parses: a stray or
    // malformed `<archive>.pubkey` still means OpenSSL verification is expected.
    let openssl_expected = archive_public_key_path(path).exists();
    let public_key = read_archive_public_key(path);
    let archive = parse_archive_with_public_key(
        &data,
        public_key.as_ref(),
        openssl_expected,
        require_hash,
    )?;
    Some((data, archive))
}

/// Checks whether an existing archive satisfies the signature policy without
/// decoding its entries.
///
/// The OOP constructor must fail an unauthenticated archive like php-src, but it
/// must still open archives whose payloads are not decodable yet (ZipCrypto
/// entries, which `setZipPassword()` decrypts later). This walk verifies only the
/// signature trailer. `is_data` mirrors `PharData`, which PHP keeps unsigned.
pub(super) fn archive_signature_is_valid(path: &std::path::Path, is_data: bool) -> bool {
    let Ok(data) = std::fs::read(path) else {
        return false;
    };
    let openssl_expected = archive_public_key_path(path).exists();
    let public_key = read_archive_public_key(path);
    let require_hash = require_hash() && !is_data;
    let decompressed;
    let data: &[u8] = if data.starts_with(b"\x1f\x8b") || data.starts_with(b"BZh") {
        decompressed = match uncompressed_archive_bytes(&data) {
            Some(bytes) => bytes,
            None => return false,
        };
        &decompressed
    } else {
        &data
    };
    match signing_format(data) {
        Some(ArchiveFormat::Zip) => {
            verify_zip_phar_signature(data, public_key.as_ref(), openssl_expected, require_hash)
                .is_some()
        }
        Some(ArchiveFormat::Tar) => {
            verify_tar_phar_signature(data, public_key.as_ref(), openssl_expected, require_hash)
                .is_some()
        }
        Some(ArchiveFormat::NativePhar) => {
            let Some(manifest_start) = native_manifest_start(data) else {
                return false;
            };
            let Some(global_flags) = le32(data, manifest_start + 10) else {
                return false;
            };
            verify_native_phar_signature(
                data,
                global_flags & PHAR_HDR_SIGNATURE != 0,
                public_key.as_ref(),
                openssl_expected,
                require_hash,
            )
            .is_some()
        }
        None => false,
    }
}

/// Decompresses a whole gzip (`.gz`) stream into its plain bytes.
pub(super) fn decompress_gzip_stream(data: &[u8]) -> Option<Vec<u8>> {
    let mut decoder = flate2::read::GzDecoder::new(data);
    read_bounded_archive_stream(&mut decoder, data.len())
}

/// Decompresses a whole bzip2 (`.bz2`) stream into its plain bytes.
pub(super) fn decompress_bzip2_stream(data: &[u8]) -> Option<Vec<u8>> {
    let mut decoder = bzip2_rs::DecoderReader::new(data);
    read_bounded_archive_stream(&mut decoder, data.len())
}

/// Reads a whole-archive compression stream without allowing an input bomb to
/// allocate beyond both a fixed ceiling and the PHAR expansion-ratio ceiling.
fn read_bounded_archive_stream(reader: &mut impl Read, compressed_len: usize) -> Option<Vec<u8>> {
    let ratio_ceiling = compressed_len.checked_mul(MAX_PHAR_DECOMPRESSION_RATIO)?;
    let ceiling = ratio_ceiling.min(MAX_PHAR_ARCHIVE_DECOMPRESSED_BYTES);
    let mut out = Vec::new();
    reader
        .take(u64::try_from(ceiling.checked_add(1)?).ok()?)
        .read_to_end(&mut out)
        .ok()?;
    (out.len() <= ceiling).then_some(out)
}

/// Returns the plain (uncompressed) archive bytes, stripping a whole-archive gzip or
/// bzip2 wrapper when present so a recompress operates on the canonical archive.
pub(super) fn uncompressed_archive_bytes(raw: &[u8]) -> Option<Vec<u8>> {
    if raw.starts_with(b"\x1f\x8b") {
        decompress_gzip_stream(raw)
    } else if raw.starts_with(b"BZh") {
        decompress_bzip2_stream(raw)
    } else {
        Some(raw.to_vec())
    }
}

/// Returns the destination path for compressing `src`: any existing `.gz`/`.bz2`
/// suffix is stripped, then `.<new_ext>` is appended (e.g. `foo.tar` → `foo.tar.gz`).
pub(super) fn compression_dest_path(src: &[u8], new_ext: &str) -> Option<Vec<u8>> {
    let s = std::str::from_utf8(src).ok()?;
    let base = s
        .strip_suffix(".gz")
        .or_else(|| s.strip_suffix(".bz2"))
        .unwrap_or(s);
    Some(format!("{base}.{new_ext}").into_bytes())
}

/// Reads `src`, gzip-wraps its plain archive bytes, writes them to `<base>.gz`, and
/// returns that destination path (PHP `PharData::compress(Phar::GZ)`).
pub(super) fn gzip_archive(src: &[u8]) -> Option<Vec<u8>> {
    let src_path = std::path::Path::new(std::str::from_utf8(src).ok()?);
    let (raw, _) = read_verified_archive(src_path)?;
    let plain = uncompressed_archive_bytes(&raw)?;
    let mut encoder =
        flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut encoder, &plain).ok()?;
    let dest = compression_dest_path(src, "gz")?;
    write_path(&dest, &encoder.finish().ok()?)?;
    copy_archive_public_key(src_path, std::path::Path::new(std::str::from_utf8(&dest).ok()?))?;
    Some(dest)
}

/// Reads `src`, bzip2-wraps its plain archive bytes, writes them to `<base>.bz2`, and
/// returns that destination path (PHP `PharData::compress(Phar::BZ2)`).
pub(super) fn bzip2_archive(src: &[u8]) -> Option<Vec<u8>> {
    let src_path = std::path::Path::new(std::str::from_utf8(src).ok()?);
    let (raw, _) = read_verified_archive(src_path)?;
    let plain = uncompressed_archive_bytes(&raw)?;
    let mut encoder = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
    std::io::Write::write_all(&mut encoder, &plain).ok()?;
    let dest = compression_dest_path(src, "bz2")?;
    write_path(&dest, &encoder.finish().ok()?)?;
    copy_archive_public_key(src_path, std::path::Path::new(std::str::from_utf8(&dest).ok()?))?;
    Some(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies whole-archive readers stop after the shared expansion ceiling
    /// instead of draining an unbounded decompression stream.
    #[test]
    fn whole_archive_stream_reader_rejects_output_beyond_ratio_ceiling() {
        let mut hostile = std::io::repeat(b'A').take(
            u64::try_from(MAX_PHAR_DECOMPRESSION_RATIO + 1)
                .expect("PHAR ratio ceiling fits u64"),
        );
        assert!(
            read_bounded_archive_stream(&mut hostile, 1).is_none(),
            "whole-archive readers must reject output beyond compressed_len * ratio"
        );
    }
}

/// Reads a whole-archive-compressed `src` (a `.gz`/`.bz2` path), writes its plain
/// bytes to the path with that suffix removed, and returns that destination path
/// (PHP `PharData::decompress()`). Fails when `src` carries no compression suffix.
pub(super) fn decompress_archive(src: &[u8]) -> Option<Vec<u8>> {
    let s = std::str::from_utf8(src).ok()?;
    let src_path = std::path::Path::new(s);
    let (raw, _) = read_verified_archive(src_path)?;
    let dest = s
        .strip_suffix(".gz")
        .or_else(|| s.strip_suffix(".bz2"))?
        .as_bytes()
        .to_vec();
    write_path(&dest, &uncompressed_archive_bytes(&raw)?)?;
    copy_archive_public_key(src_path, std::path::Path::new(std::str::from_utf8(&dest).ok()?))?;
    Some(dest)
}

/// Reads a filesystem path given as UTF-8 bytes.
pub(super) fn read_path(path: &[u8]) -> Option<Vec<u8>> {
    std::fs::read(std::path::Path::new(std::str::from_utf8(path).ok()?)).ok()
}

/// Writes `bytes` to a filesystem path given as UTF-8 bytes.
pub(super) fn write_path(path: &[u8], bytes: &[u8]) -> Option<()> {
    std::fs::write(std::path::Path::new(std::str::from_utf8(path).ok()?), bytes).ok()
}
