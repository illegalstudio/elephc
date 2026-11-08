//! Purpose:
//! Builds static ICU data, internationalization libraries and the opaque date ABI.
//!
//! Called from:
//! - `crate::native_deps::recipe::CuratedRecipes` for ICU recipe revision 40.
//!
//! Key details:
//! - C++ compilation uses the selected C compiler driver, keeping target flags and
//!   compiler identity authoritative; C++ runtime libraries are explicit.
//! - Cross builds first build native host tools from the same verified source.
//! - Only static archives and the ICU-independent shim header survive staging.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::codegen_support::platform::{Platform, Target};

use super::super::error::NativeError;
use super::super::recipe::RecipeRequest;
use super::super::toolchain::run_checked;
use super::util::{copy_regular, require_regular};

const SHIM: &str = include_str!("elephc_icu_intl_shim.cpp");
const HEADER: &str = include_str!("elephc_icu_intl_shim.h");

/// ICU's configure/compiler-link checks need an explicit C++ runtime with a C driver.
fn cpp_library(target: Target) -> &'static str {
    if target.platform == Platform::MacOS { "-lc++" } else { "-lstdc++" }
}

/// Produces a scrubbed command for the target or executable native build tools.
fn command(request: &RecipeRequest<'_>, executable: &Path, native: bool) -> Command {
    let mut command = request.toolchain.command(executable);
    if native {
        // Generic ELEPHC_NATIVE_* overrides may name cross tools. Host tools must
        // execute here; configure verifies its conventional native compiler.
        command.env("CC", "cc").env("CXX", "cc").env("AR", "ar")
            .env("RANLIB", "ranlib").env("LIBS", cpp_library(Target::detect_host()));
    } else {
        command.env("CXX", &request.toolchain.cc).env("LIBS", cpp_library(request.target));
    }
    command.env("CFLAGS", "-O2 -fPIC").env("CXXFLAGS", "-O2 -fPIC -std=c++17");
    command
}

/// Builds a tree, optionally pointing target ICU at already built native tools.
fn build_tree(request: &RecipeRequest<'_>, source: &Path, build: &Path,
              prefix: &Path, native: bool, host_build: Option<&Path>) -> Result<(), NativeError> {
    fs::create_dir_all(build)
        .map_err(|error| NativeError::io("create ICU build tree", build, error))?;
    let mut configure = command(request, Path::new("/bin/sh"), native);
    configure.current_dir(build).arg(source.join("configure")).args([
        "--enable-static", "--disable-shared", "--with-data-packaging=static",
        "--disable-tests", "--disable-samples", "--disable-extras",
    ]).arg(format!("--prefix={}", prefix.display()));
    if let Some(host) = host_build {
        configure.arg(format!("--host={}", request.toolchain.autoconf_host()))
            .arg(format!("--with-cross-build={}", host.display()));
    }
    run_checked(&mut configure, "configure trusted ICU recipe")?;
    let mut make = command(request, Path::new("make"), native);
    make.current_dir(build).arg("-j1");
    run_checked(&mut make, "build trusted ICU static libraries and tools")?;
    Ok(())
}

/// Installs and retains only the declared target archives and opaque shim header.
pub fn build(request: &RecipeRequest<'_>) -> Result<(), NativeError> {
    let source = request.source.join("source");
    require_regular("ICU", &source.join("configure"))?;
    let trees = request.staging_prefix.join("build");
    let build = trees.join("target");
    let prefix = trees.join("install");
    let host = if request.target != Target::detect_host() {
        let host = trees.join("host");
        build_tree(request, &source, &host, &trees.join("host-install"), true, None)?;
        Some(host)
    } else { None };
    build_tree(request, &source, &build, &prefix, false, host.as_deref())?;
    let mut install = command(request, Path::new("make"), false);
    install.current_dir(&build).arg("install");
    run_checked(&mut install, "install trusted ICU static libraries")?;
    let include = request.staging_prefix.join("include");
    let library = request.staging_prefix.join("lib");
    for path in [&include, &library] {
        fs::create_dir_all(path)
            .map_err(|error| NativeError::io("create ICU retained directory", path, error))?;
    }
    for name in ["libicui18n.a", "libicuuc.a", "libicudata.a"] {
        copy_regular("ICU", &prefix.join("lib").join(name), &library.join(name))?;
    }
    build_shim(request, &prefix.join("include"), &include, &library)?;
    for name in request.version.ordered_link_outputs {
        let archive = request.staging_prefix.join(name);
        let mut inspect = request.toolchain.command(&request.toolchain.ar);
        inspect.arg("t").arg(&archive);
        run_checked(&mut inspect, "validate trusted ICU static archive")?;
    }
    fs::remove_dir_all(&trees)
        .map_err(|error| NativeError::io("remove ICU build trees", &trees, error))?;
    Ok(())
}

