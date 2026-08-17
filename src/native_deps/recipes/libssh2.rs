//! Purpose:
//! Builds a static libssh2 1.11.1 archive — the SSH transport libcurl's SCP/SFTP protocol
//! handlers need — against the already materialized `openssl` and `zlib` native packages.
//!
//! Called from:
//! - `crate::native_deps::recipe::CuratedRecipes` for libssh2 recipe revision 1.
//!
//! Key details:
//! - Like `curl.rs`, this recipe NEVER probes the system for OpenSSL or zlib: it only
//!   trusts the prefixes materialization already built and handed over in
//!   `RecipeRequest::dependency_prefixes`. `--with-crypto=openssl` pins the backend by
//!   name so a machine that also has libgcrypt/mbedTLS installed cannot change what gets
//!   compiled in (libssh2's default is `auto`, which probes every backend in turn).
//! - `--with-libssl-prefix` / `--with-libz-prefix` are gnulib `AC_LIB_HAVE_LINKFLAGS`
//!   options, NOT autoconf `--with-x=PATH` options: they take the PREFIX and derive
//!   `<prefix>/include` and `<prefix>/lib` themselves. Verified on a real build — the
//!   resulting `config.log` records `LIBSSL` as the two absolute managed archive paths.
//! - Only `make -C src` runs. libssh2's top-level `Makefile` also descends into `tests/`
//!   and `docs/`, and its test suite would try to reach a local `sshd`; the library
//!   itself is entirely under `src/`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::codegen_support::platform::Target;

use super::super::error::{NativeError, NativeErrorKind};
use super::super::recipe::RecipeRequest;
use super::super::toolchain::run_checked;
use super::util::{copy_regular, require_regular};

/// Builds libssh2 into the catalog-declared staging prefix.
pub fn build(request: &RecipeRequest<'_>) -> Result<(), NativeError> {
    let build = request.staging_prefix.join("build");
    let include = request.staging_prefix.join("include");
    let library = request.staging_prefix.join("lib");
    fs::create_dir_all(&build)
        .map_err(|error| NativeError::io("create libssh2 build directory", &build, error))?;
    fs::create_dir_all(&include)
        .map_err(|error| NativeError::io("create libssh2 include directory", &include, error))?;
    fs::create_dir_all(&library)
        .map_err(|error| NativeError::io("create libssh2 library directory", &library, error))?;

    let openssl_prefix = dependency_prefix(request, "openssl")?;
    let zlib_prefix = dependency_prefix(request, "zlib")?;

    let configure = request.source.join("configure");
    require_regular("libssh2", &configure)?;
    let mut command = request.toolchain.command(Path::new("/bin/sh"));
    command.current_dir(&build).arg(&configure).args([
        "--disable-shared",
        "--enable-static",
        "--disable-examples-build",
        "--with-crypto=openssl",
        "--with-libz",
    ]);
    command.arg(format!("--with-libssl-prefix={}", openssl_prefix.display()));
    command.arg(format!("--with-libz-prefix={}", zlib_prefix.display()));
    if request.target != Target::detect_host() {
        command.arg(format!("--host={}", request.toolchain.target_tuple));
    }
    run_checked(&mut command, "configure trusted libssh2 recipe")?;

    let mut make = request.toolchain.command(Path::new("make"));
    make.current_dir(&build).args(["-C", "src"]);
    run_checked(&mut make, "build trusted libssh2 static library")?;

    copy_regular("libssh2", &build.join("src/.libs/libssh2.a"), &library.join("libssh2.a"))?;
    for header in request.version.retained_headers {
        let Some(name) = header.strip_prefix("include/") else {
            continue;
        };
        copy_regular("libssh2", &request.source.join("include").join(name), &include.join(name))?;
    }

    let archive = library.join("libssh2.a");
    let mut inspect = request.toolchain.command(&request.toolchain.ar);
    inspect.arg("t").arg(&archive);
    run_checked(&mut inspect, "validate trusted libssh2 static archive")?;
    fs::remove_dir_all(&build)
        .map_err(|error| NativeError::io("remove trusted libssh2 build tree", &build, error))?;
    Ok(())
}

/// Resolves one already-materialized catalog dependency's final artifact prefix.
fn dependency_prefix<'a>(request: &'a RecipeRequest<'a>, name: &str) -> Result<&'a PathBuf, NativeError> {
    request.dependency_prefixes.get(name).ok_or_else(|| {
        NativeError::new(
            NativeErrorKind::Build,
            format!("libssh2 recipe requires the '{name}' dependency to be materialized first"),
        )
    })
}
