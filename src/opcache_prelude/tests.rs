//! Purpose:
//! Coordinates focused tests for OPcache prelude rendering and manifest behavior.
//!
//! Called from:
//! - cargo test through Rust's test harness.
//!
//! Key details:
//! - Shared fixtures are re-exported privately so sibling test modules can reuse them.

use super::*;

/// Injects the OPcache prelude with a throwaway declaration inventory for unit tests.
pub(super) fn inject_for_test(
    program: Program,
    php_version: PhpVersion,
    web: bool,
    entry_path: Option<&str>,
    manifest: &[ScriptEntry],
    overrides: &[(String, String)],
    preload: Option<&PreloadStatistics>,
    strict: bool,
) -> (Program, ManifestBakeSites) {
    let mut inventory = crate::optimize::reachability::PreludeInventory::new();
    inject_if_used(
        program,
        php_version,
        web,
        entry_path,
        manifest,
        overrides,
        preload,
        strict,
        &|_| false,
        &mut inventory,
    )
}

mod basics;
mod env_restrict;
mod manifest_bake;
mod manifest_ini;
mod preload;

#[allow(unused_imports)]
use basics::*;
#[allow(unused_imports)]
use env_restrict::*;
#[allow(unused_imports)]
use manifest_bake::*;
#[allow(unused_imports)]
use manifest_ini::*;
#[allow(unused_imports)]
use preload::*;
