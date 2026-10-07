//! Purpose:
//! Selects the assembler and linker driver for Windows x86-64 PE output.
//! Preserves the MinGW GNU default while providing an opt-in LLVM path.
//!
//! Called from:
//! - `crate::linker` for user objects and final image linking.
//! - `crate::runtime_cache` for cached runtime-object assembly.
//!
//! Key details:
//! - LLVM mode retains the `x86_64-pc-windows-gnu` ABI used by bridge archives.
//! - Toolchain selection never advertises Guard CF by itself.

use std::env;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Environment variable selecting the Windows assembler/linker family.
const WINDOWS_TOOLCHAIN_ENV: &str = "ELEPHC_WINDOWS_TOOLCHAIN";

/// Supported Windows PE toolchain families.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowsToolchain {
    /// MinGW binutils assembler plus the MinGW GCC linker driver.
    Gnu,
    /// Clang integrated assembler plus LLD through the Clang driver.
    Llvm,
}

impl WindowsToolchain {
    /// Reads the Windows toolchain selection, defaulting to the established GNU path.
    pub(crate) fn configured() -> Result<Self, String> {
        Self::parse(env::var(WINDOWS_TOOLCHAIN_ENV).ok().as_deref())
    }

    /// Parses a toolchain selector without reading process-global environment state.
    fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.map(str::trim).filter(|value| !value.is_empty()) {
            None | Some("gnu") | Some("mingw") => Ok(Self::Gnu),
            Some("llvm") | Some("gnullvm") => Ok(Self::Llvm),
            Some(value) => Err(format!(
                "invalid {WINDOWS_TOOLCHAIN_ENV} value '{value}'; expected 'gnu' or 'llvm'"
            )),
        }
    }

    /// Returns the stable cache-key component for objects produced by this toolchain.
    pub(crate) fn cache_key(self) -> &'static str {
        match self {
            Self::Gnu => "gnu",
            Self::Llvm => "llvm",
        }
    }
}

/// Constructs the configured Windows x86-64 assembler command.
///
/// LLVM mode invokes Clang with an explicit GNU Windows target, which selects
/// the integrated assembler and emits the same PE/COFF object format consumed
/// by the existing MinGW ABI runtime and bridge archives.
pub(crate) fn assembler_command(asm_path: &Path, obj_path: &Path) -> Result<Command, String> {
    match WindowsToolchain::configured()? {
        WindowsToolchain::Gnu => {
            let mut command = Command::new("x86_64-w64-mingw32-as");
            command.arg("-o").arg(obj_path).arg(asm_path);
            Ok(command)
        }
        WindowsToolchain::Llvm => {
            let clang = env::var_os("ELEPHC_WINDOWS_CLANG").unwrap_or_else(|| "clang".into());
            let mut command = Command::new(clang);
            command
                .arg("--target=x86_64-w64-windows-gnu")
                .arg("-c")
                .arg("-o")
                .arg(obj_path)
                .arg(asm_path);
            Ok(command)
        }
    }
}

/// Constructs the configured Windows x86-64 linker driver command.
///
/// LLVM mode still targets the GNU Windows ABI so MinGW import libraries and
/// Rust `x86_64-pc-windows-gnu` bridge archives remain link-compatible. The
/// MinGW sysroot is resolved explicitly because cross-target Clang installations
/// cannot reliably infer Homebrew, distro, or gnullvm layouts.
pub(crate) fn linker_command() -> Result<Command, String> {
    match WindowsToolchain::configured()? {
        WindowsToolchain::Gnu => Ok(Command::new("x86_64-w64-mingw32-gcc")),
        WindowsToolchain::Llvm => {
            let clang = env::var_os("ELEPHC_WINDOWS_CLANG").unwrap_or_else(|| "clang".into());
            let lld = env::var("ELEPHC_WINDOWS_LLD").unwrap_or_else(|_| "lld".to_string());
            let lld = lld_for_clang(&lld)?;
            let sysroot = llvm_sysroot()?;
            let gcc_support_dir = llvm_gcc_support_library_dir()?;
            let mut command = Command::new(clang);
            command
                .arg("--target=x86_64-w64-windows-gnu")
                .arg(format!("--sysroot={}", sysroot.display()))
                // Debian's MinGW GCC support libraries live outside the sysroot
                // (`/usr/lib/gcc/<triple>/<version>/`). Clang otherwise passes
                // `-lgcc -lgcc_eh` to LLD without a search path for either.
                .arg(format!("-L{}", gcc_support_dir.display()))
                .arg(format!("-fuse-ld={}", lld.display()));
            Ok(command)
        }
    }
}

