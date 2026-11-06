//! Purpose:
//! Computes a deterministic host identity from compiler, configuration, and project inputs.
//!
//! Called from:
//! - `crate::repl::run_inner` before selecting a cache lock and executable.
//!
//! Key details:
//! - Hashes preserve ordered options and raw environment bytes without storing their contents.
//! - Actual linked archive digests live in the receipt and are rechecked separately.

use super::*;

/// Fingerprints all launch inputs that can change the compiled bootstrap or its link resolution.
pub(in crate::repl) fn identity(compiler: &Path, cwd: &Path, args: &[String], php_version: u32,
    ini: &[(String, String)]) -> Result<String, String> {
    let mut hash = Sha256::new();
    field(&mut hash, b"elephc-repl-cache-v1");
    field(&mut hash, super::super::HOST_SOURCE.as_bytes());
    field(&mut hash, env!("ELEPHC_RUNTIME_BUILD_ID").as_bytes());
    field(&mut hash, &serde_json::to_vec(&FileStamp::read(compiler)?).map_err(|error| error.to_string())?);
    field(&mut hash, std::env::consts::OS.as_bytes());
    field(&mut hash, std::env::consts::ARCH.as_bytes());
    field(&mut hash, compiler.as_os_str().as_encoded_bytes());
    field(&mut hash, cwd.as_os_str().as_encoded_bytes());
    field(&mut hash, &php_version.to_le_bytes());
    for arg in args { field(&mut hash, arg.as_bytes()); }
    for (key, value) in ini {
        field(&mut hash, key.as_bytes());
        field(&mut hash, value.as_bytes());
    }
    let mut environment = std::env::vars_os().filter(|(key, _)| {
        let key = key.to_string_lossy();
        (key.starts_with("ELEPHC_") && !key.starts_with("ELEPHC_REPL_"))
            || matches!(key.as_ref(), "PATH" | "SDKROOT" | "MACOSX_DEPLOYMENT_TARGET" | "CC" | "CFLAGS" | "LDFLAGS" | "LIBRARY_PATH" | "CARGO_TARGET_DIR")
    }).collect::<Vec<_>>();
    environment.sort();
    for (key, value) in environment {
        field(&mut hash, key.as_encoded_bytes());
        field(&mut hash, value.as_encoded_bytes());
    }
    for directory in cwd.ancestors() {
        for name in ["elephc.toml", "elephc.lock", "composer.json", "composer.lock", ".php-version"] {
            let path = directory.join(name);
            field(&mut hash, path.as_os_str().as_encoded_bytes());
            match fs::read(&path) {
                Ok(bytes) => { field(&mut hash, b"present"); field(&mut hash, &bytes); }
                Err(error) if error.kind() == io::ErrorKind::NotFound => field(&mut hash, b"absent"),
                Err(error) => return Err(format!("cannot fingerprint '{}': {error}", path.display())),
            }
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Length-prefixes every field so different option boundaries cannot collide.
fn field(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}
