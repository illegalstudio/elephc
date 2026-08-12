//! Purpose:
//! Builds static OpenSSL 3.5.7 `libssl`/`libcrypto` archives used only as libcurl's TLS backend.
//!
//! Called from:
//! - `crate::native_deps::recipe::CuratedRecipes` for openssl recipe revision 1.
//!
//! Key details:
//! - Runs OpenSSL's documented out-of-tree `Configure` + `make build_libs`, static only
//!   (`no-shared`), and retains only the two archives plus the exact catalog-declared header set;
//!   never a system OpenSSL. PHP `openssl_encrypt`/`hash()` stay on `elephc-crypto` (RustCrypto).

use std::fs;
use std::path::Path;

use crate::codegen_support::platform::{Arch, Platform, Target};

use super::super::error::{NativeError, NativeErrorKind};
use super::super::recipe::RecipeRequest;
use super::super::toolchain::run_checked;
use super::util::{copy_regular, require_regular};

/// Builds OpenSSL into the catalog-declared staging prefix.
pub fn build(request: &RecipeRequest<'_>) -> Result<(), NativeError> {
    let build = request.staging_prefix.join("build");
    let include = request.staging_prefix.join("include/openssl");
    let library = request.staging_prefix.join("lib");
    fs::create_dir_all(&build)
        .map_err(|error| NativeError::io("create OpenSSL build directory", &build, error))?;
    fs::create_dir_all(&include)
        .map_err(|error| NativeError::io("create OpenSSL include directory", &include, error))?;
    fs::create_dir_all(&library)
        .map_err(|error| NativeError::io("create OpenSSL library directory", &library, error))?;

    let configure = request.source.join("Configure");
    require_regular("OpenSSL", &configure)?;
    let target_name = configure_target(request.target)?;
    let mut command = request.toolchain.command(Path::new("perl"));
    command
        .current_dir(&build)
        .arg(&configure)
        .args([target_name, "no-shared", "no-tests", "no-docs", "no-legacy"]);
    run_checked(&mut command, "configure trusted OpenSSL recipe")?;

    let mut make = request.toolchain.command(Path::new("make"));
    make.current_dir(&build).arg("build_libs");
    run_checked(&mut make, "build trusted OpenSSL static libraries")?;

    copy_regular("OpenSSL", &build.join("libssl.a"), &library.join("libssl.a"))?;
    copy_regular("OpenSSL", &build.join("libcrypto.a"), &library.join("libcrypto.a"))?;

    // OpenSSL's out-of-tree build leaves the static public headers in the pristine source tree
    // and writes only the templates it generates from `.h.in` (e.g. `configuration.h`, `ssl.h`)
    // into the build tree. Every catalog-declared header lives in exactly one of the two.
    for header in request.version.retained_headers {
        let Some(name) = header.strip_prefix("include/openssl/") else {
            continue;
        };
        let generated = build.join("include/openssl").join(name);
        let source = if generated.is_file() { generated } else { request.source.join("include/openssl").join(name) };
        copy_regular("OpenSSL", &source, &include.join(name))?;
    }

    for archive in [library.join("libssl.a"), library.join("libcrypto.a")] {
        let mut inspect = request.toolchain.command(&request.toolchain.ar);
        inspect.arg("t").arg(&archive);
        run_checked(&mut inspect, "validate trusted OpenSSL static archive")?;
    }
    fs::remove_dir_all(&build)
        .map_err(|error| NativeError::io("remove trusted OpenSSL build tree", &build, error))?;
    Ok(())
}

/// Maps an Elephc target to OpenSSL's own documented `Configure` target name.
fn configure_target(target: Target) -> Result<&'static str, NativeError> {
    match (target.platform, target.arch) {
        (Platform::MacOS, Arch::AArch64) => Ok("darwin64-arm64-cc"),
        (Platform::Linux, Arch::AArch64) => Ok("linux-aarch64"),
        (Platform::Linux, Arch::X86_64) => Ok("linux-x86_64"),
        _ => Err(NativeError::new(
            NativeErrorKind::Catalog,
            format!("no OpenSSL Configure target for '{}'", target.as_str()),
        )),
    }
}
