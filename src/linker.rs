//! Purpose:
//! Owns assembler and linker process invocation for generated user and runtime objects.
//! Translates target metadata plus user link options into platform-specific tool commands.
//!
//! Called from:
//! - `crate::pipeline::compile()` after codegen writes assembly and prepares the runtime object.
//!
//! Key details:
//! - Target-specific command flags must stay aligned with `crate::codegen::platform::Target`.

use std::path::{Path, PathBuf};
use std::process::{self, Command};

use crate::codegen::platform::{Platform, Target};

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
    let needs_elephc_tls = extra_link_libs.iter().any(|l| l == "elephc_tls");
    let elephc_tls_dir = needs_elephc_tls.then(elephc_tls_lib_dir).flatten();

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
            if needs_elephc_tls {
                cmd.arg("-ldl");
            }
            cmd
        }
    };
    if let Some(dir) = elephc_tls_dir.as_deref() {
        ld_cmd.arg(format!("-L{}", dir));
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
        if lib != "System" {
            if lib == "elephc_tls" {
                if let Some(dir) = elephc_tls_dir.as_deref() {
                    match target.platform {
                        Platform::MacOS => {
                            let path = Path::new(dir).join("libelephc_tls.a");
                            ld_cmd.arg("-force_load").arg(path);
                        }
                        Platform::Linux => {
                            ld_cmd.arg("-Wl,--whole-archive");
                            ld_cmd.arg("-lelephc_tls");
                            ld_cmd.arg("-Wl,--no-whole-archive");
                        }
                    }
                    continue;
                }
            }
            ld_cmd.arg(format!("-l{}", lib));
        }
    }
    if target.platform == Platform::Linux && !extra_link_libs.is_empty() {
        ld_cmd.arg("-Wl,--as-needed");
    }
    if target.platform == Platform::MacOS {
        for fw in extra_frameworks {
            ld_cmd.args(["-framework", fw]);
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

/// Locates `libelephc_tls.a` for programs that use TLS-backed stream wrappers.
///
/// Searches explicit configuration, installed layouts (`bin/elephc` plus
/// sibling `lib/`), and local Cargo target directories. In a source checkout,
/// attempts to build the staticlib once when it is missing so `cargo run --`
/// can compile TLS examples without a manual preparatory command.
fn elephc_tls_lib_dir() -> Option<String> {
    if let Ok(env_dir) = std::env::var("ELEPHC_TLS_LIB_DIR") {
        if !env_dir.is_empty() {
            return Some(env_dir);
        }
    }

    if let Some(dir) = find_elephc_tls_lib_dir() {
        return Some(dir);
    }

    let workspace = find_elephc_tls_workspace()?;
    build_elephc_tls_staticlib(&workspace);
    find_elephc_tls_lib_dir()
}

/// Returns the first candidate directory that currently contains `libelephc_tls.a`.
fn find_elephc_tls_lib_dir() -> Option<String> {
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
    // Fallbacks for source-tree builds where the process cwd is the workspace
    // root or a path below it.
    candidates.push(PathBuf::from("target/debug"));
    candidates.push(PathBuf::from("target/release"));

    candidates
        .into_iter()
        .find(|candidate| candidate.join("libelephc_tls.a").exists())
        .map(|candidate| candidate.display().to_string())
}

/// Finds the nearest ancestor that looks like an elephc workspace checkout.
fn find_elephc_tls_workspace() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    cwd.ancestors()
        .find(|dir| dir.join("crates/elephc-tls/Cargo.toml").exists())
        .map(Path::to_path_buf)
}

/// Builds the TLS staticlib in the current binary's debug/release profile.
fn build_elephc_tls_staticlib(workspace: &Path) {
    let release = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .is_some_and(|dir| dir.file_name().is_some_and(|name| name == "release"));
    let mut cmd = Command::new("cargo");
    cmd.args(["build", "-p", "elephc-tls"]);
    if release {
        cmd.arg("--release");
    }
    let _ = cmd.current_dir(workspace).status();
}

/// Returns the macOS SDK path by running `xcrun --show-sdk-path`.
/// Returns an empty string if the command fails.
fn macos_sdk_path() -> String {
    Command::new("xcrun")
        .args(["--show-sdk-path"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
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
