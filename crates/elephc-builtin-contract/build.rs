//! Purpose:
//! Stamps shared Core constant metadata with the compiler build date in PHP's format.
//!
//! Called from:
//! - Cargo before building the dependency-neutral builtin contract.
//!
//! Key details:
//! - SOURCE_DATE_EPOCH permits reproducible release builds.
//! - The date is shared by both backends rather than copied from a PHP baseline image.

use std::process::Command;

/// Formats a UTC build date and publishes it to the shared constant catalog.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    let mut command = Command::new("date");
    command.arg("-u");
    if let Ok(epoch) = std::env::var("SOURCE_DATE_EPOCH") {
        let epoch: u64 = epoch.parse().expect("SOURCE_DATE_EPOCH must be an unsigned timestamp");
        if cfg!(target_os = "macos") {
            command.args(["-r", &epoch.to_string()]);
        } else {
            command.args(["-d", &format!("@{epoch}")]);
        }
    }
    let output = command.arg("+%b %d %Y %H:%M:%S").env("LC_ALL", "C")
        .output().expect("date is required to stamp Core build metadata");
    assert!(output.status.success(), "cannot format the Core build date");
    let date = String::from_utf8(output.stdout).expect("build date is ASCII");
    println!("cargo:rustc-env=ELEPHC_CORE_BUILD_DATE={}", date.trim());
}
