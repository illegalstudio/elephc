//! Purpose:
//! Computes compilation artifact paths and post-link capability warnings.
//!
//! Called from:
//! - `crate::pipeline::compile()` and its backend stage.
//!
//! Key details:
//! - Library names follow each target platform's conventional prefix and suffix.

use std::path::{Path, PathBuf};

use crate::codegen::platform::{Platform, Target};
use crate::codegen::{Emit, RuntimeFeatures};

/// Holds the paths for all compilation output files (assembly, object, binary, source map)
/// and, for `--emit npm`, the generated package directory.
pub(super) struct OutputPaths {
    pub(super) asm: PathBuf,
    /// The native object file; `None` for WASM, which assembles in-process and never writes one.
    pub(super) obj: Option<PathBuf>,
    pub(super) bin: PathBuf,
    pub(super) source_map: PathBuf,
    pub(super) header: Option<PathBuf>,
    pub(super) package_dir: Option<PathBuf>,
}

/// Returns the post-link reminder for dynamic eval without optional regex support.
///
/// `runs_interpreted_code` separates a program that EXECUTES code through the interpreter from
/// one that links it only for an OPcache operation — see
/// `crate::ir_lower::module_runs_interpreted_code`. The note is about evaluated code, so it is
/// printed for the first only.
pub(super) fn dynamic_eval_capability_warning(
    runtime_features: RuntimeFeatures,
    runs_interpreted_code: bool,
) -> Option<&'static str> {
    (runtime_features.eval_bridge && runs_interpreted_code && !runtime_features.regex).then_some(concat!(
        "warning: dynamic eval was compiled without optional regex support\n",
        "evaluated code that uses preg_* will fail at runtime; enable it with:\n",
        "  elephc native add pcre2\n",
        "  elephc --with-regex <source-file>",
    ))
}

/// Computes output paths for .s (assembly), .o (object), binary, and .map (source map) files
/// derived from the input filename.
///
/// Executable mode produces `<stem>` (no extension). Cdylib mode produces
/// `lib<stem>.so` (Linux) or `lib<stem>.dylib` (macOS), matching the conventional
/// shared-library naming that `dlopen(3)` and linker `-l` flags expect.
pub(super) fn output_paths(filename: &str, target: Target, emit: Emit) -> OutputPaths {
    let path = Path::new(filename);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
    let parent = path.parent().unwrap_or(Path::new("."));

    // The WebAssembly target emits a `.wat` text module (the readable form, the
    // analogue of `.s`) and a `.wasm` binary (the artifact). It never produces a
    // `.o` object or runs the native linker. NPM packaging writes into a
    // `<stem>-npm/` directory to avoid colliding with the ordinary binary path.
    if target.is_wasm() {
        let package_dir =
            matches!(emit, Emit::NpmPackage).then(|| parent.join(format!("{}-npm", stem)));
        let bin = package_dir
            .as_ref()
            .map(|dir| dir.join("module.wasm"))
            .unwrap_or_else(|| parent.join(format!("{}.wasm", stem)));
        return OutputPaths {
            asm: parent.join(format!("{}.wat", stem)),
            obj: None,
            bin,
            source_map: parent.join(format!("{}.map", stem)),
            header: None,
            package_dir,
        };
    }

    let bin_name = match emit {
        // NpmPackage is wasm-only; for any native build it never occurs, but keep
        // the match exhaustive by treating it like an executable.
        Emit::Executable | Emit::NpmPackage => stem.to_string(),
        Emit::Cdylib => match target.platform {
            Platform::MacOS => format!("lib{}.dylib", stem),
            Platform::Linux => format!("lib{}.so", stem),
            Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
        },
        Emit::Staticlib => format!("lib{}.a", stem),
    };
    OutputPaths {
        asm: parent.join(format!("{}.s", stem)),
        obj: Some(parent.join(format!("{}.o", stem))),
        bin: parent.join(bin_name),
        source_map: parent.join(format!("{}.map", stem)),
        header: emit.is_library().then(|| parent.join(format!("lib{}.h", stem))),
        package_dir: None,
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for target-specific compilation output path selection.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - These tests only calculate paths and never write compilation artifacts.

    use super::output_paths;
    use crate::codegen::platform::{Arch, Platform, Target};
    use crate::codegen::Emit;
    use std::path::Path;

    /// Verifies ordinary WASM output remains adjacent to the PHP source.
    #[test]
    fn wasm_executable_uses_wat_and_wasm_paths() {
        let paths = output_paths("examples/hello.php", Target::wasm(), Emit::Executable);
        assert_eq!(paths.asm, Path::new("examples/hello.wat"));
        assert_eq!(paths.bin, Path::new("examples/hello.wasm"));
        assert_eq!(paths.package_dir, None);
    }

    /// Verifies WASM plans no native object: it never writes one, so no `.o` path exists to
    /// validate or to mistake for a destination, while a native build still plans it.
    #[test]
    fn only_native_builds_plan_an_object_path() {
        let wasm = output_paths("examples/hello.php", Target::wasm(), Emit::Executable);
        assert_eq!(wasm.obj, None);
        let npm = output_paths("examples/hello.php", Target::wasm(), Emit::NpmPackage);
        assert_eq!(npm.obj, None);
        let native = output_paths(
            "examples/hello.php",
            Target::new(Platform::Linux, Arch::X86_64),
            Emit::Executable,
        );
        assert_eq!(native.obj.as_deref(), Some(Path::new("examples/hello.o")));
    }

    /// Verifies NPM output uses an isolated directory containing `module.wasm`.
    #[test]
    fn wasm_npm_uses_isolated_package_directory() {
        let paths = output_paths("examples/hello.php", Target::wasm(), Emit::NpmPackage);
        assert_eq!(
            paths.package_dir.as_deref(),
            Some(Path::new("examples/hello-npm"))
        );
        assert_eq!(paths.bin, Path::new("examples/hello-npm/module.wasm"));
    }
}
