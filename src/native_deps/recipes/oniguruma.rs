//! Purpose:
//! Builds the pinned Oniguruma library and opaque mbregex provider for every supported target.
//!
//! Called from:
//! - `crate::native_deps::recipe::CuratedRecipes` for oniguruma revision 3.
//!
//! Key details:
//! - Retains static PIC archives and public headers; no system library fallback is used.

use std::{fs, path::Path};
use crate::codegen_support::platform::Target;
use super::super::{error::NativeError, recipe::RecipeRequest, toolchain::run_checked};
use super::util::{copy_regular, require_regular};

/// Embedded provider implementation shipped inside the compiler.
pub const SHIM_SOURCE: &str = include_str!("oniguruma_shim.c");
/// Versioned native provider header retained alongside the Oniguruma headers.
pub const SHIM_HEADER: &str = include_str!("oniguruma_shim.h");

/// Builds only the static library and provider declared by the immutable package recipe.
pub fn build(request: &RecipeRequest<'_>) -> Result<(), NativeError> {
    let build = request.staging_prefix.join("build");
    let include = request.staging_prefix.join("include");
    let library = request.staging_prefix.join("lib");
    for directory in [&build, &include, &library] {
        fs::create_dir_all(directory).map_err(|error| NativeError::io("create Oniguruma staging directory", directory, error))?;
    }
    let configure = request.source.join("configure");
    require_regular("Oniguruma", &configure)?;
    let mut command = request.toolchain.command(Path::new("/bin/sh"));
    command.current_dir(&build).arg(configure).args([
        "--disable-shared", "--enable-static", "--with-pic", "--disable-posix-api",
    ]);
    if request.target != Target::detect_host() {
        command.arg(format!("--host={}", request.toolchain.autoconf_host()));
    }
    run_checked(&mut command, "configure trusted Oniguruma recipe")?;
    let mut make = request.toolchain.command(Path::new("make"));
    make.current_dir(build.join("src")).arg("libonig.la");
    run_checked(&mut make, "build trusted Oniguruma static library")?;
    copy_regular("Oniguruma", &build.join("src/.libs/libonig.a"), &library.join("libonig.a"))?;
    copy_regular("Oniguruma", &request.source.join("src/oniguruma.h"), &include.join("oniguruma.h"))?;
    copy_regular("Oniguruma", &request.source.join("src/oniggnu.h"), &include.join("oniggnu.h"))?;
    let header = include.join("elephc_oniguruma.h");
    fs::write(&header, SHIM_HEADER).map_err(|error| NativeError::io("write Oniguruma provider header", &header, error))?;
    let source = build.join("provider.c");
    let object = build.join("provider.o");
    fs::write(&source, SHIM_SOURCE).map_err(|error| NativeError::io("write Oniguruma provider source", &source, error))?;
    let mut compile = request.toolchain.command(&request.toolchain.cc);
    compile.args(["-std=c11", "-fPIC", "-DONIG_EXTERN=extern", "-I"]).arg(&include)
        .arg("-c").arg(&source).arg("-o").arg(&object);
    run_checked(&mut compile, "compile Oniguruma provider")?;
    let shim = library.join("libelephc_oniguruma_shim.a");
    let mut archive = request.toolchain.command(&request.toolchain.ar);
    archive.arg("crs").arg(&shim).arg(&object);
    run_checked(&mut archive, "archive Oniguruma provider")?;
    for archive in [shim, library.join("libonig.a")] {
        let mut index = request.toolchain.command(&request.toolchain.ranlib);
        index.arg(&archive);
        run_checked(&mut index, "index Oniguruma archive")?;
        let mut inspect = request.toolchain.command(&request.toolchain.ar);
        inspect.arg("t").arg(&archive);
        run_checked(&mut inspect, "validate Oniguruma archive")?;
    }
    fs::remove_dir_all(&build).map_err(|error| NativeError::io("remove Oniguruma build intermediates", &build, error))?;
    Ok(())
}
