//! Purpose:
//! Verifies hosted-extension admission at the persistent web lifecycle boundary.
//!
//! Called from:
//! - Focused compiler library tests for extension resolution.
//!
//! Key details:
//! - Admission runs before native artifacts or target toolchains are resolved.

use super::*;

/// Application mode accepts empty manifests and rejects declared Zend storage on every target.
#[test]
fn hosted_extensions_refuse_application_before_artifact_resolution() {
    let dir = std::env::temp_dir().join(format!("elephc-ext-web-lifecycle-{}", std::process::id()));
    fs::create_dir(&dir).expect("create unique fixture directory");
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        /// Removes only the fixture directory successfully created by this test.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let _cleanup = Cleanup(dir.clone());
    let source = dir.join("main.php");
    fs::write(&source, "<?php").unwrap();
    fs::write(dir.join("elephc.toml"), "").unwrap();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        assert!(resolve_hosted(&source, Target::parse(name).unwrap(), true).unwrap().is_none(), "{name}");
    }
    fs::write(
        dir.join("elephc.toml"),
        "[extension]\nschema = 1\n[extension.dependencies]\ndemo = { path = \"missing-extension\" }\n",
    ).unwrap();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let error = resolve_hosted(&source, Target::parse(name).unwrap(), true).unwrap_err().to_string();
        assert!(error.contains("hosted PHP extensions are not supported with --web-lifecycle=application"), "{name}: {error}");
        assert!(error.contains("--web-lifecycle=request"), "{name}: {error}");
    }
}
