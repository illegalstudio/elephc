//! Purpose:
//! Builds the catalogued zlib release as one static position-independent C archive.
//!
//! Called from:
//! - `crate::native_deps::recipe::CuratedRecipes` for zlib recipe revision 1.
//!
//! Key details:
//! - Runs the upstream static-only configure path with the selected target tools and retains only
//!   `libz.a`, `zlib.h`, and the generated `zconf.h`.

use std::fs;
use std::path::Path;

use crate::codegen_support::platform::Target;

use super::super::error::NativeError;
use super::super::recipe::RecipeRequest;
use super::super::toolchain::run_checked;
use super::util::{copy_regular, require_regular};

/// Builds zlib into the catalog-declared staging prefix.
pub fn build(request: &RecipeRequest<'_>) -> Result<(), NativeError> {
    let build = request.staging_prefix.join("build");
    let include = request.staging_prefix.join("include");
    let library = request.staging_prefix.join("lib");
    fs::create_dir_all(&build)
        .map_err(|error| NativeError::io("create zlib build directory", &build, error))?;
    fs::create_dir_all(&include)
        .map_err(|error| NativeError::io("create zlib include directory", &include, error))?;
    fs::create_dir_all(&library)
        .map_err(|error| NativeError::io("create zlib library directory", &library, error))?;

    let configure = request.source.join("configure");
    require_regular("zlib", &configure)?;
    let mut command = request.toolchain.command(Path::new("/bin/sh"));
    command.current_dir(&build).arg(&configure).arg("--static");
    if request.target != Target::detect_host() {
        command.env("CHOST", &request.toolchain.target_tuple);
    }
    run_checked(&mut command, "configure trusted zlib recipe")?;

    let mut make = request.toolchain.command(Path::new("make"));
    make.current_dir(&build).arg("libz.a");
    run_checked(&mut make, "build trusted zlib static library")?;

    copy_regular("zlib", &build.join("libz.a"), &library.join("libz.a"))?;
    copy_regular("zlib", &request.source.join("zlib.h"), &include.join("zlib.h"))?;
    copy_regular("zlib", &build.join("zconf.h"), &include.join("zconf.h"))?;

    let archive = library.join("libz.a");
    let mut inspect = request.toolchain.command(&request.toolchain.ar);
    inspect.arg("t").arg(&archive);
    run_checked(&mut inspect, "validate trusted zlib static archive")?;
    fs::remove_dir_all(&build)
        .map_err(|error| NativeError::io("remove trusted zlib build tree", &build, error))?;
    Ok(())
}
