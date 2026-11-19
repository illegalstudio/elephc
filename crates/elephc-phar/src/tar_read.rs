//! Purpose:
//! Tar-based PHAR parsing, metadata attachment, and ustar field decoding.
//!
//! Called from:
//! - Shared archive dispatch and tar extraction operations.
//!
//! Key details:
//! - Reserved control entries are consumed as metadata and hidden from public entries.

use super::*;

/// Parses a tar-based phar into regular entries plus its global metadata and stub.
///
/// The reserved `.phar/stub.php` and `.phar/.metadata.bin` files become the stub and
/// metadata; any other `.phar/*` control file is hidden from the entry listing. OpenSSL
/// signatures are authenticated with `public_key` before any entry is exposed.
pub(super) fn parse_tar_archive_with_public_key(
    data: &[u8],
    public_key: Option<&rsa::RsaPublicKey>,
    openssl_expected: bool,
    require_hash: bool,
) -> Option<Archive> {
    verify_tar_phar_signature(data, public_key, openssl_expected, require_hash)?;
    let mut p = 0usize;
    let mut entries = Vec::new();
    let mut metadata = Vec::new();
    let mut stub = Vec::new();
    // Per-file metadata side entries may appear after their target entry; collect
    // them and attach once the full entry list is known.
    let mut file_metadata: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let mut first_header = true;
    while p.checked_add(512)? <= data.len() {
        let header = &data[p..p + 512];
        if header.iter().all(|&b| b == 0) {
            break;
        }
        // Require the POSIX ustar magic on the first record so non-tar inputs
        // (e.g. native PHARs whose stub contains `__HALT_COMPILER();`) are rejected
        // rather than mis-parsed as tar.
        if first_header && header.get(257..262) != Some(b"ustar") {
            return None;
        }
        first_header = false;
        let size = parse_tar_octal(&header[124..136])?;
        if size > MAX_PHAR_ENTRY_DECOMPRESSED_BYTES {
            return None;
        }
        let payload_start = p.checked_add(512)?;
        let payload_end = payload_start.checked_add(size)?;
        let payload = data.get(payload_start..payload_end)?;
        let typeflag = header[156];
        if typeflag == 0 || typeflag == b'0' {
            let name = tar_entry_name(header)?;
            if name == PHAR_STUB_ENTRY {
                stub = payload.to_vec();
            } else if name == PHAR_METADATA_ENTRY {
                metadata = payload.to_vec();
            } else if let Some(target) = tar_file_metadata_target(&name) {
                file_metadata.push((target, payload.to_vec()));
            } else if !is_phar_control_entry(&name) {
                entries.push(ArchiveEntry {
                    name,
                    payload: payload.to_vec(),
                    compression: PharCompression::None,
                    metadata: Vec::new(),
                });
            }
        }
        p = payload_start.checked_add(round_up_to_512(size)?)?;
    }
    for (target, meta) in file_metadata {
        if let Some(entry) = entries.iter_mut().find(|e| e.name == target) {
            entry.metadata = meta;
        }
    }
    Some(Archive {
        entries,
        format: ArchiveFormat::Tar,
        metadata,
        stub,
        signature_flag: read_tar_signature(data)
            .and_then(|payload| parse_signature_bin(&payload).map(|(flag, _)| flag)),
    })
}

/// Authenticates and scans a tar PHAR while copying only `entry`.
///
/// The complete header chain remains validated so malformed trailing records are
/// rejected, while unrelated payload bodies stay borrowed from the archive buffer.
pub(super) fn parse_tar_entry_with_public_key(
    data: &[u8],
    entry: &[u8],
    public_key: Option<&rsa::RsaPublicKey>,
    openssl_expected: bool,
    require_hash: bool,
) -> Option<Vec<u8>> {
    verify_tar_phar_signature(data, public_key, openssl_expected, require_hash)?;
    let mut p = 0usize;
    let mut first_header = true;
    let mut selected: Option<&[u8]> = None;
    while p.checked_add(512)? <= data.len() {
        let header = &data[p..p + 512];
        if header.iter().all(|&byte| byte == 0) {
            break;
        }
        if first_header && header.get(257..262) != Some(b"ustar") {
            return None;
        }
        first_header = false;
        let size = parse_tar_octal(&header[124..136])?;
        if size > MAX_PHAR_ENTRY_DECOMPRESSED_BYTES {
            return None;
        }
        let payload_start = p.checked_add(512)?;
        let payload = data.get(payload_start..payload_start.checked_add(size)?)?;
        let typeflag = header[156];
        if selected.is_none() && (typeflag == 0 || typeflag == b'0') {
            let name = tar_entry_name(header)?;
            if name == entry && !is_phar_control_entry(&name) {
                selected = Some(payload);
            }
        }
        p = payload_start.checked_add(round_up_to_512(size)?)?;
    }
    selected.map(<[u8]>::to_vec)
}

/// If `name` is a `.phar/.metadata/<path>/.metadata.bin` side entry, returns the
/// target entry path `<path>`; otherwise returns `None`.
pub(super) fn tar_file_metadata_target(name: &[u8]) -> Option<Vec<u8>> {
    let rest = name.strip_prefix(PHAR_FILE_METADATA_PREFIX)?;
    let inner = rest.strip_suffix(PHAR_FILE_METADATA_SUFFIX)?;
    if inner.is_empty() {
        return None;
    }
    Some(inner.to_vec())
}

/// Builds the full tar path from the `prefix` and `name` header fields.
pub(super) fn tar_entry_name(header: &[u8]) -> Option<Vec<u8>> {
    let name = trim_nul_and_space(header.get(0..100)?);
    let prefix = trim_nul_and_space(header.get(345..500)?);
    if prefix.is_empty() {
        Some(name.to_vec())
    } else {
        let mut out = Vec::with_capacity(prefix.len() + 1 + name.len());
        out.extend_from_slice(prefix);
        out.push(b'/');
        out.extend_from_slice(name);
        Some(out)
    }
}

/// Parses a tar octal integer field.
pub(super) fn parse_tar_octal(field: &[u8]) -> Option<usize> {
    let mut value = 0usize;
    let mut saw_digit = false;
    for &byte in field {
        if byte == 0 || byte == b' ' {
            if saw_digit {
                break;
            }
            continue;
        }
        if !(b'0'..=b'7').contains(&byte) {
            return None;
        }
        saw_digit = true;
        value = value.checked_mul(8)?.checked_add((byte - b'0') as usize)?;
    }
    Some(value)
}

/// Rounds a tar payload length up to the next 512-byte block count.
pub(super) fn round_up_to_512(len: usize) -> Option<usize> {
    len.checked_add(511).map(|n| (n / 512) * 512)
}

/// Trims a NUL-terminated, space-padded archive field.
pub(super) fn trim_nul_and_space(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let mut end = end;
    while end > 0 && bytes[end - 1] == b' ' {
        end -= 1;
    }
    &bytes[..end]
}
