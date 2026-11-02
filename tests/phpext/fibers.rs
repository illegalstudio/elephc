//! Purpose:
//! Real-PHP regressions for suspended native activations on host Fiber stacks.
//!
//! Called from:
//! - The `phpext_tests` integration test binary with a PHP 8.5 development SDK.
//!
//! Key details:
//! - The same driver runs in the embed harness under native memory checks.
//! - Reentry on the owning Fiber remains valid while overlapping Fiber entry fails safely.

use super::*;

/// Rejects another host Fiber before it can import a parked activation's native stack state.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn compiled_php_fibers_reject_overlapping_native_activations() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile("fibers", include_str!("lifecycle.php"));
    let driver = project.0.join("fibers.php");
    fs::write(&driver, include_str!("fibers.php")).unwrap();
    let result = php(&sdk, &module).arg(&driver).output().unwrap();
    assert!(result.status.success(), "{:?}: {}\n{}", result.status,
        String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
    assert!(result.stdout.is_empty(), "{}", String::from_utf8_lossy(&result.stdout));
    assert!(result.stderr.is_empty(), "{}", String::from_utf8_lossy(&result.stderr));
}
