//! Purpose:
//! The C headers hosted PHP extensions compile against: Elephc's own PHP
//! extension ABI, embedded in the compiler and written out on demand.
//!
//! Called from:
//! - `crate::php_ext::install` before building an extension.
//!
//! Key details:
//! - Nothing comes from php-src. `elephc_zend_abi.h` is generated at build
//!   time from the elephc-zend crate's Rust source (see Elephc's `build.rs`),
//!   `zend_macros.h` holds the macros extensions expand, and every header name
//!   an extension includes (`php.h`, `zend_API.h`, `ext/standard/info.h`...) is
//!   a one-line file including both.
//! - The tree is written once per content digest under the extension cache,
//!   through a staging directory renamed into place, so concurrent builds and
//!   interrupted writes never leave a partial tree.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::native_deps::NativeError;

/// The ABI's types, constants and prototypes, generated from Rust.
const ABI_HEADER: &str = include_str!(concat!(env!("OUT_DIR"), "/elephc_zend_abi.h"));
/// The macros extensions expand.
const MACROS_HEADER: &str = include_str!("../../crates/elephc-zend/include/zend_macros.h");
/// The header names extensions include.
const HEADER_NAMES: &str = include_str!("../../crates/elephc-zend/include/headers.list");

const UMBRELLA: &str = "/* elephc_zend.h — the PHP extension ABI as Elephc provides it. */\n\
#ifndef ELEPHC_ZEND_H\n#define ELEPHC_ZEND_H\n#include \"elephc_zend_abi.h\"\n#include \"zend_macros.h\"\n#endif\n";

const FORWARDER: &str = "#include \"elephc_zend.h\"\n";

/// Include directories in phpize's order, relative to the tree root.
const INCLUDE_DIRS: &[&str] = &["", "main", "Zend", "TSRM", "ext"];

/// Every file of the tree, as (relative path, contents).
pub fn files() -> Vec<(String, &'static str)> {
    let mut files = vec![
        ("elephc_zend_abi.h".to_string(), ABI_HEADER),
        ("zend_macros.h".to_string(), MACROS_HEADER),
        ("elephc_zend.h".to_string(), UMBRELLA),
    ];
    for name in HEADER_NAMES.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')) {
        files.push((name.to_string(), FORWARDER));
    }
    files
}

/// A digest of the headers' contents: what an extension built against them
/// depends on, part of every installed extension's cache key.
pub fn identity() -> String {
    let mut digest = Sha256::new();
    for (path, contents) in files() {
        digest.update(path.as_bytes());
        digest.update([0u8]);
        digest.update(contents.as_bytes());
        digest.update([0u8]);
    }
    format!("{:x}", digest.finalize())
}

/// Writes the tree under `cache_root` (once per content) and returns its root.
pub fn materialize(cache_root: &Path) -> Result<PathBuf, NativeError> {
    let identity = identity();
    let root = cache_root.join("include").join(&identity[..32]);
    if root.join("elephc_zend.h").is_file() {
        return Ok(root);
    }
    let staging = root.with_extension(format!("stage.{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    for (relative, contents) in files() {
        let path = staging.join(&relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| NativeError::io("create extension include directory", parent, error))?;
        }
        fs::write(&path, contents).map_err(|error| NativeError::io("write extension header", &path, error))?;
    }
    if let Err(error) = fs::rename(&staging, &root) {
        let _ = fs::remove_dir_all(&staging);
        if !root.join("elephc_zend.h").is_file() {
            return Err(NativeError::io("publish extension include directory", &root, error));
        }
    }
    Ok(root)
}

/// `-I` flags for a tree rooted at `root`, in phpize's order.
pub fn include_flags(root: &Path) -> Vec<String> {
    INCLUDE_DIRS
        .iter()
        .flat_map(|dir| ["-I".to_string(), root.join(dir).display().to_string()])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the generated include tree provides the headers extensions include (`main/php.h`,
    /// `Zend/zend_API.h`, `ext/standard/info.h`, `ext/spl/spl_exceptions.h`), that the generated
    /// ABI and macro headers carry the expected declarations, and that no php-src text is embedded.
    #[test]
    fn every_included_name_forwards_to_the_abi() {
        let files = files();
        let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
        for expected in ["main/php.h", "Zend/zend_API.h", "ext/standard/info.h", "ext/spl/spl_exceptions.h"] {
            assert!(names.contains(&expected), "{expected} missing from the include tree");
        }
        assert!(ABI_HEADER.contains("struct _zval_struct {"), "the generated header declares zval");
        assert!(ABI_HEADER.contains("zend_result zend_parse_parameters(uint32_t num_args"));
        assert!(MACROS_HEADER.contains("#define ZEND_PARSE_PARAMETERS_START("));
        assert!(!ABI_HEADER.contains("php-src") && !MACROS_HEADER.contains("Copyright (c) The PHP Group"));
    }

    /// Verifies the include tree identity (a 64-character digest) is deterministic across calls.
    #[test]
    fn the_identity_is_stable() {
        assert_eq!(identity(), identity());
        assert_eq!(identity().len(), 64);
    }
}
