//! Purpose:
//! Selects assembler and linker commands for each supported target platform.
//! Builds the external command lines used after assembly generation.
//!
//! Called from:
//! - `crate::pipeline::compile()` after codegen emits assembly
//!
//! Key details:
//! - Tool choices must match the object format and transformed assembly produced for the target.

use std::process::Command;
use std::sync::OnceLock;

/// Checks whether the host machine has a native ARM64/AArch64 GCC toolchain.
///
/// Uses `gcc -dumpmachine` to query the host triple and caches the result
/// in a `OnceLock` to avoid repeated subprocess spawning. Returns `true`
/// if the triple contains "aarch64", `false` otherwise.
///
/// Side effects:
/// - Spawns a single subprocess on first call only.
pub(super) fn host_has_native_aarch64_toolchain() -> bool {
    static NATIVE_AARCH64: OnceLock<bool> = OnceLock::new();
    *NATIVE_AARCH64.get_or_init(|| {
        Command::new("gcc")
            .arg("-dumpmachine")
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).contains("aarch64"))
            .unwrap_or(false)
    })
}
