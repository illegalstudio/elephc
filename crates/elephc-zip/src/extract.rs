//! Purpose:
//! Safe extraction of selected archive entries into a destination directory.
//!
//! Called from:
//! - `crate::abi::elephc_zip_extract`.
//!
//! Key details:
//! - Stored names are sanitized so `..` and absolute components cannot escape the target.
//! - Directory entries create recursive empty paths, and file mtimes are restored.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::error::ZipError;
use crate::model::Archive;

/// Extracts selected entries (empty selection means every entry) into `destination`.
pub(crate) fn extract_to(archive: &mut Archive, destination: &str, selected: Option<&[Vec<u8>]>) -> Result<(), ZipError> {
    let root = Path::new(destination);
    if root.exists() && !root.is_dir() {
        return Err(ZipError::Invalid);
    }
    fs::create_dir_all(root).map_err(|_| ZipError::Write)?;
    for index in 0..archive.entries.len() {
        let entry = archive.entries[index].clone();
        if entry.deleted {
            continue;
        }
        if let Some(selected) = selected {
            if !selected.iter().any(|name| name == &entry.name) {
                continue;
            }
        }
        let relative = safe_relative(&entry.name);
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = root.join(&relative);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|_| ZipError::Write)?;
            apply_mtime(&target, entry.mtime);
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|_| ZipError::Write)?;
        }
        let data = archive.payload(index, 0, None).map_err(|error| error)?;
        fs::write(&target, data).map_err(|_| ZipError::Write)?;
        apply_mtime(&target, entry.mtime);
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
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {}
        }
    }
    relative
}

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
