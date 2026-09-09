//! Purpose:
//! Extracts verified tar.gz and tar.xz sources under strict traversal, type, count, path, and
//! size bounds.
//!
//! Called from:
//! - Curated native recipes after source SHA verification.
//!
//! Key details:
//! - Exactly one top-level component is stripped and every link or special entry is rejected.
//! - Both container formats share one bounded tar entry loop. gzip is streamed straight into it;
//!   xz is first inflated into a sibling temporary of the destination through a size-capped
//!   writer, so an inflating stream stops at the bound instead of filling the disk.

use std::collections::HashSet;
use std::fs::{self, FileTimes, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use flate2::read::GzDecoder;

use super::catalog::ArchiveFormat;
use super::error::{NativeError, NativeErrorKind};
use super::util::unique_sibling;

/// The single modification time every extracted regular file is stamped with: midnight UTC
/// on 2020-01-01, as seconds since the epoch.
///
/// WHY EVERY FILE GETS THE SAME ONE. Extraction writes files in tar order and never applies
/// the header's mtime, so each file's timestamp is the moment it happened to be written —
/// which makes the RELATIVE order of any two files a function of their position in the
/// archive. For an autotools release that is actively wrong: upstream ships `aclocal.m4`
/// NEWER than `configure.ac`/`m4/*.m4` precisely so the regeneration rules stay dormant,
/// but `aclocal.m4` sorts near the front of the tar and `configure.ac`/`m4/` near the back,
/// so extraction INVERTS the relationship. `make` then believes `aclocal.m4` is stale, runs
/// `am--refresh`, and demands `aclocal-1.16` — a hard failure on any machine without
/// automake, which is every CI runner and container this project builds on.
///
/// Stamping one identical value fixes the whole class rather than one package: GNU make
/// rebuilds a target only when a prerequisite is STRICTLY newer, so an all-equal tree can
/// never trigger a regeneration rule, whatever the package's build system.
///
/// The value is a FIXED PAST instant, not `now()`, for two reasons. It is reproducible —
/// two extractions of the same verified tarball are byte-for-byte AND timestamp-for-
/// timestamp identical. And it is safely older than anything `configure` generates during
/// the build (`config.status`, `Makefile`, `config.h`), so generated outputs are always
/// strictly newer than their sources and no build can decide to regenerate them either.
const STAGED_SOURCE_MTIME: Duration = Duration::from_secs(1_577_836_800);

const MAX_ENTRIES: u64 = 50_000;
const MAX_EXPANDED: u64 = 256 * 1024 * 1024;
const MAX_PATH_BYTES: usize = 4_096;
const MAX_FILE: u64 = 64 * 1024 * 1024;
const MAX_RATIO: u64 = 100;
/// Worst-case tar container bytes per entry beyond its file content: one 512-byte header block
/// plus at most 511 bytes of padding to the next block boundary.
const TAR_ENTRY_OVERHEAD: u64 = 1024;
/// The two zero blocks that terminate a tar stream.
const TAR_TRAILER: u64 = 1024;

/// Extracts a verified compressed tar of the catalogued format to an empty destination and
/// returns it.
pub fn extract_archive(archive_path: &Path, format: ArchiveFormat, destination: &Path) -> Result<PathBuf, NativeError> {
    if destination.exists() {
        return Err(NativeError::new(NativeErrorKind::Archive, "archive destination already exists").with_path(destination));
    }
    fs::create_dir_all(destination).map_err(|error| NativeError::io("create archive extraction root", destination, error))?;
    let compressed_size = fs::metadata(archive_path).map_err(|error| NativeError::io("inspect source archive", archive_path, error))?.len();
    if compressed_size == 0 {
        return Err(NativeError::new(NativeErrorKind::Archive, "source archive is empty").with_path(archive_path));
    }
    let file = fs::File::open(archive_path).map_err(|error| NativeError::io("open source archive", archive_path, error))?;
    match format {
        ArchiveFormat::TarGz => extract_tar(GzDecoder::new(file), archive_path, compressed_size, destination)?,
        ArchiveFormat::TarXz => {
            let inflated = inflate_xz(file, archive_path, compressed_size, destination)?;
            let result = fs::File::open(&inflated)
                .map_err(|error| NativeError::io("open inflated xz source", &inflated, error))
                .and_then(|tar| extract_tar(BufReader::new(tar), archive_path, compressed_size, destination));
            let _ = fs::remove_file(&inflated);
            result?;
        }
    }
    Ok(destination.to_path_buf())
}

/// Inflates an xz source into a sibling temporary of the destination, capped at
/// [`xz_stream_limit`], and returns the temporary's path. The temporary is removed on failure.
///
/// Bound honesty: the writer cap is the DISK bound. `lzma_rs::xz_decompress` decodes each xz
/// block into memory before handing it to the writer, so peak memory during inflation is one
/// block's uncompressed size; the catalog's exact SHA-256 pin (verified before extraction ever
/// runs) is what keeps that block from being adversarial.
fn inflate_xz(file: fs::File, archive_path: &Path, compressed_size: u64, destination: &Path) -> Result<PathBuf, NativeError> {
    let temporary = unique_sibling(destination, "xz");
    let output = OpenOptions::new().write(true).create_new(true).open(&temporary)
        .map_err(|error| NativeError::io("create xz inflation temporary file", &temporary, error))?;
    let result = (|| {
        let mut writer = BoundedWriter::new(BufWriter::new(output), xz_stream_limit(compressed_size));
        let mut reader = BufReader::new(file);
        lzma_rs::xz_decompress(&mut reader, &mut writer)
            .map_err(|error| archive_error(archive_path, format!("cannot inflate xz source: {error}")))?;
        writer.flush().map_err(|error| NativeError::io("flush xz inflation temporary file", &temporary, error))
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(temporary)
}

/// Upper bound on the raw tar stream an xz source may inflate to: the file-content cap the entry
/// loop enforces (`MAX_EXPANDED`, tightened by the compression-ratio bound against the actual
/// compressed size) plus the tar container's own per-entry overhead and trailer. Anything past
/// this could never pass the entry loop, so it is refused before it reaches disk.
fn xz_stream_limit(compressed_size: u64) -> u64 {
    let content = MAX_EXPANDED.min(compressed_size.saturating_mul(MAX_RATIO));
    content
        .saturating_add(MAX_ENTRIES.saturating_mul(TAR_ENTRY_OVERHEAD))
        .saturating_add(TAR_TRAILER)
}

/// A `Write` adapter that fails with `InvalidData` once more than `limit` bytes would pass
/// through, so an inflating decompressor stops at the bound instead of filling the disk.
struct BoundedWriter<W: Write> {
    inner: W,
    written: u64,
    limit: u64,
}

impl<W: Write> BoundedWriter<W> {
    /// Wraps `inner`, allowing at most `limit` bytes in total.
    fn new(inner: W, limit: u64) -> Self {
        Self { inner, written: 0, limit }
    }
}

impl<W: Write> Write for BoundedWriter<W> {
    /// Forwards the buffer unless it would push the running total past the limit.
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let next = self.written.saturating_add(buffer.len() as u64);
        if next > self.limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("inflated archive stream exceeds {} byte bound", self.limit),
            ));
        }
        let written = self.inner.write(buffer)?;
        self.written = self.written.saturating_add(written as u64);
        Ok(written)
    }

    /// Flushes the wrapped writer.
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Runs the shared bounded tar entry loop over an already-decompressed tar byte stream.
///
/// `compressed_size` is the on-disk size of the ORIGINAL compressed archive, whatever the
/// container, so the compression-ratio bound means the same thing for both formats.
fn extract_tar<R: Read>(reader: R, archive_path: &Path, compressed_size: u64, destination: &Path) -> Result<(), NativeError> {
    let mut archive = tar::Archive::new(reader);
    let mut root: Option<std::ffi::OsString> = None;
    let mut seen = HashSet::new();
    let mut entry_count = 0_u64;
    let mut expanded = 0_u64;

    let entries = archive.entries().map_err(|error| archive_error(archive_path, format!("cannot read tar entries: {error}")))?;
    for entry in entries {
        let mut entry = entry.map_err(|error| archive_error(archive_path, format!("cannot read tar entry: {error}")))?;
        entry_count = entry_count.checked_add(1).ok_or_else(|| archive_error(archive_path, "entry count overflow"))?;
        if entry_count > MAX_ENTRIES {
            return Err(archive_error(archive_path, format!("archive exceeds {MAX_ENTRIES} entries")));
        }
        // A PAX global extended header (typeflag 'g', synthetic name `pax_global_header`) carries
        // only archive-wide metadata (e.g. `git archive`'s embedded commit comment) and is not a
        // real path; some upstream releases (e.g. OpenSSL's) are built with `git archive` and
        // include one before the real top-level directory entry.
        if entry.header().entry_type() == tar::EntryType::XGlobalHeader {
            continue;
        }
        let original = entry.path().map_err(|error| archive_error(archive_path, format!("invalid tar path: {error}")))?.into_owned();
        let relative = stripped_path(&original, &mut root)?;
        if relative.as_os_str().is_empty() {
            if !entry.header().entry_type().is_dir() {
                return Err(archive_error(&original, "top-level archive root must be a directory"));
            }
            continue;
        }
        if !seen.insert(relative.clone()) {
            return Err(archive_error(&original, "duplicate archive path"));
        }
        let entry_type = entry.header().entry_type();
        let mode = entry.header().mode().map_err(|error| archive_error(&original, format!("invalid entry mode: {error}")))?;
        if mode & 0o6000 != 0 {
            return Err(archive_error(&original, "setuid and setgid archive modes are forbidden"));
        }
        let output = destination.join(&relative);
        if entry_type.is_dir() {
            fs::create_dir_all(&output).map_err(|error| NativeError::io("create extracted directory", &output, error))?;
            set_safe_mode(&output, mode)?;
            continue;
        }
        if !entry_type.is_file() {
            return Err(archive_error(&original, "archive links and special entries are forbidden"));
        }
        let size = entry.header().size().map_err(|error| archive_error(&original, format!("invalid file size: {error}")))?;
        if size > MAX_FILE {
            return Err(archive_error(&original, format!("file exceeds {MAX_FILE} expanded bytes")));
        }
        expanded = expanded.checked_add(size).ok_or_else(|| archive_error(&original, "expanded size overflow"))?;
        if expanded > MAX_EXPANDED || expanded > compressed_size.saturating_mul(MAX_RATIO) {
            return Err(archive_error(&original, "archive exceeds total expanded-size or compression-ratio bound"));
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|error| NativeError::io("create extracted file parent", parent, error))?;
        }
        let mut file = OpenOptions::new().write(true).create_new(true).open(&output)
            .map_err(|error| NativeError::io("create extracted regular file", &output, error))?;
        let copied = io::copy(&mut entry.by_ref().take(size + 1), &mut file)
            .map_err(|error| NativeError::io("extract regular file", &output, error))?;
        if copied != size {
            return Err(archive_error(&original, format!("tar file length mismatch: header {size}, stream {copied}")));
        }
        file.flush().map_err(|error| NativeError::io("flush extracted regular file", &output, error))?;
        set_uniform_time(&file, &output)?;
        drop(file);
        // `chmod` moves ctime, never mtime, so the stamp above survives this.
        set_safe_mode(&output, mode)?;
    }
    if root.is_none() {
        return Err(archive_error(archive_path, "archive contains no entries"));
    }
    Ok(())
}

