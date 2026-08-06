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

/// Returns the path of the named Apple SDK, or exits with an actionable
/// diagnostic. `sdk` is an `xcrun --sdk` name: `macosx`, `iphoneos`,
/// `iphonesimulator`.
///
/// The name is always passed explicitly. Letting `xcrun` pick its default would
/// resolve whatever the selected developer directory offers, which silently
/// yields the macOS SDK when an iOS one was asked for and is missing.
pub(super) fn macos_sdk_path(sdk: &str) -> String {
    let resolved = Command::new("xcrun")
        .args(["--sdk", sdk, "--show-sdk-path"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default();
    match validate_macos_sdk_path(sdk, &resolved) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("{message}");
            process::exit(1);
        }
    }
}

/// Validates a resolved Apple SDK path without performing I/O or exiting.
///
/// The iOS SDKs get their own hint: the Command Line Tools ship the macOS SDK
/// only, so `xcode-select --install` is the wrong advice there and would send
/// the reader in circles.
fn validate_macos_sdk_path(sdk: &str, resolved: &str) -> Result<String, String> {
    let trimmed = resolved.trim();
    if !trimmed.is_empty() {
        return Ok(trimmed.to_string());
    }
    if sdk == "macosx" {
        return Err(
            "Could not locate the macOS SDK. Install the Xcode Command Line Tools \
             (run: xcode-select --install) and make sure `xcrun --show-sdk-path` prints a valid path."
                .to_string(),
        );
    }
    Err(format!(
        "Could not locate the '{sdk}' SDK. The Command Line Tools do not ship iOS SDKs: \
         install full Xcode, select it (sudo xcode-select -s /Applications/Xcode.app), \
         then fetch the platform with `xcodebuild -downloadPlatform iOS`."
    ))
}

/// Returns common existing Homebrew library directories in stable preference order.
pub(super) fn default_macos_library_paths() -> Vec<&'static str> {
    ["/opt/homebrew/lib", "/usr/local/lib"]
        .into_iter()
        .filter(|path| Path::new(path).exists())
        .collect()
}

/// Returns the version of the named Apple SDK, with the existing `15.0` fallback.
pub(super) fn macos_sdk_version(sdk: &str) -> String {
    match Command::new("xcrun")
        .args(["--sdk", sdk, "--show-sdk-version"])
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
        let error = validate_macos_sdk_path("macosx", "   ").expect_err("empty path must fail");
        assert!(error.contains("xcode-select --install"), "got: {error}");
    }

    /// Verifies a valid SDK path is trimmed but otherwise unchanged.
    #[test]
    fn valid_sdk_path_is_returned_trimmed() {
        let path = validate_macos_sdk_path("macosx", "  /Library/Dev/MacOSX.sdk\n")
            .expect("valid SDK path");
        assert_eq!(path, "/Library/Dev/MacOSX.sdk");
    }

    /// A missing iOS SDK must not recommend `xcode-select --install`: the
    /// Command Line Tools it installs ship no iOS SDK, so that advice loops.
    #[test]
    fn missing_ios_sdk_points_at_full_xcode_not_the_command_line_tools() {
        let error = validate_macos_sdk_path("iphonesimulator", "").expect_err("empty path fails");
        assert!(error.contains("iphonesimulator"), "must name the SDK: {error}");
        assert!(error.contains("full Xcode"), "must point at Xcode: {error}");
        assert!(
            !error.contains("xcode-select --install"),
            "the CLT hint is wrong for iOS SDKs: {error}"
        );
    }
}
