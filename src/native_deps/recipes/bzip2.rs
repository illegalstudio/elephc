//! Purpose:
//! Builds the catalogued bzip2 release as one static position-independent C archive used by
//! libzip's bzip2 compression backend.
//!
//! Called from:
//! - `crate::native_deps::recipe::CuratedRecipes` for bzip2 recipe revision 1.
//!
//! Key details:
//! - bzip2 ships a plain Makefile with unconditional `CC=gcc`/`AR=ar`/`RANLIB=ranlib`, so
//!   every tool is passed as a make-variable override instead of trusting the environment.
//! - Only `libbz2.a` and `bzlib.h` are retained; the command-line tools and man pages are
//!   never built.

use std::fs;
use std::path::Path;

use super::super::error::NativeError;
use super::super::recipe::RecipeRequest;
use super::super::toolchain::run_checked;
use super::util::{copy_regular, require_regular};

/// Builds bzip2 into the catalog-declared staging prefix.
pub fn build(request: &RecipeRequest<'_>) -> Result<(), NativeError> {
    let include = request.staging_prefix.join("include");
    let library = request.staging_prefix.join("lib");
    fs::create_dir_all(&include)
        .map_err(|error| NativeError::io("create bzip2 include directory", &include, error))?;
    fs::create_dir_all(&library)
        .map_err(|error| NativeError::io("create bzip2 library directory", &library, error))?;

    require_regular("bzip2", &request.source.join("Makefile"))?;
    let mut make = request.toolchain.command(Path::new("make"));
    make.current_dir(request.source)
        .arg("libbz2.a")
        .arg(format!("CC={}", request.toolchain.cc.display()))
        .arg(format!("AR={}", request.toolchain.ar.display()))
        .arg(format!("RANLIB={}", request.toolchain.ranlib.display()))
        .arg("CFLAGS=-O2 -fPIC -D_FILE_OFFSET_BITS=64");
    run_checked(&mut make, "build trusted bzip2 static library")?;

    copy_regular("bzip2", &request.source.join("libbz2.a"), &library.join("libbz2.a"))?;
    copy_regular("bzip2", &request.source.join("bzlib.h"), &include.join("bzlib.h"))?;
    Ok(())
}