/// Gives Clang's linker selection logic an `ld.lld`-named path for rustup's
/// generic `rust-lld` driver, whose flavor is otherwise ambiguous at startup.
fn lld_for_clang(configured: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(configured);
    let basename = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
    if basename != "rust-lld" && basename != "rust-lld.exe" {
        return Ok(path);
    }
    if !path.is_file() {
        return Err(format!(
            "ELEPHC_WINDOWS_LLD='{}' does not name a file",
            path.display()
        ));
    }

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    let metadata = std::fs::metadata(&path).map_err(|error| {
        format!(
            "failed to inspect ELEPHC_WINDOWS_LLD '{}': {error}",
            path.display()
        )
    })?;
    metadata.len().hash(&mut hasher);
    metadata.modified().ok().hash(&mut hasher);
    let alias_dir = env::temp_dir().join(format!("elephc-rust-lld-driver-{:016x}", hasher.finish()));
    std::fs::create_dir_all(&alias_dir).map_err(|error| {
        format!(
            "failed to create rust-lld driver directory '{}': {error}",
            alias_dir.display()
        )
    })?;
    let alias_name = if cfg!(windows) { "ld.lld.exe" } else { "ld.lld" };
    let alias = alias_dir.join(alias_name);
    if !alias.exists() {
        create_lld_alias(&path, &alias)?;
    }
    Ok(alias)
}

/// Creates the flavor-selecting `ld.lld` alias used by Clang.
#[cfg(unix)]
fn create_lld_alias(source: &Path, alias: &Path) -> Result<(), String> {
    match std::os::unix::fs::symlink(source, alias) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(format!(
            "failed to alias rust-lld '{}' as '{}': {error}",
            source.display(),
            alias.display()
        )),
    }
}

/// Creates the flavor-selecting `ld.lld.exe` alias used by Clang.
#[cfg(not(unix))]
fn create_lld_alias(source: &Path, alias: &Path) -> Result<(), String> {
    match std::fs::hard_link(source, alias) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(format!(
            "failed to alias rust-lld '{}' as '{}': {error}",
            source.display(),
            alias.display()
        )),
    }
}

/// Resolves the MinGW sysroot used by the LLVM Windows linker driver.
fn llvm_sysroot() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("ELEPHC_WINDOWS_SYSROOT") {
        let path = PathBuf::from(path);
        if path.is_dir() {
            return Ok(path);
        }
        return Err(format!(
            "ELEPHC_WINDOWS_SYSROOT='{}' is not a directory",
            path.display()
        ));
    }

    let gcc = env::var_os("ELEPHC_WINDOWS_GCC").unwrap_or_else(|| "x86_64-w64-mingw32-gcc".into());
    let output = Command::new(&gcc)
        .arg("-print-sysroot")
        .output()
        .map_err(|error| {
            format!(
                "LLVM Windows toolchain needs a MinGW sysroot; failed to run '{} -print-sysroot': {error}. Set ELEPHC_WINDOWS_SYSROOT explicitly",
                Path::new(&gcc).display()
            )
        })?;
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let path = PathBuf::from(&value);
    if output.status.success() && !value.is_empty() && path.is_dir() {
        Ok(path)
    } else {
        Err(format!(
            "LLVM Windows toolchain could not resolve a MinGW sysroot from '{} -print-sysroot'; set ELEPHC_WINDOWS_SYSROOT explicitly",
            Path::new(&gcc).display()
        ))
    }
}

/// Resolves the GCC private library directory required by Clang's GNU Windows link.
///
/// MinGW packages on Debian place `libgcc.a` and `libgcc_eh.a` under GCC's
/// versioned directory rather than inside the target sysroot. Querying the
/// selected GCC driver keeps Homebrew, distro, and custom toolchains aligned.
fn llvm_gcc_support_library_dir() -> Result<PathBuf, String> {
    let gcc = env::var_os("ELEPHC_WINDOWS_GCC").unwrap_or_else(|| "x86_64-w64-mingw32-gcc".into());
    let output = Command::new(&gcc)
        .arg("-print-libgcc-file-name")
        .output()
        .map_err(|error| {
            format!(
                "LLVM Windows toolchain needs MinGW GCC support libraries; failed to run '{} -print-libgcc-file-name': {error}",
                Path::new(&gcc).display()
            )
        })?;
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let libgcc = PathBuf::from(&value);
    if output.status.success() && !value.is_empty() && libgcc.is_file() {
        return gcc_support_library_dir_from(&libgcc);
    }
    Err(format!(
        "LLVM Windows toolchain could not resolve GCC support library from '{} -print-libgcc-file-name'; set ELEPHC_WINDOWS_GCC to a MinGW GCC driver",
        Path::new(&gcc).display()
    ))
}

