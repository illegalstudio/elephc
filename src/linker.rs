//! Purpose:
//! Owns assembler and linker process invocation for generated user and runtime objects.
//! Translates target metadata plus user link options into platform-specific tool commands.
//!
//! Called from:
//! - `crate::pipeline::compile()` after codegen writes assembly and prepares the runtime object.
//!
//! Key details:
//! - Target-specific command flags must stay aligned with `crate::codegen::platform::Target`.
//! - Non-system bridge staticlibs (TLS, PDO, ...) are described once in `BRIDGES`;
//!   discovery, source-tree auto-build, and link flags are all driven from that table.

use std::path::{Path, PathBuf};
use std::process::{self, Command};

use crate::codegen::platform::{Platform, Target};

/// A non-system elephc bridge staticlib: a Rust `staticlib` crate linked into
/// compiled PHP programs that use a given feature (e.g. the `https://` TLS
/// wrapper or PDO). Each entry in [`BRIDGES`] fully describes how to locate and
/// link one bridge, so adding a new library is a single table entry rather than
/// another copy of the discovery/build/link logic.
struct BridgeStaticlib {
    /// Linker library name: `-l<lib_name>` resolves `lib<lib_name>.a`
    /// (e.g. `"elephc_tls"`). Also matched against `extra_link_libs`.
    lib_name: &'static str,
    /// Environment override pointing directly at the directory holding the
    /// staticlib (e.g. `"ELEPHC_TLS_LIB_DIR"`). Takes precedence over discovery.
    env_var: &'static str,
    /// Cargo package that produces the staticlib (e.g. `"elephc-tls"`), used for
    /// the source-checkout auto-build and workspace detection.
    crate_name: &'static str,
    /// When true the whole archive is force-loaded so the staticlib's link-time
    /// side effects survive (e.g. rustls provider registration); when false a
    /// plain `-l` is enough.
    whole_archive: bool,
    /// Extra macOS frameworks required by the staticlib's transitive native
    /// dependencies (e.g. the PDO PostgreSQL driver pulls in `whoami`, which
    /// references CoreFoundation / SystemConfiguration).
    macos_frameworks: &'static [&'static str],
    /// Whether the staticlib needs the dynamic loader (`-ldl`) on Linux for its
    /// Rust runtime/unwinder symbols.
    needs_libdl: bool,
}

/// Every bridge staticlib elephc knows how to link. To support a new bridge,
/// add an entry here — `link()` and the discovery helpers are fully table-driven.
const BRIDGES: &[BridgeStaticlib] = &[
    BridgeStaticlib {
        lib_name: "elephc_tls",
        env_var: "ELEPHC_TLS_LIB_DIR",
        crate_name: "elephc-tls",
        whole_archive: true,
        macos_frameworks: &[],
        needs_libdl: true,
    },
    BridgeStaticlib {
        lib_name: "elephc_pdo",
        env_var: "ELEPHC_PDO_LIB_DIR",
        crate_name: "elephc-pdo",
        whole_archive: false,
        // The PostgreSQL driver pulls in `whoami` (to default the connection
        // user), which references CoreFoundation / SystemConfiguration on macOS.
        macos_frameworks: &["CoreFoundation", "SystemConfiguration"],
        needs_libdl: true,
    },
];

impl BridgeStaticlib {
    /// Returns the `lib<name>.a` archive filename this bridge produces.
    fn archive_filename(&self) -> String {
        format!("lib{}.a", self.lib_name)
    }

    /// Locates the directory containing this bridge's staticlib.
    ///
    /// Searches explicit configuration (`env_var`), installed layouts
    /// (`bin/elephc` plus sibling `lib/` — the layout produced by the Homebrew
    /// formula), `CARGO_TARGET_DIR`, and local `target/{debug,release}`
    /// fallbacks. In a source checkout, builds the staticlib once when it is
    /// missing so `cargo run --` can compile examples without a manual
    /// `cargo build -p <crate>`. Returns `None` when it cannot be found or built.
    fn lib_dir(&self) -> Option<String> {
        if let Ok(env_dir) = std::env::var(self.env_var) {
            if !env_dir.is_empty() {
                return Some(env_dir);
            }
        }
        if let Some(dir) = self.find_lib_dir() {
            return Some(dir);
        }
        let workspace = self.find_workspace()?;
        self.build_staticlib(&workspace);
        self.find_lib_dir()
    }

    /// Returns the first candidate directory that currently contains the staticlib.
    /// Order: the running binary's dir, its sibling `lib/`, `CARGO_TARGET_DIR`
    /// profiles, then in-tree `target/{debug,release}`.
    fn find_lib_dir(&self) -> Option<String> {
        let archive = self.archive_filename();
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?;
        let mut candidates = vec![
            dir.to_path_buf(),
            dir.parent().map(|parent| parent.join("lib")).unwrap_or_default(),
        ];
        if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
            if !target_dir.is_empty() {
                candidates.push(PathBuf::from(&target_dir).join("debug"));
                candidates.push(PathBuf::from(target_dir).join("release"));
            }
        }
        // Fallbacks for source-tree builds where the process cwd is the
        // workspace root or a path below it.
        candidates.push(PathBuf::from("target/debug"));
        candidates.push(PathBuf::from("target/release"));

