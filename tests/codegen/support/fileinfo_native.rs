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
use super::native_cache::{
    find_locked_package_library_dir, native_cache_artifacts_root,
};

/// Returns exact database-first archive paths for the harness target, including PCRE2.
pub(crate) fn archives() -> Option<&'static [PathBuf]> {
    static ARCHIVES: OnceLock<Option<Vec<PathBuf>>> = OnceLock::new();
    ARCHIVES.get_or_init(|| {
        let magic = ["libelephc_magic_database.a", "libmagic.a"];
        let pcre2 = ["libelephc_pcre2_shim.a", "libpcre2-posix.a", "libpcre2-8.a"];
        let root = native_cache_artifacts_root()?;
        let (magic_version, magic_revision) = locked_package("libmagic")?;
        let (pcre2_version, pcre2_revision) = locked_package("pcre2")?;
        let magic_dir = find_locked_package_library_dir(
            &root,
            "libmagic",
            &magic_version,
            magic_revision,
            &magic,
        )?;
        let pcre2_dir = find_locked_package_library_dir(
            &root,
            "pcre2",
            &pcre2_version,
            pcre2_revision,
            &pcre2,
        )?;
        let mut archives: Vec<PathBuf> = magic.iter().map(|name| magic_dir.join(name)).collect();
        archives.extend(pcre2.iter().map(|name| pcre2_dir.join(name)));
        Some(archives)
    }).as_deref()
}

/// Returns the version and recipe revision pinned for `package` in the fileinfo lock.
///
/// The lock is a small, stable TOML document; parsing the two fields the harness needs
/// avoids adding a TOML dependency to the test support module.
fn locked_package(package: &str) -> Option<(String, u32)> {
    let lock = include_str!("../../../examples/fileinfo/elephc.lock");
    let mut name = String::new();
    let mut version = None;
    let mut recipe = None;
    for line in lock.lines().map(str::trim) {
        if line == "[[package]]" {
            name.clear();
            version = None;
            recipe = None;
            continue;
        }
        if let Some(value) = line
            .strip_prefix("name = \"")
            .and_then(|rest| rest.strip_suffix('"'))
        {
            name = value.to_string();
            continue;
        }
        if let Some(value) = line
            .strip_prefix("version = \"")
            .and_then(|rest| rest.strip_suffix('"'))
        {
            version = Some(value.to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("recipe = ") {
            recipe = value.trim().parse().ok();
        }
        if name == package {
            if let (Some(version), Some(recipe)) = (&version, recipe) {
                if line.starts_with("[package.source]") {
                    return Some((version.clone(), recipe));
                }
            }
        }
    }
    // The source table follows the fields; return the last complete package if the loop
    // ended on the final package without another `[[package]]` marker.
    if name == package {
        if let (Some(version), Some(recipe)) = (version, recipe) {
            return Some((version, recipe));
        }
    }
    None
}

/// Skips locally when absent; provisioning in CI makes absence an explicit failure.
pub(crate) fn skip(name: &str) -> bool {
    if archives().is_some() { return false; }
    assert!(std::env::var_os("ELEPHC_TEST_REQUIRE_FILEINFO_NATIVE").is_none(), "{name}: managed libmagic is required in CI");
    eprintln!("ELEPHC_FILEINFO_NATIVE_SKIP_GATE {name}: run elephc native add libmagic");
    true
}
