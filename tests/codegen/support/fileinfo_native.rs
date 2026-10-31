//! Purpose:
//! Finds the managed libmagic archives for fileinfo codegen fixtures.
//!
//! Called from:
//! - The codegen test link planner and fileinfo fixtures.
//!
//! Key details:
//! - Uses the native cache only, with no system-library fallback.

use std::path::PathBuf;
use std::sync::OnceLock;
use super::native_cache::{find_package_library_dir, native_cache_artifacts_root};

/// Returns exact database-first archive paths for the harness target, including PCRE2.
pub(crate) fn archives() -> Option<&'static [PathBuf]> {
    static ARCHIVES: OnceLock<Option<Vec<PathBuf>>> = OnceLock::new();
    ARCHIVES.get_or_init(|| {
        let magic = ["libelephc_magic_database.a", "libmagic.a"];
        let pcre2 = ["libelephc_pcre2_shim.a", "libpcre2-posix.a", "libpcre2-8.a"];
        let root = native_cache_artifacts_root()?;
        let magic_dir = find_package_library_dir(&root, "libmagic", &magic)?;
        let pcre2_dir = find_package_library_dir(&root, "pcre2", &pcre2)?;
        let mut archives: Vec<PathBuf> = magic.iter().map(|name| magic_dir.join(name)).collect();
        archives.extend(pcre2.iter().map(|name| pcre2_dir.join(name)));
        Some(archives)
    }).as_deref()
}

/// Skips locally when absent; provisioning in CI makes absence an explicit failure.
pub(crate) fn skip(name: &str) -> bool {
    if archives().is_some() { return false; }
    assert!(std::env::var_os("ELEPHC_TEST_REQUIRE_FILEINFO_NATIVE").is_none(), "{name}: managed libmagic is required in CI");
    eprintln!("ELEPHC_FILEINFO_NATIVE_SKIP_GATE {name}: run elephc native add libmagic");
    true
}
