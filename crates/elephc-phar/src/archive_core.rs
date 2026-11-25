//! Purpose:
//! Container-family dispatch and shared archive rebuild orchestration.
//!
//! Called from:
//! - Rust-facing PHAR operations and compression/signature helpers.
//!
//! Key details:
//! - Existing archives keep their family; new paths select it from the extension.

use super::*;

/// Parses archive bytes into decoded entries and reports the archive family.
#[cfg(test)]
pub(super) fn parse_archive_entries(data: &[u8]) -> Option<(Vec<ArchiveEntry>, ArchiveFormat)> {
    parse_archive(data).map(|archive| (archive.entries, archive.format))
}

/// Selects the archive family for a missing output path.
pub(super) fn format_for_new_archive_path(path: &std::path::Path) -> ArchiveFormat {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("tar") => ArchiveFormat::Tar,
        Some(ext) if ext.eq_ignore_ascii_case("zip") => ArchiveFormat::Zip,
        _ => ArchiveFormat::NativePhar,
    }
}

/// Builds an archive in the selected output family.
///
/// `signature_flag` preserves the source signature algorithm across implicit
/// rebuilds: `Some(hash flag)` re-signs with that algorithm, `None` keeps the
/// archive unsigned, and an OpenSSL flag cannot be reproduced without a key.
pub(super) fn build_archive(
    entries: &[ArchiveEntry],
    format: ArchiveFormat,
    metadata: &[u8],
    stub: &[u8],
    signature_flag: Option<u32>,
) -> Option<Vec<u8>> {
    match format {
        ArchiveFormat::NativePhar => {
            build_native_phar_archive(entries, metadata, stub, signature_flag)
        }
        ArchiveFormat::Tar => match signature_flag {
            Some(flag) => sign_tar_archive(entries, metadata, stub, flag, None),
            None => build_tar_archive(entries, metadata, stub),
        },
        ArchiveFormat::Zip => match signature_flag {
            Some(flag) => sign_zip_archive(entries, metadata, stub, flag, None),
            None => build_zip_archive(entries, metadata, stub),
        },
    }
}

/// Rebuilds an [`Archive`] into serialized bytes, preserving its metadata, stub,
/// and hash signature algorithm (or its unsigned state).
pub(super) fn build_archive_value(archive: &Archive) -> Option<Vec<u8>> {
    build_archive(
        &archive.entries,
        archive.format,
        &archive.metadata,
        &archive.stub,
        archive.signature_flag,
    )
}
