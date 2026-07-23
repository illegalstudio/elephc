//! Purpose:
//! Builds static PCRE2 10.47 libraries and the Elephc-owned opaque ABI shim.
//!
//! Called from:
//! - `crate::native_deps::recipe::CuratedRecipes` for PCRE2 recipe revision 1.
//!
//! Key details:
//! - Uses explicit static/PIC/8-bit/Unicode flags and only the two required Make targets.

use std::fs;
use std::path::{Path, PathBuf};

use crate::codegen_support::platform::Target;

use super::super::error::{NativeError, NativeErrorKind};
use super::super::recipe::RecipeRequest;
use super::super::toolchain::run_checked;

/// Embedded shim source used by installed Elephc binaries without repository access.
pub const SHIM_SOURCE: &str = include_str!("pcre2_shim.c");

/// Builds PCRE2 and its shim into the catalog-declared staging prefix.
pub fn build(request: &RecipeRequest<'_>) -> Result<(), NativeError> {
    let build = request.staging_prefix.join("build");
    let include = request.staging_prefix.join("include");
    let library = request.staging_prefix.join("lib");
    fs::create_dir_all(&build).map_err(|error| NativeError::io("create PCRE2 build directory", &build, error))?;
    fs::create_dir_all(&include).map_err(|error| NativeError::io("create PCRE2 include directory", &include, error))?;
    fs::create_dir_all(&library).map_err(|error| NativeError::io("create PCRE2 library directory", &library, error))?;

    let configure = request.source.join("configure");
    require_regular(&configure)?;
    let mut command = request.toolchain.command(Path::new("/bin/sh"));
    command.current_dir(&build).arg(&configure).args([
        "--disable-shared",
        "--enable-static",
        "--enable-pcre2-8",
        "--disable-pcre2-16",
        "--disable-pcre2-32",
        "--enable-unicode",
        "--disable-jit",
    ]);
    if request.target != Target::detect_host() {
        command.arg(format!("--host={}", request.toolchain.target_tuple));
    }
    run_checked(&mut command, "configure trusted PCRE2 recipe")?;

    let mut make = request.toolchain.command(Path::new("make"));
    make.current_dir(&build).args(["libpcre2-8.la", "libpcre2-posix.la"]);
    run_checked(&mut make, "build trusted PCRE2 static library targets")?;

    copy_regular(&build.join(".libs/libpcre2-8.a"), &library.join("libpcre2-8.a"))?;
    copy_regular(&build.join(".libs/libpcre2-posix.a"), &library.join("libpcre2-posix.a"))?;
    copy_regular(&build.join("src/pcre2.h"), &include.join("pcre2.h"))?;
    copy_regular(&request.source.join("src/pcre2posix.h"), &include.join("pcre2posix.h"))?;

    for archive in [library.join("libpcre2-8.a"), library.join("libpcre2-posix.a")] {
        let mut inspect = request.toolchain.command(&request.toolchain.ar);
        inspect.arg("t").arg(&archive);
        run_checked(&mut inspect, "validate trusted PCRE2 static archive")?;
    }
    build_shim(request, &include, &library)?;
    fs::remove_dir_all(&build)
        .map_err(|error| NativeError::io("remove trusted PCRE2 build tree", &build, error))?;
    Ok(())
}

/// Compiles the opaque shim as PIC and archives it with the selected target tools.
fn build_shim(request: &RecipeRequest<'_>, include: &Path, library: &Path) -> Result<(), NativeError> {
    let source = request.staging_prefix.join("elephc_pcre2_shim.c");
    let object = request.staging_prefix.join("elephc_pcre2_shim.o");
    let archive = library.join("libelephc_pcre2_shim.a");
    fs::write(&source, SHIM_SOURCE).map_err(|error| NativeError::io("write embedded PCRE2 shim", &source, error))?;
    let mut compile = request.toolchain.command(&request.toolchain.cc);
    compile.args(["-fPIC", "-DPCRE2_STATIC", "-I"]).arg(include).arg("-c").arg(&source).arg("-o").arg(&object);
    run_checked(&mut compile, "compile Elephc PCRE2 shim")?;
    let mut archive_command = request.toolchain.command(&request.toolchain.ar);
    archive_command.arg("crs").arg(&archive).arg(&object);
    run_checked(&mut archive_command, "archive Elephc PCRE2 shim")?;
    let mut ranlib = request.toolchain.command(&request.toolchain.ranlib);
    ranlib.arg(&archive);
    run_checked(&mut ranlib, "index Elephc PCRE2 shim")?;
    let mut inspect = request.toolchain.command(&request.toolchain.ar);
    inspect.arg("t").arg(&archive);
    run_checked(&mut inspect, "validate Elephc PCRE2 shim archive")?;
    fs::remove_file(&source).map_err(|error| NativeError::io("remove PCRE2 shim source intermediate", &source, error))?;
    fs::remove_file(&object).map_err(|error| NativeError::io("remove PCRE2 shim object intermediate", &object, error))?;
    Ok(())
}

/// Requires a non-symlink regular file produced by the trusted recipe.
fn require_regular(path: &Path) -> Result<(), NativeError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| NativeError::io("inspect PCRE2 recipe file", path, error))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() || metadata.len() == 0 {
        return Err(NativeError::new(NativeErrorKind::Build, "PCRE2 recipe file is missing, empty, symlinked, or not regular").with_path(path));
    }
    Ok(())
}

/// Copies one verified regular recipe output to its retained staging path.
fn copy_regular(source: &Path, destination: &Path) -> Result<PathBuf, NativeError> {
    require_regular(source)?;
    fs::copy(source, destination).map_err(|error| NativeError::io("copy retained PCRE2 output", destination, error))?;
    require_regular(destination)?;
    Ok(destination.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies installed binaries embed the exact versioned opaque shim ABI.
    #[test]
    fn shim_source_is_embedded_and_versioned() {
        assert!(SHIM_SOURCE.contains("elephc_pcre2_v1_compile"));
        assert!(SHIM_SOURCE.contains("elephc_pcre2_v1_exec"));
        assert!(SHIM_SOURCE.contains("elephc_pcre2_v1_free"));
        assert!(!SHIM_SOURCE.contains("extern regex_t"));
    }
}
