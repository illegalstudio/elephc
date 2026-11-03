//! Purpose:
//! Builds the catalogued XZ Utils release as a static `liblzma.a` used by libzip's
//! LZMA/XZ compression backend.
//!
//! Called from:
//! - `crate::native_deps::recipe::CuratedRecipes` for xz recipe revision 1.
//!
//! Key details:
//! - Out-of-tree autoconf build, static only. The command-line tools, scripts, NLS catalogs,
//!   and documentation are switched off by name.
//! - `CFLAGS` is passed as a configure operand so the toolchain's bare `-fPIC` does not
//!   suppress autoconf's optimization default (the libxml2 recipe documents the same trap).
//! - Only `lib/liblzma.a` and the `lzma.h` + `lzma/*.h` API headers are retained.

use std::fs;
use std::path::Path;

use crate::codegen_support::platform::Target;

use super::super::error::NativeError;
use super::super::recipe::RecipeRequest;
use super::super::toolchain::run_checked;
use super::util::{copy_regular, require_regular};

/// The configure switches revision 1 passes, in order.
const CONFIGURE_FLAGS: &[&str] = &[
    "--disable-shared",
    "--enable-static",
    "--disable-xz",
    "--disable-xzdec",
    "--disable-lzmadec",
    "--disable-lzmainfo",
    "--disable-scripts",
    "--disable-doc",
    "--disable-nls",
];

/// Builds XZ Utils into the catalog-declared staging prefix.
pub fn build(request: &RecipeRequest<'_>) -> Result<(), NativeError> {
    let build = request.staging_prefix.join("build");
    let include = request.staging_prefix.join("include");
    let library = request.staging_prefix.join("lib");
    fs::create_dir_all(&build)
        .map_err(|error| NativeError::io("create xz build directory", &build, error))?;
    fs::create_dir_all(include.join("lzma"))
        .map_err(|error| NativeError::io("create xz include directory", &include, error))?;
    fs::create_dir_all(&library)
        .map_err(|error| NativeError::io("create xz library directory", &library, error))?;

    let configure = request.source.join("configure");
    require_regular("XZ Utils", &configure)?;
    let install = build.join("install");
    let mut command = request.toolchain.command(&configure);
    command
        .current_dir(&build)
        .args(CONFIGURE_FLAGS)
        .arg(format!("--prefix={}", install.display()))
        .arg("CFLAGS=-O2 -fPIC");
    if request.target != Target::detect_host() {
        command.arg(format!("--host={}", request.toolchain.autoconf_host()));
    }
    run_checked(&mut command, "configure trusted XZ Utils recipe")?;

    let jobs = std::thread::available_parallelism().map(usize::from).unwrap_or(1);
    let mut make = request.toolchain.command(Path::new("make"));
    make.current_dir(&build).arg(format!("-j{jobs}")).arg("install");
    run_checked(&mut make, "build trusted XZ Utils static library")?;

    copy_regular("XZ Utils", &install.join("lib/liblzma.a"), &library.join("liblzma.a"))?;
    for header in request.version.retained_headers {
        let Some(name) = header.strip_prefix("include/") else {
            continue;
        };
        copy_regular("XZ Utils", &install.join("include").join(name), &include.join(name))?;
    }
    fs::remove_dir_all(&build)
        .map_err(|error| NativeError::io("remove trusted XZ Utils build tree", &build, error))?;
    Ok(())
}
