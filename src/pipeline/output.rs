//! Purpose:
//! Computes compilation artifact paths and post-link capability warnings.
//!
//! Called from:
//! - `crate::pipeline::compile()` and its backend stage.
//!
//! Key details:
//! - Shared-library names follow each target platform's conventional suffix.

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
/// Executable mode produces `<stem>` (no extension). Cdylib mode produces
/// `lib<stem>.so` (Linux) or `lib<stem>.dylib` (macOS), matching the conventional
/// shared-library naming that `dlopen(3)` and linker `-l` flags expect.
pub(super) fn output_paths(filename: &str, target: Target, emit: Emit) -> OutputPaths {
    let path = Path::new(filename);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
    let parent = path.parent().unwrap_or(Path::new("."));
    let bin_name = match emit {
        Emit::Executable => stem.to_string(),
        Emit::Cdylib => match target.platform {
            Platform::MacOS => format!("lib{}.dylib", stem),
            Platform::Linux => format!("lib{}.so", stem),
            Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
        },
    };
    OutputPaths {
        asm: parent.join(format!("{}.s", stem)),
        obj: parent.join(format!("{}.o", stem)),
        bin: parent.join(bin_name),
        source_map: parent.join(format!("{}.map", stem)),
        header: matches!(emit, Emit::Cdylib).then(|| parent.join(format!("lib{}.h", stem))),
    }
}
