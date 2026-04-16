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
