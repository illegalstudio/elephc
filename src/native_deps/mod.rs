//! Purpose:
//! Exposes curated native dependency commands, immutable catalog types, and read-only compilation resolution.
//!
//! Called from:
//! - Top-level CLI dispatch, compiler pipeline integration, tests, and future curated package recipes.
//!
//! Key details:
//! - Native commands own materialization; compilation resolution is read-only and never falls back to system libraries.

pub(crate) mod archive;
pub(crate) mod cache;
pub(crate) mod catalog;
mod cli;
mod doctor;
mod download;
mod error;
mod lockfile;
mod manifest;
mod materialize;
mod orchestration;
pub(crate) mod php_src_headers;
mod prune;
pub(crate) mod project;
mod receipt;
mod recipe;
pub(crate) mod recipes;
mod requirements;
mod resolver;
pub(crate) mod toolchain;
mod util;

use std::path::Path;

pub use catalog::{packages, ArchiveFormat, PackageSpec, PackageVersion, SourceArchive};
pub use cli::{native_help, parse_native_args, NativeCommand, NativeOptions, NativeParseOutcome};
pub use error::{NativeError, NativeErrorKind};
pub use orchestration::NativeRunOutput;
pub use project::{discover_for_source, ProjectPaths};
pub use receipt::ToolIdentity;
pub use requirements::NativeRequirement;
pub use resolver::{resolve_for_compilation, resolve_for_compilation_in_cache, ResolvedNativePackage};
pub use toolchain::NativeToolchain;
/// Shared with `php_ext`, which compiles hosted extensions with the same target
/// toolchain. Re-exported rather than reimplemented so both paths keep the same
/// environment hygiene and exit-code checking.
pub(crate) use toolchain::run_checked;

/// Executes a native command with the production HTTPS, curated recipe, and system toolchain services.
pub fn run_native_command(command: &NativeCommand, cwd: &Path) -> Result<NativeRunOutput, NativeError> {
    let downloader = download::HttpsDownloader::new()?;
    orchestration::run_native_command_with(
        command,
        cwd,
        &downloader,
        &recipe::CuratedRecipes,
        &toolchain::SystemToolchains,
    )
}
