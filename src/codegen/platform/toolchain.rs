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
