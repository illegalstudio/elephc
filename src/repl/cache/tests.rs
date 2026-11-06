//! Purpose:
//! Verifies executable cache receipts and configuration identity without invoking a linker.
//!
//! Called from:
//! - `cargo test --bin elephc repl::cache`.
//!
//! Key details:
//! - Each test owns and removes its isolated directory, including after a failed assertion.
//! - Invalidation covers changed dependency bytes, missing files, and executable permissions.

use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

/// Creates one exclusive fixture directory managed by the production staging cleanup guard.
fn fixture() -> Stage {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("elephc-repl-cache-{}-{nonce}", std::process::id()));
    fs::create_dir(&path).unwrap();
    Stage(path)
}

/// Publishes a small synthetic receipt to test validation independently from native toolchains.
fn receipt(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let host = root.join("host");
    let archive = root.join("bridge.a");
    let path = root.join("receipt.json");
    fs::write(&host, b"executable fixture").unwrap();
    fs::set_permissions(&host, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(&archive, b"original archive").unwrap();
    let receipt = Receipt { identity: "key".into(), executable: digest_file(&host).unwrap(),
        dependencies: vec![Dependency { path: archive.clone(), bridge: None,
            digest: digest_file(&archive).unwrap(), stamp: FileStamp::read(&archive).unwrap() }] };
    fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    (host, path, archive)
}

/// Dependency replacement invalidates executable reuse, even for equal-length changed content.
#[test]
fn repl_cache_validates_linked_dependency_content() {
    let root = fixture();
    let (host, receipt, archive) = receipt(&root.0);
    assert!(valid(&host, &receipt, "key"));
    let replacement = root.0.join("replacement.a");
    fs::write(&replacement, b"replaced archive").unwrap();
    fs::rename(replacement, &archive).unwrap();
    assert!(!valid(&host, &receipt, "key"));
    fs::write(&archive, b"original archive").unwrap();
    assert!(valid(&host, &receipt, "key"), "identical rebuilt archive remains reusable");
    fs::remove_file(archive).unwrap();
    assert!(!valid(&host, &receipt, "key"));
}

/// Invalid receipts, nonprivate executables, and symlinks cannot become cache hits.
#[test]
fn repl_cache_rejects_invalid_publications() {
    let root = fixture();
    let (host, receipt, _) = receipt(&root.0);
    assert!(!valid(&host, &receipt, "wrong-key"));
    fs::set_permissions(&host, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!valid(&host, &receipt, "key"));
    fs::set_permissions(&host, fs::Permissions::from_mode(0o700)).unwrap();
    let link = root.0.join("symlink");
    std::os::unix::fs::symlink(&host, &link).unwrap();
    assert!(!valid(&link, &receipt, "key"));
    fs::write(&receipt, b"interrupted receipt").unwrap();
    assert!(!valid(&host, &receipt, "key"));
}

/// Configuration, PHP profile, and project-file contents partition the host cache.
#[test]
fn repl_cache_identity_tracks_configuration() {
    let root = fixture();
    let compiler = root.0.join("compiler");
    fs::write(&compiler, b"compiler fixture").unwrap();
    let baseline = identity(&compiler, &root.0, &[], 80500, &[]).unwrap();
    assert_eq!(baseline, identity(&compiler, &root.0, &[], 80500, &[]).unwrap());
    assert_ne!(baseline, identity(&compiler, &root.0, &[], 80400, &[]).unwrap());
    assert_ne!(baseline, identity(&compiler, &root.0, &["--strict-php".into()], 80500, &[]).unwrap());
    assert_ne!(baseline, identity(&compiler, &root.0, &[], 80500, &[("precision".into(), "7".into())]).unwrap());
    fs::write(root.0.join("elephc.toml"), "[ini]\nprecision = 7\n").unwrap();
    assert_ne!(baseline, identity(&compiler, &root.0, &[], 80500, &[]).unwrap());
}
