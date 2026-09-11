//! Purpose:
//! Reproduces stale shared-contract staticlibs with controlled source/archive timestamps.
//!
//! Called from:
//! - The compiler library's focused bridge source-discovery unit tests.
//!
//! Key details:
//! - Synthetic local dependency graphs avoid rebuilding real bridges or sleeping for timestamps.
//! - Transitive, workspace, build, target, and cyclic paths are exercised independently.

use super::*;
use std::io::Write;

/// An isolated source tree with deterministic timestamps and automatic temporary cleanup.
struct Tree(PathBuf);

impl Tree {
    /// Creates a uniquely named root for a synthetic Cargo workspace.
    fn new() -> Self {
        let nonce = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("elephc-bridge-inputs-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    /// Writes one file and assigns its modification time without relying on filesystem resolution.
    fn write(&self, path: &str, contents: &str, second: u64) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(contents.as_bytes()).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(second))).unwrap();
    }

    /// Checks the synthetic bridge against an archive timestamp between old and changed inputs.
    fn stale(&self) -> bool {
        local_inputs_newer_than(&self.0, "bridge", SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(7200))
    }
}

impl Drop for Tree {
    /// Removes only this test's owned temporary source tree.
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}

/// Verifies shared contracts invalidate separate staticlibs through all local dependency forms.
#[test]
fn bridge_source_inputs_include_transitive_contracts() {
    let tree = Tree::new();
    tree.write("Cargo.toml", "[workspace.dependencies]\nshared = { path = 'crates/shared' }\ntool = { path = 'crates/tool' }\n", 3600);
    tree.write("crates/bridge/Cargo.toml", "[dependencies]\ncontract = { path = '../contract' }\n[build-dependencies]\ntool = { workspace = true }\n[target.'cfg(unix)'.dependencies]\nplatform = { path = '../platform', optional = true }\n[dev-dependencies]\ndev = { path = '../dev' }\n", 3600);
    tree.write("crates/contract/Cargo.toml", "[dependencies]\nshared = { workspace = true }\n", 3600);
    tree.write("crates/shared/Cargo.toml", "[dependencies]\ncycle = { path = '../bridge' }\n", 3600);
    for name in ["shared", "tool", "platform"] { tree.write(&format!("crates/{name}/src/lib.rs"), "// initial input\n", 3600); }
    assert!(!tree.stale());
    for name in ["shared", "tool", "platform"] {
        tree.write(&format!("crates/{name}/src/lib.rs"), "// changed input\n", 10800);
        assert!(tree.stale(), "changed {name} must invalidate an embedded staticlib");
        tree.write(&format!("crates/{name}/src/lib.rs"), "// initial input\n", 3600);
        assert!(!tree.stale());
    }
    tree.write("crates/dev/src/lib.rs", "// development-only dependency\n", 10800);
    tree.write("crates/unrelated/src/lib.rs", "// unrelated crate\n", 10800);
    tree.write("crates/bridge/target/debug/output.a", "build output", 10800);
    assert!(!tree.stale());
}

/// Verifies lockfiles and workspace Cargo configuration also invalidate an existing archive.
#[test]
fn bridge_source_inputs_include_workspace_configuration() {
    let tree = Tree::new();
    tree.write("Cargo.toml", "[workspace]\n", 3600);
    tree.write("crates/bridge/Cargo.toml", "[package]\nname = 'bridge'\n", 3600);
    assert!(!tree.stale());
    for name in ["Cargo.toml", "Cargo.lock", ".cargo/config", ".cargo/config.toml"] {
        tree.write(name, "# changed input\n", 10800);
        assert!(tree.stale(), "changed {name} must invalidate the archive");
        tree.write(name, "# old input\n", 3600);
        assert!(!tree.stale());
    }
}
