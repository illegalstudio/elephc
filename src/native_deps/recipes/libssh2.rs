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
//! - THE MANAGED PREFIXES ARE ASSERTED, NOT ASSUMED. `--with-crypto=openssl` and
//!   `--with-libz` fail closed on ABSENCE but not on SUBSTITUTION: if the managed archives
//!   were ever missing or mis-pathed, `AC_LIB_HAVE_LINKFLAGS` would fall through to the
//!   system search path and configure would SUCCEED against a distro OpenSSL/zlib — and
//!   nothing downstream could reveal it, because `curl_version()['ssl_version']` reports
//!   the version curl's own `--with-openssl` prefix supplied, not libssh2's. So the recipe
//!   reads back the `LIBSSL`/`LIBZ` that configure actually resolved and requires both to
//!   live under the dependency prefixes this run was handed.

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
    require_configured_against(&build, openssl_prefix, zlib_prefix)?;

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

/// Fails the build unless configure resolved OpenSSL and zlib to the managed prefixes.
///
/// `configure` writes what it actually found into the generated `src/Makefile` as the
/// `LIBSSL` and `LIBZ` substitutions (absolute archive paths for a static prefix hit). A
/// system fallback would put `/usr/lib/...` or `-lssl` there instead, which is precisely
/// the substitution the `--with-*-prefix` options cannot rule out on their own.
fn require_configured_against(
    build: &Path,
    openssl_prefix: &Path,
    zlib_prefix: &Path,
) -> Result<(), NativeError> {
    let makefile = build.join("src/Makefile");
    require_regular("libssh2", &makefile)?;
    let text = fs::read_to_string(&makefile)
        .map_err(|error| NativeError::io("read configured libssh2 makefile", &makefile, error))?;
    for (variable, prefix) in [("LIBSSL", openssl_prefix), ("LIBZ", zlib_prefix)] {
        let line = text
            .lines()
            .find(|line| line.starts_with(&format!("{variable} = ")))
            .ok_or_else(|| {
                NativeError::new(
                    NativeErrorKind::Build,
                    format!("configured libssh2 makefile declares no {variable}"),
                )
                .with_path(&makefile)
            })?;
        if !line.contains(&prefix.display().to_string()) {
            return Err(NativeError::new(
                NativeErrorKind::Build,
                format!(
                    "libssh2 configure resolved {variable} to a library outside the managed prefix '{}'; it found a system library instead of the one this recipe was handed. Configured as: {line}",
                    prefix.display()
                ),
            )
            .with_path(&makefile));
        }
    }
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
