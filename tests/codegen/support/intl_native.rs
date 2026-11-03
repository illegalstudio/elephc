//! Purpose:
//! Finds the managed ICU archives for Intl codegen fixtures.
//!
//! Called from:
//! - The codegen test link planner and Intl fixtures.
//!
//! Key details:
//! - Uses the native cache only, with no system-library fallback.
//! - The shim archive precedes ICU's three static archives; the C++ runtime follows them.

use std::path::PathBuf;
use std::sync::OnceLock;

use elephc::codegen::platform::{Arch, Platform, Target};

use super::native_cache::{find_package_library_dir, native_cache_artifacts_root};

/// Returns exact shim-first ICU archive paths for the harness target.
pub(crate) fn archives() -> Option<&'static [PathBuf]> {
    static ARCHIVES: OnceLock<Option<Vec<PathBuf>>> = OnceLock::new();
    ARCHIVES.get_or_init(|| {
        let names = [
            "libelephc_icu_intl_shim.a",
            "libicui18n.a",
            "libicuuc.a",
            "libicudata.a",
        ];
        let root = native_cache_artifacts_root()?;
        let dir = find_package_library_dir(&root, "icu", &names)?;
        Some(names.iter().map(|name| dir.join(name)).collect())
    }).as_deref()
}

/// Returns the platform's ICU C++ runtime and base system libraries.
pub(crate) fn system_libraries() -> &'static [&'static str] {
    match Target::detect_host().platform {
        Platform::MacOS => &["c++"],
        Platform::Linux => &["stdc++", "m", "dl", "pthread"],
        Platform::Windows => &[],
    }
}

/// Skips locally when absent; provisioning in CI makes absence an explicit failure.
pub(crate) fn skip(name: &str) -> bool {
    if archives().is_some() { return false; }
    assert!(
        std::env::var_os("ELEPHC_TEST_REQUIRE_INTL_NATIVE").is_none(),
        "{name}: managed ICU is required in CI"
    );
    eprintln!("ELEPHC_INTL_NATIVE_SKIP_GATE {name}: run elephc native add icu");
    true
}

/// Returns true on an architecture with a supported ICU C++ ABI in this harness.
pub(crate) fn host_supported() -> bool {
    matches!(Target::detect_host().arch, Arch::AArch64 | Arch::X86_64)
}