/// Returns the linker-search directory containing a verified GCC support archive.
fn gcc_support_library_dir_from(libgcc: &Path) -> Result<PathBuf, String> {
    libgcc.parent().map(Path::to_path_buf).ok_or_else(|| {
        format!(
            "LLVM Windows toolchain could not determine the parent directory of GCC support library '{}'",
            libgcc.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the absent selector preserves the established MinGW GNU default.
    #[test]
    fn defaults_to_gnu() {
        assert_eq!(WindowsToolchain::parse(None), Ok(WindowsToolchain::Gnu));
        assert_eq!(WindowsToolchain::parse(Some("")), Ok(WindowsToolchain::Gnu));
    }

    /// Verifies documented GNU, LLVM, and gnullvm selector spellings.
    #[test]
    fn accepts_documented_values() {
        assert_eq!(WindowsToolchain::parse(Some("gnu")), Ok(WindowsToolchain::Gnu));
        assert_eq!(WindowsToolchain::parse(Some("mingw")), Ok(WindowsToolchain::Gnu));
        assert_eq!(WindowsToolchain::parse(Some("llvm")), Ok(WindowsToolchain::Llvm));
        assert_eq!(WindowsToolchain::parse(Some("gnullvm")), Ok(WindowsToolchain::Llvm));
    }

    /// Verifies a typo produces an actionable diagnostic instead of silently changing tools.
    #[test]
    fn rejects_unknown_values() {
        let error = WindowsToolchain::parse(Some("msvc")).expect_err("MSVC ABI is unsupported");
        assert!(error.contains(WINDOWS_TOOLCHAIN_ENV));
        assert!(error.contains("expected 'gnu' or 'llvm'"));
    }

    /// Verifies a normal LLD selector is passed through without filesystem changes.
    #[test]
    fn preserves_named_lld_driver() {
        assert_eq!(lld_for_clang("lld").expect("named driver"), PathBuf::from("lld"));
        assert_eq!(
            lld_for_clang("/opt/toolchain/ld.lld").expect("absolute driver"),
            PathBuf::from("/opt/toolchain/ld.lld")
        );
    }

    /// Verifies rustup's generic driver receives an `ld.lld` alias so Clang selects GNU flavor.
    #[test]
    fn aliases_rust_lld_for_clang_flavor_detection() {
        let fixture_dir = env::temp_dir().join(format!("elephc-rust-lld-test-{}", std::process::id()));
        std::fs::create_dir_all(&fixture_dir).expect("create rust-lld fixture directory");
        let fixture = fixture_dir.join(if cfg!(windows) { "rust-lld.exe" } else { "rust-lld" });
        std::fs::write(&fixture, b"fixture").expect("write rust-lld fixture");

        let alias = lld_for_clang(fixture.to_str().expect("UTF-8 fixture path"))
            .expect("create flavor-selecting alias");

        assert_eq!(
            alias.file_name().and_then(|name| name.to_str()),
            Some(if cfg!(windows) { "ld.lld.exe" } else { "ld.lld" })
        );
        assert_eq!(std::fs::read(&alias).expect("read alias"), b"fixture");
        let _ = std::fs::remove_dir_all(&fixture_dir);
        if let Some(alias_dir) = alias.parent() {
            let _ = std::fs::remove_dir_all(alias_dir);
        }
    }

    /// Verifies a resolved GCC support archive yields its containing linker directory.
    #[test]
    fn derives_gcc_support_directory_from_libgcc_archive() {
        let fixture_dir = env::temp_dir().join(format!("elephc-libgcc-test-{}", std::process::id()));
        std::fs::create_dir_all(&fixture_dir).expect("create libgcc fixture directory");
        let archive = fixture_dir.join("libgcc.a");
        std::fs::write(&archive, b"fixture").expect("write libgcc fixture archive");

        let parent = archive.parent().expect("fixture archive parent").to_path_buf();
        assert_eq!(
            gcc_support_library_dir_from(&archive).expect("derive GCC support directory"),
            parent
        );

        let _ = std::fs::remove_dir_all(&fixture_dir);
    }
}
