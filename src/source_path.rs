//! Purpose:
//! Normalizes compiler source paths before they become PHP-visible strings or
//! EIR metadata.
//!
//! Called from:
//! - Magic-constant substitution for `__FILE__`, `__DIR__`, and closure names.
//! - EIR module construction for source-file metadata.
//!
//! Key details:
//! - Windows `std::fs::canonicalize` may add a verbatim `\\?\` prefix. PHP
//!   exposes ordinary drive or UNC paths, and verbatim paths reject the `/`
//!   separators commonly concatenated onto `__DIR__`.

use std::path::Path;

/// Canonicalizes a source path when possible and returns its PHP-display form.
pub(crate) fn canonical_source_path(path: &Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    source_path_display(&canonical)
}

/// Converts a path to a display string without Windows' internal verbatim
/// prefix, preserving ordinary drive-letter and UNC syntax.
pub(crate) fn source_path_display(path: &Path) -> String {
    let display = path.to_string_lossy();
    if cfg!(windows) {
        strip_windows_verbatim_prefix(&display)
    } else {
        display.into_owned()
    }
}

/// Removes a Windows verbatim drive or UNC prefix from a display string.
fn strip_windows_verbatim_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies drive-letter canonical paths are made suitable for PHP magic
    /// constants and later `/child` concatenation.
    #[test]
    fn strips_windows_verbatim_drive_prefix() {
        assert_eq!(
            strip_windows_verbatim_prefix(r"\\?\C:\work\app.php"),
            r"C:\work\app.php"
        );
    }

    /// Verifies verbatim UNC paths retain their ordinary leading UNC pair.
    #[test]
    fn converts_windows_verbatim_unc_prefix() {
        assert_eq!(
            strip_windows_verbatim_prefix(r"\\?\UNC\server\share\app.php"),
            r"\\server\share\app.php"
        );
    }

    /// Verifies ordinary Unix and Windows display paths are unchanged.
    #[test]
    fn preserves_non_verbatim_paths() {
        assert_eq!(strip_windows_verbatim_prefix("/tmp/app.php"), "/tmp/app.php");
        assert_eq!(
            strip_windows_verbatim_prefix(r"C:\work\app.php"),
            r"C:\work\app.php"
        );
    }
}