/// Compiles the embedded boundary against the freshly installed target headers.
fn build_shim(request: &RecipeRequest<'_>, icu_include: &Path,
              include: &Path, library: &Path) -> Result<(), NativeError> {
    let source = request.staging_prefix.join("elephc_icu_intl_shim.cpp");
    let object = request.staging_prefix.join("elephc_icu_intl_shim.o");
    let header = include.join("elephc_icu_intl_shim.h");
    fs::write(&source, SHIM)
        .map_err(|error| NativeError::io("write embedded ICU shim", &source, error))?;
    fs::write(&header, HEADER)
        .map_err(|error| NativeError::io("write embedded ICU header", &header, error))?;
    let mut compile = request.toolchain.command(&request.toolchain.cc);
    compile.args(["-std=c++17", "-fPIC", "-fexceptions", "-I"]).arg(icu_include)
        .arg("-I").arg(include).arg("-c").arg(&source).arg("-o").arg(&object);
    run_checked(&mut compile, "compile trusted ICU Intl shim")?;
    let archive: PathBuf = library.join("libelephc_icu_intl_shim.a");
    let mut ar = request.toolchain.command(&request.toolchain.ar);
    ar.arg("crs").arg(&archive).arg(&object);
    run_checked(&mut ar, "archive trusted ICU date shim")?;
    let mut ranlib = request.toolchain.command(&request.toolchain.ranlib);
    ranlib.arg(&archive);
    run_checked(&mut ranlib, "index trusted ICU date shim")?;
    for file in [&source, &object] {
        fs::remove_file(file)
            .map_err(|error| NativeError::io("remove ICU shim intermediate", file, error))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{AppleVariant, Arch};
    use crate::native_deps::{catalog, receipt::ToolIdentity, toolchain::NativeToolchain};
    use std::collections::BTreeMap;
    use std::ffi::OsStr;

    #[test]
    fn host_tools_and_target_libraries_use_distinct_compilers() {
        let tool = ToolIdentity { command: "selected-target-cc".into(), version: "fixture".into() };
        let toolchain = NativeToolchain {
            cc: tool.command.clone().into(), ar: "selected-target-ar".into(),
            ranlib: "selected-target-ranlib".into(), target_tuple: "fixture".into(),
            abi: "fixture".into(), fingerprint: "fixture".into(), compiler: tool.clone(),
            archiver: tool.clone(), ranlib_identity: tool,
        };
        let dependencies = BTreeMap::new();
        let request = RecipeRequest {
            package: "icu", version: catalog::version("icu", None).unwrap(),
            target: Target::new(Platform::Linux, Arch::X86_64), source: Path::new("source"),
            staging_prefix: Path::new("stage"), toolchain: &toolchain,
            dependency_prefixes: &dependencies,
        };
        let target = command(&request, Path::new("make"), false);
        let host = command(&request, Path::new("make"), true);
        let target_env: BTreeMap<_, _> = target.get_envs().collect();
        let host_env: BTreeMap<_, _> = host.get_envs().collect();
        assert_eq!(target_env[OsStr::new("CC")], Some(OsStr::new("selected-target-cc")));
        assert_eq!(target_env[OsStr::new("CXX")], Some(OsStr::new("selected-target-cc")));
        for (key, value) in [("CC", "cc"), ("CXX", "cc"), ("AR", "ar"), ("RANLIB", "ranlib")] {
            assert_eq!(host_env[OsStr::new(key)], Some(OsStr::new(value)));
        }
        assert_eq!(target_env[OsStr::new("LIBS")], Some(OsStr::new("-lstdc++")));
        assert_eq!(host_env[OsStr::new("LIBS")], Some(OsStr::new(cpp_library(Target::detect_host()))));
    }

    #[test]
    fn cpp_runtime_matches_every_supported_target() {
        assert_eq!(cpp_library(Target::new(Platform::MacOS, Arch::AArch64)), "-lc++");
        for variant in [AppleVariant::IOS, AppleVariant::IOSSimulator] {
            assert_eq!(cpp_library(Target::new_apple(Arch::AArch64, variant)), "-lc++");
        }
        for arch in [Arch::AArch64, Arch::X86_64] {
            assert_eq!(cpp_library(Target::new(Platform::Linux, arch)), "-lstdc++");
        }
    }
}
