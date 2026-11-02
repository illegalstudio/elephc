//! Purpose:
//! Computes a compile-time identity for the source tree that emits the shared runtime object.
//! This invalidates runtime-cache entries from another worktree without generating runtime assembly on hits.
//!
//! Called from:
//! - Cargo before compiling the `elephc` crate.
//!
//! Key details:
//! - The digest is deterministic, includes ordered Rust source paths and bytes, and emits rerun directives.
//! - Also generates `elephc_zend_abi.h`, the C header hosted PHP extensions compile
//!   against, from the elephc-zend crate's Rust source with that crate's own generator,
//!   so `elephc` embeds it without linking the engine.
//! - Embeds the neutral Rust value-layout facts for fail-closed producer SDK checks.

use std::fs;
use std::path::{Path, PathBuf};

#[path = "crates/elephc-zend/abi_header.rs"]
mod abi_header;

#[path = "crates/elephc-zend/src/abi/mod.rs"]
#[allow(dead_code, unused_imports)]
mod abi;

/// Recursively hashes Rust source files in lexical path order and asks Cargo to rerun for each one.
fn hash_rust_sources(root: &Path, state: &mut u64) {
    let mut entries = fs::read_dir(root)
        .unwrap_or_else(|err| panic!("failed to read runtime identity directory '{}': {err}", root.display()))
        .map(|entry| entry.expect("failed to enumerate runtime identity source"))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            hash_rust_sources(&path, state);
            continue;
        }
        if path.extension().map_or(true, |extension| extension != "rs") {
            continue;
        }
        let relative = path
            .strip_prefix("src")
            .expect("runtime identity source stays under src");
        println!("cargo:rerun-if-changed={}", path.display());
        hash_bytes(state, relative.to_string_lossy().as_bytes());
        hash_bytes(state, &fs::read(&path).unwrap_or_else(|err| {
            panic!("failed to read runtime identity source '{}': {err}", path.display())
        }));
    }
}

/// Applies a stable FNV-1a update so identity bytes do not depend on process-random hashing.
fn hash_bytes(state: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *state ^= u64::from(*byte);
        *state = state.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

/// Publishes a source-derived runtime-emitter identity to `runtime_cache`.
fn main() {
    let mut state = 0xcbf2_9ce4_8422_2325;
    // Runtime emission can depend on shared codegen/ABI helpers as well as the
    // runtime modules. Hashing all crate sources is intentionally conservative:
    // correctness matters more than a rare cache invalidation after unrelated edits.
    // Per-file directives cover edits. The directory directive additionally
    // invalidates the cache when a runtime source is added or removed.
    println!("cargo:rerun-if-changed=src");
    hash_rust_sources(Path::new("src"), &mut state);
    println!("cargo:rustc-env=ELEPHC_RUNTIME_BUILD_ID={state:016x}");

    println!("cargo:rerun-if-changed=crates/elephc-zend/src");
    println!("cargo:rerun-if-changed=crates/elephc-zend/abi_header.rs");
    let header = abi_header::generate(Path::new("crates/elephc-zend/src"))
        .unwrap_or_else(|message| panic!("elephc-zend ABI header: {message}"));
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    fs::write(out.join("elephc_zend_abi.h"), header).expect("write elephc_zend_abi.h");
    let layout = abi::values::value_layout_facts();
    fs::write(out.join("elephc_zend_value_layout.rs"), format!("{layout:?}"))
        .expect("write elephc_zend_value_layout.rs");
}