        candidates
            .into_iter()
            .find(|candidate| candidate.join(&archive).exists())
            .map(|candidate| candidate.display().to_string())
    }

    /// Finds the nearest ancestor that looks like the elephc workspace checkout
    /// providing this bridge's crate (`crates/<crate_name>/Cargo.toml`).
    fn find_workspace(&self) -> Option<PathBuf> {
        let manifest = format!("crates/{}/Cargo.toml", self.crate_name);
        let cwd = std::env::current_dir().ok()?;
        cwd.ancestors()
            .find(|dir| dir.join(&manifest).exists())
            .map(Path::to_path_buf)
    }

    /// Builds this bridge's staticlib in the current binary's debug/release
    /// profile (best-effort; failures are ignored so callers fall back to other
    /// discovery candidates).
    fn build_staticlib(&self, workspace: &Path) {
        let release = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
            .is_some_and(|dir| dir.file_name().is_some_and(|name| name == "release"));
        let mut cmd = Command::new("cargo");
        cmd.args(["build", "-p", self.crate_name]);
        if release {
            cmd.arg("--release");
        }
        let _ = cmd.current_dir(workspace).status();
    }
}

/// Invokes the target assembler to produce an object file from assembly source.
/// - `target`: Compiler target (controls assembler command and flags).
/// - `asm_path`: Path to the generated `.s` assembly file.
/// - `obj_path`: Output path for the resulting `.o` object file.
/// Exits with status 1 if the assembler fails.
pub(crate) fn assemble(target: Target, asm_path: &Path, obj_path: &Path) {
    let mut as_cmd = Command::new(target.assembler_cmd());
    if target.platform == Platform::MacOS {
        as_cmd.args(["-arch", target.darwin_arch_name()]);
    }
    as_cmd.arg("-o").arg(obj_path).arg(asm_path);
    run_tool("Assembler", &mut as_cmd);
}

/// Links object files and runtime objects into a final binary.
/// - `target`: Compiler target (controls platform, linker command, and flags).
/// - `bin_path`: Output path for the final executable.
/// - `obj_path`: Path to the user code object file.
/// - `runtime_object_path`: Path to the compiler runtime object file.
/// - `extra_link_libs`: Additional libraries to link against (e.g., `["m", "pthread"]`).
/// - `extra_link_paths`: Additional `-L` search paths for libraries.
/// - `extra_frameworks`: Additional macOS frameworks to link against.
/// On macOS, `-lSystem` is always added. On Linux, `-static` is used when no extra libs are provided.
/// Bridge staticlibs named in `extra_link_libs` are located, search-pathed, and
/// linked (whole-archived when required) via the [`BRIDGES`] table.
/// Exits with status 1 if linking fails.
pub(crate) fn link(
    target: Target,
    bin_path: &Path,
    obj_path: &Path,
    runtime_object_path: &Path,
    extra_link_libs: &[String],
    extra_link_paths: &[String],
    extra_frameworks: &[String],
) {
    // Bridge staticlibs this program actually links, paired with the directory
    // each one resolved to (`None` when it could not be located/built). Driven
    // by the `BRIDGES` table so a new library needs no changes in this function.
    let needed_bridges: Vec<(&BridgeStaticlib, Option<String>)> = BRIDGES
        .iter()
        .filter(|bridge| extra_link_libs.iter().any(|l| l.as_str() == bridge.lib_name))
        .map(|bridge| (bridge, bridge.lib_dir()))
        .collect();
    let needs_libdl = needed_bridges.iter().any(|(bridge, _)| bridge.needs_libdl);

    let mut ld_cmd = match target.platform {
        Platform::MacOS => {
            let sdk_path = macos_sdk_path();
            let sdk_version = macos_sdk_version();
            let mut cmd = Command::new("ld");
            cmd.args(["-arch", target.darwin_arch_name(), "-e", "_main", "-o"]);
            cmd.arg(bin_path);
            cmd.arg(obj_path);
            cmd.arg(runtime_object_path);
            cmd.args(["-lSystem", "-syslibroot"]);
            cmd.arg(&sdk_path);
            cmd.args(["-platform_version", "macos", &sdk_version, &sdk_version]);
            cmd
        }
        Platform::Linux => {
            let mut cmd = Command::new(target.linker_cmd());
            cmd.arg("-o").arg(bin_path).arg(obj_path).arg(runtime_object_path);
            if extra_link_libs.is_empty() {
                cmd.arg("-static");
            }
            if !extra_link_libs.is_empty() {
                cmd.arg("-Wl,--no-as-needed");
            }
            cmd.args(["-lm", "-lpthread"]);
            if needs_libdl {
                cmd.arg("-ldl");
            }
            cmd
        }
    };
    // Search paths for the located bridge staticlibs.
    for (_, dir) in &needed_bridges {
        if let Some(dir) = dir.as_deref() {
            ld_cmd.arg(format!("-L{}", dir));
        }
    }
    if target.platform == Platform::MacOS && !extra_link_libs.is_empty() {
        for path in default_macos_library_paths() {
            ld_cmd.arg(format!("-L{}", path));
        }
    }
    for path in extra_link_paths {
        ld_cmd.arg(format!("-L{}", path));
    }
    for lib in extra_link_libs {
        if lib == "System" {
            continue;
        }
        // A bridge that must be whole-archived (and whose staticlib we located)
        // is force-loaded so its link-time side effects survive; everything else
        // links with a plain `-l`.
        let whole_archive_bridge = needed_bridges.iter().find(|(bridge, dir)| {
            bridge.lib_name == lib.as_str() && bridge.whole_archive && dir.is_some()
        });
        match whole_archive_bridge {
            Some((bridge, dir)) => {
                let dir = dir.as_deref().expect("whole-archive bridge has a located dir");
                match target.platform {
                    Platform::MacOS => {
                        let path = Path::new(dir).join(bridge.archive_filename());
                        ld_cmd.arg("-force_load").arg(path);
                    }
                    Platform::Linux => {
                        ld_cmd.arg("-Wl,--whole-archive");
                        ld_cmd.arg(format!("-l{}", bridge.lib_name));
                        ld_cmd.arg("-Wl,--no-whole-archive");
                    }
                }
            }
            None => {
                ld_cmd.arg(format!("-l{}", lib));
            }
        }
    }
    if target.platform == Platform::Linux && !extra_link_libs.is_empty() {
        ld_cmd.arg("-Wl,--as-needed");
    }
    if target.platform == Platform::MacOS {
        for fw in extra_frameworks {
            ld_cmd.args(["-framework", fw]);
        }
        // Frameworks required by the linked bridge staticlibs' transitive deps.
        for (bridge, _) in &needed_bridges {
            for fw in bridge.macos_frameworks {
                ld_cmd.args(["-framework", fw]);
            }
        }
    }
    run_tool("Linker", &mut ld_cmd);
}

