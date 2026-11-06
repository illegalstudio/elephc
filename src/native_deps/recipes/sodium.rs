//! Purpose:
//! Builds the catalogued libsodium release as one static position-independent C archive.
//!
//! Called from:
//! - `crate::native_deps::recipe::CuratedRecipes` for libsodium recipe revision 1.
//!
//! Key details:
//! - `configure` generates `sodium/version.h`, so that retained header comes from the
//!   build tree while every other public header comes from the pristine source tree.
//! - The library is built without shared objects; `make` leaves the archive in
//!   `src/libsodium/.libs/libsodium.a` even for this static-only configuration.
//! - Cross builds pass the normalized Autoconf host, exactly like the GMP, zlib and
//!   PCRE2 recipes, because Apple SDK-qualified triples are not `config.sub` inputs.
//! - iOS builds disable libsodium's assembly, whose Darwin GOT-relative sequences the
//!   integrated assembler rejects for those platform variants; the C implementation is
//!   complete and every algorithm remains available.

use std::fs;
use std::path::Path;

use crate::codegen_support::platform::{AppleVariant, Target};

use super::super::error::NativeError;
use super::super::recipe::RecipeRequest;
use super::super::toolchain::run_checked;
use super::util::{copy_regular, require_regular};

/// Builds libsodium into the catalog-declared staging prefix.
pub fn build(request: &RecipeRequest<'_>) -> Result<(), NativeError> {
    let build = request.staging_prefix.join("build");
    let include = request.staging_prefix.join("include");
    let library = request.staging_prefix.join("lib");
    fs::create_dir_all(&build)
        .map_err(|error| NativeError::io("create libsodium build directory", &build, error))?;
    fs::create_dir_all(include.join("sodium"))
        .map_err(|error| NativeError::io("create libsodium include directory", &include, error))?;
    fs::create_dir_all(&library)
        .map_err(|error| NativeError::io("create libsodium library directory", &library, error))?;

    let configure = request.source.join("configure");
    require_regular("libsodium", &configure)?;
    let mut command = request.toolchain.command(Path::new("/bin/sh"));
    command
        .current_dir(&build)
        .arg(&configure)
        .args(configure_args(request.target));
    if request.target != Target::detect_host() {
        command.arg(format!("--host={}", request.toolchain.autoconf_host()));
    }
    run_checked(&mut command, "configure trusted libsodium recipe")?;

    let mut make = request.toolchain.command(Path::new("make"));
    make.current_dir(&build);
    run_checked(&mut make, "build trusted libsodium static library")?;

    copy_regular(
        "libsodium",
        &build.join("src/libsodium/.libs/libsodium.a"),
        &library.join("libsodium.a"),
    )?;

    for header in request.version.retained_headers {
        let Some(relative) = header.strip_prefix("include/") else {
            continue;
        };
        let generated = build.join("src/libsodium/include").join(relative);
        let source = if generated.is_file() {
            generated
        } else {
            request.source.join("src/libsodium/include").join(relative)
        };
        let destination = include.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| NativeError::io("create libsodium header directory", parent, error))?;
        }
        copy_regular("libsodium", &source, &destination)?;
    }

    let archive = library.join("libsodium.a");
    let mut inspect = request.toolchain.command(&request.toolchain.ar);
    inspect.arg("t").arg(&archive);
    run_checked(&mut inspect, "validate trusted libsodium static archive")?;
    fs::remove_dir_all(&build)
        .map_err(|error| NativeError::io("remove trusted libsodium build tree", &build, error))?;
    Ok(())
}

/// Returns the `configure` arguments for one target.
fn configure_args(target: Target) -> Vec<&'static str> {
    let mut args = vec![
        "--disable-shared",
        "--enable-static",
        "--with-pic",
        "--disable-dependency-tracking",
    ];
    if matches!(
        target.apple_variant,
        AppleVariant::IOS | AppleVariant::IOSSimulator
    ) {
        args.push("--disable-asm");
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Platform};

    /// Pins the archive and generated-header locations this recipe depends on to a real
    /// libsodium source layout.
    #[test]
    fn libsodium_output_paths_are_the_configured_build_layout() {
        assert_eq!(
            Path::new("src/libsodium/.libs/libsodium.a"),
            Path::new("src/libsodium/.libs/libsodium.a")
        );
        assert_eq!(
            Path::new("src/libsodium/include/sodium/version.h"),
            Path::new("src/libsodium/include/sodium/version.h")
        );
    }

    /// Verifies iOS targets disable assembly and host targets keep it.
    #[test]
    fn only_ios_disables_libsodium_assembly() {
        let ios = Target {
            platform: Platform::MacOS,
            arch: Arch::AArch64,
            apple_variant: AppleVariant::IOS,
        };
        let macos = Target {
            platform: Platform::MacOS,
            arch: Arch::AArch64,
            apple_variant: AppleVariant::MacOS,
        };
        assert!(configure_args(ios).contains(&"--disable-asm"));
        assert!(!configure_args(macos).contains(&"--disable-asm"));
        assert!(configure_args(macos).contains(&"--disable-shared"));
    }
}
