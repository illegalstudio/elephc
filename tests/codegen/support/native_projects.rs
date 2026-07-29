//! Purpose:
//! Adapts the shared managed-PCRE2 fixture to the codegen harness target.
//!
//! Called from:
//! - Codegen runner helpers and native-project integration fixtures.
//!
//! Key details:
//! - The codegen harness target remains authoritative, including cross-target test runs.
//! - Provider discovery and cache construction live in `tests/support/managed_pcre2.rs`.

use super::*;

/// Creates a managed-PCRE2 project for the codegen harness target.
pub(crate) fn prepare_managed_pcre2_cli_project(dir: &Path) -> std::path::PathBuf {
    crate::managed_pcre2_support::prepare_managed_pcre2_cli_project(dir, target())
}

/// Returns the shared PCRE2 shim archive for the codegen harness target.
pub(crate) fn test_pcre2_shim_archive() -> &'static Path {
    crate::managed_pcre2_support::test_pcre2_shim_archive(target())
}

/// Returns one PCRE2 header for the codegen harness target.
pub(crate) fn test_pcre2_header_path(name: &str) -> std::path::PathBuf {
    crate::managed_pcre2_support::test_pcre2_header_path(target(), name)
}

/// Returns the shared PCRE2 library directory for the codegen harness target.
pub(crate) fn test_pcre2_library_dir() -> std::path::PathBuf {
    crate::managed_pcre2_support::test_pcre2_library_dir(target())
}

/// Returns one PCRE2 static archive for the codegen harness target.
pub(crate) fn test_pcre2_static_archive_path(name: &str) -> std::path::PathBuf {
    crate::managed_pcre2_support::test_pcre2_static_archive_path(target(), name)
}