/// Executes a tool command and exits the process if the command fails.
/// - `name`: Human-readable name for error messages (e.g., "Assembler", "Linker").
/// - `cmd`: Prepared `Command` to execute.
/// Prints an error message and exits with status 1 on failure.
fn run_tool(name: &str, cmd: &mut Command) {
    match cmd.status() {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!("{} failed with exit code {}", name, s);
            process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to run {}: {}", name, e);
            process::exit(1);
        }
    }
}

/// Returns the macOS SDK path by running `xcrun --show-sdk-path`.
///
/// Exits with an actionable diagnostic when no SDK path can be resolved (xcrun missing,
/// or returning empty output because the Xcode Command Line Tools are not installed /
/// `xcode-select` points at a bad directory) instead of passing an empty `-syslibroot`
/// argument to `ld`, which fails with a cryptic `ld: -syslibroot missing <path>`.
fn macos_sdk_path() -> String {
    let resolved = Command::new("xcrun")
        .args(["--show-sdk-path"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    match validate_macos_sdk_path(&resolved) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("{}", message);
            process::exit(1);
        }
    }
}

/// Validates a resolved macOS SDK path, returning the trimmed path or an actionable
/// error message when `xcrun` produced no path. Kept pure (no IO/exit) so the
/// empty-path diagnostic can be unit-tested.
fn validate_macos_sdk_path(resolved: &str) -> Result<String, String> {
    let trimmed = resolved.trim();
    if trimmed.is_empty() {
        return Err(
            "Could not locate the macOS SDK. Install the Xcode Command Line Tools \
             (run: xcode-select --install) and make sure `xcrun --show-sdk-path` prints a valid path."
                .to_string(),
        );
    }
    Ok(trimmed.to_string())
}

/// Returns common Homebrew library directories used for optional native deps on macOS.
fn default_macos_library_paths() -> Vec<&'static str> {
    ["/opt/homebrew/lib", "/usr/local/lib"]
        .into_iter()
        .filter(|path| Path::new(path).exists())
        .collect()
}

/// Returns the macOS SDK version string by running `xcrun --sdk macosx --show-sdk-version`.
/// Returns `"15.0"` as a fallback if the command fails or returns an empty version.
fn macos_sdk_version() -> String {
    match Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-version"])
        .output()
    {
        Ok(output) => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if version.is_empty() {
                "15.0".to_string()
            } else {
                version
            }
        }
        Err(_) => "15.0".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies an empty or whitespace-only SDK path (xcrun missing or misconfigured)
    /// yields an actionable Xcode Command Line Tools hint instead of being silently
    /// passed to `ld` as an empty `-syslibroot` argument.
    #[test]
    fn empty_sdk_path_produces_actionable_error() {
        let err = validate_macos_sdk_path("   ").expect_err("empty path must error");
        assert!(err.contains("xcode-select --install"), "got: {err}");
    }

    /// Verifies a real SDK path is returned trimmed and otherwise unchanged.
    #[test]
    fn valid_sdk_path_is_returned_trimmed() {
        let ok = validate_macos_sdk_path("  /Library/Dev/MacOSX.sdk\n").expect("valid path");
        assert_eq!(ok, "/Library/Dev/MacOSX.sdk");
    }
}
