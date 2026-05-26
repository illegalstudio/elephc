//! Purpose:
//! Owns assembler and linker process invocation for generated user and runtime objects.
//! Translates target metadata plus user link options into platform-specific tool commands.
//!
//! Called from:
//! - `crate::pipeline::compile()` after codegen writes assembly and prepares the runtime object.
//!
//! Key details:
//! - Target-specific command flags must stay aligned with `crate::codegen::platform::Target`.

use std::path::Path;
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
            cmd
        }
    };
    for path in extra_link_paths {
        ld_cmd.arg(format!("-L{}", path));
    }
    for lib in extra_link_libs {
        if lib != "System" {
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

/// Returns the macOS SDK path by running `xcrun --show-sdk-path`.
/// Returns an empty string if the command fails.
fn macos_sdk_path() -> String {
    Command::new("xcrun")
        .args(["--show-sdk-path"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
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