/// Stamps one extracted regular file with [`STAGED_SOURCE_MTIME`].
///
/// Only regular files are stamped. Directory mtimes are not load-bearing for any build
/// system's staleness rules, and every directory's mtime moves anyway the moment the build
/// writes its first object file into the tree, so normalizing them would buy nothing.
fn set_uniform_time(file: &fs::File, path: &Path) -> Result<(), NativeError> {
    let stamp = UNIX_EPOCH + STAGED_SOURCE_MTIME;
    let times = FileTimes::new().set_accessed(stamp).set_modified(stamp);
    file.set_times(times)
        .map_err(|error| NativeError::io("stamp extracted regular file", path, error))
}

/// Preserves ordinary executable/read/write bits while excluding privilege-elevation bits.
fn set_safe_mode(path: &Path, mode: u32) -> Result<(), NativeError> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o777))
            .map_err(|error| NativeError::io("set safe extracted permissions", path, error))?;
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
    Ok(())
}

/// Validates and strips the archive's one common top-level directory component.
fn stripped_path(path: &Path, root: &mut Option<std::ffi::OsString>) -> Result<PathBuf, NativeError> {
    if path.is_absolute() || path.to_string_lossy().as_bytes().len() > MAX_PATH_BYTES {
        return Err(archive_error(path, "archive path is absolute or too long"));
    }
    let mut components = path.components();
    let first = match components.next() {
        Some(Component::Normal(value)) => value.to_os_string(),
        _ => return Err(archive_error(path, "archive path must begin with one normal root component")),
    };
    if let Some(expected) = root {
        if expected != &first {
            return Err(archive_error(path, "archive has more than one top-level root"));
        }
    } else {
        *root = Some(first);
    }
    let mut relative = PathBuf::new();
    for component in components {
        match component {
            Component::Normal(value) => relative.push(value),
            _ => return Err(archive_error(path, "archive path contains traversal or platform prefix")),
        }
    }
    Ok(relative)
}

