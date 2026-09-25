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
pub(super) fn dynamic_eval_capability_warning(
    runtime_features: RuntimeFeatures,
) -> Option<&'static str> {
    (runtime_features.eval_bridge && !runtime_features.regex).then_some(concat!(
        "warning: dynamic eval was compiled without optional regex support\n",
        "evaluated code that uses preg_* or mb_ereg_match() will fail at runtime; enable it with:\n",
        "  elephc native add pcre2\n",
        "  elephc --with-regex <source-file>",
    ))
}

/// Computes output paths for .s (assembly), .o (object), binary, and .map (source map) files
/// derived from the input filename.
///
/// Executable mode produces `<stem>` on Unix and `<stem>.exe` on Windows.
/// Cdylib mode follows each target's conventional shared-library spelling.
pub(super) fn output_paths(filename: &str, target: Target, emit: Emit) -> OutputPaths {
    let path = Path::new(filename);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
    let parent = path.parent().unwrap_or(Path::new("."));
    let bin_name = match emit {
        Emit::Executable => match target.platform {
            Platform::Windows => format!("{stem}.exe"),
            Platform::MacOS | Platform::Linux => stem.to_string(),
        },
        Emit::Cdylib => match target.platform {
            Platform::MacOS => format!("lib{}.dylib", stem),
            Platform::Linux => format!("lib{}.so", stem),
            Platform::Windows => format!("{stem}.dll"),
        },
        Emit::Staticlib => format!("lib{}.a", stem),
    };
    OutputPaths {
        asm: parent.join(format!("{}.s", stem)),
        obj: parent.join(format!("{}.o", stem)),
        bin: parent.join(bin_name),
        source_map: parent.join(format!("{}.map", stem)),
        header: emit.is_library().then(|| parent.join(format!("lib{}.h", stem))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Arch;

    /// Verifies Windows executables use the suffix expected by native process launchers.
    #[test]
    fn windows_executable_uses_exe_suffix() {
        let target = Target::new(Platform::Windows, Arch::X86_64);
        assert_eq!(
            output_paths("demo.php", target, Emit::Executable).bin,
            Path::new("demo.exe")
        );
    }

    /// Verifies Windows dynamic libraries use PE's conventional DLL spelling.
    #[test]
    fn windows_cdylib_uses_dll_suffix() {
        let target = Target::new(Platform::Windows, Arch::X86_64);
        assert_eq!(
            output_paths("demo.php", target, Emit::Cdylib).bin,
            Path::new("demo.dll")
        );
    }
}
