//! Purpose:
//! Resolves compiler and fixture paths for Cargo and relocated nextest archives.
//!
//! Called from:
//! - `Project::compile()` and SDK-independent validation in `tests/phpext_tests.rs`.
//!
//! Key details:
//! - Nextest supplies runtime paths that replace the archive build machine's paths.

use std::path::PathBuf;

/// Uses the runtime compiler override or the binary beside the current test executable.
pub(super) fn compiler_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_elephc").map(PathBuf::from).unwrap_or_else(|| {
        let mut path = std::env::current_exe().expect("resolve PHP extension test binary");
        path.pop();
        if path.ends_with("deps") { path.pop(); }
        path.join("elephc")
    })
}

/// Locates fixture sources in nextest's remapped workspace, with Cargo's normal fallback.
pub(super) fn fixture_dir() -> PathBuf {
    std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .join("tests/fixtures/php_ext")
}