/// Creates an archive-category failure naming the offending entry.
fn archive_error(path: &Path, message: impl Into<String>) -> NativeError {
    NativeError::new(NativeErrorKind::Archive, message).with_path(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Creates a unique extraction fixture root.
    fn fixture(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("elephc-archive-{label}-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()))
    }

    /// Serializes one controlled tar entry (path, type, mode) into an in-memory tar stream.
    fn tar_bytes(entries: &[(&str, tar::EntryType, u32)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (entry_path, entry_type, mode) in entries {
            let bytes = b"fixture";
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(*entry_type);
            header.set_size(if entry_type.is_file() { bytes.len() as u64 } else { 0 });
            header.set_mode(*mode);
            header.set_cksum();
            builder.append_data(&mut header, entry_path, &bytes[..if entry_type.is_file() { bytes.len() } else { 0 }]).unwrap();
        }
        builder.into_inner().unwrap()
    }

    /// Builds a tiny gzip tar with one controlled entry type and path.
    fn write_tar_mode(path: &Path, entry_path: &str, entry_type: tar::EntryType, mode: u32) {
        let file = fs::File::create(path).unwrap();
        let mut encoder = GzEncoder::new(file, Compression::default());
        encoder.write_all(&tar_bytes(&[(entry_path, entry_type, mode)])).unwrap();
        encoder.finish().unwrap();
    }

    /// Builds a normal non-executable tiny archive entry.
    fn write_tar(path: &Path, entry_path: &str, entry_type: tar::EntryType) {
        write_tar_mode(path, entry_path, entry_type, 0o644);
    }

    /// Wraps `bytes` in a single-stream xz container in-process with `lzma_rs::xz_compress`
    /// (one block of uncompressed LZMA2 chunks, no check), so the xz tests run everywhere —
    /// including the Alpine Docker images, which ship no `xz` CLI — instead of skipping.
    fn xz_bytes(mut bytes: &[u8]) -> Vec<u8> {
        let mut output = Vec::new();
        lzma_rs::xz_compress(&mut bytes, &mut output).unwrap();
        output
    }

    /// Compresses a tar stream of controlled entries into `path` as an xz container.
    fn write_tar_xz(path: &Path, entries: &[(&str, tar::EntryType, u32)]) {
        fs::write(path, xz_bytes(&tar_bytes(entries))).unwrap();
    }

    /// Lists the fixture root's entry names, to prove inflation temporaries were cleaned up.
    fn entry_names(root: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(root).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    /// EVERY EXTRACTED FILE CARRIES THE SAME MODIFICATION TIME, whatever the archive said
    /// and whatever order the entries came in.
    ///
    /// This is the regression guard for a real CI outage. Extraction applies no header
    /// mtime, so before this each file inherited the instant it was written — making the
    /// relative age of any two files a function of their tar position. An autotools release
    /// ships `aclocal.m4` deliberately NEWER than `configure.ac` and `m4/*.m4` so the
    /// regeneration rules stay dormant, but `aclocal.m4` sits near the FRONT of the tar and
    /// `configure.ac`/`m4/` near the BACK, so extraction inverted exactly the relationship
    /// upstream set up. `make` then ran `am--refresh` and demanded `aclocal-1.16`, which no
    /// CI runner or container here has: nghttp2 failed to build on all three platforms.
    ///
    /// The fixture below mirrors that shape — a front entry and a back entry, with the
    /// front one deliberately given the NEWER header mtime, i.e. the upstream arrangement —
    /// and asserts the extracted tree flattens both to one value. Equal mtimes are enough:
    /// GNU make rebuilds only on a STRICTLY newer prerequisite.
    #[test]
    fn extraction_flattens_every_file_mtime() {
        let root = fixture("uniform-mtime");
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("a.tar.gz");
        let file = fs::File::create(&archive).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = tar::Builder::new(encoder);
        // Front of the archive, newest upstream mtime — `aclocal.m4`'s role.
        for (name, mtime) in [("root/aclocal.m4", 2_000_000_100_u64), ("root/m4/generated.m4", 2_000_000_000)] {
            let bytes = b"fixture";
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_mtime(mtime);
            header.set_cksum();
            builder.append_data(&mut header, name, &bytes[..]).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();

        let output = root.join("out");
        extract_archive(&archive, ArchiveFormat::TarGz, &output).unwrap();

        let expected = UNIX_EPOCH + STAGED_SOURCE_MTIME;
        let first = fs::metadata(output.join("aclocal.m4")).unwrap().modified().unwrap();
        let second = fs::metadata(output.join("m4/generated.m4")).unwrap().modified().unwrap();
        assert_eq!(first, expected, "extracted files must carry the fixed staging stamp");
        assert_eq!(second, expected, "extracted files must carry the fixed staging stamp");
        // The property the build systems actually depend on: neither file is newer.
        assert_eq!(first, second);
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies a leading `git archive`-style PAX global extended header (as shipped in
    /// OpenSSL's real release tarball) is skipped rather than rejected as a non-directory root.
    #[test]
    fn skips_leading_pax_global_header_entry() {
        let root = fixture("pax-global");
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("a.tar.gz");
        let file = fs::File::create(&archive).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let pax_body = b"52 comment=8cf17aaeb4599f8af87fefd810b5b5fee90fe69e\n";
        let mut pax_header = tar::Header::new_ustar();
        pax_header.set_entry_type(tar::EntryType::XGlobalHeader);
        pax_header.set_size(pax_body.len() as u64);
        pax_header.set_mode(0o666);
        pax_header.set_cksum();
        builder.append_data(&mut pax_header, "pax_global_header", &pax_body[..]).unwrap();
        let mut file_header = tar::Header::new_gnu();
        file_header.set_entry_type(tar::EntryType::Regular);
        file_header.set_size(7);
        file_header.set_mode(0o644);
        file_header.set_cksum();
        builder.append_data(&mut file_header, "root/file.txt", &b"fixture"[..]).unwrap();
        builder.into_inner().unwrap().finish().unwrap();

        let output = root.join("out");
        extract_archive(&archive, ArchiveFormat::TarGz, &output).unwrap();
        assert_eq!(fs::read(output.join("file.txt")).unwrap(), b"fixture");
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies a normal single-root archive is stripped and extracted.
    #[test]
    fn extracts_regular_single_root_archive() {
        let root = fixture("ok");
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("a.tar.gz");
        write_tar(&archive, "root/file.txt", tar::EntryType::Regular);
        let output = root.join("out");
        extract_archive(&archive, ArchiveFormat::TarGz, &output).unwrap();
        assert_eq!(fs::read(output.join("file.txt")).unwrap(), b"fixture");
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies symlink and traversal entries fail without escaping staging.
    #[test]
    fn rejects_links_and_parent_paths() {
        let root = fixture("bad");
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("link.tar.gz");
        write_tar(&archive, "root/link", tar::EntryType::Symlink);
        assert!(extract_archive(&archive, ArchiveFormat::TarGz, &root.join("out-link")).is_err());
        let mut archive_root = None;
        assert!(stripped_path(Path::new("root/../escape"), &mut archive_root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies executable helpers retain owner execute while setuid/setgid modes fail closed.
    #[test]
    #[cfg(unix)]
    fn preserves_safe_executable_mode_and_rejects_privileged_mode() {
        let root = fixture("mode");
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("exec.tar.gz");
        write_tar_mode(&archive, "root/configure", tar::EntryType::Regular, 0o755);
        let output = root.join("out");
        extract_archive(&archive, ArchiveFormat::TarGz, &output).unwrap();
        assert_eq!(fs::metadata(output.join("configure")).unwrap().permissions().mode() & 0o777, 0o755);
        let privileged = root.join("privileged.tar.gz");
        write_tar_mode(&privileged, "root/tool", tar::EntryType::Regular, 0o4755);
        assert!(extract_archive(&privileged, ArchiveFormat::TarGz, &root.join("bad")).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies an xz source (the container libxml2 ships in) goes through the same
    /// root-stripping entry loop as gzip: files land at their stripped paths with their safe
    /// modes and the fixed staging stamp, and the inflation temporary next to the destination
    /// is gone afterwards so a recipe's staging tree stays exact.
    #[test]
    #[cfg(unix)]
    fn extracts_xz_single_root_archive() {
        let root = fixture("xz-ok");
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("a.tar.xz");
        write_tar_xz(&archive, &[("root/file.txt", tar::EntryType::Regular, 0o644), ("root/sub/configure", tar::EntryType::Regular, 0o755)]);
        let output = root.join("out");
        extract_archive(&archive, ArchiveFormat::TarXz, &output).unwrap();
        assert_eq!(fs::read(output.join("file.txt")).unwrap(), b"fixture");
        assert_eq!(fs::metadata(output.join("sub/configure")).unwrap().permissions().mode() & 0o777, 0o755);
        assert_eq!(fs::metadata(output.join("file.txt")).unwrap().modified().unwrap(), UNIX_EPOCH + STAGED_SOURCE_MTIME);
        assert_eq!(entry_names(&root), vec!["a.tar.xz".to_string(), "out".to_string()]);
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies the xz path shares the gzip entry bounds: a symlink entry is refused with the
    /// same diagnostic, and the failure leaves no inflation temporary behind.
    #[test]
    fn xz_extraction_shares_entry_bounds_and_cleans_up() {
        let root = fixture("xz-link");
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("link.tar.xz");
        write_tar_xz(&archive, &[("root/link", tar::EntryType::Symlink, 0o644)]);
        let error = extract_archive(&archive, ArchiveFormat::TarXz, &root.join("out")).unwrap_err();
        assert_eq!(error.kind, NativeErrorKind::Archive);
        assert!(error.to_string().contains("archive links and special entries are forbidden"), "{error}");
        assert_eq!(entry_names(&root), vec!["link.tar.xz".to_string(), "out".to_string()]);
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies an xz stream that inflates PAST THE CAP is refused inside `inflate_xz` as an
    /// archive failure naming the bound, and leaves no inflation temporary behind. The decoder
    /// hands the writer a whole block at once, so the bounded writer's refusal is the only
    /// thing between the stream and the disk — this is the path that keeps an inflating
    /// source from filling it.
    ///
    /// The cap never drops below the container-overhead allowance (`MAX_ENTRIES` headers plus
    /// the trailer, about 51 MB, which `compressed_size` 0 leaves as the whole bound), so the
    /// fixture has to be that big: one byte more than the bound, streamed from `io::repeat`
    /// into a single uncompressed-chunk block. Nothing beyond the fixture file itself reaches
    /// disk, because the first (and only) write is the one refused.
    #[test]
    fn over_limit_xz_stream_is_refused_without_temporary() {
        let root = fixture("xz-over-limit");
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("huge.tar.xz");
        let limit = xz_stream_limit(0);
        let mut zeros = BufReader::new(io::repeat(0).take(limit + 1));
        let mut stream = Vec::new();
        lzma_rs::xz_compress(&mut zeros, &mut stream).unwrap();
        fs::write(&archive, stream).unwrap();
        let file = fs::File::open(&archive).unwrap();
        let error = inflate_xz(file, &archive, 0, &root.join("out")).unwrap_err();
        assert_eq!(error.kind, NativeErrorKind::Archive);
        assert!(error.to_string().contains("cannot inflate xz source"), "{error}");
        assert!(error.to_string().contains(&format!("exceeds {limit} byte bound")), "{error}");
        assert_eq!(entry_names(&root), vec!["huge.tar.xz".to_string()]);
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies a stream that stays within the cap inflates through `inflate_xz` into a
    /// sibling temporary of the destination holding exactly the original bytes.
    #[test]
    fn within_limit_xz_stream_inflates_into_sibling_temporary() {
        let root = fixture("xz-within-limit");
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("small.tar.xz");
        let payload = tar_bytes(&[("root/file.txt", tar::EntryType::Regular, 0o644)]);
        fs::write(&archive, xz_bytes(&payload)).unwrap();
        let file = fs::File::open(&archive).unwrap();
        let temporary = inflate_xz(file, &archive, fs::metadata(&archive).unwrap().len(), &root.join("out")).unwrap();
        assert_eq!(temporary.parent().unwrap(), root.as_path());
        assert_eq!(fs::read(&temporary).unwrap(), payload);
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies bytes that are not an xz stream are rejected as an archive failure naming the
    /// source, without leaving the inflation temporary next to the destination.
    #[test]
    fn corrupt_xz_source_is_rejected_without_temporary() {
        let root = fixture("xz-corrupt");
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("corrupt.tar.xz");
        fs::write(&archive, b"this is not an xz stream at all").unwrap();
        let error = extract_archive(&archive, ArchiveFormat::TarXz, &root.join("out")).unwrap_err();
        assert_eq!(error.kind, NativeErrorKind::Archive);
        assert!(error.to_string().contains("cannot inflate xz source"), "{error}");
        assert_eq!(entry_names(&root), vec!["corrupt.tar.xz".to_string(), "out".to_string()]);
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies the gzip decoder is never handed an xz stream by accident: the format is the
    /// caller's explicit choice, so mislabelling fails closed instead of extracting garbage.
    #[test]
    fn mislabelled_container_fails_closed() {
        let root = fixture("mislabel");
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("a.tar.gz");
        write_tar(&archive, "root/file.txt", tar::EntryType::Regular);
        assert!(extract_archive(&archive, ArchiveFormat::TarXz, &root.join("out")).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies the size-capped writer forwards bytes up to its bound and refuses the write that
    /// would cross it, which is what stops an inflating xz stream before it fills the disk.
    #[test]
    fn bounded_writer_refuses_bytes_past_its_limit() {
        let mut writer = BoundedWriter::new(Vec::new(), 4);
        writer.write_all(b"abc").unwrap();
        let error = writer.write_all(b"de").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("exceeds 4 byte bound"));
        writer.write_all(b"d").unwrap();
        assert_eq!(writer.inner, b"abcd");
        assert!(writer.write_all(b"e").is_err());
    }

    /// Verifies the xz stream cap tracks the entry loop's own bounds: the ratio bound for a
    /// small compressed source, the absolute expanded cap for a large one, plus container
    /// overhead in both cases.
    #[test]
    fn xz_stream_limit_tracks_content_bounds() {
        let overhead = MAX_ENTRIES * TAR_ENTRY_OVERHEAD + TAR_TRAILER;
        assert_eq!(xz_stream_limit(1024 * 1024), 100 * 1024 * 1024 + overhead);
        assert_eq!(xz_stream_limit(3 * 1024 * 1024), MAX_EXPANDED + overhead);
        assert_eq!(xz_stream_limit(u64::MAX), MAX_EXPANDED + overhead);
    }
}
