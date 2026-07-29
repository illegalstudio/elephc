//! Purpose:
//! Shares filesystem validation and retained-output copying across curated native recipes.
//!
//! Called from:
//! - `crate::native_deps::recipes::pcre2` and `crate::native_deps::recipes::zlib`.
//!
//! Key details:
//! - Rejects empty, symlinked, or non-regular files while preserving package-specific diagnostics.

use std::fs;
use std::path::{Path, PathBuf};

use super::super::error::{NativeError, NativeErrorKind};

/// Requires a non-empty, non-symlink regular file produced by a trusted recipe.
pub(super) fn require_regular(package: &str, path: &Path) -> Result<(), NativeError> {
    let action = format!("inspect {package} recipe file");
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| NativeError::io(&action, path, error))?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
    {
        return Err(NativeError::new(
            NativeErrorKind::Build,
            format!("{package} recipe file is missing, empty, symlinked, or not regular"),
        )
        .with_path(path));
    }
    Ok(())
}

/// Copies one verified regular recipe output to its retained staging path.
pub(super) fn copy_regular(
    package: &str,
    source: &Path,
    destination: &Path,
) -> Result<PathBuf, NativeError> {
    require_regular(package, source)?;
    let action = format!("copy retained {package} output");
    fs::copy(source, destination)
        .map_err(|error| NativeError::io(&action, destination, error))?;
    require_regular(package, destination)?;
    Ok(destination.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_deps::util::unique_sibling;

    /// Verifies a non-empty regular file is copied and revalidated.
    #[test]
    fn copies_non_empty_regular_file() {
        let directory = unique_sibling(
            &std::env::temp_dir().join("elephc-recipe-util"),
            "test",
        );
        fs::create_dir_all(&directory).unwrap();
        let source = directory.join("source.a");
        let destination = directory.join("destination.a");
        fs::write(&source, b"archive").unwrap();

        let copied = copy_regular("fixture", &source, &destination).unwrap();

        assert_eq!(copied, destination);
        assert_eq!(fs::read(&copied).unwrap(), b"archive");
        fs::remove_dir_all(directory).unwrap();
    }

    /// Verifies empty files and directories fail the shared recipe invariant.
    #[test]
    fn rejects_empty_and_non_regular_files() {
        let directory = unique_sibling(
            &std::env::temp_dir().join("elephc-recipe-util"),
            "test",
        );
        fs::create_dir_all(&directory).unwrap();
        let empty = directory.join("empty.a");
        let nested = directory.join("directory");
        fs::write(&empty, []).unwrap();
        fs::create_dir(&nested).unwrap();

        let empty_error = require_regular("fixture", &empty).unwrap_err();
        let directory_error = require_regular("fixture", &nested).unwrap_err();

        let expected = "fixture recipe file is missing, empty, symlinked, or not regular";
        assert!(empty_error.to_string().contains(expected));
        assert!(directory_error.to_string().contains(expected));
        fs::remove_dir_all(directory).unwrap();
    }
}
