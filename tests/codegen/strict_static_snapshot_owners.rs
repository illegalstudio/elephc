//! Purpose:
//! Checks retaining Reflection static/capture snapshots before stronger static origin integration.
//!
//! Called from:
//! - Focused strict static snapshot ownership regressions.
//!
//! Key details:
//! - Repeated snapshots retain the source values while retiring temporary keys and value leases.

use crate::support::*;

/// Static and closure snapshots preserve their values and retire every temporary owner.
#[test]
fn test_strict_static_snapshot_temporary_owners_bundle() {
    let source = include_str!("../fixtures/strict/reflection_static_snapshot_owners.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for strict in [false, true] {
            let mut flags = vec![mode, "--heap-debug"];
            if strict { flags.push("--strict-locals"); }
            let output = StrictFixture::new(source).capture(&flags);
            assert_eq!(String::from_utf8_lossy(&output.stdout), "7:11|7:11|7:11|", "{flags:?}");
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

/// Opaque eval keeps dynamic static placeholders lazy in both optimizer modes and profiles.
#[test]
fn test_strict_static_reflection_deferred_initializers_bundle() {
    let body = include_str!("../fixtures/strict/reflection_static_constants.php").strip_prefix("<?php\n").unwrap();
    let quoted = body.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $code=$argc>0?'{quoted}':'';eval($code);gc_collect_cycles();");
    let expected = "NULL:NULL:9:9:3:two:4:2:3:C:NULL:4:reflected_pending:ff00:0|";
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for strict in [false, true] {
            let mut flags = vec![mode, "--heap-debug"];
            if strict { flags.push("--strict-locals"); }
            let output = StrictFixture::new(&source).capture(&flags);
            assert_eq!(String::from_utf8_lossy(&output.stdout), format!("{expected}{expected}NULL|NULL|NULL|7:1:7:1|E1:1:1:ff:NULL:NULL:NULL:NULL:0:1:1:1::1|"), "{flags:?}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(!stderr.contains("Warning:"), "{flags:?}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{flags:?}: {stderr}");
        }
    }
}
