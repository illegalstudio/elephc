//! Purpose:
//! Build-time native-library discovery for optional PDO system-client profiles.
//!
//! Called from:
//! - Cargo while compiling `elephc-pdo`.
//!
//! Key details:
//! - Homebrew's keg-only FreeTDS directory is added only for the `dblib` profile.

use std::path::Path;

/// Emits native search paths required by enabled system-client features.
fn main() {
    if std::env::var_os("CARGO_FEATURE_DBLIB").is_some() {
        for path in ["/opt/homebrew/opt/freetds/lib", "/usr/local/opt/freetds/lib"] {
            if Path::new(path).is_dir() {
                println!("cargo:rustc-link-search=native={path}");
            }
        }
    }
}
