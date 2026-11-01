//! Purpose:
//! Builds the pinned libzip 1.11.4 release as a static archive linked by the `elephc-zip`
//! bridge, against the managed zlib, bzip2, XZ Utils, and OpenSSL packages.
//!
//! Called from:
//! - `crate::native_deps::recipe::CuratedRecipes` for libzip recipe revision 1.
//!
//! Key details:
//! - libzip ships CMake only. The build is static, tools/tests/examples/docs are off, and
//!   every optional backend except bzip2/LZMA/OpenSSL is switched off by name so a build
//!   machine cannot change the artifact.
//! - Dependency discovery is explicit: `FindZLIB`, `FindBZip2`, `FindLibLZMA`, and
//!   `FindOpenSSL` receive the already-materialized catalog prefixes, never a system probe.
//! - Cross builds get a generated CMake toolchain file carrying the selected compiler and,
//!   on Apple, the SDK and deployment target. Host builds pass the compiler directly.
//! - Only `lib/libzip.a`, `include/zip.h`, and the generated `include/zipconf.h` are retained.

use std::fs;
use std::path::{Path, PathBuf};

use crate::codegen_support::platform::{AppleVariant, Platform, Target};

use super::super::error::{NativeError, NativeErrorKind};
use super::super::recipe::RecipeRequest;
use super::super::toolchain::run_checked;
use super::util::{copy_regular, require_regular};

/// The CMake switches revision 1 passes, independent of the target.
const CMAKE_FLAGS: &[&str] = &[
    "-DCMAKE_BUILD_TYPE=Release",
    "-DCMAKE_POSITION_INDEPENDENT_CODE=ON",
    "-DCMAKE_C_FLAGS=-O2 -fPIC",
    "-DBUILD_SHARED_LIBS=OFF",
    "-DLIBZIP_DO_INSTALL=OFF",
    "-DBUILD_TOOLS=OFF",
    "-DBUILD_REGRESS=OFF",
    "-DBUILD_OSSFUZZ=OFF",
    "-DBUILD_EXAMPLES=OFF",
    "-DBUILD_DOC=OFF",
    "-DENABLE_ZSTD=OFF",
    "-DENABLE_GNUTLS=OFF",
    "-DENABLE_MBEDTLS=OFF",
    "-DENABLE_WINDOWS_CRYPTO=OFF",
    "-DENABLE_COMMONCRYPTO=OFF",
    "-DENABLE_OPENSSL=ON",
    "-DENABLE_BZIP2=ON",
    "-DENABLE_LZMA=ON",
];

/// Builds libzip into the catalog-declared staging prefix.
pub fn build(request: &RecipeRequest<'_>) -> Result<(), NativeError> {
    let build = request.staging_prefix.join("build");
    let include = request.staging_prefix.join("include");
    let library = request.staging_prefix.join("lib");
    fs::create_dir_all(&build)
        .map_err(|error| NativeError::io("create libzip build directory", &build, error))?;
    fs::create_dir_all(&include)
        .map_err(|error| NativeError::io("create libzip include directory", &include, error))?;
    fs::create_dir_all(&library)
        .map_err(|error| NativeError::io("create libzip library directory", &library, error))?;

    require_regular("libzip", &request.source.join("CMakeLists.txt"))?;
    let zlib = dependency(request, "zlib")?;
    let bzip2 = dependency(request, "bzip2")?;
    let xz = dependency(request, "xz")?;
    let openssl = dependency(request, "openssl")?;
    let cmake = absolute_tool(Path::new("cmake"));
    if cmake.components().count() == 1 {
        return Err(NativeError::new(
            NativeErrorKind::Build,
            "cmake is required to build the managed libzip package; install CMake and retry",
        ));
    }

    let mut command = request.toolchain.command(&cmake);
    command
        .arg("-S")
        .arg(request.source)
        .arg("-B")
        .arg(&build)
        .args(CMAKE_FLAGS)
        .arg(format!("-DCMAKE_INSTALL_PREFIX={}", request.staging_prefix.display()))
        .arg(format!("-DZLIB_INCLUDE_DIR={}", zlib.join("include").display()))
        .arg(format!("-DZLIB_LIBRARY={}", zlib.join("lib/libz.a").display()))
        .arg(format!("-DBZIP2_INCLUDE_DIR={}", bzip2.join("include").display()))
        .arg(format!("-DBZIP2_LIBRARY_RELEASE={}", bzip2.join("lib/libbz2.a").display()))
        .arg(format!("-DLIBLZMA_INCLUDE_DIR={}", xz.join("include").display()))
        .arg(format!("-DLIBLZMA_LIBRARY={}", xz.join("lib/liblzma.a").display()))
        .arg(format!("-DOPENSSL_ROOT_DIR={}", openssl.display()))
        .arg(format!("-DOPENSSL_INCLUDE_DIR={}", openssl.join("include").display()))
        .arg(format!("-DOPENSSL_SSL_LIBRARY={}", openssl.join("lib/libssl.a").display()))
        .arg(format!("-DOPENSSL_CRYPTO_LIBRARY={}", openssl.join("lib/libcrypto.a").display()))
        .arg("-DOPENSSL_USE_STATIC_LIBS=TRUE");

    if request.target != Target::detect_host() {
        let toolchain = build.join("elephc-cmake-toolchain.cmake");
        fs::write(&toolchain, toolchain_file(request))
            .map_err(|error| NativeError::io("write libzip CMake toolchain", &toolchain, error))?;
        command.arg(format!("-DCMAKE_TOOLCHAIN_FILE={}", toolchain.display()));
    } else {
        command.arg(format!(
            "-DCMAKE_C_COMPILER={}",
            absolute_tool(&request.toolchain.cc).display()
        ));
    }
    command
        .arg(format!("-DCMAKE_AR={}", absolute_tool(&request.toolchain.ar).display()))
        .arg(format!("-DCMAKE_RANLIB={}", absolute_tool(&request.toolchain.ranlib).display()));
    run_checked(&mut command, "configure trusted libzip recipe")?;

    let jobs = std::thread::available_parallelism().map(usize::from).unwrap_or(1);
    let mut make = request.toolchain.command(&cmake);
    make.arg("--build")
        .arg(&build)
        .arg("--target")
        .arg("zip")
        .arg("--parallel")
        .arg(jobs.to_string());
    run_checked(&mut make, "build trusted libzip static library")?;

    copy_regular("libzip", &build.join("lib/libzip.a"), &library.join("libzip.a"))?;
    copy_regular("libzip", &request.source.join("lib/zip.h"), &include.join("zip.h"))?;
    copy_regular("libzip", &build.join("zipconf.h"), &include.join("zipconf.h"))?;

    let mut inspect = request.toolchain.command(&request.toolchain.ar);
    inspect.arg("t").arg(library.join("libzip.a"));
    run_checked(&mut inspect, "validate trusted libzip static archive")?;
    fs::remove_dir_all(&build)
        .map_err(|error| NativeError::io("remove trusted libzip build tree", &build, error))?;
    Ok(())
}

