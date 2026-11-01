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

/// Holds the paths for all compilation output files (assembly, object, binary, source map).
pub(super) struct OutputPaths {
    pub(super) asm: PathBuf,
    pub(super) obj: PathBuf,
    pub(super) bin: PathBuf,
    pub(super) source_map: PathBuf,
    pub(super) header: Option<PathBuf>,
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
pub(super) fn output_paths(
    filename: &str,
    target: Target,
    emit: Emit,
    output_dir: Option<&Path>,
) -> OutputPaths {
    let path = Path::new(filename);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
    let parent = output_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| path.parent().unwrap_or(Path::new(".")).to_path_buf());
    let bin_name = match emit {
        Emit::Executable => stem.to_string(),
        Emit::Cdylib => match target.platform {
            Platform::MacOS => format!("lib{}.dylib", stem),
            Platform::Linux => format!("lib{}.so", stem),
            Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
        },
        Emit::Staticlib => format!("lib{}.a", stem),
        Emit::Phpext => format!("{}.so", stem),
    };
    OutputPaths {
        asm: parent.join(format!("{}.s", stem)),
        obj: parent.join(format!("{}.o", stem)),
        bin: parent.join(bin_name),
        source_map: parent.join(format!("{}.map", stem)),
        header: (emit.is_library() && emit != Emit::Phpext).then(|| parent.join(format!("lib{}.h", stem))),
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for compiler artifact path selection.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Explicit output directories must relocate every artifact together.

    use super::output_paths;
    use crate::codegen::platform::{Arch, Platform, Target};
    use crate::codegen::Emit;
    use std::path::Path;

    /// Verifies the legacy default keeps artifacts beside the PHP source.
    #[test]
    fn default_paths_remain_beside_source() {
        let target = Target::new(Platform::Linux, Arch::AArch64);
        let paths = output_paths("app/public/index.php", target, Emit::Executable, None);
        assert_eq!(paths.asm, Path::new("app/public/index.s"));
        assert_eq!(paths.obj, Path::new("app/public/index.o"));
        assert_eq!(paths.bin, Path::new("app/public/index"));
        assert_eq!(paths.source_map, Path::new("app/public/index.map"));
    }

    /// Verifies an explicit directory relocates all artifacts without changing names.
    #[test]
    fn explicit_directory_relocates_all_artifacts() {
        let target = Target::new(Platform::Linux, Arch::AArch64);
        let paths = output_paths(
            "app/public/index.php",
            target,
            Emit::Executable,
            Some(Path::new("target/application")),
        );
        assert_eq!(paths.asm, Path::new("target/application/index.s"));
        assert_eq!(paths.obj, Path::new("target/application/index.o"));
        assert_eq!(paths.bin, Path::new("target/application/index"));
        assert_eq!(paths.source_map, Path::new("target/application/index.map"));
    }
}
