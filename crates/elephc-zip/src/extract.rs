//! Purpose:
//! Safe extraction of selected archive entries into a destination directory through libzip.
//!
//! Called from:
//! - `crate::abi::elephc_zip_extract`.
//!
//! Key details:
//! - Stored names are sanitized so `..` and absolute components cannot escape the target.
//! - Directory entries create recursive empty paths and file mtimes are restored; extracted
//!   files keep the process umask's mode, matching php-src's `php_zip_extract_file`.
//! - Deleted placeholders are skipped; libzip reports them as missing on access.

use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path, PathBuf};

use crate::archive::Archive;
use crate::error::ZipError;

/// Extracts selected entries (empty selection means every entry) into `destination`.
///
/// On a failed entry, `archive.extract_failure` records its stored name so the prelude
/// can emit php-src's `Cannot extract "<entry>": "<reason>"` warning.
pub(crate) fn extract_to(archive: &mut Archive, destination: &str, selected: Option<&[Vec<u8>]>) -> Result<(), ZipError> {
    archive.extract_failure = None;
    archive.extract_warning = None;
    let root = Path::new(destination);
    if root.exists() && !root.is_dir() {
        return Err(ZipError::Invalid);
    }
    fs::create_dir_all(root).map_err(|_| ZipError::Write)?;
    let root_canonical = fs::canonicalize(root).map_err(|_| ZipError::Write)?;
    let count = archive.num_entries().max(0);
    for index in 0..count {
        // php-src's extract-all loop names entries with `ZIP_FL_UNCHANGED`: a pending
        // addition has no original name and fails the whole call, while a pending
        // rename is extracted under its original name. The selected-name path uses
        // current names, matching PHP_ZIP_STAT_PATH.
        let name = if selected.is_none() {
            archive.name(index, crate::consts::FL_UNCHANGED)
        } else {
            archive.name(index, 0)
        };
        let Some(name) = name else {
            return Err(ZipError::NoEnt);
        };
        if let Some(selected) = selected {
            if !selected.iter().any(|wanted| wanted == &name) {
                continue;
            }
        }
        let relative = safe_relative(&name);
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = root_canonical.join(&relative);
        let full_len = target.as_os_str().len();
        if full_len > MAXPATHLEN {
            archive.extract_warning = Some(format!(
                "ZipArchive::extractTo(): Full extraction path exceed MAXPATHLEN ({MAXPATHLEN})"
            ));
            return Err(ZipError::Invalid);
        }
        let mtime = archive.stat(index, 0).map(|stat| stat.mtime).unwrap_or(0);
        if name.ends_with(b"/") {
            create_safe_dirs(&root_canonical, &relative)?;
            apply_mtime(&target, mtime);
            continue;
        }
        if let Some(parent) = relative.parent() {
            create_safe_dirs(&root_canonical, parent)?;
        }
        if fs::symlink_metadata(&target)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(ZipError::Write);
        }
        // PHP 8.5.11+ treats a failed zip_fread() as a failed extraction and warns with
        // the entry name and libzip's reason. A failed zip_fopen_index() is silent: the
        // file handle never existed, so there is no stream error to report.
        let (data, crc_error) = match archive.payload_stream(index, 0) {
            Ok(payload) => payload,
            Err(error) => {
                if archive.read_opened() {
                    archive.extract_failure = Some(name);
                }
                return Err(error);
            }
        };
        if crc_error {
            archive.extract_failure = Some(name);
            return Err(ZipError::Crc);
        }
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        options.custom_flags(O_NOFOLLOW);
        let mut file = options.open(&target).map_err(|_| ZipError::Write)?;
        file.write_all(&data).map_err(|_| ZipError::Write)?;
        apply_mtime(&target, mtime);
    }
    Ok(())
}

/// Creates `relative` below `root` without following any pre-planted symlink.
///
/// `create_dir_all()` follows a symlinked component before the caller can canonicalize
/// the result, which would create directories outside the destination. Walking one
/// component at a time rejects symlinks before they are traversed.
fn create_safe_dirs(root: &Path, relative: &Path) -> Result<(), ZipError> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            continue;
        };
        let next = current.join(part);
        match fs::symlink_metadata(&next) {
            Ok(metadata) if metadata.file_type().is_symlink() => return Err(ZipError::Write),
            Ok(metadata) if metadata.is_dir() => {
                current = fs::canonicalize(&next).map_err(|_| ZipError::Write)?;
                if !current.starts_with(root) {
                    return Err(ZipError::Write);
                }
            }
            Ok(_) => return Err(ZipError::Write),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&next).map_err(|_| ZipError::Write)?;
                current = next;
            }
            Err(_) => return Err(ZipError::Write),
        }
    }
    Ok(())
}

/// Strips absolute roots, `.`/`..` traversal, and drive prefixes from a stored name.
fn safe_relative(name: &[u8]) -> PathBuf {
    let text = String::from_utf8_lossy(name);
    let mut relative = PathBuf::new();
    for component in Path::new(text.as_ref()).components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            // Resolve `..` against the accumulated path like php_zip_make_relative_path,
            // but never above the destination root. Dropping the component outright would
            // extract `sub/../../x` as `sub/x` instead of PHP's `x`.
            Component::ParentDir => {
                relative.pop();
            }
            Component::RootDir | Component::Prefix(_) => {}
        }
    }
    relative
}

/// `MAXPATHLEN` values for the supported Unix targets, matching the C header
/// php-src compiles against so the length limit and warning text agree.
#[cfg(target_os = "linux")]
const MAXPATHLEN: usize = 4096;
#[cfg(any(target_os = "macos", target_os = "ios"))]
const MAXPATHLEN: usize = 1024;

/// `O_NOFOLLOW` values for the supported Unix targets; an extraction never follows a
/// final-component symlink, which would let an archive write through a pre-planted link.
#[cfg(target_os = "linux")]
const O_NOFOLLOW: i32 = 0o400000;
#[cfg(any(target_os = "macos", target_os = "ios"))]
const O_NOFOLLOW: i32 = 0x100;

/// Applies an exact mtime without depending on the platform's timestamp API surface.
fn apply_mtime(path: &Path, timestamp: i64) {
    let _ = crate::clock::set_mtime(path, timestamp);
}

/// Selects the requested entries from a JSON-encoded name list.
pub(crate) fn parse_selection(json: &[u8]) -> Option<Vec<Vec<u8>>> {
    let text = std::str::from_utf8(json).ok()?;
    if text.is_empty() || text == "null" {
        return None;
    }
    let values: Vec<String> = serde_json::from_str(text).ok()?;
    Some(values.into_iter().map(String::into_bytes).collect())
}
