//! Purpose:
//! Locates this machine's managed native `openssl-crypto` and `zlib` artifacts so a
//! codegen fixture that links the `elephc_openssl` cipher bridge can resolve
//! libcrypto's EVP/provider symbols and the zlib archive libcrypto references.
//!
//! Called from:
//! - `super::runner::test_link_plan` when a fixture's link plan names `elephc_openssl`.
//! - `tests/codegen/strings/openssl.rs` through `skip_without_openssl_native`.
//!
//! Key details:
//! - This is a test provider, not a fallback: it reads the same durable cache the
//!   production resolver writes and never consults a system or Homebrew OpenSSL.
//! - The `openssl-crypto` package is deliberately separate from curl's 3.5.8
//!   `openssl` pin: the cipher bridge needs 3.6.4's extra AES-CBC-HMAC-ETM methods.
//! - Missing artifacts skip visibly by default; `ELEPHC_TEST_REQUIRE_OPENSSL_NATIVE`
//!   turns that into a panic in CI jobs that materialize the package first.

use std::path::PathBuf;
use std::sync::OnceLock;

use super::native_cache::{find_package_library_dir, native_cache_artifacts_root};
use super::target;

/// The packages a linked `elephc_openssl` needs, in link order.
pub(crate) const OPENSSL_NATIVE_PACKAGES: &[(&str, &[&str])] = &[
    ("openssl-crypto", &["libcrypto.a"]),
    ("zlib", &["libz.a"]),
];

/// Environment variable that turns a missing artifact into a panic.
pub(crate) const REQUIRE_ENV: &str = "ELEPHC_TEST_REQUIRE_OPENSSL_NATIVE";

/// Stable token a log gate can grep for.
pub(crate) const SKIP_GATE_MARKER: &str = "ELEPHC_OPENSSL_NATIVE_SKIP_GATE";

/// One discovered package's `lib/` directory and exact archive paths.
#[derive(Clone, Debug)]
pub(crate) struct OpensslNativePackage {
    pub(crate) name: &'static str,
    pub(crate) library_dir: PathBuf,
    pub(crate) archives: Vec<PathBuf>,
}

/// Returns the managed packages a cipher fixture links, or `None` when any is missing.
pub(crate) fn openssl_native_packages() -> Option<&'static [OpensslNativePackage]> {
    static PACKAGES: OnceLock<Option<Vec<OpensslNativePackage>>> = OnceLock::new();
    PACKAGES.get_or_init(discover_packages).as_deref()
}

/// Returns whether this machine can link an OpenSSL cipher fixture at all.
pub(crate) fn available() -> bool {
    openssl_native_packages().is_some()
}

/// Reports a skip for `test_name` and returns whether the caller should return early.
pub(crate) fn skip_without_openssl_native(test_name: &str) -> bool {
    if available() {
        return false;
    }
    let target = target().as_str();
    if std::env::var_os(REQUIRE_ENV).is_some_and(|value| !value.is_empty()) {
        panic!(
            "{test_name}: {REQUIRE_ENV} is set but managed native openssl-crypto/zlib are not \
             installed for {target} (run: elephc native add openssl-crypto --target {target})"
        );
    }
    eprintln!(
        "{SKIP_GATE_MARKER} skipping {test_name}: managed native openssl-crypto/zlib are not \
         installed for {target} (run: elephc native add openssl-crypto --target {target})"
    );
    true
}

/// Discovers both packages, requiring every declared archive.
fn discover_packages() -> Option<Vec<OpensslNativePackage>> {
    let artifacts = native_cache_artifacts_root()?;
    let mut packages = Vec::with_capacity(OPENSSL_NATIVE_PACKAGES.len());
    for (name, archives) in OPENSSL_NATIVE_PACKAGES {
        let library_dir = find_package_library_dir(&artifacts, name, archives)?;
        packages.push(OpensslNativePackage {
            name,
            library_dir: library_dir.clone(),
            archives: archives.iter().map(|archive| library_dir.join(archive)).collect(),
        });
    }
    Some(packages)
}

/// Creates a CLI project that pins `openssl-crypto` and points the child at the
/// durable managed cache. Returns `None` when the package is not installed.
pub(crate) fn openssl_cli_command(dir: &std::path::Path) -> Option<std::process::Command> {
    openssl_native_packages()?;
    let cache_root = native_cache_artifacts_root()?.parent()?.to_path_buf();
    for (name, bytes) in [
        ("elephc.toml", include_bytes!("../../../examples/openssl-cipher/elephc.toml").as_slice()),
        ("elephc.lock", include_bytes!("../../../examples/openssl-cipher/elephc.lock").as_slice()),
    ] {
        let destination = dir.join(name);
        assert!(!destination.exists(), "OpenSSL CLI fixture requires a new owned project");
        std::fs::write(destination, bytes).expect("write OpenSSL native project metadata");
    }
    let mut command = super::elephc_cli_command(dir);
    command.env("ELEPHC_NATIVE_CACHE", cache_root);
    Some(command)
}
