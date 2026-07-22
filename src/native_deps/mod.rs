//! Purpose:
//! Exposes curated native dependency commands, immutable catalog types, and read-only compilation resolution.
//!
//! Called from:
//! - Top-level CLI dispatch, compiler pipeline integration, tests, and future curated package recipes.
//!
//! Key details:
//! - Native commands own materialization; compilation resolution is read-only and never falls back to system libraries.

mod archive;
mod cache;
mod catalog;
mod cli;
mod doctor;
mod download;
mod error;
mod lockfile;
mod manifest;
mod orchestration;
mod project;
mod receipt;
mod recipe;
mod recipes;
mod requirements;
mod resolver;
mod toolchain;
mod util;

use std::path::Path;

pub use catalog::{packages, PackageSpec, PackageVersion, SourceArchive};
pub use cli::{native_help, parse_native_args, NativeCommand, NativeOptions, NativeParseOutcome};
pub use error::{NativeError, NativeErrorKind};
pub use orchestration::NativeRunOutput;
pub use requirements::NativeRequirement;
pub use resolver::{resolve_for_compilation, ResolvedNativePackage};

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
