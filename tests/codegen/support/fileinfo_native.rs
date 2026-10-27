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

/// Returns exact database-first archive paths for the harness target.
pub(crate) fn archives() -> Option<&'static [PathBuf]> {
    static ARCHIVES: OnceLock<Option<Vec<PathBuf>>> = OnceLock::new();
    ARCHIVES.get_or_init(|| {
        let names = ["libelephc_magic_database.a", "libmagic.a"];
        let root = native_cache_artifacts_root()?;
        let dir = find_package_library_dir(&root, "libmagic", &names)?;
        Some(names.iter().map(|name| dir.join(name)).collect())
    }).as_deref()
}

/// Skips locally when absent; provisioning in CI makes absence an explicit failure.
pub(crate) fn skip(name: &str) -> bool {
    if archives().is_some() { return false; }
    assert!(std::env::var_os("ELEPHC_TEST_REQUIRE_FILEINFO_NATIVE").is_none(), "{name}: managed libmagic is required in CI");
    eprintln!("ELEPHC_FILEINFO_NATIVE_SKIP_GATE {name}: run elephc native add libmagic");
    true
}
