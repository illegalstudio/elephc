//! Purpose:
//! Platform fixture helpers for selecting assembler, linker, SDK, and target settings in codegen tests.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Includes a focused target-selection test plus host and cross-target command assumptions.

use super::*;

/// Returns the current test target, initialized from `ELEPHC_TEST_TARGET` env var
/// or auto-detected from the host platform. Cached across the test run.
pub(crate) fn target() -> Target {
    *TEST_TARGET.get_or_init(|| {
        std::env::var("ELEPHC_TEST_TARGET")
            .ok()
            .map(|value| Target::parse(&value).expect("invalid ELEPHC_TEST_TARGET"))
            .unwrap_or_else(Target::detect_host)
    })
}

/// Returns the macOS SDK path by running `xcrun --show-sdk-path`.
/// Falls back to an empty string if the command fails.
pub(crate) fn get_sdk_path() -> &'static str {
    SDK_PATH.get_or_init(|| {
        Command::new("xcrun")
            .args(["--show-sdk-path"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    })
}

/// Returns the macOS SDK version by running `xcrun --sdk macosx --show-sdk-version`.
/// Falls back to "15.0" if the command fails or returns an empty string.
pub(crate) fn get_sdk_version() -> &'static str {
    SDK_VERSION.get_or_init(|| {
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
    })
}

/// Returns the platform-specific assembler command (e.g., `as` on Linux, or `as` with
/// `-arch arm64` on macOS). Delegates to `target().assembler_cmd()`.
pub(crate) fn assembler_cmd() -> &'static str {
    target().assembler_cmd()
}

/// Returns the platform-specific linker/gcc command (e.g., `gcc` on Linux).
/// Delegates to `target().linker_cmd()`.
pub(crate) fn gcc_cmd() -> &'static str {
    target().linker_cmd()
}

/// Returns platform-specific library search paths used during linking.
/// On macOS checks `/opt/homebrew/lib` and `/usr/local/lib`.
/// On Linux checks aarch64 sysroot paths.
pub(crate) fn default_link_paths() -> Vec<String> {
    let mut paths = Vec::new();
    match target().platform {
        Platform::MacOS => {
            for candidate in ["/opt/homebrew/lib", "/usr/local/lib"] {
                if std::path::Path::new(candidate).exists() {
                    paths.push(candidate.to_string());
                }
            }
        }
        Platform::Linux => {
            for candidate in ["/usr/aarch64-linux-gnu/lib", "/usr/lib/aarch64-linux-gnu"] {
                if std::path::Path::new(candidate).exists() {
                    paths.push(candidate.to_string());
                }
            }
        }
    }
    paths
}

/// Filters a list of library names, removing "System" which is handled
/// separately by the macOS linker. Returns the remaining library names as string slices.
pub(crate) fn effective_link_libs(extra_link_libs: &[String]) -> Vec<&str> {
    extra_link_libs
        .iter()
        .map(String::as_str)
        .filter(|lib| *lib != "System")
        .collect()
}

/// Returns the sysroot path for qemu-aarch64 when running ARM64 binaries
/// on a host cross-compiler. Queries the gcc sysroot or scans known aarch64
/// linux GNU paths. Returns `None` on macOS or when no sysroot is found.
pub(crate) fn qemu_sysroot() -> Option<&'static str> {
    QEMU_SYSROOT
        .get_or_init(|| match target().platform {
            Platform::Linux => {
                let compiler = gcc_cmd();
                if let Ok(output) = Command::new(compiler).arg("-print-sysroot").output() {
                    let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !sysroot.is_empty()
                        && sysroot != "/"
                        && std::path::Path::new(&sysroot).exists()
                    {
                        return Some(sysroot);
                    }
                }
                for candidate in ["/usr/aarch64-linux-gnu", "/usr/local/aarch64-linux-gnu"] {
                    if std::path::Path::new(candidate)
                        .join("lib/ld-linux-aarch64.so.1")
                        .exists()
                        || std::path::Path::new(candidate)
                            .join("lib64/ld-linux-aarch64.so.1")
                            .exists()
                    {
                        return Some(candidate.to_string());
                    }
                }
                None
            }
            Platform::MacOS => None,
        })
        .as_deref()
}

/// Verifies `effective_link_libs` filters out "System" from the library list.
/// The macOS linker handles "System" specially and does not accept it as a
/// normal `-l` argument. Input fixture: ["System", "crypto"] → ["crypto"].
#[test]
fn test_effective_link_libs_ignores_system() {
    let libs = vec!["System".to_string(), "crypto".to_string()];
    assert_eq!(effective_link_libs(&libs), vec!["crypto"]);
}