/// Returns one materialized dependency prefix, failing closed when it is absent.
fn dependency<'a>(request: &'a RecipeRequest<'_>, name: &str) -> Result<&'a Path, NativeError> {
    request
        .dependency_prefixes
        .get(name)
        .map(PathBuf::as_path)
        .ok_or_else(|| {
            NativeError::new(
                NativeErrorKind::Build,
                format!("libzip requires the materialized '{name}' dependency prefix"),
            )
        })
}

/// Renders the CMake toolchain file used for cross targets.
fn toolchain_file(request: &RecipeRequest<'_>) -> String {
    let mut file = String::new();
    match (request.target.platform, request.target.apple_variant) {
        (Platform::MacOS, AppleVariant::MacOS) => {
            file.push_str("set(CMAKE_SYSTEM_NAME Darwin)\n");
            file.push_str("set(CMAKE_OSX_ARCHITECTURES arm64)\n");
        }
        (Platform::MacOS, AppleVariant::IOS) => {
            file.push_str("set(CMAKE_SYSTEM_NAME iOS)\n");
            file.push_str("set(CMAKE_OSX_ARCHITECTURES arm64)\n");
            file.push_str("set(CMAKE_OSX_SYSROOT iphoneos)\n");
            file.push_str("set(CMAKE_OSX_DEPLOYMENT_TARGET 13.0)\n");
        }
        (Platform::MacOS, AppleVariant::IOSSimulator) => {
            file.push_str("set(CMAKE_SYSTEM_NAME iOS)\n");
            file.push_str("set(CMAKE_OSX_ARCHITECTURES arm64)\n");
            file.push_str("set(CMAKE_OSX_SYSROOT iphonesimulator)\n");
            file.push_str("set(CMAKE_OSX_DEPLOYMENT_TARGET 13.0)\n");
        }
        (Platform::Linux, _) => {
            file.push_str("set(CMAKE_SYSTEM_NAME Linux)\n");
            file.push_str("set(CMAKE_SYSTEM_PROCESSOR ");
            file.push_str(match request.target.arch {
                crate::codegen_support::platform::Arch::AArch64 => "aarch64",
                crate::codegen_support::platform::Arch::X86_64 => "x86_64",
            });
            file.push_str(")\n");
        }
        (Platform::Windows, _) => {}
    }
    file.push_str("set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)\n");
    file.push_str(&format!(
        "set(CMAKE_C_COMPILER \"{}\")\n",
        absolute_tool(&request.toolchain.cc).display()
    ));
    file.push_str(&format!(
        "set(CMAKE_AR \"{}\")\nset(CMAKE_RANLIB \"{}\")\n",
        absolute_tool(&request.toolchain.ar).display(),
        absolute_tool(&request.toolchain.ranlib).display()
    ));
    file
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the recipe never enables a backend the catalog does not pin, and keeps the
    /// bzip2/LZMA/OpenSSL backends PHP's compression_methods fixture expects.
    #[test]
    fn cmake_flags_pin_the_reviewed_backend_set() {
        for expected in [
            "-DBUILD_SHARED_LIBS=OFF",
            "-DBUILD_TOOLS=OFF",
            "-DBUILD_REGRESS=OFF",
            "-DBUILD_OSSFUZZ=OFF",
            "-DENABLE_BZIP2=ON",
            "-DENABLE_LZMA=ON",
            "-DENABLE_OPENSSL=ON",
        ] {
            assert!(CMAKE_FLAGS.contains(&expected), "missing {expected}");
        }
        for disabled in ["-DENABLE_ZSTD=OFF", "-DENABLE_COMMONCRYPTO=OFF"] {
            assert!(CMAKE_FLAGS.contains(&disabled), "missing {disabled}");
        }
        assert!(!CMAKE_FLAGS.iter().any(|flag| flag.contains("SYSTEM")));
    }
}

/// Resolves a bare tool name through the inherited `PATH` for CMake, which records
/// archiver paths relative to the configure working directory instead of searching `PATH`
/// at link time.
fn absolute_tool(tool: &Path) -> PathBuf {
    if tool.components().count() > 1 {
        return tool.to_path_buf();
    }
    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|directory| directory.join(tool))
                .find(|candidate| candidate.is_file())
        })
        .unwrap_or_else(|| tool.to_path_buf())
}
