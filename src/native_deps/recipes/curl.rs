//! Purpose:
//! Builds a static libcurl 8.21.0 archive (HTTP/HTTPS/FILE/FTP/FTPS) against the already
//! materialized `openssl` and `zlib` native packages.
//!
//! Called from:
//! - `crate::native_deps::recipe::CuratedRecipes` for curl recipe revision 1.
//!
//! Key details:
//! - `curl` is the first catalog package with non-empty `dependencies`; its recipe never probes
//!   the system for OpenSSL/zlib and only trusts the prefixes materialization already built and
//!   passed through `RecipeRequest::dependency_prefixes`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::codegen_support::platform::Target;

use super::super::error::{NativeError, NativeErrorKind};
use super::super::recipe::RecipeRequest;
use super::super::toolchain::run_checked;
use super::util::{copy_regular, require_regular};

/// Builds libcurl into the catalog-declared staging prefix.
pub fn build(request: &RecipeRequest<'_>) -> Result<(), NativeError> {
    let build = request.staging_prefix.join("build");
    let include = request.staging_prefix.join("include/curl");
    let library = request.staging_prefix.join("lib");
    fs::create_dir_all(&build)
        .map_err(|error| NativeError::io("create curl build directory", &build, error))?;
    fs::create_dir_all(&include)
        .map_err(|error| NativeError::io("create curl include directory", &include, error))?;
    fs::create_dir_all(&library)
        .map_err(|error| NativeError::io("create curl library directory", &library, error))?;

    let openssl_prefix = dependency_prefix(request, "openssl")?;
    let zlib_prefix = dependency_prefix(request, "zlib")?;

    let configure = request.source.join("configure");
    require_regular("curl", &configure)?;
    let mut command = request.toolchain.command(Path::new("/bin/sh"));
    command.current_dir(&build).arg(&configure).args([
        "--disable-shared",
        "--enable-static",
    ]);
    command.arg(format!("--with-openssl={}", openssl_prefix.display()));
    command.arg(format!("--with-zlib={}", zlib_prefix.display()));
    command.args([
        "--disable-ldap",
        "--disable-ldaps",
        "--disable-rtsp",
        "--disable-dict",
        "--disable-telnet",
        "--disable-tftp",
        "--disable-pop3",
        "--disable-imap",
        "--disable-smb",
        "--disable-smtp",
        "--disable-gopher",
        "--disable-mqtt",
        "--disable-manual",
        "--disable-docs",
        "--without-libpsl",
        "--without-libssh2",
        "--without-nghttp2",
        "--without-brotli",
        "--without-zstd",
        "--without-librtmp",
        "--without-libidn2",
    ]);
    if request.target != Target::detect_host() {
        command.arg(format!("--host={}", request.toolchain.target_tuple));
    }
    run_checked(&mut command, "configure trusted curl recipe")?;

    let mut make = request.toolchain.command(Path::new("make"));
    make.current_dir(&build).args(["-C", "lib"]);
    run_checked(&mut make, "build trusted libcurl static library")?;

    copy_regular("curl", &build.join("lib/.libs/libcurl.a"), &library.join("libcurl.a"))?;
    for header in request.version.retained_headers {
        let Some(name) = header.strip_prefix("include/curl/") else {
            continue;
        };
        copy_regular("curl", &request.source.join("include/curl").join(name), &include.join(name))?;
    }

    let archive = library.join("libcurl.a");
    let mut inspect = request.toolchain.command(&request.toolchain.ar);
    inspect.arg("t").arg(&archive);
    run_checked(&mut inspect, "validate trusted libcurl static archive")?;
    fs::remove_dir_all(&build)
        .map_err(|error| NativeError::io("remove trusted curl build tree", &build, error))?;
    Ok(())
}

/// Resolves one already-materialized catalog dependency's final artifact prefix.
fn dependency_prefix<'a>(request: &'a RecipeRequest<'a>, name: &str) -> Result<&'a PathBuf, NativeError> {
    request.dependency_prefixes.get(name).ok_or_else(|| {
        NativeError::new(
            NativeErrorKind::Build,
            format!("curl recipe requires the '{name}' dependency to be materialized first"),
        )
    })
}
