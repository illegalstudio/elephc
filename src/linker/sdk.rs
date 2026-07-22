//! Purpose:
//! Resolves macOS SDK paths, deployment versions, and optional library search paths.
//! Keeps host-tool probing out of the pure linker command renderer.
//!
//! Called from:
//! - `crate::linker` while preparing a macOS link invocation.
//!
//! Key details:
//! - Missing SDK tooling produces an actionable diagnostic before `ld` is invoked.
//! - Homebrew paths are supplied only when the typed plan contains named libraries.

use std::path::Path;
use std::process::{self, Command};

/// Returns the selected macOS SDK path or exits with an actionable diagnostic.
pub(super) fn macos_sdk_path() -> String {
    let resolved = Command::new("xcrun")
        .args(["--show-sdk-path"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default();
    match validate_macos_sdk_path(&resolved) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("{message}");
            process::exit(1);
        }
    }
}

/// Validates a resolved macOS SDK path without performing I/O or exiting.
fn validate_macos_sdk_path(resolved: &str) -> Result<String, String> {
    let trimmed = resolved.trim();
    if trimmed.is_empty() {
        return Err(
            "Could not locate the macOS SDK. Install the Xcode Command Line Tools \
             (run: xcode-select --install) and make sure `xcrun --show-sdk-path` prints a valid path."
                .to_string(),
        );
    }
    Ok(trimmed.to_string())
}

/// Returns common existing Homebrew library directories in stable preference order.
pub(super) fn default_macos_library_paths() -> Vec<&'static str> {
    ["/opt/homebrew/lib", "/usr/local/lib"]
        .into_iter()
        .filter(|path| Path::new(path).exists())
        .collect()
}

/// Returns the selected macOS SDK version, with the existing `15.0` fallback.
pub(super) fn macos_sdk_version() -> String {
    match Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-version"])
        .output()
    {
        Ok(output) => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if version.is_empty() {
                "15.0".to_string()
            } else {
                version
            }
        }
        Err(_) => "15.0".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies an empty SDK result yields an actionable Xcode tools hint.
    #[test]
    fn empty_sdk_path_produces_actionable_error() {
        let error = validate_macos_sdk_path("   ").expect_err("empty path must fail");
        assert!(error.contains("xcode-select --install"), "got: {error}");
    }

    /// Verifies a valid SDK path is trimmed but otherwise unchanged.
    #[test]
    fn valid_sdk_path_is_returned_trimmed() {
        let path = validate_macos_sdk_path("  /Library/Dev/MacOSX.sdk\n")
            .expect("valid SDK path");
        assert_eq!(path, "/Library/Dev/MacOSX.sdk");
    }
}
